//! `lancet rg`: ripgrep's command line, semantics and output on lancet's engine.
//!
//! Same regex engine (Rust `regex`), same ignore rules (`ignore` crate, including
//! `.rgignore`), same `-g`/`-t` machinery, same exit codes. On top of that: the
//! index narrows candidate files (always fresh, as in auto mode), documents and
//! archives are searched through lancet's extractors, and output is sorted by path
//! so it is deterministic.

use crate::content;
use crate::index::{self, Req};
use crate::output::Stats;
use crate::query::{Expr, Matcher};
use crate::scope::{plan_targets_with_req, IndexMode, Target};
use crate::walk::{walk, FileFilter, WalkOpts};
use rayon::prelude::*;
use regex::bytes::{Captures, Regex, RegexBuilder};
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Sensitive,
    Insensitive,
    /// Insensitive unless a pattern contains an uppercase letter (`-S`).
    Smart,
}

#[derive(Clone, Debug)]
pub struct RgOptions {
    pub patterns: Vec<String>,
    /// Paths to search; empty means the current directory.
    pub paths: Vec<PathBuf>,
    pub fixed_strings: bool,
    pub case: Case,
    pub word: bool,
    pub line_regexp: bool,
    pub invert: bool,
    pub multiline: bool,
    pub multiline_dotall: bool,
    pub only_matching: bool,
    pub count: bool,
    pub count_matches: bool,
    pub files_with_matches: bool,
    pub files_without_match: bool,
    pub quiet: bool,
    pub max_count: Option<u64>,
    pub before: usize,
    pub after: usize,
    pub line_number: bool,
    pub with_filename: Option<bool>,
    pub column: bool,
    pub vimgrep: bool,
    pub json: bool,
    pub null: bool,
    pub color: bool,
    pub replace: Option<String>,
    /// Search binary files as text (`-a`, `-uuu`).
    pub text: bool,
    /// Search inside PDFs, Office files, HWP and archives (lancet's extractors).
    /// Off by default so output matches ripgrep, which treats them as binary.
    pub docs: bool,
    pub globs: Vec<String>,
    pub iglobs: Vec<String>,
    pub types: Vec<String>,
    pub types_not: Vec<String>,
    /// `--type-add NAME:GLOB` definitions, applied before `-t`/`-T`.
    pub type_add: Vec<String>,
    /// `--passthru`: print every line of every searched file (callers also set
    /// `before`/`after` to cover the whole file).
    pub passthru: bool,
    /// `-M`: longer lines are omitted (or cut, with `max_columns_preview`).
    pub max_columns: Option<usize>,
    pub max_columns_preview: bool,
    /// `-b`: byte offset of the line (of the match with `-o`) before the text.
    pub byte_offset: bool,
    /// `--trim`: leading whitespace removed from printed lines.
    pub trim: bool,
    /// `--heading`: the file name on its own line above that file's lines.
    pub heading: bool,
    /// Printed between context groups; `None` = `--no-context-separator`.
    pub context_separator: Option<Vec<u8>>,
    pub field_match_separator: Vec<u8>,
    pub field_context_separator: Vec<u8>,
    /// `--path-separator`: printed in place of `/` in paths.
    pub path_separator: Option<String>,
    /// `--include-zero`: `-c` also prints files with no match.
    pub include_zero: bool,
    /// `--stats`: totals after the results.
    pub stats: bool,
    /// `--sort` / `--sortr`.
    pub sort: Option<(SortBy, bool)>,
    pub glob_case_insensitive: bool,
    pub type_clear: Vec<String>,
    /// `--stop-on-nonmatch`: a file's search ends at the first non-matching line after a match.
    pub stop_on_nonmatch: bool,
    /// `--no-messages`: errors about files and paths are not printed (still exit 2).
    pub no_messages: bool,
    /// `-E`: `None`/`auto` sniffs a UTF-8/UTF-16 BOM; `none` searches raw bytes.
    pub encoding: Option<String>,
    /// `--crlf`: `$` matches before `\r\n`.
    pub crlf: bool,
    /// `--null-data`: lines end at NUL instead of `\n`.
    pub null_data: bool,
    /// `--binary`: binary files in directory searches are searched as named ones are.
    pub binary: bool,
    /// `-z`: search inside `.gz` files.
    pub search_zip: bool,
    /// `--no-unicode`: ASCII-only `\w`, `\d`, `\b` and case folding.
    pub no_unicode: bool,
    /// `--max-bytes`: search at most this many bytes of each file.
    pub max_bytes: Option<u64>,
    pub walk: WalkOpts,
    pub index: IndexMode,
    pub index_dir: Option<PathBuf>,
}

impl RgOptions {
    pub fn new(patterns: Vec<String>) -> Self {
        RgOptions {
            patterns,
            paths: Vec::new(),
            fixed_strings: false,
            case: Case::Sensitive,
            word: false,
            line_regexp: false,
            invert: false,
            multiline: false,
            multiline_dotall: false,
            only_matching: false,
            count: false,
            count_matches: false,
            files_with_matches: false,
            files_without_match: false,
            quiet: false,
            max_count: None,
            before: 0,
            after: 0,
            line_number: true,
            with_filename: None,
            column: false,
            vimgrep: false,
            json: false,
            null: false,
            color: false,
            replace: None,
            text: false,
            docs: false,
            globs: Vec::new(),
            iglobs: Vec::new(),
            types: Vec::new(),
            types_not: Vec::new(),
            type_add: Vec::new(),
            passthru: false,
            max_columns: None,
            max_columns_preview: false,
            byte_offset: false,
            trim: false,
            heading: false,
            context_separator: Some(b"--".to_vec()),
            field_match_separator: b":".to_vec(),
            field_context_separator: b"-".to_vec(),
            path_separator: None,
            include_zero: false,
            stats: false,
            sort: None,
            glob_case_insensitive: false,
            type_clear: Vec::new(),
            stop_on_nonmatch: false,
            no_messages: false,
            encoding: None,
            crlf: false,
            null_data: false,
            binary: false,
            search_zip: false,
            no_unicode: false,
            max_bytes: None,
            walk: WalkOpts::default(),
            index: IndexMode::Auto,
            index_dir: None,
        }
    }
}

