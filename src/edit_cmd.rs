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
    /// `count`: the text must occur exactly this many times (all are changed),
    /// like `expected_replacements` in other agents' edit tools.
    Find {
        text: String,
        all: bool,
        count: Option<usize>,
    },
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
    /// `old_string` / `new_string` / `replace_all` / `expected_replacements` are
    /// accepted too, as other agents' edit tools name them.
    #[serde(alias = "old_string")]
    find: Option<String>,
    #[serde(default, alias = "replace_all")]
    all: bool,
    #[serde(alias = "expected_replacements")]
    count: Option<usize>,
    #[serde(alias = "new_string")]
    with: Option<String>,
    /// Multi-file edits: the whole content of `path` (`lancet write`).
    content: Option<String>,
    #[serde(default)]
    delete: bool,
    insert_before: Option<String>,
    insert_after: Option<String>,
    /// Multi-file edits only: the file this hunk changes, and the rev it was read at.
    path: Option<String>,
    rev: Option<String>,
}

fn parse_specs(json: &str) -> Result<Vec<HunkSpec>, String> {
    let specs: Vec<HunkSpec> =
        serde_json::from_str(json).map_err(|e| format!("invalid --edits JSON: {e}"))?;
    if specs.is_empty() {
        return Err("--edits needs at least one hunk".into());
    }
    Ok(specs)
}

/// Parse `[{"lines": "10:12", "with": "…"}, {"symbol": "f", "delete": true}, …]`
/// for one file. A hunk naming a `path` is refused here rather than applied to
/// the wrong file: that takes [`parse_file_hunks`].
pub fn parse_hunks(json: &str) -> Result<Vec<Hunk>, String> {
    let specs = parse_specs(json)?;
    if let Some(i) = specs
        .iter()
        .position(|h| h.path.is_some() || h.rev.is_some() || h.content.is_some())
    {
        return Err(format!(
            "hunk {}: `path`/`rev`/`content` belong to a multi-file edit (`lancet edit --edits FILE` without a PATH argument)",
            i + 1
        ));
    }
    specs
        .into_iter()
        .enumerate()
        .map(|(i, h)| spec_to_hunk(i + 1, h))
        .collect()
}

/// One file's share of a multi-file edit.
#[derive(Clone, Debug)]
pub struct FileEdits {
    pub path: PathBuf,
    pub rev: Option<String>,
    pub hunks: Vec<Hunk>,
    /// `{"path", "content"}`: the file's whole new content (created if missing).
    pub content: Option<String>,
}

/// Parse hunks that may name their file: `[{"path": "a.rs", "rev": "…", "find":
/// "x", "with": "y"}, …]`. Hunks without `path` go to `default_path`; files keep
/// the order they first appear in. Every hunk of one file must agree on `rev`.
pub fn parse_file_hunks(
    json: &str,
    default_path: Option<&std::path::Path>,
    default_rev: Option<&str>,
) -> Result<Vec<FileEdits>, String> {
    let mut files: Vec<FileEdits> = Vec::new();
    for (i, mut h) in parse_specs(json)?.into_iter().enumerate() {
        let n = i + 1;
        let path = match (h.path.take(), default_path) {
            (Some(p), _) => PathBuf::from(p),
            (None, Some(d)) => d.to_path_buf(),
            (None, None) => {
                return Err(format!(
                    "hunk {n}: no `path` (give one per hunk, or pass the file as PATH)"
                ))
            }
        };
        let explicit_rev = h.rev.take();
        let rev = explicit_rev.or_else(|| {
            (Some(path.as_path()) == default_path)
                .then(|| default_rev.map(str::to_string))
                .flatten()
        });
        let content = h.content.take();
        let hunk = match content {
            Some(_) => {
                if h.lines.is_some()
                    || h.symbol.is_some()
                    || h.find.is_some()
                    || h.with.is_some()
                    || h.delete
                    || h.insert_before.is_some()
                    || h.insert_after.is_some()
                {
                    return Err(format!("hunk {n}: `content` replaces the whole file; it can't be combined with other fields"));
                }
                None
            }
            None => Some(spec_to_hunk(n, h)?),
        };
        let same = |f: &FileEdits| {
            f.path == path
                || matches!(
                    (std::fs::canonicalize(&f.path), std::fs::canonicalize(&path)),
                    (Ok(a), Ok(b)) if a == b
                )
        };
        match files.iter_mut().find(|f| same(f)) {
            Some(f) => {
                match (&f.rev, &rev) {
                    (Some(a), Some(b)) if a != b => {
                        return Err(format!(
                            "hunk {n}: {} has two different revs ({a} and {b})",
                            path.display()
                        ))
                    }
                    (None, Some(_)) => f.rev = rev,
                    _ => {}
                }
                if content.is_some() || f.content.is_some() {
                    return Err(format!("hunk {n}: {} gets whole `content` and other hunks too; give one or the other", path.display()));
                }
                f.hunks.extend(hunk);
            }
            None => files.push(FileEdits {
                path,
                rev,
                hunks: hunk.into_iter().collect(),
                content,
            }),
        }
    }
    Ok(files)
}

