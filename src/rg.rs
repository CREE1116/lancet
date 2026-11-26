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
use crate::search::{plan_targets_with_req, IndexMode, Target};
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
    pub replace: Option<String>,
    /// Search binary files as text (`-a`, `-uuu`).
    pub text: bool,
    pub globs: Vec<String>,
    pub iglobs: Vec<String>,
    pub types: Vec<String>,
    pub types_not: Vec<String>,
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
            replace: None,
            text: false,
            globs: Vec::new(),
            iglobs: Vec::new(),
            types: Vec::new(),
            types_not: Vec::new(),
            walk: WalkOpts::default(),
            index: IndexMode::Auto,
            index_dir: None,
        }
    }
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
            },
            full,
        ))
    }

    /// Match spans in `hay` (the user's match, without `-w` boundary characters).
    fn spans(&self, hay: &[u8], out: &mut Vec<(usize, usize)>) {
        out.clear();
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
fn search_file(cx: &Ctx<'_>, path: &Path, display: String, explicit: bool) -> Option<FileOut> {
    let o = cx.o;
    let data = match content::load_searchable(path, false) {
        Ok(Some(d)) => d,
        Ok(None) if o.text => content::read(path).ok()?,
        _ => return None,
    };
    let data: &[u8] = &data;
    // In directory searches ripgrep treats a NUL byte anywhere as binary and skips the
    // file; lancet's 8 KiB sniff alone would let late-NUL files (protobufs…) through.
    if !explicit
        && !o.text
        && !crate::adapters::is_adapted_path(path, false)
        && memchr::memchr(0, data).is_some()
    {
        return None;
    }
    let mut fo = FileOut {
        display,
        out: Vec::new(),
        selected: 0,
        has_output_group: false,
    };

    // Prefilter: no match anywhere in the buffer means no matching line.
    let any = cx.eng.buf_re.is_match(data);
    if !any && !o.invert {
        if o.files_without_match {
            emit_path(&mut fo.out, o, &fo.display);
        }
        return Some(fo);
    }

    let lines = line_table(data);
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
        if fo.selected > 0 {
            let n = if o.count_matches && !o.invert {
                matches_total
            } else {
                fo.selected
            };
            if cx.show_name {
                fo.out.extend_from_slice(fo.display.as_bytes());
                fo.out.push(if o.null { 0 } else { b':' });
            }
            fo.out.extend_from_slice(format!("{n}\n").as_bytes());
        }
        return Some(fo);
    }
    if sel.is_empty() {
        return Some(fo);
    }
    if o.json {
        render_json(&mut fo, o, cx.eng, data, &lines, &sel, matches_total);
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
                if context {
                    fo.out.extend_from_slice(b"--\n");
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
                emit(
                    &mut fo.out,
                    cx,
                    &fo.display,
                    *i + 1,
                    b':',
                    col.filter(|_| o.column || o.vimgrep),
                    &shown,
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
                    &render_replace(cx, text),
                );
            }
        } else {
            let col = spans.first().map(|s| s.0 + 1).filter(|_| o.column);
            emit(
                &mut fo.out,
                cx,
                &fo.display,
                *i + 1,
                b':',
                col,
                &render_replace(cx, text),
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
            let sep = if !o.invert && !tmp.is_empty() && !o.multiline {
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
    emit(buf, cx, display, i + 1, sep, col, &data[s..e]);
}

fn emit(
    buf: &mut Vec<u8>,
    cx: &Ctx<'_>,
    display: &str,
    line_no: usize,
    sep: u8,
    col: Option<usize>,
    text: &[u8],
) {
    let o = cx.o;
    if cx.show_name || o.vimgrep {
        buf.extend_from_slice(display.as_bytes());
        buf.push(if o.null { 0 } else { sep });
    }
    if o.line_number || o.vimgrep {
        buf.extend_from_slice(line_no.to_string().as_bytes());
        buf.push(sep);
    }
    if let Some(c) = col {
        buf.extend_from_slice(c.to_string().as_bytes());
        buf.push(sep);
    }
    buf.extend_from_slice(text);
    buf.push(b'\n');
}

fn emit_path(buf: &mut Vec<u8>, o: &RgOptions, display: &str) {
    buf.extend_from_slice(display.as_bytes());
    buf.push(if o.null { 0 } else { b'\n' });
}

/// `(start, end)` of each line, `end` excluding the `\n`.
fn line_table(data: &[u8]) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut s = 0;
    for nl in memchr::memchr_iter(b'\n', data) {
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

fn render_json(
    fo: &mut FileOut,
    o: &RgOptions,
    eng: &Engine,
    data: &[u8],
    lines: &[(usize, usize)],
    sel: &[(usize, Vec<(usize, usize)>)],
    matches_total: u64,
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
    let mut evs = Vec::new();
    for (i, spans) in sel {
        for c in i.saturating_sub(o.before)..*i {
            if emitted.insert(c) {
                evs.push(line_ev(c, "context", &[]));
            }
        }
        emitted.insert(*i);
        evs.push(line_ev(*i, "match", spans));
        for c in (*i + 1)..=(*i + o.after).min(lines.len().saturating_sub(1)) {
            if sel.iter().any(|(n, _)| *n == c) {
                break;
            }
            if emitted.insert(c) {
                evs.push(line_ev(c, "context", &[]));
            }
        }
    }
    for e in evs {
        w(e, &mut fo.out);
    }
    w(
        json!({"type": "end", "data": {"path": path, "binary_offset": null, "stats": {
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

fn targets(o: &RgOptions, req: &Req) -> Result<(TargetList, bool), String> {
    let roots: Vec<PathBuf> = if o.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        o.paths.clone()
    };
    let implicit = o.paths.is_empty();
    let mut out = Vec::new();
    let mut any_dir = false;
    for root in &roots {
        if root.is_file() {
            // Explicitly named files are searched as given, like ripgrep.
            out.push((root.clone(), root.to_string_lossy().into_owned(), true));
            continue;
        }
        if !root.is_dir() {
            return Err(format!(
                "{}: No such file or directory (os error 2)",
                root.display()
            ));
        }
        any_dir = true;
        let filter = FileFilter::rg(root, &o.globs, &o.iglobs, &o.types, &o.types_not)?;
        let mode = if o.text { IndexMode::Off } else { o.index };
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
    // Component-wise, like ripgrep's `--sort path` (`a/b.txt` before `a.txt`).
    out.sort_by(|a, b| a.1.split('/').cmp(b.1.split('/')));
    out.dedup_by(|a, b| a.1 == b.1);
    let show_name = roots.len() > 1 || any_dir;
    Ok((out, show_name))
}

/// `lancet rg --files`: the files that would be searched.
pub fn list_files(o: &RgOptions, w: &mut dyn Write) -> Result<bool, String> {
    let (ts, _) = targets(o, &Req::All)?;
    for (_, d, _) in &ts {
        let _ = w.write_all(d.as_bytes());
        let _ = w.write_all(if o.null { b"\0" } else { b"\n" });
    }
    Ok(!ts.is_empty())
}

/// Run a search; `Ok(true)` if anything matched (exit 0), `Ok(false)` if not (exit 1).
pub fn run(o: &RgOptions, w: &mut dyn Write) -> Result<bool, String> {
    let (eng, full) = Engine::new(o)?;
    // Inverted / files-without-match searches need every file; otherwise the regex's
    // required trigrams narrow the candidates (verified by the regex itself).
    let req = if o.invert || o.files_without_match {
        Req::All
    } else {
        Req::from_expr(&Expr::Match {
            m: Matcher::Regex { pattern: full },
        })
    };
    let (ts, dir_search) = targets(o, &req)?;
    let show_name = o.with_filename.unwrap_or(dir_search);
    let cx = Ctx {
        o,
        eng: &eng,
        show_name,
    };
    let results: Vec<FileOut> = ts
        .par_iter()
        .filter_map(|(p, d, explicit)| search_file(&cx, p, d.clone(), *explicit))
        .collect();
    let mut any = false;
    let mut first_group = true;
    let (mut matched_lines, mut searches_with_match) = (0u64, 0u64);
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
        if r.has_output_group && (o.before > 0 || o.after > 0) && !o.json {
            if !first_group {
                let _ = w.write_all(b"--\n");
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
    Ok(any)
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
}
