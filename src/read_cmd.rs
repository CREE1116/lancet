//! Surgical file, AST-symbol, page, sheet, and archive-entry reader (`lancet read`).
//!
//! Uses the exact same text/adapter extraction pipeline as `lancet` search so
//! line numbers reported by search match `lancet read` 1-to-1 across source code,
//! PDFs, Office documents, Korean HWP/HWPX, spreadsheets, and archives.

use crate::code;
use crate::content;
use crate::output::{ErrorInfo, Status};
use serde::Serialize;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct ReadOptions {
    pub target: String,
    pub lines: Option<(usize, Option<usize>)>,
    pub symbol: Option<String>,
    pub pages: Option<(usize, usize)>,
    pub sheet: Option<String>,
    pub rows: Option<(usize, Option<usize>)>,
    pub cols: Option<(usize, usize)>,
    pub entry: Option<String>,
    pub budget_tokens: Option<usize>,
    pub raw: bool,
    pub ocr: bool,
}

impl ReadOptions {
    pub fn new(target: impl Into<String>) -> Self {
        ReadOptions {
            target: target.into(),
            lines: None,
            symbol: None,
            pages: None,
            sheet: None,
            rows: None,
            cols: None,
            entry: None,
            budget_tokens: None,
            raw: false,
            ocr: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReadLine {
    pub num: usize,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReadResponse {
    pub status: Status,
    pub path: String,
    /// Content revision of the file on disk (see [`crate::edit::rev`]); present
    /// only for plain-text files, i.e. the ones an edit can write back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    pub total_lines: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub returned_lines: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_line: Option<usize>,
    pub estimated_tokens: usize,
    pub lines: Vec<ReadLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub elapsed_ms: f64,
}

impl ReadResponse {
    fn failure(status: Status, path: String, message: String, elapsed_ms: f64) -> Self {
        ReadResponse {
            status,
            path,
            rev: None,
            section: None,
            total_lines: 0,
            start_line: 0,
            end_line: 0,
            returned_lines: 0,
            truncated: false,
            next_line: None,
            estimated_tokens: 0,
            lines: Vec::new(),
            error: Some(ErrorInfo { message, pos: None }),
            elapsed_ms,
        }
    }
}

/// Parse `START[:END]` or `START[-END]` or `LINE` (1-based). Open-ended `START:` returns `(START, None)`.
pub fn parse_line_spec(raw: &str) -> Result<(usize, Option<usize>), String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("empty range specification".into());
    }
    if let Some((a, b)) = s.split_once(':').or_else(|| s.split_once('-')) {
        let start: usize = if a.trim().is_empty() {
            1
        } else {
            a.trim()
                .parse()
                .map_err(|_| format!("invalid start `{a}` in range `{raw}`"))?
        };
        let end: Option<usize> = if b.trim().is_empty() {
            None
        } else {
            Some(
                b.trim()
                    .parse()
                    .map_err(|_| format!("invalid end `{b}` in range `{raw}`"))?,
            )
        };
        if start == 0 || end.is_some_and(|e| e < start) {
            return Err(format!("invalid 1-based range `{raw}`"));
        }
        Ok((start, end))
    } else {
        let line: usize = s
            .parse()
            .map_err(|_| format!("invalid line number `{raw}`"))?;
        if line == 0 {
            return Err("line numbers are 1-based (>= 1)".into());
        }
        Ok((line, Some(line)))
    }
}

/// Parse column range like `A:D`, `B`, or `1:4` into 1-based `(start_col, end_col)`.
pub fn parse_col_spec(raw: &str) -> Result<(usize, usize), String> {
    fn one_col(tok: &str) -> Result<usize, String> {
        let t = tok.trim();
        if t.is_empty() {
            return Err("empty column specifier".into());
        }
        if t.bytes().all(|b| b.is_ascii_digit()) {
            let n: usize = t.parse().map_err(|_| format!("invalid column `{t}`"))?;
            if n == 0 {
                return Err("column numbers are 1-based".into());
            }
            return Ok(n);
        }
        if t.bytes().all(|b| b.is_ascii_alphabetic()) {
            let mut acc = 0usize;
            for b in t.bytes() {
                acc = acc * 26 + (b.to_ascii_uppercase() - b'A' + 1) as usize;
            }
            return Ok(acc);
        }
        Err(format!("invalid column `{t}` (use e.g. `A:D` or `1:4`)"))
    }
    if let Some((a, b)) = raw
        .trim()
        .split_once(':')
        .or_else(|| raw.trim().split_once('-'))
    {
        let sc = one_col(a)?;
        let ec = one_col(b)?;
        if ec < sc {
            return Err(format!("column range `{raw}` has end < start"));
        }
        Ok((sc, ec))
    } else {
        let c = one_col(raw)?;
        Ok((c, c))
    }
}

type ParsedTarget = (PathBuf, Option<String>, Option<(usize, Option<usize>)>);

fn split_target(raw: &str) -> ParsedTarget {
    if Path::new(raw).exists() {
        return (PathBuf::from(raw), None, None);
    }
    if let Some((left, right)) = raw.split_once('!') {
        if Path::new(left).is_file() && !right.is_empty() {
            let (entry_name, inline_lines) = parse_trailing_range(right);
            return (
                PathBuf::from(left),
                Some(entry_name.to_string()),
                inline_lines,
            );
        }
    }
    let (base, inline_lines) = parse_trailing_range(raw);
    if inline_lines.is_some() && Path::new(base).exists() {
        return (PathBuf::from(base), None, inline_lines);
    }
    (PathBuf::from(raw), None, None)
}

fn parse_trailing_range(s: &str) -> (&str, Option<(usize, Option<usize>)>) {
    // Handle `path:10:50` first, then `path:10-50` or `path:42`
    if let Some((rest, b)) = s.rsplit_once(':') {
        if let Some((base, a)) = rest.rsplit_once(':') {
            if !a.is_empty()
                && a.bytes().all(|x| x.is_ascii_digit())
                && b.bytes().all(|x| x.is_ascii_digit())
            {
                if let Ok(spec) = parse_line_spec(&format!("{a}:{b}")) {
                    return (base, Some(spec));
                }
            }
        }
        if let Ok(spec) = parse_line_spec(b) {
            return (rest, Some(spec));
        }
    }
    (s, None)
}

fn estimate_line_tokens(s: &str) -> usize {
    let ascii = s.bytes().filter(|b| b.is_ascii()).count();
    let non_ascii = s.chars().filter(|c| !c.is_ascii()).count();
    (ascii + non_ascii * 6 + 6).div_ceil(4).max(1)
}

fn byte_to_line(line_starts: &[usize], byte_off: usize) -> usize {
    match line_starts.binary_search(&byte_off) {
        Ok(idx) => idx + 1,
        Err(idx) => idx.max(1),
    }
}

pub(crate) fn find_symbol_lines(
    path: &Path,
    bytes: &[u8],
    all_lines: &[&str],
    sym_query: &str,
) -> Result<(usize, usize, String), String> {
    if !code::ENABLED {
        return Err("`--symbol` requires building with `--features ast`".into());
    }
    if !code::supports_path(path) {
        return Err(format!(
            "`--symbol` is not supported for `{}` (supported: 24 AST languages including rs, py, js/ts, go, c/cpp, zig, java, cs, kt, scala, swift, rb, php, bash, lua, hcl, css, json, yaml, toml)",
            path.display()
        ));
    }
    let mut cands = code::extract_symbols(path, bytes).unwrap_or_default();
    if cands.is_empty() {
        cands = code::extract_functions(path, bytes).unwrap_or_default();
    }
    let exact = cands
        .iter()
        .find(|s| s.name.as_deref() == Some(sym_query))
        .or_else(|| {
            let low = sym_query.to_ascii_lowercase();
            cands.iter().find(|s| {
                s.name
                    .as_deref()
                    .is_some_and(|n| n.to_ascii_lowercase() == low)
            })
        })
        .ok_or_else(|| format!("symbol `{sym_query}` not found in {}", path.display()))?;

    let mut line_starts = vec![0usize];
    for pos in memchr::memchr_iter(b'\n', bytes) {
        line_starts.push(pos + 1);
    }
    let mut start_line = byte_to_line(&line_starts, exact.start);
    let end_byte = exact.end.saturating_sub(1).max(exact.start);
    let end_line = byte_to_line(&line_starts, end_byte).min(all_lines.len().max(1));

    // Include contiguous doc-comments / attributes right above declaration.
    while start_line > 1 {
        let prev = all_lines[start_line - 2].trim();
        if prev.starts_with("///")
            || prev.starts_with("//!")
            || prev.starts_with("#[")
            || prev.starts_with('@')
        {
            start_line -= 1;
        } else {
            break;
        }
    }
    let label = format!(
        "{} {}",
        exact.kind,
        exact.name.as_deref().unwrap_or(sym_query)
    );
    Ok((start_line, end_line, label))
}

fn filter_marker_blocks(
    numbered: &[(usize, &str)],
    prefixes: &[&str],
    p_start: usize,
    p_end: usize,
) -> Option<(Vec<(usize, String)>, String)> {
    let mut out = Vec::new();
    let mut cur_num: Option<usize> = None;
    let mut saw_any_marker = false;
    let mut matched_kind = "Page";

    for &(ln, text) in numbered {
        let trimmed = text.trim();
        let mut new_marker: Option<usize> = None;
        for &pref in prefixes {
            if let Some(rest) = trimmed.strip_prefix(pref) {
                if let Some(end_br) = rest.find([']', ' ']) {
                    if let Ok(n) = rest[..end_br].parse::<usize>() {
                        new_marker = Some(n);
                        saw_any_marker = true;
                        matched_kind = pref.trim_start_matches('[').trim();
                        break;
                    }
                }
            }
        }
        if let Some(n) = new_marker {
            cur_num = Some(n);
        }
        if let Some(n) = cur_num {
            if n >= p_start && n <= p_end {
                out.push((ln, text.to_string()));
            }
        }
    }
    if !saw_any_marker {
        return None;
    }
    let label = if p_start == p_end {
        format!("{matched_kind} {p_start}")
    } else {
        format!("{matched_kind} {p_start}-{p_end}")
    };
    Some((out, label))
}

fn filter_archive_entry(
    numbered: &[(usize, &str)],
    entry_query: &str,
) -> Result<(Vec<(usize, String)>, String), String> {
    let mut sections: Vec<(String, Vec<(usize, String)>)> = Vec::new();
    for &(ln, text) in numbered {
        if let Some(rest) = text
            .strip_prefix("=== ")
            .and_then(|s| s.strip_suffix(" ==="))
        {
            sections.push((rest.to_string(), Vec::new()));
            continue;
        }
        if let Some(last) = sections.last_mut() {
            last.1.push((ln, text.to_string()));
        }
    }
    if sections.is_empty() {
        return Err("target does not contain `=== <entry> ===` archive sections".into());
    }
    let found = sections
        .iter()
        .find(|(name, _)| name == entry_query)
        .or_else(|| {
            let q_low = entry_query.to_ascii_lowercase();
            sections.iter().find(|(name, _)| {
                name.to_ascii_lowercase().ends_with(&q_low)
                    || name.to_ascii_lowercase().contains(&q_low)
            })
        })
        .ok_or_else(|| {
            let avail: Vec<&str> = sections.iter().take(8).map(|(n, _)| n.as_str()).collect();
            format!(
                "archive entry `{entry_query}` not found (available: {})",
                avail.join(", ")
            )
        })?;
    Ok((found.1.clone(), format!("entry:{}", found.0)))
}

fn filter_sheet(
    numbered: &[(usize, &str)],
    sheet_query: &str,
    rows: Option<(usize, Option<usize>)>,
    cols: Option<(usize, usize)>,
) -> Result<(Vec<(usize, String)>, String), String> {
    let mut sheets: Vec<(String, Vec<(usize, &str)>)> = Vec::new();
    for &(ln, text) in numbered {
        if let Some(name) = text
            .strip_prefix("[Sheet: ")
            .and_then(|s| s.strip_suffix(']'))
        {
            sheets.push((name.to_string(), Vec::new()));
            continue;
        }
        if let Some(last) = sheets.last_mut() {
            if !text.is_empty() {
                last.1.push((ln, text));
            }
        }
    }
    if sheets.is_empty() {
        return Err("target does not contain `[Sheet: ...]` spreadsheet data".into());
    }
    let picked = if let Ok(idx1) = sheet_query.parse::<usize>() {
        if idx1 == 0 || idx1 > sheets.len() {
            return Err(format!(
                "sheet index {idx1} out of range (workbook has {} sheet(s))",
                sheets.len()
            ));
        }
        &sheets[idx1 - 1]
    } else {
        let low = sheet_query.to_ascii_lowercase();
        sheets
            .iter()
            .find(|(n, _)| n == sheet_query)
            .or_else(|| sheets.iter().find(|(n, _)| n.to_ascii_lowercase() == low))
            .ok_or_else(|| {
                let names: Vec<&str> = sheets.iter().map(|(n, _)| n.as_str()).collect();
                format!(
                    "sheet `{sheet_query}` not found (sheets: {})",
                    names.join(", ")
                )
            })?
    };

    let (r_start, r_end_opt) = rows.unwrap_or((1, None));
    let r_end = r_end_opt.unwrap_or(picked.1.len());
    let mut out = Vec::new();
    for (row_idx0, &(_doc_ln, row_str)) in picked.1.iter().enumerate() {
        let row_num = row_idx0 + 1;
        if row_num < r_start || row_num > r_end {
            continue;
        }
        let formatted = if let Some((c_start, c_end)) = cols {
            let cells: Vec<&str> = row_str.split('\t').collect();
            let lo = (c_start - 1).min(cells.len());
            let hi = c_end.min(cells.len());
            cells[lo..hi].join("\t")
        } else {
            row_str.to_string()
        };
        out.push((row_num, formatted));
    }
    Ok((out, format!("sheet:{}", picked.0)))
}

pub fn read_file_range(opts: &ReadOptions) -> ReadResponse {
    let t0 = Instant::now();
    let (path, bang_entry, inline_lines) = split_target(&opts.target);
    let path_str = path.to_string_lossy().into_owned();

    if !path.exists() {
        return ReadResponse::failure(
            Status::InvalidQuery,
            path_str.clone(),
            format!("file not found: `{path_str}`"),
            ms(t0),
        );
    }
    if path.is_dir() {
        return ReadResponse::failure(
            Status::InvalidQuery,
            path_str.clone(),
            format!("`{path_str}` is a directory (use `lancet fd` or `lancet outline`)"),
            ms(t0),
        );
    }

    let data = match content::load_searchable(&path, opts.ocr) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return ReadResponse::failure(
                Status::Unsupported,
                path_str,
                "binary or unreadable file (for scanned images/PDFs pass `--ocr`)".into(),
                ms(t0),
            )
        }
        Err(e) => {
            return ReadResponse::failure(
                Status::InvalidQuery,
                path_str,
                format!("read error: {e}"),
                ms(t0),
            )
        }
    };

