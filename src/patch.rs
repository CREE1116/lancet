//! `lancet patch`: apply a unified diff (`git diff` / `diff -u` output) with the
//! same guarantees as `lancet edit`.
//!
//! Every hunk's context and removed lines must match the file exactly — at the
//! stated line, or at the nearest offset where they match uniquely (like `patch`,
//! but never fuzzy). All files are verified before anything is written; writes are
//! locked compare-and-swaps against the content that was verified, and if one of
//! them loses a race the files already written are restored.

use crate::edit::{self, Edit};
use crate::output::{ErrorInfo, Status};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Hunk {
    old_start: usize,
    /// Lines the file must contain (context and removals), without line endings.
    old: Vec<String>,
    /// Lines that replace them (context and additions).
    new: Vec<String>,
    /// `\ No newline at end of file` after the old / new side.
    old_no_eol: bool,
    new_no_eol: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FilePatch {
    /// `None` for `/dev/null` (file created / deleted).
    old_path: Option<String>,
    new_path: Option<String>,
    hunks: Vec<Hunk>,
    /// Git `new mode` / `new file mode` (unix permission bits).
    new_mode: Option<u32>,
}

/// What a `diff --git` block says before (or instead of) its `---`/`+++` lines.
#[derive(Default)]
struct GitHeader {
    old_path: Option<String>,
    new_path: Option<String>,
    rename_from: Option<String>,
    rename_to: Option<String>,
    new_mode: Option<u32>,
    created: bool,
    deleted: bool,
}

/// `diff --git a/x b/y` → (x, y), honouring `strip`. Paths with spaces are split at
/// ` b/` when present, which is what git writes for the default prefixes.
fn git_paths(rest: &str, strip: usize) -> (Option<String>, Option<String>) {
    let (a, b) = match rest.find(" b/") {
        Some(i) if rest.starts_with("a/") => (&rest[..i], &rest[i + 1..]),
        _ => match rest.split_once(' ') {
            Some(ab) => ab,
            None => return (None, None),
        },
    };
    (strip_prefix(a, strip), strip_prefix(b, strip))
}

fn strip_prefix(p: &str, strip: usize) -> Option<String> {
    let p = p.split('\t').next().unwrap_or(p).trim_end();
    if p == "/dev/null" {
        return None;
    }
    let mut rest = p;
    for _ in 0..strip {
        match rest.find('/') {
            Some(i) => rest = &rest[i + 1..],
            None => break,
        }
    }
    Some(rest.to_string())
}

fn parse_range(s: &str) -> Option<(usize, usize)> {
    let mut it = s.splitn(2, ',');
    let start = it.next()?.parse().ok()?;
    let len = match it.next() {
        Some(l) => l.parse().ok()?,
        None => 1,
    };
    Some((start, len))
}

/// Parse a unified diff. `strip` is `patch -p`: 1 removes `a/` / `b/`.
fn parse(diff: &str, strip: usize) -> Result<Vec<FilePatch>, String> {
    let lines: Vec<&str> = diff.lines().collect();
    let mut files = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let mut git = None;
        if let Some(rest) = lines[i].strip_prefix("diff --git ") {
            let (a, b) = git_paths(rest, strip);
            let mut h = GitHeader {
                old_path: a,
                new_path: b,
                ..Default::default()
            };
            i += 1;
            while i < lines.len()
                && !lines[i].starts_with("diff --git ")
                && !lines[i].starts_with("--- ")
            {
                let l = lines[i];
                let mode = |m: &str| u32::from_str_radix(m.trim(), 8).ok();
                if let Some(x) = l.strip_prefix("rename from ") {
                    h.rename_from = Some(x.to_string());
                } else if let Some(x) = l.strip_prefix("rename to ") {
                    h.rename_to = Some(x.to_string());
                } else if let Some(m) = l.strip_prefix("new file mode ") {
                    (h.created, h.new_mode) = (true, mode(m));
                } else if l.starts_with("deleted file mode ") {
                    h.deleted = true;
                } else if let Some(m) = l.strip_prefix("new mode ") {
                    h.new_mode = mode(m);
                } else if l.starts_with("copy from ") || l.starts_with("copy to ") {
                    return Err(format!("line {}: copies are not supported", i + 1));
                } else if l.starts_with("Binary files ") || l == "GIT binary patch" {
                    return Err(format!(
                        "line {}: binary changes are not supported ({})",
                        i + 1,
                        h.new_path.as_deref().unwrap_or("?")
                    ));
                }
                i += 1;
            }
            if h.rename_from.is_some() {
                h.old_path = h.rename_from.clone();
                h.new_path = h.rename_to.clone();
            }
            if h.created {
                h.old_path = None;
            }
            if h.deleted {
                h.new_path = None;
            }
            if !lines.get(i).is_some_and(|l| l.starts_with("--- ")) {
                // A change with no text hunks: rename, mode change, empty file.
                if h.old_path.is_none() && h.new_path.is_none() {
                    return Err("`diff --git` header names no file".into());
                }
                let fp = FilePatch {
                    old_path: h.old_path,
                    new_path: h.new_path,
                    hunks: Vec::new(),
                    new_mode: h.new_mode,
                };
                if fp.old_path == fp.new_path && fp.new_mode.is_none() {
                    return Err(format!(
                        "{}: `diff --git` block with no change lancet can apply",
                        fp.new_path.as_deref().unwrap_or("?")
                    ));
                }
                files.push(fp);
                continue;
            }
            git = Some(h);
        }
        let Some(old) = lines[i].strip_prefix("--- ") else {
            i += 1; // text before the first file, `index` lines…
            continue;
        };
        let new = lines
            .get(i + 1)
            .and_then(|l| l.strip_prefix("+++ "))
            .ok_or_else(|| format!("line {}: `---` without `+++`", i + 1))?;
        let mut fp = FilePatch {
            old_path: strip_prefix(old, strip),
            new_path: strip_prefix(new, strip),
            hunks: Vec::new(),
            new_mode: git.as_ref().and_then(|g| g.new_mode),
        };
        i += 2;
        while i < lines.len() && lines[i].starts_with("@@ ") {
            let header = lines[i];
            let inner = header
                .strip_prefix("@@ -")
                .and_then(|h| h.split(" @@").next())
                .ok_or_else(|| format!("line {}: bad hunk header `{header}`", i + 1))?;
            let (o, n) = inner
                .split_once(" +")
                .ok_or_else(|| format!("line {}: bad hunk header `{header}`", i + 1))?;
            let (old_start, old_len) =
                parse_range(o).ok_or_else(|| format!("line {}: bad range `{o}`", i + 1))?;
            let (_, new_len) =
                parse_range(n).ok_or_else(|| format!("line {}: bad range `{n}`", i + 1))?;
            let mut h = Hunk {
                old_start,
                old: Vec::new(),
                new: Vec::new(),
                old_no_eol: false,
                new_no_eol: false,
            };
            i += 1;
            let mut last = ' ';
            while i < lines.len() && (h.old.len() < old_len || h.new.len() < new_len) {
                let l = lines[i];
                let (tag, body) = match l.chars().next() {
                    Some(c @ (' ' | '-' | '+')) => (c, &l[1..]),
                    None => (' ', ""), // an empty context line whose space was stripped
                    Some('\\') => {
                        match last {
                            '-' => h.old_no_eol = true,
                            '+' => h.new_no_eol = true,
                            _ => (h.old_no_eol, h.new_no_eol) = (true, true),
                        }
                        i += 1;
                        continue;
                    }
                    Some(_) => {
                        return Err(format!("line {}: unexpected `{l}` inside a hunk", i + 1))
                    }
                };
                if tag != '+' {
                    h.old.push(body.to_string());
                }
                if tag != '-' {
                    h.new.push(body.to_string());
                }
                last = tag;
                i += 1;
            }
            if h.old.len() != old_len || h.new.len() != new_len {
                return Err(format!(
                    "hunk `{header}`: line counts do not match its header"
                ));
            }
            if lines.get(i).is_some_and(|l| l.starts_with('\\')) {
                match last {
                    '-' => h.old_no_eol = true,
                    '+' => h.new_no_eol = true,
                    _ => (h.old_no_eol, h.new_no_eol) = (true, true),
                }
                i += 1;
            }
            fp.hunks.push(h);
        }
        if fp.hunks.is_empty() && fp.old_path == fp.new_path && fp.new_mode.is_none() {
            return Err(format!(
                "{}: `---`/`+++` with no hunks",
                fp.new_path.as_deref().unwrap_or("?")
            ));
        }
        files.push(fp);
    }
    if files.is_empty() {
        return Err("no file changes found in the patch".into());
    }
    Ok(files)
}