/// `--sort` / `--sortr` keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortBy {
    Path,
    Modified,
    Accessed,
    Created,
    None,
}

impl SortBy {
    pub fn parse(s: &str) -> Result<SortBy, String> {
        Ok(match s {
            "path" => SortBy::Path,
            "modified" => SortBy::Modified,
            "accessed" => SortBy::Accessed,
            "created" => SortBy::Created,
            "none" => SortBy::None,
            _ => {
                return Err(format!(
                    "invalid sort key '{s}' (path, modified, accessed, created, none)"
                ))
            }
        })
    }
}

/// What ripgrep's default decoder does: a UTF-8 BOM is dropped, a UTF-16 one
/// transcodes the file; `-E LABEL` transcodes from LABEL; `-E none` keeps raw bytes.
/// Only a file that needs it is copied; the rest stay memory-mapped.
fn decode(o: &RgOptions, raw: content::Content) -> content::Content {
    use content::Content::Owned;
    if o.encoding.as_deref() == Some("none") {
        return raw;
    }
    if let Some((enc, bom)) = encoding_rs::Encoding::for_bom(&raw) {
        if enc == encoding_rs::UTF_8 {
            return Owned(raw[bom..].to_vec());
        }
        return Owned(
            enc.decode_without_bom_handling(&raw[bom..])
                .0
                .into_owned()
                .into_bytes(),
        );
    }
    match o.encoding.as_deref().filter(|l| *l != "auto") {
        Some(label) => match encoding_rs::Encoding::for_label(label.as_bytes()) {
            Some(enc) if enc != encoding_rs::UTF_8 => Owned(
                enc.decode_without_bom_handling(&raw)
                    .0
                    .into_owned()
                    .into_bytes(),
            ),
            _ => raw,
        },
        None => raw,
    }
}

/// Parse ripgrep-style `--max-filesize` (`NUM[K|M|G]` or `KB`/`KiB` etc., base 1024).
pub fn parse_filesize(s: &str) -> Result<u64, String> {
    let raw = s.trim();
    if raw.is_empty() {
        return Err("empty filesize".into());
    }
    let split = raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len());
    let (num_str, suf) = raw.split_at(split);
    if num_str.is_empty() {
        return Err(format!("invalid filesize '{s}'"));
    }
    let n: u64 = num_str
        .parse()
        .map_err(|_| format!("invalid filesize '{s}'"))?;
    let mult: u64 = match suf.to_ascii_uppercase().as_str() {
        "" | "B" => 1,
        "K" | "KB" | "KIB" => 1024,
        "M" | "MB" | "MIB" => 1024 * 1024,
        "G" | "GB" | "GIB" => 1024 * 1024 * 1024,
        _ => return Err(format!("unknown filesize suffix in '{s}' (use K, M, or G)")),
    };
    n.checked_mul(mult)
        .ok_or_else(|| format!("filesize '{s}' overflows u64"))
}

/// ripgrep's smart case: does the pattern contain an uppercase *literal*? Parsed
/// like ripgrep does, so classes such as `\W`, `\S` or `\P{Greek}` don't count.
fn has_uppercase_literal(p: &str) -> bool {
    use regex_syntax::ast::{Ast, ClassSet, ClassSetItem};
    fn item(i: &ClassSetItem) -> bool {
        match i {
            ClassSetItem::Literal(l) => l.c.is_uppercase(),
            ClassSetItem::Range(r) => r.start.c.is_uppercase() || r.end.c.is_uppercase(),
            ClassSetItem::Bracketed(b) => set(&b.kind),
            ClassSetItem::Union(u) => u.items.iter().any(item),
            _ => false,
        }
    }
    fn set(s: &ClassSet) -> bool {
        match s {
            ClassSet::Item(i) => item(i),
            ClassSet::BinaryOp(op) => set(&op.lhs) || set(&op.rhs),
        }
    }
    fn ast(a: &Ast) -> bool {
        match a {
            Ast::Literal(l) => l.c.is_uppercase(),
            Ast::ClassBracketed(b) => set(&b.kind),
            Ast::Repetition(r) => ast(&r.ast),
            Ast::Group(g) => ast(&g.ast),
            Ast::Alternation(x) => x.asts.iter().any(ast),
            Ast::Concat(x) => x.asts.iter().any(ast),
            _ => false,
        }
    }
    match regex_syntax::ast::parse::Parser::new().parse(p) {
        Ok(a) => ast(&a),
        Err(_) => p.chars().any(char::is_uppercase),
    }
}

struct Engine {
    /// Line-at-a-time (or whole-buffer with `-U`) matcher.
    re: Regex,
    /// Whole-buffer prefilter: if it finds nothing, no line can match.
    buf_re: Regex,
    /// `-w`: the real match is capture group 1 (boundaries are matched around it).
    word: bool,
    /// Replacement with group numbers shifted for the `-w` wrapper group.
    replace: Option<Vec<u8>>,
    /// `--crlf`: a line's trailing `\r` is not part of what is matched.
    crlf: bool,
}