    let text = String::from_utf8_lossy(&data);
    let raw_lines: Vec<&str> = text.lines().collect();
    let total_lines = raw_lines.len();
    let numbered: Vec<(usize, &str)> = raw_lines
        .iter()
        .enumerate()
        .map(|(i, &s)| (i + 1, s))
        .collect();

    let effective_entry = opts.entry.as_deref().or(bang_entry.as_deref());
    // Extracted text (PDF, HWP, archive entries…) can't be written back, so no rev.
    let rev = (effective_entry.is_none() && !crate::adapters::is_adapted_path(&path, opts.ocr))
        .then(|| crate::edit::rev(&data));
    let effective_lines = opts.lines.or(inline_lines);

    let mut section: Option<String> = None;
    let mut selected: Vec<(usize, String)> = if let Some(sym) = &opts.symbol {
        match find_symbol_lines(&path, &data, &raw_lines, sym) {
            Ok((sl, el, label)) => {
                section = Some(label);
                numbered[sl.saturating_sub(1)..el.min(numbered.len())]
                    .iter()
                    .map(|&(n, s)| (n, s.to_string()))
                    .collect()
            }
            Err(msg) => {
                return ReadResponse::failure(Status::InvalidQuery, path_str, msg, ms(t0));
            }
        }
    } else if let Some(ent) = effective_entry {
        match filter_archive_entry(&numbered, ent) {
            Ok((v, label)) => {
                section = Some(label);
                v
            }
            Err(msg) => {
                return ReadResponse::failure(Status::InvalidQuery, path_str, msg, ms(t0));
            }
        }
    } else if let Some(sh) = &opts.sheet {
        match filter_sheet(&numbered, sh, opts.rows, opts.cols) {
            Ok((v, label)) => {
                section = Some(label);
                v
            }
            Err(msg) => {
                return ReadResponse::failure(Status::InvalidQuery, path_str, msg, ms(t0));
            }
        }
    } else if opts.rows.is_some() || opts.cols.is_some() {
        // Default to first sheet when --rows/--cols used on a spreadsheet without --sheet
        match filter_sheet(&numbered, "1", opts.rows, opts.cols) {
            Ok((v, label)) => {
                section = Some(label);
                v
            }
            Err(_) => numbered.iter().map(|&(n, s)| (n, s.to_string())).collect(),
        }
    } else if let Some((ps, pe)) = opts.pages {
        match filter_marker_blocks(&numbered, &["[Page ", "[Slide "], ps, pe) {
            Some((v, label)) => {
                section = Some(label);
                v
            }
            None => {
                return ReadResponse::failure(
                    Status::InvalidQuery,
                    path_str,
                    "document has no `[Page N]` or `[Slide N]` markers (use `--lines START:END` instead)".into(),
                    ms(t0),
                );
            }
        }
    } else {
        numbered.iter().map(|&(n, s)| (n, s.to_string())).collect()
    };