#[derive(Clone, Debug, Serialize)]
pub struct PatchFileResult {
    pub path: String,
    /// `modify`, `create`, `delete`, `rename` or `mode`.
    pub action: &'static str,
    /// For `rename`: the path the file had before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub hunks: usize,
    /// Per hunk: lines it was found away from its header's position (0 = exact).
    pub offsets: Vec<isize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev_after: Option<String>,
    pub diff: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PatchResponse {
    pub status: Status,
    pub mode: &'static str,
    pub applied: bool,
    pub files: Vec<PatchFileResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub elapsed_ms: f64,
}

#[derive(Clone, Debug)]
pub struct PatchOptions {
    pub patch: String,
    /// Paths in the patch are relative to this directory.
    pub root: PathBuf,
    /// `patch -p` (default 1: strip `a/` and `b/`).
    pub strip: usize,
    pub apply: bool,
    pub allow_syntax_errors: bool,
}

impl PatchOptions {
    pub fn new(patch: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        PatchOptions {
            patch: patch.into(),
            root: root.into(),
            strip: 1,
            apply: false,
            allow_syntax_errors: false,
        }
    }
}

/// A verified change, ready to write.
struct Planned {
    path: PathBuf,
    rel: String,
    /// Shown to the caller; `None` for the removal half of a rename.
    result: Option<PatchFileResult>,
    /// Content we verified against; `None` for a file to create.
    old: Option<String>,
    /// New content; `None` to delete.
    new: Option<String>,
    /// Permission bits to set after writing, and the ones to restore on undo.
    mode: Option<u32>,
    old_mode: Option<u32>,
}

pub fn apply_patch(o: &PatchOptions) -> PatchResponse {
    let t0 = Instant::now();
    let mut r = PatchResponse {
        status: Status::Complete,
        mode: if o.apply { "applied" } else { "dry-run" },
        applied: false,
        files: Vec::new(),
        error: None,
        elapsed_ms: 0.0,
    };
    let fail = |mut r: PatchResponse, status: Status, msg: String| {
        r.status = status;
        r.error = Some(ErrorInfo {
            message: msg,
            pos: None,
        });
        r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        r
    };
    let files = match parse(&o.patch, o.strip) {
        Ok(f) => f,
        Err(e) => return fail(r, Status::InvalidQuery, format!("invalid patch: {e}")),
    };

    // 1. Verify every file before touching any.
    let mut plans = Vec::with_capacity(files.len());
    for fp in &files {
        match plan_file(o, fp) {
            Ok(p) => plans.extend(p),
            Err((status, msg)) => return fail(r, status, msg),
        }
    }
    // Two changes to one path (or a rename onto a path the patch also touches) would
    // verify against stale content; refuse rather than guess an order.
    let mut seen = std::collections::HashSet::new();
    if let Some(p) = plans.iter().find(|p| !seen.insert(p.rel.clone())) {
        return fail(
            r,
            Status::InvalidQuery,
            format!("invalid patch: {} is changed more than once", p.rel),
        );
    }
    r.files = plans.iter().filter_map(|p| p.result.clone()).collect();
    if !o.apply {
        r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        return r;
    }

    // 2. Write; on a lost race, restore what was already written.
    let mut done: Vec<&Planned> = Vec::new();
    for p in &plans {
        if let Err(msg) = write_one(p) {
            let mut restore_errors = Vec::new();
            for d in done.iter().rev() {
                if let Err(e) = undo_one(d) {
                    restore_errors.push(e);
                }
            }
            let mut msg = format!("{msg}; nothing applied");
            if !restore_errors.is_empty() {
                msg = format!(
                    "{msg} — but restoring failed: {}",
                    restore_errors.join("; ")
                );
            }
            return fail(r, Status::Conflict, msg);
        }
        done.push(p);
    }
    r.applied = true;
    r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    r
}

fn mode_of(path: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .ok()
            .map(|m| m.permissions().mode() & 0o7777)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

fn set_mode(path: &Path, mode: Option<u32>) -> std::io::Result<()> {
    #[cfg(unix)]
    if let Some(m) = mode {
        use std::os::unix::fs::PermissionsExt;
        return fs::set_permissions(path, fs::Permissions::from_mode(m & 0o7777));
    }
    // Windows has no executable bit; git there ignores mode changes too.
    let _ = (path, mode);
    Ok(())
}

fn plan_file(o: &PatchOptions, fp: &FilePatch) -> Result<Vec<Planned>, (Status, String)> {
    let invalid = |m: String| (Status::InvalidQuery, m);
    let check = |rel: &String| -> Result<PathBuf, (Status, String)> {
        if Path::new(rel).is_absolute() || rel.split('/').any(|c| c == "..") {
            return Err(invalid(format!("{rel}: paths must stay inside the root")));
        }
        let path = o.root.join(rel);
        if crate::adapters::is_adapted_path(&path, false) {
            return Err((
                Status::Unsupported,
                format!("{rel}: documents and archives are read-only"),
            ));
        }
        Ok(path)
    };
    let new_target = fp.new_path.as_ref().map(&check).transpose()?;
    let old_target = fp.old_path.as_ref().map(&check).transpose()?;
    // Git spells a regular file's mode in full (100755); keep the permission bits.
    let want_mode = fp.new_mode.map(|m| m & 0o7777);

    let Some(path) = old_target else {
        // Creation: the patch is the whole file (an empty file has no hunk).
        let (rel, path) = (fp.new_path.clone().unwrap(), new_target.unwrap());
        if path.exists() {
            return Err((Status::Conflict, format!("{rel}: already exists")));
        }
        if fp.hunks.len() > 1 {
            return Err(invalid(format!("{rel}: a new file must be one hunk")));
        }
        let new = fp.hunks.first().map_or(String::new(), |h| {
            let mut new = h.new.join("\n");
            if !h.new.is_empty() && !h.new_no_eol {
                new.push('\n');
            }
            new
        });
        return Ok(vec![Planned {
            result: Some(PatchFileResult {
                path: rel.clone(),
                action: "create",
                from: None,
                hunks: fp.hunks.len(),
                offsets: vec![0; fp.hunks.len()],
                rev: None,
                rev_after: Some(edit::rev(new.as_bytes())),
                diff: edit::unified_diff(&rel, "", &new),
            }),
            path,
            rel,
            old: None,
            new: Some(new),
            mode: want_mode,
            old_mode: None,
        }]);
    };
    let rel = fp.old_path.clone().unwrap();

    let bytes = edit::read_shared(&path).map_err(|e| invalid(format!("{rel}: {e}")))?;
    if crate::content::is_binary(&bytes) {
        return Err((Status::Unsupported, format!("{rel}: binary file")));
    }
    let src = String::from_utf8(bytes)
        .map_err(|_| (Status::Unsupported, format!("{rel}: not valid UTF-8")))?;
    let rev = edit::rev(src.as_bytes());
    let old_mode = mode_of(&path);
    let lines: Vec<&str> = src.lines().collect();
    let starts: Vec<usize> = std::iter::once(0)
        .chain(memchr::memchr_iter(b'\n', src.as_bytes()).map(|i| i + 1))
        .collect();
    let eol = if src[..starts.get(1).copied().unwrap_or(0)].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let strip_cr = |l: &str| l.strip_suffix('\r').unwrap_or(l).to_string();
    let file_lines: Vec<String> = lines.iter().map(|l| strip_cr(l)).collect();

    let Some(new_path) = new_target else {
        // Deletion: the old side must be the whole file (an empty file has no hunk).
        let whole = match fp.hunks.as_slice() {
            [] => src.is_empty(),
            [h] => h.old == file_lines,
            _ => false,
        };
        if !whole {
            return Err((
                Status::Conflict,
                format!("{rel}: file differs from the patch; not deleted"),
            ));
        }
        return Ok(vec![Planned {
            result: Some(PatchFileResult {
                path: rel.clone(),
                action: "delete",
                from: None,
                hunks: fp.hunks.len(),
                offsets: vec![0; fp.hunks.len()],
                rev: Some(rev),
                rev_after: None,
                diff: edit::unified_diff(&rel, &src, ""),
            }),
            path,
            rel,
            old: Some(src),
            new: None,
            mode: None,
            old_mode,
        }]);
    };
    let new_rel = fp.new_path.clone().unwrap();

    let mut edits = Vec::with_capacity(fp.hunks.len());
    let mut offsets = Vec::with_capacity(fp.hunks.len());
    let mut min_line = 0usize; // hunks apply in order and may not overlap
    for (n, h) in fp.hunks.iter().enumerate() {
        let want = h.old_start.saturating_sub(1);
        let at = match locate(&file_lines, &h.old, want, min_line) {
            Located::At(at) => at,
            Located::Missing => {
                return Err((
                    Status::Conflict,
                    format!(
                        "{rel}: hunk {} (@@ -{} @@) does not match the file; nothing applied",
                        n + 1,
                        h.old_start
                    ),
                ))
            }
            Located::Ambiguous(a, b) => {
                return Err((
                    Status::Conflict,
                    format!(
                        "{rel}: hunk {} (@@ -{} @@) matches equally well at lines {} and {}; nothing applied (regenerate the diff against the current file)",
                        n + 1,
                        h.old_start,
                        a + 1,
                        b + 1
                    ),
                ))
            }
        };
        offsets.push(at as isize - want as isize);
        min_line = at + h.old.len();
        let s = starts.get(at).copied().unwrap_or(src.len());
        let e = starts.get(at + h.old.len()).copied().unwrap_or(src.len());
        let mut rep: String = h.new.iter().map(|l| format!("{l}{eol}")).collect();
        // At the end of the file: the new side's "\ No newline" wins; a marker on the
        // old side only means the patch adds the newline; otherwise keep the file's.
        if !h.new.is_empty() && e == src.len() {
            let keep_eol = if h.new_no_eol {
                false
            } else if h.old_no_eol {
                true
            } else {
                src.ends_with('\n')
            };
            if !keep_eol {
                rep.truncate(rep.len() - eol.len());
            }
        }
        edits.push(Edit {
            start: s,
            end: e,
            replacement: rep,
        });
    }
    let new = edit::plan(&src, &edits).new_src;
    if !o.allow_syntax_errors && !edits.is_empty() {
        if let (Some(b0), Some(b1)) = (
            crate::code::syntax_errors(&path, src.as_bytes()),
            crate::code::syntax_errors(&new_path, new.as_bytes()),
        ) {
            if b1 > b0 {
                return Err(invalid(format!(
                    "{new_rel}: patch would add {} syntax error(s) ({b0} -> {b1}); nothing applied (pass --allow-syntax-errors to write anyway)",
                    b1 - b0
                )));
            }
        }
    }
    let mode_note = match (old_mode, want_mode) {
        (Some(a), Some(b)) if a != b => format!("mode {a:o} -> {b:o} {new_rel}\n"),
        _ => String::new(),
    };

    if new_rel == rel {
        let action = if edits.is_empty() { "mode" } else { "modify" };
        return Ok(vec![Planned {
            result: Some(PatchFileResult {
                path: rel.clone(),
                action,
                from: None,
                hunks: fp.hunks.len(),
                offsets,
                rev: Some(rev),
                rev_after: Some(edit::rev(new.as_bytes())),
                diff: mode_note + &edit::unified_diff(&rel, &src, &new),
            }),
            path,
            rel,
            old: Some(src),
            new: Some(new),
            mode: want_mode,
            old_mode,
        }]);
    }

    // Rename: create the new path (create_new, so an existing file is never
    // overwritten), then remove the old one if it is still what we verified.
    if new_path.exists() {
        return Err((
            Status::Conflict,
            format!("{rel} -> {new_rel}: {new_rel} already exists"),
        ));
    }
    let diff = format!("rename {rel} -> {new_rel}\n{mode_note}")
        + &edit::unified_diff(&new_rel, &src, &new);
    Ok(vec![
        Planned {
            result: Some(PatchFileResult {
                path: new_rel.clone(),
                action: "rename",
                from: Some(rel.clone()),
                hunks: fp.hunks.len(),
                offsets,
                rev: Some(rev.clone()),
                rev_after: Some(edit::rev(new.as_bytes())),
                diff,
            }),
            path: new_path,
            rel: new_rel,
            old: None,
            new: Some(new),
            mode: want_mode.or(old_mode),
            old_mode: None,
        },
        Planned {
            result: None,
            path,
            rel,
            old: Some(src),
            new: None,
            mode: None,
            old_mode,
        },
    ])
}

#[derive(Debug, PartialEq, Eq)]
enum Located {
    At(usize),
    Missing,
    /// Two matches at the same distance from the stated line: refuse, don't guess.
    Ambiguous(usize, usize),
}

/// Where `old` occurs in `file` at or after `min`: the stated position if it
/// matches, else the nearest match — unless the nearest distance has two.
fn locate(file: &[String], old: &[String], want: usize, min: usize) -> Located {
    let fits =
        |at: usize| at >= min && at + old.len() <= file.len() && file[at..at + old.len()] == *old;
    if old.is_empty() {
        // Pure insertion: trust the header position.
        let at = want.max(min);
        return if at <= file.len() {
            Located::At(at)
        } else {
            Located::Missing
        };
    }
    if fits(want) {
        return Located::At(want);
    }
    for d in 1..=file.len() {
        let before = want.checked_sub(d).filter(|&a| fits(a));
        let after = Some(want + d).filter(|&a| fits(a));
        match (before, after) {
            (Some(a), Some(b)) => return Located::Ambiguous(a, b),
            (Some(a), None) | (None, Some(a)) => return Located::At(a),
            (None, None) => {}
        }
    }
    Located::Missing
}

fn write_one(p: &Planned) -> Result<(), String> {
    let rel = &p.rel;
    write_content(p)?;
    if p.new.is_some() && p.mode.is_some() && p.mode != p.old_mode {
        set_mode(&p.path, p.mode).map_err(|e| format!("{rel}: {e}"))?;
    }
    Ok(())
}

fn write_content(p: &Planned) -> Result<(), String> {
    let rel = &p.rel;
    match (&p.old, &p.new) {
        (Some(old), Some(new)) if old == new => Ok(()), // mode-only change
        (Some(old), Some(new)) => {
            match edit::write_if_rev(&p.path, &edit::rev(old.as_bytes()), new.as_bytes()) {
                Ok(()) => Ok(()),
                Err(edit::WriteError::Conflict { .. }) => {
                    Err(format!("{rel} changed while patching"))
                }
                Err(edit::WriteError::Io(e)) => Err(format!("{rel}: {e}")),
            }
        }
        (None, Some(new)) => {
            if let Some(dir) = p.path.parent() {
                fs::create_dir_all(dir).map_err(|e| format!("{rel}: {e}"))?;
            }
            // create_new: someone else creating it first is a conflict, not an overwrite.
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&p.path)
                .and_then(|mut f| {
                    std::io::Write::write_all(&mut f, new.as_bytes())?;
                    f.sync_all()
                })
                .map_err(|e| format!("{rel}: {e}"))
        }
        (Some(old), None) => {
            if fs::read(&p.path)
                .map(|b| b == old.as_bytes())
                .unwrap_or(false)
            {
                fs::remove_file(&p.path).map_err(|e| format!("{rel}: {e}"))
            } else {
                Err(format!("{rel} changed while patching"))
            }
        }
        (None, None) => Ok(()),
    }
}

fn undo_one(p: &Planned) -> Result<(), String> {
    let rel = &p.rel;
    match (&p.old, &p.new) {
        (Some(old), Some(new)) => {
            if old != new {
                edit::write_if_rev(&p.path, &edit::rev(new.as_bytes()), old.as_bytes())
                    .map_err(|_| format!("{rel} could not be restored"))?;
            }
            set_mode(&p.path, p.old_mode).map_err(|e| format!("{rel}: {e}"))
        }
        (None, Some(_)) => fs::remove_file(&p.path).map_err(|e| format!("{rel}: {e}")),
        (Some(old), None) => fs::write(&p.path, old)
            .and_then(|_| set_mode(&p.path, p.old_mode))
            .map_err(|e| format!("{rel}: {e}")),
        (None, None) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_git_diff_with_no_newline_markers() {
        let d = "diff --git a/x.rs b/x.rs\nindex 1..2 100644\n--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+c\n\\ No newline at end of file\n";
        let f = parse(d, 1).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].new_path.as_deref(), Some("x.rs"));
        let h = &f[0].hunks[0];
        assert_eq!(
            (h.old.clone(), h.new.clone()),
            (vec!["a".into(), "b".into()], vec!["a".into(), "c".into()])
        );
        assert!(h.old_no_eol && h.new_no_eol);
    }

    #[test]
    fn locate_prefers_stated_line_then_nearest() {
        let f: Vec<String> = ["x", "a", "b", "y", "a", "b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let old: Vec<String> = vec!["a".into(), "b".into()];
        assert_eq!(locate(&f, &old, 4, 0), Located::At(4));
        assert_eq!(locate(&f, &old, 3, 0), Located::At(4)); // distance 1 after beats 2 before
        assert_eq!(locate(&f, &old, 3, 5), Located::Missing);
        // Equidistant copies: refuse instead of picking one.
        let g: Vec<String> = ["a", "b", "x", "x", "a", "b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(locate(&g, &old, 2, 0), Located::Ambiguous(0, 4));
    }
}
