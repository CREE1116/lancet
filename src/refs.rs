//! Lightweight index-accelerated symbol definition + lexical AST reference finder (`lancet refs`).
//!
//! Finds `[def]` declarations and `[ref]` identifier occurrences across codebases while
//! automatically excluding comments and string/char literals on AST-supported languages.

use crate::code;
use crate::index::{self, Req};
use crate::output::{ErrorInfo, Stats, Status};
use crate::query::{Expr, FileType, Filters, Matcher};
use crate::scope::{plan_targets_with_req, IndexMode, Target};
use crate::walk::{walk, FileFilter, WalkOpts};
use memchr::memmem;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct RefsOptions {
    pub symbol: String,
    /// Directories or files to search; results show paths joined onto each.
    pub roots: Vec<PathBuf>,
    pub lang: Option<String>,
    pub exts: Vec<String>,
    pub paths: Vec<String>,
    pub all_files: bool,
    pub limit: Option<usize>,
    pub walk: WalkOpts,
    pub index: IndexMode,
    pub index_dir: Option<PathBuf>,
}

impl RefsOptions {
    pub fn new(symbol: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        RefsOptions {
            symbol: symbol.into(),
            roots: vec![root.into()],
            lang: None,
            exts: Vec::new(),
            paths: Vec::new(),
            all_files: false,
            limit: None,
            walk: WalkOpts::default(),
            index: IndexMode::Auto,
            index_dir: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RefHit {
    pub path: String,
    pub line: usize,
    pub col: usize,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enclosing: Option<String>,
    pub text: String,
    pub ast_verified: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RefsStats {
    pub files_considered: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_candidates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_files: Option<usize>,
    pub files_scanned: usize,
    pub elapsed_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct RefsResponse {
    pub status: Status,
    pub mode: String,
    pub symbol: String,
    pub definitions: Vec<RefHit>,
    pub references: Vec<RefHit>,
    pub total_references: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub stats: RefsStats,
}

impl RefsResponse {
    fn failure(status: Status, symbol: String, message: String, elapsed_ms: f64) -> Self {
        RefsResponse {
            status,
            mode: "scan".into(),
            symbol,
            definitions: Vec::new(),
            references: Vec::new(),
            total_references: 0,
            truncated: false,
            warnings: Vec::new(),
            error: Some(ErrorInfo { message, pos: None }),
            stats: RefsStats {
                elapsed_ms,
                ..RefsStats::default()
            },
        }
    }
}

fn byte_to_line_col(line_starts: &[usize], byte_off: usize) -> (usize, usize) {
    match line_starts.binary_search(&byte_off) {
        Ok(idx) => (idx + 1, 1),
        Err(idx) => {
            let line = idx.max(1);
            let ls = line_starts[line - 1];
            (line, byte_off.saturating_sub(ls) + 1)
        }
    }
}

struct FileRefs {
    scanned: bool,
    defs: Vec<RefHit>,
    refs: Vec<RefHit>,
    /// Rust `.name` / `name:` uses, dropped when every definition is a free `fn`.
    member: Vec<RefHit>,
    /// Uses of a same-named local binding, dropped when the name has a definition.
    local: Vec<RefHit>,
    /// How many of `defs` are Rust `fn`s outside any `impl` / `trait`.
    free_fns: usize,
}

fn scan_one_file(target: &Target, symbol: &str, finder: &memmem::Finder<'_>) -> Option<FileRefs> {
    let bytes = fs::read(&target.path).ok()?;
    if memchr::memchr(0, &bytes).is_some() {
        return None;
    }
    if finder.find(&bytes).is_none() {
        return Some(FileRefs {
            scanned: false,
            defs: Vec::new(),
            refs: Vec::new(),
            member: Vec::new(),
            local: Vec::new(),
            free_fns: 0,
        });
    }
    let src = String::from_utf8(bytes).ok()?;
    let raw_bytes = src.as_bytes();

    let mut line_starts = vec![0usize];
    for pos in memchr::memchr_iter(b'\n', raw_bytes) {
        line_starts.push(pos + 1);
    }
    let lines: Vec<&str> = src.lines().collect();
    let line_snippet = |line1: usize| -> String {
        lines
            .get(line1.saturating_sub(1))
            .map(|s| s.trim().chars().take(160).collect())
            .unwrap_or_default()
    };

    let mut defs = Vec::new();
    let mut refs = Vec::new();
    let mut member = Vec::new();
    let mut local = Vec::new();
    let mut free_fns = 0;

    if code::ENABLED && code::supports_path(&target.path) {
        let syms = code::extract_symbols(&target.path, raw_bytes).unwrap_or_default();
        let funcs = code::extract_functions(&target.path, raw_bytes).unwrap_or_default();
        let mut def_spans: HashSet<(usize, usize)> = HashSet::new();

        for sp in &syms {
            if sp.name.as_deref() == Some(symbol) {
                def_spans.insert((sp.name_start, sp.name_end));
                let (ln, col) = byte_to_line_col(&line_starts, sp.name_start);
                defs.push(RefHit {
                    path: target.rel.clone(),
                    line: ln,
                    col,
                    kind: sp.kind.to_string(),
                    enclosing: None,
                    text: line_snippet(ln),
                    ast_verified: true,
                });
            }
        }

        let locals = crate::rename::local_name_starts(&target.path, &src).unwrap_or_default();
        if let Some(id_spans) = code::find_identifier_spans(&target.path, raw_bytes, symbol) {
            for (s, e, role) in id_spans {
                if def_spans.contains(&(s, e)) {
                    free_fns += (role == code::IdRole::FreeFn) as usize;
                    continue;
                }
                let (ln, col) = byte_to_line_col(&line_starts, s);
                let enclosing = funcs
                    .iter()
                    .filter(|f| {
                        f.start <= s && e <= f.end && !(f.name_start == s && f.name_end == e)
                    })
                    .min_by_key(|f| f.end - f.start)
                    .and_then(|f| f.name.clone());
                let hit = RefHit {
                    path: target.rel.clone(),
                    line: ln,
                    col,
                    kind: "ref".into(),
                    enclosing,
                    text: line_snippet(ln),
                    ast_verified: true,
                };
                if role == code::IdRole::Member {
                    member.push(hit);
                } else if locals.binary_search(&s).is_ok() {
                    local.push(hit);
                } else {
                    refs.push(hit);
                }
            }
        }
    } else {
        for_each_token(raw_bytes, &mut |bs, be| {
            if &raw_bytes[bs..be] == symbol.as_bytes() {
                let (ln, col) = byte_to_line_col(&line_starts, bs);
                refs.push(RefHit {
                    path: target.rel.clone(),
                    line: ln,
                    col,
                    kind: "ref".into(),
                    enclosing: None,
                    text: line_snippet(ln),
                    ast_verified: false,
                });
            }
            true
        });
    }

    Some(FileRefs {
        scanned: true,
        defs,
        refs,
        member,
        local,
        free_fns,
    })
}

pub fn find_refs(opts: &RefsOptions) -> RefsResponse {
    if let Some(msg) = missing_roots(opts) {
        return RefsResponse::failure(Status::InvalidQuery, opts.symbol.clone(), msg, 0.0);
    }
    find_refs_raw(opts)
}

fn missing_roots(opts: &RefsOptions) -> Option<String> {
    let msgs: Vec<String> = opts
        .roots
        .iter()
        .filter_map(|r| crate::walk::missing_root(r))
        .collect();
    (!msgs.is_empty()).then(|| msgs.join("; "))
}

/// Files to scan for `opts` (code files under the root, narrowed by `--lang` /
/// `--ext` / `--path`), using the index to keep only files that can contain `req`.
struct Planned {
    targets: Vec<Target>,
    mode: String,
    warnings: Vec<String>,
    stats: Stats,
}

/// Every root's files, as one list: `refs` decides what a name means across all
/// of them at once. Paths are already joined onto the root they came from.
fn plan_files(opts: &RefsOptions, req: &Req) -> Result<Planned, (Status, String)> {
    let mut all = Planned {
        targets: Vec::new(),
        mode: String::new(),
        warnings: Vec::new(),
        stats: Stats::default(),
    };
    let mut seen = HashSet::new();
    for root in &opts.roots {
        let p = plan_root(opts, root, req)?;
        all.mode = match all.mode.as_str() {
            "" => p.mode,
            m if m == p.mode => p.mode,
            _ => "mixed".into(),
        };
        all.warnings.extend(p.warnings);
        all.stats.files_considered += p.stats.files_considered;
        for (dst, src) in [
            (&mut all.stats.index_candidates, p.stats.index_candidates),
            (&mut all.stats.stale_files, p.stats.stale_files),
        ] {
            if let Some(n) = src {
                *dst = Some(dst.unwrap_or(0) + n);
            }
        }
        for mut t in p.targets {
            if seen.insert(t.path.clone()) {
                t.rel = crate::walk::display_path(root, &t.rel);
                all.targets.push(t);
            }
        }
    }
    Ok(all)
}

fn plan_root(opts: &RefsOptions, root: &Path, req: &Req) -> Result<Planned, (Status, String)> {
    let mut filters = Filters::default();
    if let Some(lang_s) = &opts.lang {
        #[cfg(feature = "ast")]
        {
            match crate::code::Lang::parse(lang_s) {
                Some(l) => {
                    filters.exts = l.extensions().iter().map(|s| (*s).to_string()).collect();
                }
                None => {
                    return Err((Status::InvalidQuery, format!("unknown --lang `{lang_s}`")));
                }
            }
        }
        #[cfg(not(feature = "ast"))]
        {
            let _ = lang_s;
            return Err((
                Status::Unsupported,
                "`--lang` requires building with `--features ast`".into(),
            ));
        }
    } else if !opts.exts.is_empty() {
        filters.exts = opts
            .exts
            .iter()
            .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
            .filter(|e| !e.is_empty())
            .collect();
    } else if !opts.all_files && root.is_dir() {
        filters.types = vec![FileType::Code];
    }
    filters.paths = opts.paths.clone();

    let filter = match FileFilter::new(&filters) {
        Ok(f) => f,
        Err(e) => return Err((Status::InvalidQuery, e)),
    };

    let mut warnings = Vec::new();
    let mut mode = "scan".to_string();
    let mut core_stats = Stats::default();
    let mut targets: Option<Vec<Target>> = None;

    if opts.index != IndexMode::Off {
        match index::discover(root, opts.index_dir.as_deref()) {
            Ok(Some((ix, rel_prefix))) => {
                mode = if opts.index == IndexMode::Trust {
                    "index_trusted".into()
                } else {
                    "index".into()
                };
                targets = Some(plan_targets_with_req(
                    &ix,
                    req,
                    &filter,
                    root,
                    &rel_prefix,
                    &opts.walk,
                    opts.index,
                    None,
                    &mut core_stats,
                    None,
                ));
            }
            Ok(None) => {
                if opts.index == IndexMode::Trust {
                    warnings.push(
                        "no index found; fell back to a full scan (run `lancet index`)".into(),
                    );
                }
            }
            Err(e) => {
                warnings.push(format!("index unusable ({e:#}); fell back to a full scan"));
            }
        }
    }

    let targets = targets.unwrap_or_else(|| {
        walk(root, &opts.walk, &filter, false)
            .into_iter()
            .map(|e| Target {
                path: e.path,
                rel: e.rel,
            })
            .collect()
    });
    if core_stats.files_considered == 0 {
        core_stats.files_considered = targets.len();
    }
    Ok(Planned {
        targets,
        mode,
        warnings,
        stats: core_stats,
    })
}

fn find_refs_raw(opts: &RefsOptions) -> RefsResponse {
    let t0 = Instant::now();
    let sym = opts.symbol.trim();
    if sym.is_empty() {
        return RefsResponse::failure(
            Status::InvalidQuery,
            opts.symbol.clone(),
            "symbol name cannot be empty".into(),
            ms(t0),
        );
    }

    let req = Req::from_expr(&Expr::Match {
        m: Matcher::Lit {
            text: sym.to_string(),
        },
    });
    let Planned {
        targets,
        mode,
        warnings,
        stats: core_stats,
    } = match plan_files(opts, &req) {
        Ok(p) => p,
        Err((status, msg)) => return RefsResponse::failure(status, sym.to_string(), msg, ms(t0)),
    };

    let finder = memmem::Finder::new(sym.as_bytes());
    let per_file: Vec<Option<FileRefs>> = targets
        .par_iter()
        .map(|t| scan_one_file(t, sym, &finder))
        .collect();

    let per_file: Vec<FileRefs> = per_file.into_iter().flatten().collect();
    let n_defs: usize = per_file.iter().map(|f| f.defs.len()).sum();
    // A free `fn` is never called as `x.name()` and is no field: those uses are
    // other types' methods and fields that share the name (`.rev()`, `rev:`).
    let only_free_fns = n_defs > 0 && per_file.iter().map(|f| f.free_fns).sum::<usize>() == n_defs;
    let mut warnings = warnings;
    let mut files_scanned = 0usize;
    let mut definitions = Vec::new();
    let mut references = Vec::new();
    let (mut hidden, mut hidden_locals) = (0usize, 0usize);
    for mut item in per_file {
        files_scanned += item.scanned as usize;
        definitions.extend(item.defs);
        if only_free_fns {
            hidden += item.member.len();
        } else {
            item.refs.append(&mut item.member);
        }
        // A local binding shadows the definition: its uses mean the local.
        if n_defs > 0 {
            hidden_locals += item.local.len();
        } else {
            item.refs.append(&mut item.local);
        }
        item.refs.sort_by_key(|r| (r.line, r.col));
        references.extend(item.refs);
    }
    if hidden > 0 {
        warnings.push(format!(
            "left out {hidden} `.{sym}` / `{sym}:` use(s): `{sym}` is a free function, so those are methods or fields of other types"
        ));
    }
    if hidden_locals > 0 {
        warnings.push(format!(
            "left out {hidden_locals} use(s) of local variables also named `{sym}`"
        ));
    }

    let total_references = references.len();
    let mut truncated = false;
    if let Some(lim) = opts.limit {
        if references.len() > lim {
            references.truncate(lim);
            truncated = true;
        }
    }

    let empty = definitions.is_empty() && total_references == 0;
    RefsResponse {
        status: if empty {
            Status::Empty
        } else {
            Status::Complete
        },
        mode,
        symbol: sym.to_string(),
        definitions,
        references,
        total_references,
        truncated,
        warnings,
        error: None,
        stats: RefsStats {
            files_considered: core_stats.files_considered,
            index_candidates: core_stats.index_candidates,
            stale_files: core_stats.stale_files,
            files_scanned,
            elapsed_ms: ms(t0),
        },
    }
}

pub fn print_refs_human(resp: &RefsResponse, w: &mut impl Write) -> io::Result<()> {
    if let Some(err) = &resp.error {
        eprintln!("error: {}", err.message);
        return Ok(());
    }
    if !resp.definitions.is_empty() {
        writeln!(w, "Definitions ({}):", resp.definitions.len())?;
        for d in &resp.definitions {
            writeln!(
                w,
                "  [def:{}] {}:{}:{}: {}",
                d.kind, d.path, d.line, d.col, d.text
            )?;
        }
    }
    if !resp.references.is_empty() {
        if !resp.definitions.is_empty() {
            writeln!(w)?;
        }
        writeln!(
            w,
            "References ({} shown of {}):",
            resp.references.len(),
            resp.total_references
        )?;
        for r in &resp.references {
            let scope = r
                .enclosing
                .as_deref()
                .map(|s| format!(" (in {s})"))
                .unwrap_or_default();
            writeln!(
                w,
                "  [ref] {}:{}:{}{scope}: {}",
                r.path, r.line, r.col, r.text
            )?;
        }
    }
    Ok(())
}

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}

/// One declaration and how often its name is used elsewhere (`lancet refs --rank`).
#[derive(Clone, Debug, Serialize)]
pub struct RankEntry {
    pub name: String,
    pub kind: &'static str,
    pub path: String,
    pub line: usize,
    /// Occurrences of the name in code (not comments or strings), minus its declarations.
    pub references: usize,
    /// Declarations sharing this name (overloads, same-named types in different
    /// modules): they share the count, as `refs` is lexical.
    pub definitions: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct RankResponse {
    pub status: Status,
    pub mode: String,
    pub pattern: String,
    pub entries: Vec<RankEntry>,
    /// Declarations that matched before `limit`.
    pub total: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub files_parsed: usize,
    pub elapsed_ms: f64,
}

/// The longest run of literal text in a glob (`*Service` → `Service`).
fn glob_literal(g: &str) -> &str {
    g.split(['*', '?', '[', ']', '{', '}', ','])
        .max_by_key(|s| s.len())
        .unwrap_or("")
}

/// Declarations whose name matches the glob `opts.symbol` (and `kinds`, if any),
/// ranked by how many times the name is used — "the most used `*Service` classes".
pub fn rank_symbols(opts: &RefsOptions, kinds: &[String]) -> RankResponse {
    let t0 = Instant::now();
    let pattern = opts.symbol.trim().to_string();
    let fail = |status: Status, msg: String| RankResponse {
        status,
        mode: "scan".into(),
        pattern: pattern.clone(),
        entries: Vec::new(),
        total: 0,
        truncated: false,
        warnings: Vec::new(),
        error: Some(ErrorInfo {
            message: msg,
            pos: None,
        }),
        files_parsed: 0,
        elapsed_ms: ms(t0),
    };
    if !code::ENABLED {
        return fail(
            Status::Unsupported,
            "`--rank` requires building with `--features ast`".into(),
        );
    }
    if let Some(msg) = missing_roots(opts) {
        return fail(Status::InvalidQuery, msg);
    }
    let glob = match globset::Glob::new(if pattern.is_empty() { "*" } else { &pattern }) {
        Ok(g) => g.compile_matcher(),
        Err(e) => {
            return fail(
                Status::InvalidQuery,
                format!("bad pattern `{pattern}`: {e}"),
            )
        }
    };
    let lit = glob_literal(&pattern).to_string();
    let req = if lit.is_empty() {
        Req::All
    } else {
        Req::from_expr(&Expr::Match {
            m: Matcher::Lit { text: lit.clone() },
        })
    };
    let planned = match plan_files(opts, &req) {
        Ok(p) => p,
        Err((status, msg)) => return fail(status, msg),
    };
    let finder = memmem::Finder::new(lit.as_bytes());
    let keep = |t: &str| glob.is_match(t);
    // Per file: matching declarations (name, kind, line) and token counts.
    type Found = (
        String,
        Vec<(String, &'static str, usize)>,
        std::collections::HashMap<String, usize>,
    );
    let per_file: Vec<Found> = planned
        .targets
        .par_iter()
        .filter_map(|t| {
            let src = fs::read(&t.path).ok()?;
            if !lit.is_empty() && finder.find(&src).is_none() {
                return None;
            }
            let (syms, counts) = code::symbols_and_token_counts(&t.path, &src, &keep)?;
            let defs = syms
                .into_iter()
                .filter_map(|s| {
                    let name = s.name?;
                    glob.is_match(&name).then(|| {
                        let line = memchr::memchr_iter(b'\n', &src[..s.name_start]).count() + 1;
                        (name, s.kind, line)
                    })
                })
                .collect();
            Some((t.rel.clone(), defs, counts))
        })
        .collect();
    let mut uses: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut decls: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (_, defs, counts) in &per_file {
        for (k, v) in counts {
            *uses.entry(k.clone()).or_insert(0) += v;
        }
        for (name, _, _) in defs {
            *decls.entry(name.clone()).or_insert(0) += 1;
        }
    }
    let wanted = |k: &str| kinds.is_empty() || kinds.iter().any(|w| w.eq_ignore_ascii_case(k));
    let mut entries: Vec<RankEntry> = per_file
        .iter()
        .flat_map(|(rel, defs, _)| {
            defs.iter()
                .filter(|d| wanted(d.1))
                .map(|(name, kind, line)| {
                    let n_decl = decls[name];
                    RankEntry {
                        name: name.clone(),
                        kind,
                        path: rel.clone(),
                        line: *line,
                        references: uses.get(name).copied().unwrap_or(0).saturating_sub(n_decl),
                        definitions: n_decl,
                    }
                })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.references
            .cmp(&a.references)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.path.cmp(&b.path))
            .then(a.line.cmp(&b.line))
    });
    let total = entries.len();
    let limit = opts.limit.unwrap_or(20);
    let truncated = entries.len() > limit;
    entries.truncate(limit);
    RankResponse {
        status: if total == 0 {
            Status::Empty
        } else {
            Status::Complete
        },
        mode: planned.mode,
        pattern,
        entries,
        total,
        truncated,
        warnings: planned.warnings,
        error: None,
        files_parsed: per_file.len(),
        elapsed_ms: ms(t0),
    }
}

#[inline]
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Visit byte ranges of word tokens (`[\p{Alphanumeric}_]+`). Stops when `f`
/// returns false.
pub fn for_each_token(hay: &[u8], f: &mut dyn FnMut(usize, usize) -> bool) {
    let base = hay.as_ptr() as usize;
    let mut start: Option<usize> = None;
    for chunk in hay.utf8_chunks() {
        let v = chunk.valid();
        let off = v.as_ptr() as usize - base;
        for (i, c) in v.char_indices() {
            let w = if c.is_ascii() {
                c.is_ascii_alphanumeric() || c == '_'
            } else {
                is_word_char(c)
            };
            match (w, start) {
                (true, None) => start = Some(off + i),
                (false, Some(s)) => {
                    start = None;
                    if !f(s, off + i) {
                        return;
                    }
                }
                _ => {}
            }
        }
        if !chunk.invalid().is_empty() {
            if let Some(s) = start.take() {
                if !f(s, off + v.len()) {
                    return;
                }
            }
        }
    }
    if let Some(s) = start {
        f(s, hay.len());
    }
}