    // Apply `--lines START:END` slice if present:
    // - If a sub-section (`entry` / `page`) was selected, allow filtering either by 1-based offset inside section OR matching line number.
    if let Some((ls, le_opt)) = effective_lines {
        if section.is_some() && opts.symbol.is_none() {
            let le = le_opt.unwrap_or(selected.len());
            let lo = ls.saturating_sub(1).min(selected.len());
            let hi = le.min(selected.len());
            selected = selected[lo..hi].to_vec();
        } else {
            let le = le_opt.unwrap_or(usize::MAX);
            selected.retain(|&(num, _)| num >= ls && num <= le);
        }
    }

    let mut truncated = false;
    let mut next_line = None;
    let mut used_tokens = 0usize;
    let mut out_lines = Vec::with_capacity(selected.len());

    for (idx, (num, line_txt)) in selected.into_iter().enumerate() {
        let est = estimate_line_tokens(&line_txt);
        if let Some(budget) = opts.budget_tokens {
            if idx > 0 && used_tokens + est > budget {
                truncated = true;
                next_line = Some(num);
                break;
            }
        }
        used_tokens += est;
        out_lines.push(ReadLine {
            num,
            text: line_txt,
        });
    }

    let start_line = out_lines.first().map_or(0, |l| l.num);
    let end_line = out_lines.last().map_or(0, |l| l.num);
    let returned_lines = out_lines.len();