impl Engine {
    fn new(o: &RgOptions) -> Result<(Engine, String), String> {
        if o.patterns.is_empty() {
            return Err("no pattern given".into());
        }
        let parts: Vec<String> = o
            .patterns
            .iter()
            .map(|p| {
                if o.fixed_strings {
                    regex::escape(p)
                } else {
                    p.clone()
                }
            })
            .collect();
        let mut pat = if parts.len() == 1 {
            parts[0].clone()
        } else {
            parts
                .iter()
                .map(|p| format!("(?:{p})"))
                .collect::<Vec<_>>()
                .join("|")
        };
        if o.line_regexp {
            pat = format!("^(?:{pat})$");
        } else if o.word {
            // ripgrep's word mode: boundaries may be non-word chars, so `-w '@x'` works.
            pat = format!(r"(?:^|\W)({pat})(?:$|\W)");
        }
        let insensitive = match o.case {
            Case::Sensitive => false,
            Case::Insensitive => true,
            Case::Smart => !o.patterns.iter().any(|p| {
                if o.fixed_strings {
                    p.chars().any(char::is_uppercase)
                } else {
                    has_uppercase_literal(p)
                }
            }),
        };
        let mut flags = String::new();
        if insensitive {
            flags.push('i');
        }
        if o.multiline {
            flags.push('m');
            if o.multiline_dotall {
                flags.push('s');
            }
        }
        let full = if flags.is_empty() {
            pat.clone()
        } else {
            format!("(?{flags}){pat}")
        };
        let build = |p: &str| {
            RegexBuilder::new(p)
                .size_limit(1 << 28)
                .crlf(o.crlf)
                .unicode(!o.no_unicode)
                .build()
                .map_err(|e| format!("regex parse error:\n{e}"))
        };
        let re = build(&full)?;
        let buf_re = if o.multiline {
            re.clone()
        } else {
            let m = if insensitive { "(?mi)" } else { "(?m)" };
            build(&format!("{m}{pat}"))?
        };
        let word = o.word && !o.line_regexp;
        let replace = o.replace.as_ref().map(|r| {
            if word {
                shift_group_refs(r).into_bytes()
            } else {
                r.clone().into_bytes()
            }
        });
        Ok((
            Engine {
                re,
                buf_re,
                word,
                replace,
                crlf: o.crlf,
            },
            full,
        ))
    }

    /// Match spans in `hay` (the user's match, without `-w` boundary characters).
    fn spans(&self, hay: &[u8], out: &mut Vec<(usize, usize)>) {
        out.clear();
        let hay = match hay.strip_suffix(b"\r") {
            Some(h) if self.crlf => h,
            _ => hay,
        };
        if !self.word {
            out.extend(self.re.find_iter(hay).map(|m| (m.start(), m.end())));
            return;
        }
        // Restart at the end of the inner group, not of the whole match, so a
        // boundary character can be shared by two adjacent matches.
        let mut at = 0;
        while at <= hay.len() {
            let Some(c) = self.re.captures_at(hay, at) else {
                break;
            };
            let g = c.get(1).expect("word wrapper group");
            out.push((g.start(), g.end()));
            at = if g.end() > at { g.end() } else { at + 1 };
        }
    }

    /// `text` with every match replaced by the expanded replacement.
    fn replaced(&self, text: &[u8], rep: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(text.len());
        let mut last = 0;
        let push = |c: &Captures<'_>, out: &mut Vec<u8>, last: &mut usize| {
            let g = c.get(if self.word { 1 } else { 0 }).unwrap();
            out.extend_from_slice(&text[*last..g.start()]);
            c.expand(rep, out);
            *last = g.end();
        };
        if self.word {
            let mut at = 0;
            while at <= text.len() {
                let Some(c) = self.re.captures_at(text, at) else {
                    break;
                };
                let end = c.get(1).unwrap().end();
                push(&c, &mut out, &mut last);
                at = if end > at { end } else { at + 1 };
            }
        } else {
            for c in self.re.captures_iter(text) {
                push(&c, &mut out, &mut last);
            }
        }
        out.extend_from_slice(&text[last..]);
        out
    }

    fn expand_one(&self, text: &[u8], span: (usize, usize), rep: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let group = if self.word { 1 } else { 0 };
        let mut at = 0;
        while at <= text.len() {
            let Some(c) = self.re.captures_at(text, at) else {
                break;
            };
            let g = c.get(group).unwrap();
            if (g.start(), g.end()) == span {
                c.expand(rep, &mut out);
                return out;
            }
            at = if g.end() > at { g.end() } else { at + 1 };
        }
        text[span.0..span.1].to_vec()
    }
}

/// `$N`/`${N}` -> `$(N+1)` so user groups still refer to their own groups when the
/// `-w` wrapper adds group 1 in front.
fn shift_group_refs(r: &str) -> String {
    let re = Regex::new(r"\$(\{)?(\d+)(\})?").unwrap();
    let out = re.replace_all(r.as_bytes(), |c: &Captures<'_>| {
        let n: usize = std::str::from_utf8(&c[2]).unwrap().parse().unwrap_or(0);
        match (c.get(1), c.get(3)) {
            (Some(_), Some(_)) => format!("${{{}}}", n + 1).into_bytes(),
            _ => format!("${{{}}}", n + 1).into_bytes(),
        }
    });
    String::from_utf8_lossy(&out).into_owned()
}

/// One searched file's printed output and counts.
struct FileOut {
    /// Matches (not lines) in the file, for `--stats`.
    matches: u64,
    /// Bytes searched, for `--stats`.
    bytes: u64,
    display: String,
    out: Vec<u8>,
    selected: u64,
    has_output_group: bool,
}

struct Ctx<'a> {
    o: &'a RgOptions,
    eng: &'a Engine,
    show_name: bool,
}

/// Search one file and render its output.
/// `explicit`: named on the command line (searched even if binary, like ripgrep).
/// The first `--max-bytes` bytes of a file, as ripgrep reads them.
fn head(o: &RgOptions, c: content::Content) -> content::Content {
    match o.max_bytes {
        Some(n) if (c.len() as u64) > n => content::Content::Owned(c[..n as usize].to_vec()),
        _ => c,
    }
}

