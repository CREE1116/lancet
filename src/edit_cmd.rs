//! `lancet edit`: replace, delete, or insert whole lines addressed exactly the way
//! `lancet read` addresses them (`--lines`, `--symbol`), written through [`crate::edit`].
//!
//! Safety, in order: documents/archives are read-only, `--rev` must match the file
//! on disk, the edit must not add Tree-sitter syntax errors, and nothing is written
//! without `apply`.

use crate::edit::{self, Edit};
use crate::output::{ErrorInfo, Status};
use crate::read_cmd::find_symbol_lines;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

/// Which lines to edit (1-based, inclusive), as in `lancet read`.
#[derive(Clone, Debug)]
pub enum Target {
    /// `(start, end)`; `end = None` means through the last line.
    Lines(usize, Option<usize>),
    /// The symbol's lines as `read --symbol` shows them (doc comments included).
    Symbol(String),
}

#[derive(Clone, Debug)]
pub enum Op {
    Replace(String),
    Delete,
    InsertBefore(String),
    InsertAfter(String),
}

#[derive(Clone, Debug)]
pub struct EditOptions {
    pub path: PathBuf,
    pub target: Target,
    pub op: Op,
    /// Expected content revision (from `read`/`ast`/a previous `edit`).
    pub rev: Option<String>,
    pub apply: bool,
    pub allow_syntax_errors: bool,
}

impl EditOptions {
    pub fn new(path: impl Into<PathBuf>, target: Target, op: Op) -> Self {
        EditOptions {
            path: path.into(),
            target,
            op,
            rev: None,
            apply: false,
            allow_syntax_errors: false,
        }
    }
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
    /// Addressed lines in the original file.
    pub start_line: usize,
    pub end_line: usize,
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
        applied: false,
        diff: String::new(),
        error: None,
        elapsed_ms: 0.0,
    };
    let fail = |mut r: EditResponse, status: Status, msg: String| {
        r.status = status;
        r.error = Some(ErrorInfo {
            message: msg,
            pos: None,
        });
        r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        r
    };

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
    let bytes = match fs::read(&o.path) {
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
    let (a, b) = match &o.target {
        Target::Lines(a, b) => (*a, b.unwrap_or(lines.len())),
        Target::Symbol(name) => match find_symbol_lines(&o.path, src.as_bytes(), &lines, name) {
            Ok((s, e, _)) => (s, e),
            Err(msg) => return fail(r, Status::InvalidQuery, msg),
        },
    };
    if a == 0 || a > b || b > lines.len() {
        return fail(
            r,
            Status::InvalidQuery,
            format!("line range {a}:{b} is outside the file (1:{})", lines.len()),
        );
    }
    r.start_line = a;
    r.end_line = b;

    let starts: Vec<usize> = std::iter::once(0)
        .chain(memchr::memchr_iter(b'\n', src.as_bytes()).map(|i| i + 1))
        .collect();
    let (s, e) = (starts[a - 1], starts.get(b).copied().unwrap_or(src.len()));
    let eol = if src[..starts.get(1).copied().unwrap_or(0)].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    // Normalise the new text to the file's line ending and end it with one.
    let norm = |t: &str| -> String {
        let mut out = t.replace("\r\n", "\n").replace('\n', eol);
        if !out.is_empty() && !out.ends_with(eol) {
            out.push_str(eol);
        }
        out
    };
    let last_has_eol = src[s..e].ends_with('\n');
    let ed = match &o.op {
        Op::Replace(t) => {
            let mut rep = norm(t);
            if !last_has_eol && rep.ends_with(eol) {
                rep.truncate(rep.len() - eol.len()); // keep "no newline at end of file"
            }
            Edit {
                start: s,
                end: e,
                replacement: rep,
            }
        }
        Op::Delete => Edit {
            start: s,
            end: e,
            replacement: String::new(),
        },
        Op::InsertBefore(t) => Edit {
            start: s,
            end: s,
            replacement: norm(t),
        },
        Op::InsertAfter(t) if !last_has_eol => {
            let mut rep = format!("{eol}{}", norm(t));
            rep.truncate(rep.len() - eol.len());
            Edit {
                start: e,
                end: e,
                replacement: rep,
            }
        }
        Op::InsertAfter(t) => Edit {
            start: e,
            end: e,
            replacement: norm(t),
        },
    };
    let new_src = edit::plan(&src, &[ed]).new_src;
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
            if b1 > b0 {
                return fail(
                    r,
                    Status::InvalidQuery,
                    format!(
                        "edit would add {} syntax error(s); nothing written (pass --allow-syntax-errors to write anyway)",
                        b1 - b0
                    ),
                );
            }
        }
    }

    r.rev_after = Some(edit::rev(new_src.as_bytes()));
    if o.apply {
        if let Err(e) = edit::write_atomic(&o.path, new_src.as_bytes()) {
            return fail(r, Status::InvalidQuery, format!("write failed: {e}"));
        }
        r.applied = true;
    }
    r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    r
}