    ReadResponse {
        status: if returned_lines == 0 {
            Status::Empty
        } else {
            Status::Complete
        },
        path: path_str,
        rev,
        section,
        total_lines,
        start_line,
        end_line,
        returned_lines,
        truncated,
        next_line,
        estimated_tokens: used_tokens,
        lines: out_lines,
        error: None,
        elapsed_ms: ms(t0),
    }
}

pub fn print_read_human(resp: &ReadResponse, raw: bool, w: &mut impl Write) -> io::Result<()> {
    if let Some(err) = &resp.error {
        return writeln!(w, "error: {}", err.message);
    }
    if !raw {
        // The rev is what `edit` / `rename` / `patch` take as `--rev`.
        let sec = resp
            .section
            .as_ref()
            .map(|s| format!(" [{s}]"))
            .unwrap_or_default();
        let rev = resp
            .rev
            .as_ref()
            .map(|r| format!(" rev {r}"))
            .unwrap_or_default();
        if !sec.is_empty() || !rev.is_empty() {
            writeln!(w, "# {}{sec}{rev}", resp.path)?;
        }
    }
    let width = resp.end_line.max(1).ilog10() as usize + 1;
    for l in &resp.lines {
        if raw {
            writeln!(w, "{}", l.text)?;
        } else {
            writeln!(w, "{:>width$} | {}", l.num, l.text, width = width.max(4))?;
        }
    }
    Ok(())
}

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}
