//! Change core: the one place where writing commands read a file for editing,
//! check a result, and write (docs/ARCHITECTURE.md, pattern ④).
//!
//! `edit`, `write`, `patch`, `rename` and `ast --rewrite` plan without touching
//! the disk, building [`FileChange`]s; [`commit`] writes them all or none. So
//! the safety rules (no documents, no binaries, UTF-8 only, compare-and-swap,
//! exclusive create, rollback, syntax check) cannot differ between commands.

use crate::edit;
use crate::output::{Refusal, Status};
use std::fs;
use std::path::{Path, PathBuf};

/// A file's text as an edit sees it, and the revision it was read at.
pub struct Text {
    pub src: String,
    pub rev: String,
}

/// Read `path` for editing, under a shared lock (see [`edit::read_shared`]).
/// Documents and archives are refused (their text is extracted, so there is
/// no byte range to write back), and so are binary and non-UTF-8 files.
pub fn load(path: &Path) -> Result<Text, Refusal> {
    if crate::adapters::is_adapted_path(path, false) {
        return Err(Refusal::new(
            Status::Unsupported,
            "documents and archives are read-only: their text is extracted, so there is no byte range to write back",
        ));
    }
    let bytes = edit::read_shared(path)
        .map_err(|e| Refusal::new(Status::InvalidQuery, format!("read error: {e}")))?;
    if crate::content::is_binary(&bytes) {
        return Err(Refusal::new(Status::Unsupported, "binary file"));
    }
    let src = String::from_utf8(bytes)
        .map_err(|_| Refusal::new(Status::Unsupported, "not valid UTF-8"))?;
    let rev = edit::rev(src.as_bytes());
    Ok(Text { src, rev })
}

/// Byte offset where each line starts: line `n` (1-based, as `read` numbers
/// them) starts at `[n - 1]`.
pub fn line_starts(src: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(memchr::memchr_iter(b'\n', src.as_bytes()).map(|i| i + 1))
        .collect()
}

/// The file's line ending, from its first line, for text an edit inserts.
pub fn eol(src: &str) -> &'static str {
    match src.find('\n') {
        Some(i) if src[..i].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

/// `Some((before, after))` when `new` has more Tree-sitter syntax errors than
/// `old`. Only a net increase counts, so a broken file can be repaired step by
/// step; a language without a grammar is never refused.
pub fn added_syntax_errors(
    old_path: &Path,
    old: &str,
    new_path: &Path,
    new: &str,
) -> Option<(usize, usize)> {
    let b0 = crate::code::syntax_errors(old_path, old.as_bytes())?;
    let b1 = crate::code::syntax_errors(new_path, new.as_bytes())?;
    (b1 > b0).then_some((b0, b1))
}

/// One verified change to one file.
#[derive(Clone, Debug)]
pub struct FileChange {
    pub path: PathBuf,
    /// The path as messages show it.
    pub rel: String,
    /// The content the plan was verified against; `None`: the file must not exist.
    pub old: Option<String>,
    /// The content to write; `None`: delete the file.
    pub new: Option<String>,
    /// Permission bits to set after writing (`None`: leave them), and the ones to restore.
    pub mode: Option<u32>,
    pub old_mode: Option<u32>,
}

impl FileChange {
    /// Replace `old` (what was read) with `new`.
    pub fn update(
        path: impl Into<PathBuf>,
        rel: impl Into<String>,
        old: String,
        new: String,
    ) -> Self {
        FileChange {
            path: path.into(),
            rel: rel.into(),
            old: Some(old),
            new: Some(new),
            mode: None,
            old_mode: None,
        }
    }

    /// Create a file that must not exist yet.
    pub fn create(path: impl Into<PathBuf>, rel: impl Into<String>, new: String) -> Self {
        FileChange {
            path: path.into(),
            rel: rel.into(),
            old: None,
            new: Some(new),
            mode: None,
            old_mode: None,
        }
    }
}

/// Write every change, or none: each is a locked compare-and-swap against the
/// content it was planned from (or an exclusive create, or a delete of exactly
/// that content). If one fails, the ones already written are put back and the
/// refusal names the file; a restore that fails too is named in the message.
pub fn commit(changes: &[FileChange]) -> Result<(), Refusal> {
    let mut seen = std::collections::HashSet::new();
    if let Some(c) = changes.iter().find(|c| !seen.insert(&c.path)) {
        // Two changes to one file would verify against stale content.
        return Err(Refusal::new(
            Status::InvalidQuery,
            format!("{} is changed more than once; nothing written", c.rel),
        ));
    }
    for (i, c) in changes.iter().enumerate() {
        if let Err(r) = write_one(c) {
            let failed: Vec<&str> = changes[..i]
                .iter()
                .rev()
                .filter(|d| !undo_one(d))
                .map(|d| d.rel.as_str())
                .collect();
            let mut message = format!("{}: {}; nothing written", c.rel, r.message);
            if !failed.is_empty() {
                message = format!("{message} — but restoring failed for {}", failed.join(", "));
            }
            return Err(Refusal::new(r.status, message));
        }
    }
    Ok(())
}

fn write_one(c: &FileChange) -> Result<(), Refusal> {
    let io = |e: std::io::Error| Refusal::new(Status::InvalidQuery, format!("write failed: {e}"));
    match (&c.old, &c.new) {
        (Some(old), Some(new)) if old == new => {} // mode-only change
        (Some(old), Some(new)) => {
            edit::write_if_rev(&c.path, &edit::rev(old.as_bytes()), new.as_bytes()).map_err(
                |e| match e {
                    edit::WriteError::Conflict { now } => Refusal::new(
                        Status::Conflict,
                        format!("changed meanwhile (now rev {now})"),
                    ),
                    edit::WriteError::Io(e) => io(e),
                },
            )?;
        }
        (None, Some(new)) => {
            if let Some(dir) = c.path.parent().filter(|d| !d.as_os_str().is_empty()) {
                fs::create_dir_all(dir).map_err(io)?;
            }
            // create_new: someone else creating it first is a conflict, not an overwrite.
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&c.path)
                .and_then(|mut f| {
                    std::io::Write::write_all(&mut f, new.as_bytes())?;
                    f.sync_all()
                })
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        Refusal::new(Status::Conflict, "created by someone else meanwhile")
                    } else {
                        io(e)
                    }
                })?;
        }
        (Some(old), None) => {
            if !fs::read(&c.path).is_ok_and(|b| b == old.as_bytes()) {
                return Err(Refusal::new(Status::Conflict, "changed meanwhile"));
            }
            fs::remove_file(&c.path).map_err(io)?;
        }
        (None, None) => {}
    }
    if c.new.is_some() && c.mode.is_some() && c.mode != c.old_mode {
        set_mode(&c.path, c.mode).map_err(io)?;
    }
    Ok(())
}