fn search_file(cx: &Ctx<'_>, path: &Path, display: String, explicit: bool) -> Option<FileOut> {
    let o = cx.o;
    let gz = o.search_zip
        && path.extension().is_some_and(|e| e == "gz")
        && !path.to_string_lossy().ends_with(".tar.gz");
    let mut binary_offset = None;
    let data = if (o.docs && crate::adapters::is_adapted_path(path, false)) || gz {
        // --docs: the text lancet's extractors see (PDF, Office, HWP, archives);
        // -z: the decompressed bytes of a .gz.
        head(o, content::load_searchable(path, false).ok()??)
    } else {
        // Raw bytes, as ripgrep sees them — PDFs and Office files included —
        // after ripgrep's BOM sniffing / `-E` transcoding.
        let raw = decode(o, head(o, content::read(path).ok()?));
        // Binary files are skipped unless -a: in directory searches ripgrep treats a
        // NUL anywhere as binary (lancet's 8 KiB sniff alone would let late-NUL
        // protobufs through); a file named explicitly is judged by its first bytes.
        let binary = !o.text
            && !o.null_data
            && if explicit {
                content::is_binary(&raw)
            } else {
                memchr::memchr(0, &raw).is_some()
            };
        if binary && !(explicit || o.binary) {
            return None;
        }
        if binary {
            // Named (or `--binary`) binary files are searched with NULs as line
            // breaks, and text output becomes one "binary file matches" line.
            binary_offset = memchr::memchr(0, &raw);
            content::Content::Owned(
                raw.iter()
                    .map(|&b| if b == 0 { b'\n' } else { b })
                    .collect(),
            )
        } else {
            raw
        }
    };
    let data: &[u8] = &data;
    let display = match &o.path_separator {
        Some(sep) => display.replace('/', sep),
        None => display,
    };
    let mut fo = FileOut {
        matches: 0,
        bytes: data.len() as u64,
        display,
        out: Vec::new(),
        selected: 0,
        has_output_group: false,
    };

    // Prefilter: no match anywhere in the buffer means no matching line.
    let any = cx.eng.buf_re.is_match(data);
    if !any && !o.invert && !o.passthru {
        if o.files_without_match {
            emit_path(&mut fo.out, o, &fo.display);
        }
        if (o.count || o.count_matches) && o.include_zero && !o.quiet {
            emit_count(&mut fo, cx, 0);
        }
        return Some(fo);
    }

    let lines = line_table(data, if o.null_data { 0 } else { b'\n' });
    let mut sel: Vec<(usize, Vec<(usize, usize)>)> = Vec::new();
    let mut matches_total: u64 = 0;
    let limit = o.max_count.unwrap_or(u64::MAX);
    if o.multiline {
        let mut spans = Vec::new();
        cx.eng.spans(data, &mut spans);
        matches_total = spans.len() as u64;
        // Lines touched by a match (a zero-width match touches its own line).
        let mut hit = vec![false; lines.len()];
        for &(s, e) in spans.iter().filter(|_| !lines.is_empty()) {
            let first = line_of(&lines, s);
            let last = line_of(&lines, e.saturating_sub(1).max(s));
            for h in &mut hit[first..=last] {
                *h = true;
            }
        }
        for (i, &(ls, le)) in lines.iter().enumerate() {
            if hit[i] != o.invert {
                let per: Vec<(usize, usize)> = spans
                    .iter()
                    .filter(|&&(s, e)| s < le.max(ls + 1) && e.max(s + 1) > ls)
                    .map(|&(s, e)| (s.max(ls) - ls, e.min(le).max(s.max(ls)) - ls))
                    .collect();
                sel.push((i, if o.invert { Vec::new() } else { per }));
                if sel.len() as u64 >= limit {
                    break;
                }
            }
        }
    } else {
        // A match within a line is also a match of the whole buffer at the same
        // offset, so lines no buffer match touches cannot match: only the touched
        // lines are re-checked one by one (and under `-v` the rest are selected as is).
        let mut touched = vec![false; lines.len()];
        if any && !lines.is_empty() {
            for m in cx.eng.buf_re.find_iter(data) {
                let first = line_of(&lines, m.start());
                let last = line_of(&lines, m.end().saturating_sub(1).max(m.start()));
                for t in &mut touched[first..=last] {
                    *t = true;
                }
            }
        }
        let mut tmp = Vec::new();
        for (i, &(s, e)) in lines.iter().enumerate() {
            if touched[i] {
                cx.eng.spans(&data[s..e], &mut tmp);
            } else {
                tmp.clear();
            }
            if o.stop_on_nonmatch && !sel.is_empty() && tmp.is_empty() != o.invert {
                break;
            }
            if tmp.is_empty() == o.invert {
                matches_total += tmp.len() as u64;
                sel.push((i, if o.invert { Vec::new() } else { tmp.clone() }));
                if sel.len() as u64 >= limit {
                    break;
                }
            }
        }
    }
    fo.selected = sel.len() as u64;
    fo.matches = matches_total;

    if o.quiet {
        return Some(fo);
    }
    if o.files_with_matches || o.files_without_match {
        if (fo.selected > 0) == o.files_with_matches {
            emit_path(&mut fo.out, o, &fo.display);
        }
        return Some(fo);
    }
    if o.count || o.count_matches {
        if fo.selected > 0 || o.include_zero {
            let n = if o.count_matches && !o.invert {
                matches_total
            } else {
                fo.selected
            };
            emit_count(&mut fo, cx, n);
        }
        return Some(fo);
    }
    if sel.is_empty() && (!o.passthru || lines.is_empty()) {
        return Some(fo);
    }
    if o.json {
        render_json(
            &mut fo,
            o,
            cx.eng,
            data,
            &lines,
            &sel,
            matches_total,
            binary_offset,
        );
        return Some(fo);
    }
    if let Some(off) = binary_offset {
        if !sel.is_empty() {
            if cx.show_name {
                fo.out.extend_from_slice(fo.display.as_bytes());
                fo.out.extend_from_slice(b": ");
            }
            fo.out.extend_from_slice(
                format!("binary file matches (found \"\\0\" byte around offset {off})\n")
                    .as_bytes(),
            );
        }
        return Some(fo);
    }

    if sel.is_empty() {
        // --passthru: a file without matches is printed whole, as context.
        for c in 0..lines.len() {
            emit_text_line(&mut fo.out, cx, &fo.display, c, &lines, data, b'-', None);
        }
        fo.has_output_group = !lines.is_empty();
        return Some(fo);
    }

    // Text output with context groups.
    let mut printed_upto: Option<usize> = None; // last printed line index
    for (k, (i, spans)) in sel.iter().enumerate() {
        let start = i.saturating_sub(o.before);
        let context = o.before > 0 || o.after > 0;
        let from = match printed_upto {
            Some(p) if p + 1 >= start => p + 1,
            Some(_) => {
                if let (true, Some(sep)) = (context, &o.context_separator) {
                    fo.out.extend_from_slice(sep);
                    fo.out.push(b'\n');
                }
                start
            }
            None => start,
        };
        for c in from..*i {
            emit_text_line(&mut fo.out, cx, &fo.display, c, &lines, data, b'-', None);
        }
        let (ls, le) = lines[*i];
        let text = &data[ls..le];
        if o.vimgrep || o.only_matching {
            for &(s, e) in spans {
                if s == e && o.only_matching {
                    continue;
                }
                let col = Some(s + 1);
                let shown: Vec<u8> = if o.only_matching {
                    match &cx.eng.replace {
                        Some(r) => cx.eng.expand_one(text, (s, e), r),
                        None => text[s..e].to_vec(),
                    }
                } else {
                    render_replace(cx, text)
                };
                let hl_one = [(0, shown.len())];
                let hl = if o.only_matching {
                    Some(&hl_one[..])
                } else if o.replace.is_none() {
                    Some(spans.as_slice())
                } else {
                    None
                };
                emit(
                    &mut fo.out,
                    cx,
                    &fo.display,
                    *i + 1,
                    b':',
                    col.filter(|_| o.column || o.vimgrep),
                    if o.only_matching { ls + s } else { ls },
                    le < data.len(),
                    &shown,
                    hl,
                );
            }
            if spans.is_empty() && !o.only_matching {
                emit(
                    &mut fo.out,
                    cx,
                    &fo.display,
                    *i + 1,
                    b':',
                    None,
                    ls,
                    le < data.len(),
                    &render_replace(cx, text),
                    None,
                );
            }
        } else {
            let col = spans.first().map(|s| s.0 + 1).filter(|_| o.column);
            let hl = (!o.invert && o.replace.is_none()).then_some(spans.as_slice());
            emit(
                &mut fo.out,
                cx,
                &fo.display,
                *i + 1,
                b':',
                col,
                ls,
                le < data.len(),
                &render_replace(cx, text),
                hl,
            );
        }
        let mut last = *i;
        // After-context, stopping before the next selected line.
        let next_sel = sel.get(k + 1).map(|(n, _)| *n);
        let mut tmp = Vec::new();
        for c in (*i + 1)..=(*i + o.after).min(lines.len().saturating_sub(1)) {
            if Some(c) == next_sel {
                break;
            }
            // Past `-m`, ripgrep still marks matching lines in the trailing context as matches.
            let (cs, ce) = lines[c];
            cx.eng.spans(&data[cs..ce], &mut tmp);
            // …but not under --passthru, where everything past `-m` is plain context.
            let sep = if !o.invert && !tmp.is_empty() && !o.multiline && !o.passthru {
                b':'
            } else {
                b'-'
            };
            let col = tmp
                .first()
                .map(|s| s.0 + 1)
                .filter(|_| o.column && sep == b':');
            emit_text_line(&mut fo.out, cx, &fo.display, c, &lines, data, sep, col);
            last = c;
        }
        printed_upto = Some(last);
    }
    fo.has_output_group = true;
    Some(fo)
}

