//! `lancet edit`: replace, delete, or insert whole lines addressed exactly the way
//! `lancet read` addresses them (`--lines`, `--symbol`), written through [`crate::edit`].
//!
//! Safety, in order: documents/archives are read-only, `--rev` must match the file
//! on disk, the edit must not add Tree-sitter syntax errors, and nothing is written
//! without `apply`. Several hunks in one call are all addressed in the *original*
//! file's coordinates (no drift between them), must not overlap, and land in one
//! locked compare-and-swap write — all of them or none.

use crate::edit::{self, Edit};
use crate::output::{ErrorInfo, Status};
use crate::read_cmd::find_symbol_lines;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

/// Which lines to edit (1-based, inclusive), as in `lancet read`.
#[derive(Clone, Debug)]
pub enum Target {
    /// `(start, end)`; `end = None` means through the last line.
    Lines(usize, Option<usize>),
    /// The symbol's lines as `read --symbol` shows them (doc comments included).
    Symbol(String),
    /// Exact text (not whole lines). It must occur once, or `all` edits every
    /// occurrence. `\n` in it matches the file's line ending (`\r\n` in CRLF files).
    Find { text: String, all: bool },
}

#[derive(Clone, Debug)]
pub enum Op {
    Replace(String),
    Delete,
    InsertBefore(String),
    InsertAfter(String),
}

/// One change: which lines, and what to do with them.
#[derive(Clone, Debug)]
pub struct Hunk {
    pub target: Target,
    pub op: Op,
}

#[derive(Clone, Debug)]
pub struct EditOptions {
    pub path: PathBuf,
    /// Applied together; all addressed in the original file's coordinates.
    pub hunks: Vec<Hunk>,
    /// Expected content revision (from `read`/`ast`/a previous `edit`).
    pub rev: Option<String>,
    pub apply: bool,
    pub allow_syntax_errors: bool,
}

impl EditOptions {
    pub fn new(path: impl Into<PathBuf>, target: Target, op: Op) -> Self {
        Self::multi(path, vec![Hunk { target, op }])
    }

    pub fn multi(path: impl Into<PathBuf>, hunks: Vec<Hunk>) -> Self {
        EditOptions {
            path: path.into(),
            hunks,
            rev: None,
            apply: false,
            allow_syntax_errors: false,
        }
    }
}

/// JSON form of a hunk for `--edits`: exactly one of `lines`/`symbol` and one of
/// `with`/`delete`/`insert_before`/`insert_after`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HunkSpec {
    lines: Option<String>,
    symbol: Option<String>,
    find: Option<String>,
    #[serde(default)]
    all: bool,
    with: Option<String>,
    #[serde(default)]
    delete: bool,
    insert_before: Option<String>,
    insert_after: Option<String>,
}

/// Parse `[{"lines": "10:12", "with": "…"}, {"symbol": "f", "delete": true}, …]`.
pub fn parse_hunks(json: &str) -> Result<Vec<Hunk>, String> {
    let specs: Vec<HunkSpec> =
        serde_json::from_str(json).map_err(|e| format!("invalid --edits JSON: {e}"))?;
    if specs.is_empty() {
        return Err("--edits needs at least one hunk".into());
    }
    specs
        .into_iter()
        .enumerate()
        .map(|(i, h)| {
            let n = i + 1;
            if h.all && h.find.is_none() {
                return Err(format!("hunk {n}: `all` only applies to `find`"));
            }
            let target = match (h.lines, h.symbol, h.find) {
                (Some(l), None, None) => {
                    let (a, b) = crate::read_cmd::parse_line_spec(&l)
                        .map_err(|e| format!("hunk {n}: {e}"))?;
                    Target::Lines(a, b)
                }
                (None, Some(s), None) => Target::Symbol(s),
                (None, None, Some(text)) => Target::Find { text, all: h.all },
                _ => {
                    return Err(format!(
                        "hunk {n}: give exactly one of `lines`, `symbol` or `find`"
                    ))
                }
            };
            let ops = [
                h.with.map(Op::Replace),
                h.delete.then_some(Op::Delete),
                h.insert_before.map(Op::InsertBefore),
                h.insert_after.map(Op::InsertAfter),
            ];
            let mut ops = ops.into_iter().flatten();
            match (ops.next(), ops.next()) {
                (Some(op), None) => Ok(Hunk { target, op }),
                _ => Err(format!(
                    "hunk {n}: give exactly one of `with`, `delete`, `insert_before`, `insert_after`"
                )),
            }
        })
        .collect()
}

#[derive(Clone, Debug, Serialize)]
pub struct EditResponse {
    pub status: Status,
    /// `dry-run` or `applied`.
    pub mode: &'static str,
    pub path: String,
    /// Revision of the file before the edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    /// Revision after the edit; pass it as `--rev` to chain the next edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev_after: Option<String>,
    /// Addressed lines in the original file (span of all hunks).
    pub start_line: usize,
    pub end_line: usize,
    /// Per-hunk addressed lines `[start, end]` in the original file, in the order given.
    pub hunks: Vec<[usize; 2]>,
    pub applied: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub diff: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub elapsed_ms: f64,
}