fn spec_to_hunk(n: usize, h: HunkSpec) -> Result<Hunk, String> {
    {
        {
            if (h.all || h.count.is_some()) && h.find.is_none() {
                return Err(format!("hunk {n}: `all` only applies to `find`"));
            }
            let target = match (h.lines, h.symbol, h.find) {
                (Some(l), None, None) => {
                    let (a, b) = crate::read_cmd::parse_line_spec(&l)
                        .map_err(|e| format!("hunk {n}: {e}"))?;
                    Target::Lines(a, b)
                }
                (None, Some(s), None) => Target::Symbol(s),
                (None, None, Some(text)) => Target::Find {
                    text,
                    all: h.all,
                    count: h.count,
                },
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

/// Everything up to the write: the response (diff, revs) and, when there is a
/// change to make.
fn plan_edit(o: &EditOptions) -> (EditResponse, Option<Change>) {
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
        return (fail(r, Status::InvalidQuery, "no edit given".into()), None);
    }
    if !o.path.is_file() {
        return (
            fail(
                r,
                Status::InvalidQuery,
                format!("file not found: `{}`", o.path.display()),
            ),
            None,
        );
    }
    if crate::adapters::is_adapted_path(&o.path, false) {
        return (fail(
            r,
            Status::Unsupported,
            "documents and archives are read-only: their text is extracted, so there is no byte range to write back".into(),
        ), None);
    }
    let bytes = match edit::read_shared(&o.path) {
        Ok(b) => b,
        Err(e) => {
            return (
                fail(r, Status::InvalidQuery, format!("read error: {e}")),
                None,
            )
        }
    };
    if crate::content::is_binary(&bytes) {
        return (fail(r, Status::Unsupported, "binary file".into()), None);
    }
    let src = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            return (
                fail(r, Status::Unsupported, "file is not valid UTF-8".into()),
                None,
            )
        }
    };
    let rev = edit::rev(src.as_bytes());
    r.rev = Some(rev.clone());
    if let Some(want) = &o.rev {
        if *want != rev {
            return (
                fail(
                    r,
                    Status::Conflict,
                    format!(
                        "file changed since rev {want} (now {rev}); read it again before editing"
                    ),
                ),
                None,
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
            Target::Find { text, all, count } => {
                match find_edits(&src, &starts, text, *all, *count, &h.op, eol) {
                    Ok(found) => {
                        for (span, e) in found {
                            r.hunks.push(span);
                            edits.push((i + 1, e));
                        }
                        continue;
                    }
                    Err(msg) => {
                        return (fail(r, Status::InvalidQuery, format!("{tag}{msg}")), None)
                    }
                }
            }
            Target::Lines(a, b) => (*a, b.unwrap_or(lines.len())),
            Target::Symbol(name) => {
                match find_symbol_lines(&o.path, src.as_bytes(), &lines, name) {
                    Ok((s, e, _)) => (s, e),
                    Err(msg) => {
                        return (fail(r, Status::InvalidQuery, format!("{tag}{msg}")), None)
                    }
                }
            }
        };
        if a == 0 || a > b || b > lines.len() {
            return (
                fail(
                    r,
                    Status::InvalidQuery,
                    format!(
                        "{tag}line range {a}:{b} is outside the file (1:{})",
                        lines.len()
                    ),
                ),
                None,
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
            return (
                fail(
                    r,
                    Status::InvalidQuery,
                    format!("hunks {x} and {y} overlap; merge them into one hunk"),
                ),
                None,
            );
        }
    }

    let edits: Vec<Edit> = edits.into_iter().map(|(_, e)| e).collect();
    let new_src = edit::plan(&src, &edits).new_src;
    r.diff = edit::unified_diff(&r.path, &src, &new_src);
    if new_src == src {
        r.status = Status::Empty;
        r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        return (r, None);
    }
    if !o.allow_syntax_errors {
        let before = crate::code::syntax_errors(&o.path, src.as_bytes());
        let after = crate::code::syntax_errors(&o.path, new_src.as_bytes());
        if let (Some(b0), Some(b1)) = (before, after) {
            // Only a net increase is refused, so a file that is already broken can
            // still be repaired step by step.
            if b1 > b0 {
                return (fail(
                    r,
                    Status::InvalidQuery,
                    format!(
                        "edit would add {} syntax error(s) ({b0} -> {b1}); nothing written (pass --allow-syntax-errors to write anyway)",
                        b1 - b0
                    ),
                ), None);
            }
        }
    }

    r.rev_after = Some(edit::rev(new_src.as_bytes()));
    r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    (
        r,
        Some(Change {
            rev: Some(rev),
            old: src,
            new: new_src,
        }),
    )
}

pub fn edit_file(o: &EditOptions) -> EditResponse {
    let t0 = Instant::now();
    let (mut r, change) = plan_edit(o);
    if let (true, Some(change)) = (o.apply, change) {
        apply_or_fail(&mut r, &o.path, &change);
    }
    r.elapsed_ms += t0.elapsed().as_secs_f64() * 1000.0;
    r
}

/// A verified change to one file: what to write, and what it must still be.
/// `rev: None` means the file must not exist yet (a creation).
#[derive(Clone, Debug)]
pub struct Change {
    pub rev: Option<String>,
    pub old: String,
    pub new: String,
}

/// Write `c`: a locked compare-and-swap on an existing file, or an exclusive
/// create (someone creating it first is a conflict, not an overwrite).
fn write_change(path: &std::path::Path, c: &Change) -> Result<(), (Status, String)> {
    match &c.rev {
        Some(rev) => edit::write_if_rev(path, rev, c.new.as_bytes()).map_err(|e| match e {
            edit::WriteError::Conflict { now } => (
                Status::Conflict,
                format!("file changed while editing (now rev {now}); nothing written"),
            ),
            edit::WriteError::Io(e) => (Status::InvalidQuery, format!("write failed: {e}")),
        }),
        None => {
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir)
                    .map_err(|e| (Status::InvalidQuery, format!("write failed: {e}")))?;
            }
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .and_then(|mut f| {
                    std::io::Write::write_all(&mut f, c.new.as_bytes())?;
                    f.sync_all()
                })
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        (
                            Status::Conflict,
                            "the file was created by someone else meanwhile; nothing written"
                                .into(),
                        )
                    } else {
                        (Status::InvalidQuery, format!("write failed: {e}"))
                    }
                })
        }
    }
}