fn render_replace(cx: &Ctx<'_>, text: &[u8]) -> Vec<u8> {
    match &cx.eng.replace {
        Some(r) => cx.eng.replaced(text, r),
        None => text.to_vec(),
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_text_line(
    buf: &mut Vec<u8>,
    cx: &Ctx<'_>,
    display: &str,
    i: usize,
    lines: &[(usize, usize)],
    data: &[u8],
    sep: u8,
    col: Option<usize>,
) {
    let (s, e) = lines[i];
    emit(
        buf,
        cx,
        display,
        i + 1,
        sep,
        col,
        s,
        e < data.len(),
        &data[s..e],
        None,
    );
}

/// `path:count` (or `count` alone) for `-c` / `--count-matches`.
fn emit_count(fo: &mut FileOut, cx: &Ctx<'_>, n: u64) {
    let o = cx.o;
    if cx.show_name {
        if o.color && !o.null {
            fo.out.extend_from_slice(b"\x1b[35m");
            fo.out.extend_from_slice(fo.display.as_bytes());
            fo.out.extend_from_slice(b"\x1b[0m");
        } else {
            fo.out.extend_from_slice(fo.display.as_bytes());
        }
        if o.null {
            fo.out.push(0);
        } else {
            fo.out.extend_from_slice(&o.field_match_separator);
        }
    }
    fo.out
        .extend_from_slice(format!("{n}{}", if o.crlf { "\r\n" } else { "\n" }).as_bytes());
}

/// One output line. `sep` is `:` for a matching line and `-` for context (the
/// `--field-*-separator`s replace them); `off` is the byte offset for `-b`.
#[allow(clippy::too_many_arguments)]
fn emit(
    buf: &mut Vec<u8>,
    cx: &Ctx<'_>,
    display: &str,
    line_no: usize,
    sep: u8,
    col: Option<usize>,
    off: usize,
    eol: bool,
    text: &[u8],
    hl_spans: Option<&[(usize, usize)]>,
) {
    let o = cx.o;
    let use_color = o.color && !o.vimgrep && !o.null;
    let is_match = sep == b':';
    let field: &[u8] = if o.vimgrep {
        b":"
    } else if is_match {
        &o.field_match_separator
    } else {
        &o.field_context_separator
    };
    if cx.show_name || o.vimgrep {
        if use_color {
            buf.extend_from_slice(b"\x1b[35m");
            buf.extend_from_slice(display.as_bytes());
            buf.extend_from_slice(b"\x1b[0m");
        } else {
            buf.extend_from_slice(display.as_bytes());
        }
        if o.null {
            buf.push(0);
        } else {
            buf.extend_from_slice(field);
        }
    }
    if o.line_number || o.vimgrep {
        if use_color {
            buf.extend_from_slice(b"\x1b[32m");
            buf.extend_from_slice(line_no.to_string().as_bytes());
            buf.extend_from_slice(b"\x1b[0m");
        } else {
            buf.extend_from_slice(line_no.to_string().as_bytes());
        }
        buf.extend_from_slice(field);
    }
    if let Some(c) = col {
        buf.extend_from_slice(c.to_string().as_bytes());
        buf.extend_from_slice(field);
    }
    if o.byte_offset {
        buf.extend_from_slice(off.to_string().as_bytes());
        buf.extend_from_slice(field);
    }
    // A line keeps its own terminator; one that had none gets ripgrep's (CRLF with --crlf).
    let term: &[u8] = match (eol, o.null_data, o.crlf) {
        (_, true, _) => b"\0",
        (true, _, _) | (false, false, false) => b"\n",
        (false, false, true) => b"\r\n",
    };
    // --trim: leading whitespace goes, and highlight spans move with the text.
    let cut = if o.trim {
        text.iter().take_while(|b| b.is_ascii_whitespace()).count()
    } else {
        0
    };
    let text = &text[cut..];
    // ripgrep measures a line with its terminator.
    if let Some(max) = o.max_columns.filter(|&m| text.len() + eol as usize > m) {
        if o.max_columns_preview {
            let mut end = max;
            while end > 0 && end < text.len() && (text[end] & 0xc0) == 0x80 {
                end -= 1;
            }
            buf.extend_from_slice(&text[..end]);
            buf.extend_from_slice(b" [... omitted end of long line]");
        } else if is_match {
            buf.extend_from_slice(b"[Omitted long matching line]");
        } else {
            buf.extend_from_slice(b"[Omitted long context line]");
        }
        buf.extend_from_slice(term);
        return;
    }
    if use_color {
        if let Some(spans) = hl_spans.filter(|s| !s.is_empty()) {
            let mut pos = 0;
            for &(s, e) in spans {
                let s = s.saturating_sub(cut).min(text.len());
                let e = e.saturating_sub(cut).min(text.len());
                if s >= pos && e > s {
                    buf.extend_from_slice(&text[pos..s]);
                    buf.extend_from_slice(b"\x1b[1;31m");
                    buf.extend_from_slice(&text[s..e]);
                    buf.extend_from_slice(b"\x1b[0m");
                    pos = e;
                }
            }
            buf.extend_from_slice(&text[pos..]);
            buf.extend_from_slice(term);
            return;
        }
    }
    buf.extend_from_slice(text);
    buf.extend_from_slice(term);
}

fn emit_path(buf: &mut Vec<u8>, o: &RgOptions, display: &str) {
    if o.color && !o.null {
        buf.extend_from_slice(b"\x1b[35m");
        buf.extend_from_slice(display.as_bytes());
        buf.extend_from_slice(b"\x1b[0m");
    } else {
        buf.extend_from_slice(display.as_bytes());
    }
    buf.extend_from_slice(match (o.null, o.crlf) {
        (true, _) => b"\0",
        (false, true) => b"\r\n",
        (false, false) => b"\n",
    });
}

/// `(start, end)` of each line, `end` excluding the terminator (`\n`, or NUL with `--null-data`).
fn line_table(data: &[u8], term: u8) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut s = 0;
    for nl in memchr::memchr_iter(term, data) {
        v.push((s, nl));
        s = nl + 1;
    }
    if s < data.len() {
        v.push((s, data.len()));
    }
    v
}

fn line_of(lines: &[(usize, usize)], off: usize) -> usize {
    lines
        .partition_point(|&(s, _)| s <= off)
        .saturating_sub(1)
        .min(lines.len().saturating_sub(1))
}

#[allow(clippy::too_many_arguments)]
fn render_json(
    fo: &mut FileOut,
    o: &RgOptions,
    eng: &Engine,
    data: &[u8],
    lines: &[(usize, usize)],
    sel: &[(usize, Vec<(usize, usize)>)],
    matches_total: u64,
    binary_offset: Option<usize>,
) {
    let path = json!({ "text": fo.display });
    let w = |v: serde_json::Value, out: &mut Vec<u8>| {
        out.extend_from_slice(v.to_string().as_bytes());
        out.push(b'\n');
    };
    w(
        json!({"type": "begin", "data": {"path": path}}),
        &mut fo.out,
    );
    let mut emitted = std::collections::BTreeSet::new();
    let line_ev = |i: usize, kind: &str, subs: &[(usize, usize)]| {
        let (s, e) = lines[i];
        let text = &data[s..e];
        let mut t = String::from_utf8_lossy(text).into_owned();
        if e < data.len() {
            t.push('\n');
        }
        let submatches: Vec<_> = subs
            .iter()
            .map(|&(a, b)| {
                let m = match &eng.replace {
                    Some(r) => {
                        String::from_utf8_lossy(&eng.expand_one(text, (a, b), r)).into_owned()
                    }
                    None => String::from_utf8_lossy(&text[a..b]).into_owned(),
                };
                json!({"match": {"text": m}, "start": a, "end": b})
            })
            .collect();
        json!({"type": kind, "data": {
            "path": {"text": fo.display}, "lines": {"text": t},
            "line_number": i + 1, "absolute_offset": s, "submatches": submatches}})
    };
    let ctx_ev = |c: usize| {
        if o.invert {
            let (cs, ce) = lines[c];
            let mut sp = Vec::new();
            eng.spans(&data[cs..ce], &mut sp);
            line_ev(c, "context", &sp)
        } else {
            line_ev(c, "context", &[])
        }
    };
    let mut evs = Vec::new();
    if sel.is_empty() {
        // --passthru: every line of a file without matches is context.
        for c in 0..lines.len() {
            evs.push(ctx_ev(c));
        }
    }
    let mut next = 0; // lines before this were emitted already (keeps --passthru linear)
    for (k, (i, spans)) in sel.iter().enumerate() {
        for c in i.saturating_sub(o.before).max(next)..*i {
            if emitted.insert(c) {
                evs.push(ctx_ev(c));
            }
        }
        emitted.insert(*i);
        evs.push(line_ev(*i, "match", spans));
        next = *i + 1;
        let next_sel = sel.get(k + 1).map(|(n, _)| *n);
        let mut sp = Vec::new();
        let last = (*i + o.after).min(lines.len().saturating_sub(1));
        for (c, &(cs, ce)) in lines.iter().enumerate().take(last + 1).skip(*i + 1) {
            if Some(c) == next_sel {
                break;
            }
            if !emitted.insert(c) {
                continue;
            }
            // Past `-m`, ripgrep reports matching lines in the trailing context as
            // matches (as the text output does) — except under --passthru.
            if !o.invert && !o.multiline && !o.passthru {
                eng.spans(&data[cs..ce], &mut sp);
                if !sp.is_empty() {
                    evs.push(line_ev(c, "match", &sp));
                    continue;
                }
            }
            evs.push(ctx_ev(c));
        }
    }
    for e in evs {
        w(e, &mut fo.out);
    }
    w(
        json!({"type": "end", "data": {"path": path, "binary_offset": binary_offset, "stats": {
            "elapsed": {"secs": 0, "nanos": 0, "human": "0s"},
            "searches": 1, "searches_with_match": 1,
            "bytes_searched": data.len(), "bytes_printed": 0,
            "matched_lines": fo.selected, "matches": matches_total}}}),
        &mut fo.out,
    );
}

/// Every file `lancet rg` would search, as `(path, display)`, sorted by display.
/// `(path, display, explicitly named)` triples.
type TargetList = Vec<(PathBuf, String, bool)>;

/// Files to search, whether file names are shown, and the paths that don't exist
/// (reported, like ripgrep, without stopping the search of the others).
fn targets(o: &RgOptions, req: &Req) -> Result<(TargetList, bool, Vec<String>), String> {
    let roots: Vec<PathBuf> = if o.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        o.paths.clone()
    };
    let implicit = o.paths.is_empty();
    let mut out = Vec::new();
    let mut any_dir = false;
    let mut missing = Vec::new();
    // ripgrep keeps the order of the paths given and sorts within each one.
    let mut group_start = 0;
    let sort_group = |out: &mut TargetList, from: &mut usize| {
        out[*from..].sort_by(|a, b| a.1.split('/').cmp(b.1.split('/')));
        *from = out.len();
    };
    for root in &roots {
        sort_group(&mut out, &mut group_start);
        if root.is_file() {
            if o.walk
                .max_filesize
                .is_some_and(|max| root.metadata().is_ok_and(|m| m.len() > max))
            {
                continue;
            }
            // Explicitly named files are searched as given, like ripgrep.
            out.push((root.clone(), root.to_string_lossy().into_owned(), true));
            continue;
        }
        if !root.is_dir() {
            missing.push(format!(
                "{}: No such file or directory (os error 2)",
                root.display()
            ));
            continue;
        }
        any_dir = true;
        let (globs, iglobs) = if o.glob_case_insensitive {
            (Vec::new(), [o.globs.clone(), o.iglobs.clone()].concat())
        } else {
            (o.globs.clone(), o.iglobs.clone())
        };
        let filter = FileFilter::rg(
            root,
            &globs,
            &iglobs,
            &o.type_clear,
            &o.type_add,
            &o.types,
            &o.types_not,
        )?;
        // `.lancet` is built without following symlinks, so `--trust-index -L`
        // falls back to a live walk rather than missing symlinked targets.
        // The index follows the default ignore rules; other rules need a live walk.
        let mode = if o.text
            || (o.walk.follow && o.index == IndexMode::Trust)
            || !o.walk.ignore.is_default()
            || o.walk.one_file_system
        {
            IndexMode::Off
        } else {
            o.index
        };
        let mut planned: Option<Vec<Target>> = None;
        if mode != IndexMode::Off {
            match index::discover(root, o.index_dir.as_deref()) {
                Ok(Some((ix, prefix))) => {
                    let mut stats = Stats::default();
                    planned = Some(plan_targets_with_req(
                        &ix, req, &filter, root, &prefix, &o.walk, mode, None, &mut stats, None,
                    ));
                }
                Ok(None) => {}
                Err(e) => eprintln!("lancet rg: index unusable ({e:#}); searching without it"),
            }
        }
        let ts = planned.unwrap_or_else(|| {
            walk(root, &o.walk, &filter, false)
                .into_iter()
                .map(|e| Target {
                    path: e.path,
                    rel: e.rel,
                })
                .collect()
        });
        let base = root.to_string_lossy();
        let base = base.trim_end_matches('/');
        for t in ts {
            let display = if implicit {
                t.rel.clone()
            } else {
                format!("{base}/{}", t.rel)
            };
            out.push((t.path, display, false));
        }
    }
    // Component-wise within each path, like ripgrep's `--sort path` (`a/b.txt` before `a.txt`).
    sort_group(&mut out, &mut group_start);
    let mut seen = std::collections::HashSet::new();
    out.retain(|t| seen.insert(t.1.clone()));
    if let Some((by, reverse)) = o.sort {
        let time = |p: &Path| {
            std::fs::metadata(p).ok().and_then(|m| match by {
                SortBy::Modified => m.modified().ok(),
                SortBy::Accessed => m.accessed().ok(),
                SortBy::Created => m.created().ok(),
                SortBy::Path | SortBy::None => None,
            })
        };
        if matches!(by, SortBy::Modified | SortBy::Accessed | SortBy::Created) {
            // Stable: equal times keep path order.
            out.sort_by_cached_key(|t| time(&t.0));
        }
        if reverse {
            out.reverse();
        }
    }
    let show_name = roots.len() > 1 || any_dir;
    Ok((out, show_name, missing))
}

