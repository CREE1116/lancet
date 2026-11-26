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
        let Some(old) = lines[i].strip_prefix("--- ") else {
            i += 1; // `diff --git`, `index`, mode lines… are ignored
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
        if fp.hunks.is_empty() && fp.old_path.is_some() && fp.new_path.is_some() {
            return Err(format!(
                "{}: no hunks (renames and mode changes are not supported)",
                fp.new_path.as_deref().unwrap_or("?")
            ));
        }
        if let (Some(a), Some(b)) = (&fp.old_path, &fp.new_path) {
            if a != b {
                return Err(format!("{a} -> {b}: renames are not supported"));
            }
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
    /// `modify`, `create` or `delete`.
    pub action: &'static str,
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
    result: PatchFileResult,
    /// Content we verified against; `None` for a file to create.
    old: Option<String>,
    /// New content; `None` to delete.
    new: Option<String>,
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
            Ok(p) => plans.push(p),
            Err((status, msg)) => return fail(r, status, msg),
        }
    }
    r.files = plans.iter().map(|p| p.result.clone()).collect();
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

fn plan_file(o: &PatchOptions, fp: &FilePatch) -> Result<Planned, (Status, String)> {
    let invalid = |m: String| (Status::InvalidQuery, m);
    let rel = fp
        .new_path
        .clone()
        .or_else(|| fp.old_path.clone())
        .ok_or_else(|| invalid("patch names neither an old nor a new file".into()))?;
    if Path::new(&rel).is_absolute() || rel.split('/').any(|c| c == "..") {
        return Err(invalid(format!("{rel}: paths must stay inside the root")));
    }
    let path = o.root.join(&rel);
    if crate::adapters::is_adapted_path(&path, false) {
        return Err((
            Status::Unsupported,
            format!("{rel}: documents and archives are read-only"),
        ));
    }

    if fp.old_path.is_none() {
        // Creation: the patch is the whole file.
        if path.exists() {
            return Err((Status::Conflict, format!("{rel}: already exists")));
        }
        let h = fp
            .hunks
            .first()
            .ok_or_else(|| invalid(format!("{rel}: empty creation")))?;
        let mut new = h.new.join("\n");
        if !h.new.is_empty() && !h.new_no_eol {
            new.push('\n');
        }
        return Ok(Planned {
            path,
            result: PatchFileResult {
                path: rel.clone(),
                action: "create",
                hunks: fp.hunks.len(),
                offsets: vec![0],
                rev: None,
                rev_after: Some(edit::rev(new.as_bytes())),
                diff: edit::unified_diff(&rel, "", &new),
            },
            old: None,
            new: Some(new),
        });
    }

    let bytes = fs::read(&path).map_err(|e| invalid(format!("{rel}: {e}")))?;
    if crate::content::is_binary(&bytes) {
        return Err((Status::Unsupported, format!("{rel}: binary file")));
    }
    let src = String::from_utf8(bytes)
        .map_err(|_| (Status::Unsupported, format!("{rel}: not valid UTF-8")))?;
    let rev = edit::rev(src.as_bytes());
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

    if fp.new_path.is_none() {
        // Deletion: the old side must be the whole file.
        let h = &fp.hunks[0];
        if fp.hunks.len() != 1 || h.old != file_lines {
            return Err((
                Status::Conflict,
                format!("{rel}: file differs from the patch; not deleted"),
            ));
        }
        return Ok(Planned {
            path,
            result: PatchFileResult {
                path: rel.clone(),
                action: "delete",
                hunks: 1,
                offsets: vec![0],
                rev: Some(rev),
                rev_after: None,
                diff: edit::unified_diff(&rel, &src, ""),
            },
            old: Some(src),
            new: None,
        });
    }

    let mut edits = Vec::with_capacity(fp.hunks.len());
    let mut offsets = Vec::with_capacity(fp.hunks.len());
    let mut min_line = 0usize; // hunks apply in order and may not overlap
    for (n, h) in fp.hunks.iter().enumerate() {
        let want = h.old_start.saturating_sub(1);
        let at = locate(&file_lines, &h.old, want, min_line).ok_or_else(|| {
            (
                Status::Conflict,
                format!(
                    "{rel}: hunk {} (@@ -{} @@) does not match the file; nothing applied",
                    n + 1,
                    h.old_start
                ),
            )
        })?;
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
    if !o.allow_syntax_errors {
        if let (Some(b0), Some(b1)) = (
            crate::code::syntax_errors(&path, src.as_bytes()),
            crate::code::syntax_errors(&path, new.as_bytes()),
        ) {
            if b1 > b0 {
                return Err(invalid(format!(
                    "{rel}: patch would add {} syntax error(s) ({b0} -> {b1}); nothing applied (pass --allow-syntax-errors to write anyway)",
                    b1 - b0
                )));
            }
        }
    }
    Ok(Planned {
        path,
        result: PatchFileResult {
            path: rel.clone(),
            action: "modify",
            hunks: fp.hunks.len(),
            offsets,
            rev: Some(rev),
            rev_after: Some(edit::rev(new.as_bytes())),
            diff: edit::unified_diff(&rel, &src, &new),
        },
        old: Some(src),
        new: Some(new),
    })
}

/// Where `old` occurs in `file` at or after `min`: the stated position if it
/// matches, else the nearest unique-distance match (ties prefer earlier lines).
fn locate(file: &[String], old: &[String], want: usize, min: usize) -> Option<usize> {
    let fits =
        |at: usize| at >= min && at + old.len() <= file.len() && file[at..at + old.len()] == *old;
    if old.is_empty() {
        // Pure insertion: trust the header position.
        return (want.max(min) <= file.len()).then_some(want.max(min));
    }
    if fits(want) {
        return Some(want);
    }
    (1..=file.len()).find_map(|d| {
        let before = want.checked_sub(d).filter(|&a| fits(a));
        let after = Some(want + d).filter(|&a| fits(a));
        before.or(after)
    })
}

fn write_one(p: &Planned) -> Result<(), String> {
    let rel = &p.result.path;
    match (&p.old, &p.new) {
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
                .and_then(|mut f| std::io::Write::write_all(&mut f, new.as_bytes()))
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
    let rel = &p.result.path;
    match (&p.old, &p.new) {
        (Some(old), Some(new)) => {
            edit::write_if_rev(&p.path, &edit::rev(new.as_bytes()), old.as_bytes())
                .map_err(|_| format!("{rel} could not be restored"))
        }
        (None, Some(_)) => fs::remove_file(&p.path).map_err(|e| format!("{rel}: {e}")),
        (Some(old), None) => fs::write(&p.path, old).map_err(|e| format!("{rel}: {e}")),
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
        assert_eq!(locate(&f, &old, 4, 0), Some(4));
        assert_eq!(locate(&f, &old, 3, 0), Some(4)); // distance 1 after beats 2 before
        assert_eq!(locate(&f, &old, 3, 5), None);
    }
}
