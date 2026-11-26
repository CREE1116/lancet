//! Response types and the human-readable printer.

use crate::query::Unit;
use serde::Serialize;
use std::io::{self, Write};

fn is_false(b: &bool) -> bool {
    !*b
}
fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Search finished over the requested scope and found results.
    Complete,
    /// Search finished over the requested scope and found nothing.
    Empty,
    InvalidQuery,
    Unsupported,
    /// The file changed since the caller's `rev` was taken; nothing was written.
    Conflict,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct LineOut {
    pub line: u32,
    pub text: String,
    #[serde(skip_serializing_if = "is_false")]
    pub context: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ResultItem {
    pub id: String,
    pub path: String,
    pub unit: Unit,
    pub start_line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<LineOut>,
    /// Matching lines omitted because of `--max-lines`.
    #[serde(skip_serializing_if = "is_zero")]
    pub more_lines: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct TermStat {
    pub term: String,
    pub count: usize,
    pub files: usize,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct BudgetInfo {
    pub limit_tokens: usize,
    pub used_tokens: usize,
    pub estimated: bool,
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct Stats {
    pub files_considered: usize,
    pub files_searched: usize,
    pub files_matched: usize,
    pub binary_skipped: usize,
    pub read_errors: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_candidates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_files: Option<usize>,
    pub elapsed_ms: f64,
}

#[derive(Serialize, Debug, Clone)]
pub struct ErrorInfo {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<usize>,
}

#[derive(Serialize, Debug, Clone)]
pub struct Response {
    pub status: Status,
    /// Canonical form of the executed query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    pub mode: String,
    /// Number of results (units, or files with `group file`/`count`).
    pub matched: usize,
    /// `matched` is a lower bound: the search stopped once `limit` was reached.
    #[serde(skip_serializing_if = "is_false")]
    pub matched_is_lower_bound: bool,
    pub returned: usize,
    pub truncated: bool,
    pub results: Vec<ResultItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<TermStat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub stats: Stats,
}

impl Response {
    pub fn failure(
        status: Status,
        query: Option<String>,
        message: String,
        pos: Option<usize>,
        elapsed_ms: f64,
    ) -> Self {
        Response {
            status,
            query,
            mode: "none".into(),
            matched: 0,
            matched_is_lower_bound: false,
            returned: 0,
            truncated: false,
            results: vec![],
            terms: vec![],
            budget: None,
            warnings: vec![],
            error: Some(ErrorInfo { message, pos }),
            stats: Stats {
                elapsed_ms,
                ..Default::default()
            },
        }
    }
}

pub fn print_human(r: &Response, w: &mut dyn Write) -> io::Result<()> {
    if let Some(e) = &r.error {
        match e.pos {
            Some(p) => writeln!(w, "error: {} (at byte {p})", e.message)?,
            None => writeln!(w, "error: {}", e.message)?,
        }
        return Ok(());
    }
    if !r.terms.is_empty() {
        for t in &r.terms {
            writeln!(w, "{}\t{} ({} files)", t.term, t.count, t.files)?;
        }
        return Ok(());
    }
    let mut last: Option<&str> = None;
    for it in &r.results {
        if let Some(c) = it.count {
            writeln!(w, "{}:{c}", it.path)?;
            continue;
        }
        if it.lines.is_empty() && it.text.is_none() {
            writeln!(w, "{}", it.path)?;
            continue;
        }
        let new_file = last != Some(it.path.as_str());
        if new_file {
            if last.is_some() {
                writeln!(w)?;
            }
            if let Some(sym) = &it.symbol {
                writeln!(w, "{} :: [fn {sym}]", it.path)?;
            } else {
                writeln!(w, "{}", it.path)?;
            }
            last = Some(&it.path);
        } else if let Some(sym) = &it.symbol {
            writeln!(w, "-- [fn {sym}]")?;
        } else if it.unit != Unit::Line || it.lines.iter().any(|l| l.context) {
            writeln!(w, "--")?;
        }
        if let Some(t) = &it.text {
            for (i, l) in t.lines().enumerate() {
                writeln!(w, "{}|{l}", it.start_line as usize + i)?;
            }
        } else {
            for l in &it.lines {
                writeln!(
                    w,
                    "{}{}{}",
                    l.line,
                    if l.context { '-' } else { ':' },
                    l.text
                )?;
            }
        }
        if it.more_lines > 0 {
            writeln!(
                w,
                "... +{} more matching lines in L{}-L{}",
                it.more_lines, it.start_line, it.end_line
            )?;
        }
    }
    Ok(())
}