fn report_missing(o: &RgOptions, missing: &[String]) {
    if !o.no_messages {
        for m in missing {
            eprintln!("rg: {m}");
        }
    }
}

/// `lancet rg --files`: the files that would be searched.
/// `Err("")` means errors were already reported on stderr (exit 2).
pub fn list_files(o: &RgOptions, w: &mut dyn Write) -> Result<bool, String> {
    let (ts, _, missing) = targets(o, &Req::All)?;
    report_missing(o, &missing);
    for (_, d, _) in &ts {
        let d = match &o.path_separator {
            Some(sep) => d.replace('/', sep),
            None => d.clone(),
        };
        let _ = w.write_all(d.as_bytes());
        let _ = w.write_all(if o.null { b"\0" } else { b"\n" });
    }
    if !missing.is_empty() {
        return Err(String::new());
    }
    Ok(!ts.is_empty())
}

/// Run a search; `Ok(true)` if anything matched (exit 0), `Ok(false)` if not (exit 1).
/// `Err("")` means errors were already reported on stderr (exit 2).
pub fn run(o: &RgOptions, w: &mut dyn Write) -> Result<bool, String> {
    let t0 = std::time::Instant::now();
    let (eng, full) = Engine::new(o)?;
    // Inverted / files-without-match searches need every file; otherwise the regex's
    // required trigrams narrow the candidates (verified by the regex itself).
    // --stats and --include-zero report every file ripgrep would search, so the
    // index doesn't narrow them.
    let req = if o.invert || o.files_without_match || o.passthru || o.stats || o.include_zero {
        Req::All
    } else {
        Req::from_expr(&Expr::Match {
            m: Matcher::Regex { pattern: full },
        })
    };
    let (ts, dir_search, missing) = targets(o, &req)?;
    report_missing(o, &missing);
    let show_name = o.with_filename.unwrap_or(dir_search);
    // --heading: names go on their own line above each file's lines.
    let heading = o.heading
        && show_name
        && !(o.count || o.count_matches || o.files_with_matches || o.files_without_match)
        && !(o.json || o.quiet || o.vimgrep);
    let cx = Ctx {
        o,
        eng: &eng,
        show_name: show_name && !heading,
    };
    let results: Vec<FileOut> = ts
        .par_iter()
        .filter_map(|(p, d, explicit)| search_file(&cx, p, d.clone(), *explicit))
        .collect();
    let searched_at = t0.elapsed();
    let mut any = false;
    let mut first_group = true;
    let (mut matched_lines, mut searches_with_match) = (0u64, 0u64);
    let mut printed = 0u64;
    let mut w = CountingWriter {
        inner: w,
        n: &mut printed,
    };
    let w = &mut w;
    for r in &results {
        if r.selected > 0 {
            any = true;
            searches_with_match += 1;
            matched_lines += r.selected;
        }
        if o.files_without_match && r.selected == 0 {
            any = true;
        }
        if r.out.is_empty() {
            continue;
        }
        if heading {
            if !first_group {
                let _ = w.write_all(b"\n");
            }
            first_group = false;
            if o.color {
                let _ = w.write_all(format!("\x1b[35m{}\x1b[0m\n", r.display).as_bytes());
            } else {
                let _ = w.write_all(format!("{}\n", r.display).as_bytes());
            }
        } else if r.has_output_group && (o.before > 0 || o.after > 0) && !o.json && !o.passthru {
            // --passthru prints whole files, which ripgrep never separates with `--`.
            if let (false, Some(sep)) = (first_group, &o.context_separator) {
                let _ = w.write_all(sep);
                let _ = w.write_all(b"\n");
            }
            first_group = false;
        }
        let _ = w.write_all(&r.out);
    }
    if o.json && !o.quiet {
        let summary = json!({"type": "summary", "data": {
            "elapsed_total": {"secs": 0, "nanos": 0, "human": "0s"},
            "stats": {"searches": results.len(), "searches_with_match": searches_with_match,
                      "matched_lines": matched_lines}}});
        let _ = writeln!(w, "{summary}");
    }
    if o.stats && !o.json {
        let matches: u64 = results.iter().map(|r| r.matches).sum();
        // Decoded bytes, as ripgrep counts them. (For binary files ripgrep counts
        // only what it read before stopping, which this doesn't reproduce.)
        let bytes: u64 = results.iter().map(|r| r.bytes).sum();
        // ripgrep's "bytes printed" counts matching/context lines, not -c / -l output.
        let printed = if o.count || o.count_matches || o.files_with_matches || o.files_without_match
        {
            0
        } else {
            *w.n
        };
        let total = t0.elapsed();
        let text = format!(
            "\n{matches} matches\n{matched_lines} matched lines\n{searches_with_match} files contained matches\n{} files searched\n{} bytes printed\n{bytes} bytes searched\n{:.6} seconds spent searching\n{:.6} seconds total\n",
            ts.len(),
            printed,
            searched_at.as_secs_f64(),
            total.as_secs_f64()
        );
        let _ = w.write_all(text.as_bytes());
    }
    if !missing.is_empty() {
        return Err(String::new());
    }
    Ok(any)
}