pub fn edit_file(o: &EditOptions) -> EditResponse {
    let t0 = Instant::now();
    let mut r = EditResponse {
        status: Status::Complete,
        mode: if o.apply { "applied" } else { "dry-run" },
        path: o.path.to_string_lossy().into_owned(),
        rev: None,
        rev_after: None,
        start_line: 0,
        end_line: 0,
        hunks: Vec::new(),
        applied: false,
        diff: String::new(),
        error: None,
        elapsed_ms: 0.0,
    };
    let fail = |mut r: EditResponse, status: Status, msg: String| {
        r.status = status;
        r.rev_after = None;
        r.error = Some(ErrorInfo {
            message: msg,
            pos: None,
        });
        r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        r
    };

    if o.hunks.is_empty() {
        return fail(r, Status::InvalidQuery, "no edit given".into());
    }
    if !o.path.is_file() {
        return fail(
            r,
            Status::InvalidQuery,
            format!("file not found: `{}`", o.path.display()),
        );
    }
    if crate::adapters::is_adapted_path(&o.path, false) {
        return fail(
            r,
            Status::Unsupported,
            "documents and archives are read-only: their text is extracted, so there is no byte range to write back".into(),
        );
    }
    let bytes = match edit::read_shared(&o.path) {
        Ok(b) => b,
        Err(e) => return fail(r, Status::InvalidQuery, format!("read error: {e}")),
    };
    if crate::content::is_binary(&bytes) {
        return fail(r, Status::Unsupported, "binary file".into());
    }
    let src = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return fail(r, Status::Unsupported, "file is not valid UTF-8".into()),
    };
    let rev = edit::rev(src.as_bytes());
    r.rev = Some(rev.clone());
    if let Some(want) = &o.rev {
        if *want != rev {
            return fail(
                r,
                Status::Conflict,
                format!("file changed since rev {want} (now {rev}); read it again before editing"),
            );
        }
    }

    // Same line model as `read`: `str::lines`, 1-based.
    let lines: Vec<&str> = src.lines().collect();
    let starts: Vec<usize> = std::iter::once(0)
        .chain(memchr::memchr_iter(b'\n', src.as_bytes()).map(|i| i + 1))
        .collect();
    let eol = if src[..starts.get(1).copied().unwrap_or(0)].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let multi = o.hunks.len() > 1;
    let mut edits: Vec<(usize, Edit)> = Vec::with_capacity(o.hunks.len());
    for (i, h) in o.hunks.iter().enumerate() {
        let tag = if multi {
            format!("hunk {}: ", i + 1)
        } else {
            String::new()
        };
        let (a, b) = match &h.target {
            Target::Find { text, all } => match find_edits(&src, &starts, text, *all, &h.op, eol) {
                Ok(found) => {
                    for (span, e) in found {
                        r.hunks.push(span);
                        edits.push((i + 1, e));
                    }
                    continue;
                }
                Err(msg) => return fail(r, Status::InvalidQuery, format!("{tag}{msg}")),
            },
            Target::Lines(a, b) => (*a, b.unwrap_or(lines.len())),
            Target::Symbol(name) => {
                match find_symbol_lines(&o.path, src.as_bytes(), &lines, name) {
                    Ok((s, e, _)) => (s, e),
                    Err(msg) => return fail(r, Status::InvalidQuery, format!("{tag}{msg}")),
                }
            }
        };
        if a == 0 || a > b || b > lines.len() {
            return fail(
                r,
                Status::InvalidQuery,
                format!(
                    "{tag}line range {a}:{b} is outside the file (1:{})",
                    lines.len()
                ),
            );
        }
        r.hunks.push([a, b]);
        let (s, e) = (starts[a - 1], starts.get(b).copied().unwrap_or(src.len()));
        edits.push((i + 1, line_edit(&src, s, e, &h.op, eol)));
    }
    r.start_line = r.hunks.iter().map(|h| h[0]).min().unwrap_or(0);
    r.end_line = r.hunks.iter().map(|h| h[1]).max().unwrap_or(0);

    // Hunks must not overlap, and two hunks at the same spot where one is an
    // insertion have no defined order: refuse instead of guessing.
    let mut by_pos: Vec<&(usize, Edit)> = edits.iter().collect();
    by_pos.sort_by_key(|(_, e)| (e.start, e.end));
    for w in by_pos.windows(2) {
        let ((n1, e1), (n2, e2)) = (w[0], w[1]);
        let same_spot_insert = e1.start == e2.start && (e1.start == e1.end || e2.start == e2.end);
        if e1.end > e2.start || same_spot_insert {
            let (x, y) = ((*n1).min(*n2), (*n1).max(*n2));
            return fail(
                r,
                Status::InvalidQuery,
                format!("hunks {x} and {y} overlap; merge them into one hunk"),
            );
        }
    }

    let edits: Vec<Edit> = edits.into_iter().map(|(_, e)| e).collect();
    let new_src = edit::plan(&src, &edits).new_src;
    r.diff = edit::unified_diff(&r.path, &src, &new_src);
    if new_src == src {
        r.status = Status::Empty;
        r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        return r;
    }
    if !o.allow_syntax_errors {
        let before = crate::code::syntax_errors(&o.path, src.as_bytes());
        let after = crate::code::syntax_errors(&o.path, new_src.as_bytes());
        if let (Some(b0), Some(b1)) = (before, after) {
            // Only a net increase is refused, so a file that is already broken can
            // still be repaired step by step.
            if b1 > b0 {
                return fail(
                    r,
                    Status::InvalidQuery,
                    format!(
                        "edit would add {} syntax error(s) ({b0} -> {b1}); nothing written (pass --allow-syntax-errors to write anyway)",
                        b1 - b0
                    ),
                );
            }
        }
    }

    r.rev_after = Some(edit::rev(new_src.as_bytes()));
    if o.apply {
        // Check-and-write under the file lock: another writer since our read => conflict.
        match edit::write_if_rev(&o.path, &rev, new_src.as_bytes()) {
            Ok(()) => r.applied = true,
            Err(edit::WriteError::Conflict { now }) => {
                return fail(
                    r,
                    Status::Conflict,
                    format!("file changed while editing (now rev {now}); nothing written"),
                );
            }
            Err(edit::WriteError::Io(e)) => {
                return fail(r, Status::InvalidQuery, format!("write failed: {e}"));
            }
        }
    }
    r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    r
}