/// Undo a written change (best effort): put the old content back, or remove a created file.
fn undo_change(path: &std::path::Path, c: &Change) -> bool {
    match &c.rev {
        Some(_) => edit::write_if_rev(path, &edit::rev(c.new.as_bytes()), c.old.as_bytes()).is_ok(),
        None => std::fs::remove_file(path).is_ok(),
    }
}

fn apply_or_fail(r: &mut EditResponse, path: &std::path::Path, c: &Change) {
    match write_change(path, c) {
        Ok(()) => {
            r.applied = true;
            r.mode = "applied";
        }
        Err((status, message)) => {
            r.status = status;
            r.rev_after = None;
            r.error = Some(ErrorInfo { message, pos: None });
        }
    }
}

/// `lancet write`: the whole content of a file. A new file is created; an existing
/// one is replaced only at the given `rev` (the file must have been read first).
#[derive(Clone, Debug)]
pub struct WriteOptions {
    pub path: PathBuf,
    pub content: String,
    pub rev: Option<String>,
    pub apply: bool,
    pub allow_syntax_errors: bool,
}

pub fn write_file(o: &WriteOptions) -> EditResponse {
    let t0 = Instant::now();
    let (mut r, change) = plan_write(&o.path, &o.content, o.rev.as_deref(), o.allow_syntax_errors);
    if let (true, Some(c)) = (o.apply, change) {
        apply_or_fail(&mut r, &o.path, &c);
    }
    r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    r
}

