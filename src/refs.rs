//! Lightweight index-accelerated symbol definition + lexical AST reference finder (`lancet refs`).
//!
//! Finds `[def]` declarations and `[ref]` identifier occurrences across codebases while
//! automatically excluding comments and string/char literals on AST-supported languages.

use crate::code;
use crate::index::{self, Req};
use crate::matcher::for_each_token;
use crate::output::{ErrorInfo, Stats, Status};
use crate::query::{Expr, FileType, Filters, Matcher};
use crate::search::{plan_targets_with_req, IndexMode, Target};
use crate::walk::{walk, FileFilter, WalkOpts};
use memchr::memmem;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct RefsOptions {
    pub symbol: String,
    pub root: PathBuf,
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
            root: root.into(),
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

        if let Some(id_spans) = code::find_identifier_spans(&target.path, raw_bytes, symbol) {
            for (s, e) in id_spans {
                if def_spans.contains(&(s, e)) {
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
                refs.push(RefHit {
                    path: target.rel.clone(),
                    line: ln,
                    col,
                    kind: "ref".into(),
                    enclosing,
                    text: line_snippet(ln),
                    ast_verified: true,
                });
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
    })
}

pub fn find_refs(opts: &RefsOptions) -> RefsResponse {
    let mut r = find_refs_raw(opts);
    for h in r.definitions.iter_mut().chain(r.references.iter_mut()) {
        h.path = crate::walk::display_path(&opts.root, &h.path);
    }
    r
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

    let mut filters = Filters::default();
    if let Some(lang_s) = &opts.lang {
        #[cfg(feature = "ast")]
        {
            match crate::code::Lang::parse(lang_s) {
                Some(l) => {
                    filters.exts = l.extensions().iter().map(|s| (*s).to_string()).collect();
                }
                None => {
                    return RefsResponse::failure(
                        Status::InvalidQuery,
                        sym.to_string(),
                        format!("unknown --lang `{lang_s}`"),
                        ms(t0),
                    );
                }
            }
        }
        #[cfg(not(feature = "ast"))]
        {
            let _ = lang_s;
            return RefsResponse::failure(
                Status::Unsupported,
                sym.to_string(),
                "`--lang` requires building with `--features ast`".into(),
                ms(t0),
            );
        }
    } else if !opts.exts.is_empty() {
        filters.exts = opts
            .exts
            .iter()
            .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
            .filter(|e| !e.is_empty())
            .collect();
    } else if !opts.all_files && opts.root.is_dir() {
        filters.types = vec![FileType::Code];
    }
    filters.paths = opts.paths.clone();

    let filter = match FileFilter::new(&filters) {
        Ok(f) => f,
        Err(e) => return RefsResponse::failure(Status::InvalidQuery, sym.to_string(), e, ms(t0)),
    };

    let req = Req::from_expr(&Expr::Match {
        m: Matcher::Lit {
            text: sym.to_string(),
        },
    });

    let mut warnings = Vec::new();
    let mut mode = "scan".to_string();
    let mut core_stats = Stats::default();
    let mut targets: Option<Vec<Target>> = None;

    if opts.index != IndexMode::Off {
        match index::discover(&opts.root, opts.index_dir.as_deref()) {
            Ok(Some((ix, rel_prefix))) => {
                mode = if opts.index == IndexMode::Trust {
                    "index_trusted".into()
                } else {
                    "index".into()
                };
                targets = Some(plan_targets_with_req(
                    &ix,
                    &req,
                    &filter,
                    &opts.root,
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
        walk(&opts.root, &opts.walk, &filter, false)
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

    let finder = memmem::Finder::new(sym.as_bytes());
    let per_file: Vec<Option<FileRefs>> = targets
        .par_iter()
        .map(|t| scan_one_file(t, sym, &finder))
        .collect();

    let mut files_scanned = 0usize;
    let mut definitions = Vec::new();
    let mut references = Vec::new();
    for item in per_file.into_iter().flatten() {
        files_scanned += item.scanned as usize;
        definitions.extend(item.defs);
        references.extend(item.refs);
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
        return writeln!(w, "error: {}", err.message);
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