/// Put a written change back (best effort); false if that failed.
fn undo_one(c: &FileChange) -> bool {
    match (&c.old, &c.new) {
        (Some(old), Some(new)) => {
            (old == new
                || edit::write_if_rev(&c.path, &edit::rev(new.as_bytes()), old.as_bytes()).is_ok())
                && set_mode(&c.path, c.old_mode).is_ok()
        }
        (None, Some(_)) => fs::remove_file(&c.path).is_ok(),
        (Some(old), None) => {
            fs::write(&c.path, old).is_ok() && set_mode(&c.path, c.old_mode).is_ok()
        }
        (None, None) => true,
    }
}

/// The file's permission bits (Unix), to keep or restore them.
pub fn mode_of(path: &Path) -> Option<u32> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_is_all_or_nothing() {
        let d = tempfile::tempdir().unwrap();
        let (a, b, c) = (d.path().join("a"), d.path().join("b"), d.path().join("c"));
        fs::write(&a, "a1").unwrap();
        fs::write(&b, "b-changed-by-someone").unwrap();
        let set = [
            FileChange::update(&a, "a", "a1".into(), "a2".into()),
            FileChange::create(&c, "c", "new".into()),
            FileChange::update(&b, "b", "b1".into(), "b2".into()),
        ];
        let r = commit(&set).unwrap_err();
        assert_eq!(r.status, Status::Conflict);
        assert!(
            r.message.starts_with("b: changed meanwhile"),
            "{}",
            r.message
        );
        assert!(r.message.contains("nothing written"));
        assert_eq!(fs::read_to_string(&a).unwrap(), "a1");
        assert!(!c.exists());

        fs::write(&b, "b1").unwrap();
        commit(&set).unwrap();
        assert_eq!(fs::read_to_string(&a).unwrap(), "a2");
        assert_eq!(fs::read_to_string(&b).unwrap(), "b2");
        assert_eq!(fs::read_to_string(&c).unwrap(), "new");
    }

    #[test]
    fn commit_refuses_two_changes_to_one_file() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a");
        fs::write(&a, "x").unwrap();
        let set = [
            FileChange::update(&a, "a", "x".into(), "y".into()),
            FileChange::update(&a, "a", "y".into(), "z".into()),
        ];
        assert_eq!(commit(&set).unwrap_err().status, Status::InvalidQuery);
        assert_eq!(fs::read_to_string(&a).unwrap(), "x");
    }

    #[test]
    fn load_refuses_what_cannot_be_written_back() {
        let d = tempfile::tempdir().unwrap();
        let bin = d.path().join("b.dat");
        fs::write(&bin, b"a\0b").unwrap();
        assert_eq!(load(&bin).err().unwrap().status, Status::Unsupported);
        let latin = d.path().join("l.txt");
        fs::write(&latin, b"caf\xe9").unwrap();
        assert_eq!(load(&latin).err().unwrap().status, Status::Unsupported);
        let ok = d.path().join("ok.txt");
        fs::write(&ok, "hi").unwrap();
        assert_eq!(load(&ok).ok().unwrap().rev, edit::rev(b"hi"));
    }

    #[test]
    fn line_model() {
        assert_eq!(line_starts("a\nbc\n"), [0, 2, 5]);
        assert_eq!(line_starts(""), [0]);
        assert_eq!(eol("a\r\nb\n"), "\r\n");
        assert_eq!(eol("a\nb\r\n"), "\n");
        assert_eq!(eol("no newline\r"), "\n");
    }
}