fn plan_write(
    path: &std::path::Path,
    content: &str,
    want_rev: Option<&str>,
    allow_syntax_errors: bool,
) -> (EditResponse, Option<Change>) {
    let mut r = EditResponse {
        status: Status::Complete,
        mode: "dry-run",
        path: path.to_string_lossy().into_owned(),
        rev: None,
        rev_after: None,
        start_line: 1,
        end_line: content.lines().count().max(1),
        hunks: Vec::new(),
        applied: false,
        diff: String::new(),
        error: None,
        elapsed_ms: 0.0,
    };
    let fail = |mut r: EditResponse, status: Status, message: String| {
        r.status = status;
        r.error = Some(ErrorInfo { message, pos: None });
        (r, None)
    };
    if crate::adapters::is_adapted_path(path, false) {
        return fail(
            r,
            Status::Unsupported,
            "documents and archives are read-only".into(),
        );
    }
    let old = if path.exists() {
        if !path.is_file() {
            return fail(
                r,
                Status::InvalidQuery,
                format!("`{}` is not a file", path.display()),
            );
        }
        let bytes = match edit::read_shared(path) {
            Ok(b) => b,
            Err(e) => return fail(r, Status::InvalidQuery, format!("read error: {e}")),
        };
        let Ok(old) = String::from_utf8(bytes) else {
            return fail(
                r,
                Status::Unsupported,
                "the existing file is not valid UTF-8".into(),
            );
        };
        let rev = edit::rev(old.as_bytes());
        r.rev = Some(rev.clone());
        match want_rev {
            None => {
                return fail(
                    r,
                    Status::Conflict,
                    format!(
                        "`{}` already exists (rev {rev}); read it and pass --rev {rev} to replace it, or change part of it with `lancet edit`",
                        path.display()
                    ),
                )
            }
            Some(w) if w != rev => {
                return fail(
                    r,
                    Status::Conflict,
                    format!("file changed since rev {w} (now {rev}); read it again before writing"),
                )
            }
            _ => Some(old),
        }
    } else {
        if let Some(w) = want_rev {
            return fail(
                r,
                Status::Conflict,
                format!("`{}` doesn't exist (expected rev {w})", path.display()),
            );
        }
        None
    };
    let before = old.as_deref().unwrap_or("");
    if before == content {
        r.status = Status::Empty;
        return (r, None);
    }
    if !allow_syntax_errors {
        let b0 = crate::code::syntax_errors(path, before.as_bytes()).unwrap_or(0);
        if let Some(b1) = crate::code::syntax_errors(path, content.as_bytes()) {
            if b1 > b0 {
                return fail(
                    r,
                    Status::InvalidQuery,
                    format!(
                        "the content has {} more syntax error(s) than the file it replaces ({b0} -> {b1}); nothing written (pass --allow-syntax-errors to write anyway)",
                        b1 - b0
                    ),
                );
            }
        }
    }
    r.hunks = vec![[r.start_line, r.end_line]];
    r.diff = edit::unified_diff(&r.path, before, content);
    r.rev_after = Some(edit::rev(content.as_bytes()));
    let rev = r.rev.clone();
    (
        r,
        Some(Change {
            rev,
            old: before.to_string(),
            new: content.to_string(),
        }),
    )
}