/// Edits for every occurrence of `text` (exactly one unless `all`), with the
/// 1-based lines each occurrence spans.
fn find_edits(
    src: &str,
    starts: &[usize],
    text: &str,
    all: bool,
    op: &Op,
    eol: &str,
) -> Result<Vec<([usize; 2], Edit)>, String> {
    let fix = |t: &str| t.replace("\r\n", "\n").replace('\n', eol);
    let needle = fix(text);
    if needle.is_empty() {
        return Err("--find text is empty".into());
    }
    let line_of = |byte: usize| starts.partition_point(|&s| s <= byte);
    let hits: Vec<usize> = src.match_indices(&needle).map(|(at, _)| at).collect();
    match hits.len() {
        0 => return Err("--find text does not occur in the file".into()),
        1 => {}
        n if !all => {
            let at: Vec<String> = hits
                .iter()
                .take(5)
                .map(|&h| line_of(h).to_string())
                .collect();
            return Err(format!(
                "--find text occurs {n} times (lines {}{}); add surrounding text to make it unique, or pass --all",
                at.join(", "),
                if n > 5 { ", …" } else { "" }
            ));
        }
        _ => {}
    }
    Ok(hits
        .into_iter()
        .map(|at| {
            let end = at + needle.len();
            let span = [line_of(at), line_of(end - 1)];
            let (start, end, replacement) = match op {
                Op::Replace(t) => (at, end, fix(t)),
                Op::Delete => (at, end, String::new()),
                Op::InsertBefore(t) => (at, at, fix(t)),
                Op::InsertAfter(t) => (end, end, fix(t)),
            };
            (
                span,
                Edit {
                    start,
                    end,
                    replacement,
                },
            )
        })
        .collect())
}

/// The byte edit for `op` on the whole lines `src[s..e]`, keeping the file's line
/// ending and its (missing) final newline.
fn line_edit(src: &str, s: usize, e: usize, op: &Op, eol: &str) -> Edit {
    let norm = |t: &str| -> String {
        let mut out = t.replace("\r\n", "\n").replace('\n', eol);
        if !out.is_empty() && !out.ends_with(eol) {
            out.push_str(eol);
        }
        out
    };
    let last_has_eol = src[s..e].ends_with('\n');
    let (start, end, replacement) = match op {
        Op::Replace(t) => {
            let mut rep = norm(t);
            if !last_has_eol && rep.ends_with(eol) {
                rep.truncate(rep.len() - eol.len()); // keep "no newline at end of file"
            }
            (s, e, rep)
        }
        Op::Delete => (s, e, String::new()),
        Op::InsertBefore(t) => (s, s, norm(t)),
        Op::InsertAfter(t) if !last_has_eol => {
            let mut rep = format!("{eol}{}", norm(t));
            rep.truncate(rep.len() - eol.len());
            (e, e, rep)
        }
        Op::InsertAfter(t) => (e, e, norm(t)),
    };
    Edit {
        start,
        end,
        replacement,
    }
}