/// Counts what is written, for `--stats`' "bytes printed".
struct CountingWriter<'a> {
    inner: &'a mut dyn Write,
    n: &'a mut u64,
}

impl Write for CountingWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let k = self.inner.write(buf)?;
        *self.n += k as u64;
        Ok(k)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_case_ignores_escapes() {
        assert!(!has_uppercase_literal(r"\W+foo\S"));
        assert!(has_uppercase_literal("Foo"));
        assert!(!has_uppercase_literal(r"\P{Greek}x"));
        assert!(has_uppercase_literal("[A-Z]x"));
        assert!(!has_uppercase_literal(r"(?i)\w+\b"));
    }

    #[test]
    fn word_mode_shares_boundaries_and_allows_symbol_edges() {
        let mut o = RgOptions::new(vec!["foo".into()]);
        o.word = true;
        let (e, _) = Engine::new(&o).unwrap();
        let mut v = Vec::new();
        e.spans(b"foo foo,foo foobar", &mut v);
        assert_eq!(v, vec![(0, 3), (4, 7), (8, 11)]);
        let mut o = RgOptions::new(vec!["@x".into()]);
        o.word = true;
        let (e, _) = Engine::new(&o).unwrap();
        e.spans(b"a @x b", &mut v);
        assert_eq!(v, vec![(2, 4)]);
    }

    #[test]
    fn replace_shifts_groups_under_word_mode() {
        assert_eq!(shift_group_refs("<$1|${2}|$0>"), "<${2}|${3}|${1}>");
    }

    #[test]
    fn parse_filesize_supports_suffixes_and_rejects_invalid() {
        assert_eq!(parse_filesize("100").unwrap(), 100);
        assert_eq!(parse_filesize("1K").unwrap(), 1024);
        assert_eq!(parse_filesize("2m").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_filesize("1GiB").unwrap(), 1024 * 1024 * 1024);
        assert!(parse_filesize("").is_err());
        assert!(parse_filesize("10X").is_err());
        assert!(parse_filesize("abc").is_err());
    }
}