/// Edits for every occurrence of `text` (exactly one unless `all`), with the
/// 1-based lines each occurrence spans.
fn find_edits(
    src: &str,
    starts: &[usize],
    text: &str,
    all: bool,
    count: Option<usize>,
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
    if let Some(n) = count {
        if hits.len() != n {
            return Err(format!(
                "expected {n} occurrence(s) of the --find text, found {}; nothing changed",
                hits.len()
            ));
        }
    }
    let all = all || count.is_some();
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

#[derive(Clone, Debug, Serialize)]
pub struct MultiEditResponse {
    pub status: Status,
    /// `dry-run` or `applied`.
    pub mode: &'static str,
    pub applied: bool,
    /// One entry per file, in the order the files first appear in the hunks.
    pub files: Vec<EditResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub elapsed_ms: f64,
}

/// Edit several files as one change: every file is planned and checked (rev,
/// targets, overlaps, syntax) before anything is written; then each is written
/// with a locked compare-and-swap, and if one loses a race the files already
/// written are put back (best effort, as in `lancet patch`).
pub fn edit_files(
    files: &[FileEdits],
    apply: bool,
    allow_syntax_errors: bool,
) -> MultiEditResponse {
    let t0 = Instant::now();
    let mut out = MultiEditResponse {
        status: Status::Complete,
        mode: if apply { "applied" } else { "dry-run" },
        applied: false,
        files: Vec::new(),
        error: None,
        elapsed_ms: 0.0,
    };
    let finish = |mut out: MultiEditResponse| {
        out.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        out
    };
    let mut planned = Vec::with_capacity(files.len());
    for f in files {
        let (r, change) = match &f.content {
            Some(c) => plan_write(&f.path, c, f.rev.as_deref(), allow_syntax_errors),
            None => {
                let mut o = EditOptions::multi(&f.path, f.hunks.clone());
                o.rev = f.rev.clone();
                o.allow_syntax_errors = allow_syntax_errors;
                plan_edit(&o)
            }
        };
        planned.push((f.path.clone(), change));
        out.files.push(r);
    }
    if let Some(bad) = out.files.iter().find(|r| r.error.is_some()) {
        out.status = bad.status;
        out.error = Some(ErrorInfo {
            message: {
                let m = bad.error.as_ref().map(|e| e.message.as_str()).unwrap_or("");
                let tail = if m.contains("nothing written") {
                    ""
                } else {
                    "; nothing written"
                };
                format!("{}: {m}{tail}", bad.path)
            },
            pos: None,
        });
        return finish(out);
    }
    if out.files.iter().all(|r| r.status == Status::Empty) {
        out.status = Status::Empty;
        return finish(out);
    }
    if !apply {
        return finish(out);
    }
    let mut written: Vec<(&PathBuf, &Change)> = Vec::new();
    for (i, (path, change)) in planned.iter().enumerate() {
        let Some(change) = change else {
            continue;
        };
        if let Err((status, why)) = write_change(path, change) {
            out.status = status;
            let mut failed_restore = Vec::new();
            for (p, c) in written.iter().rev() {
                if !undo_change(p, c) {
                    failed_restore.push(p.display().to_string());
                }
            }
            let why = why.trim_end_matches("; nothing written");
            let mut msg = format!("{}: {why}; nothing applied", path.display());
            if !failed_restore.is_empty() {
                msg = format!(
                    "{msg} — but restoring failed for {}",
                    failed_restore.join(", ")
                );
            }
            out.error = Some(ErrorInfo {
                message: msg,
                pos: None,
            });
            for r in &mut out.files {
                r.rev_after = None;
            }
            out.files[i].status = out.status;
            return finish(out);
        }
        written.push((path, change));
        out.files[i].applied = true;
        out.files[i].mode = "applied";
    }
    out.applied = true;
    finish(out)
}
