//! The single write path for every file-changing command: byte-span edits,
//! overlap resolution, content revisions, unified diffs, and atomic writes.
//!
//! Commands only *compute* [`Edit`]s from their own coordinates (AST matches,
//! line ranges, symbols, reference sites); everything that touches disk is here,
//! so every command gets the same safety guarantees.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;

/// Replace `src[start..end]` with `replacement` (byte offsets on char boundaries).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    pub new_src: String,
    /// Edits actually applied.
    pub applied: usize,
    /// Edits dropped because they overlapped an earlier (or longer) one.
    pub skipped_overlaps: usize,
}

/// Apply non-overlapping edits to `src`. On overlap the edit starting first wins,
/// and among equal starts the longest wins; the rest are counted, never merged.
pub fn plan(src: &str, edits: &[Edit]) -> Planned {
    let mut order: Vec<&Edit> = edits.iter().collect();
    order.sort_by_key(|e| (e.start, std::cmp::Reverse(e.end)));
    let mut chosen: Vec<&Edit> = Vec::with_capacity(order.len());
    let mut last_end = 0usize;
    let mut skipped_overlaps = 0usize;
    for e in order {
        if e.start >= last_end {
            last_end = e.end;
            chosen.push(e);
        } else {
            skipped_overlaps += 1;
        }
    }
    let mut new_src = src.to_string();
    for e in chosen.iter().rev() {
        new_src.replace_range(e.start..e.end, &e.replacement);
    }
    Planned {
        new_src,
        applied: chosen.len(),
        skipped_overlaps,
    }
}

/// Content revision of a file: XXH3-64 of its bytes, as 16 hex digits.
/// Commands report it so a later edit can refuse to touch a file that changed.
/// (Change detection between cooperating writers, not a cryptographic digest.)
pub fn rev(bytes: &[u8]) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(bytes))
}

/// Replace the contents of `path` with `data`, keeping it the *same file*:
/// - a symlink is followed: the link stays, its target gets the new content;
/// - permissions are kept, and owner/group too where the OS allows (Unix);
/// - a file with several hard links is rewritten in place, because renaming a new
///   file over it would silently split the links (the other names keep the old text);
/// - otherwise the content goes to a temp file next to it that is renamed over it,
///   so a reader sees the old or the new file, never a mix.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let real = fs::canonicalize(path)?;
    let path = real.as_path();
    let meta = fs::metadata(path)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("lancet_tmp");
    // Unique per write: concurrent writers (threads included) never share a temp file.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = parent.join(format!(".{name}.lancet-tmp.{}.{seq}", std::process::id()));
    // Written first even for the in-place case, so a full disk fails before the
    // original is touched.
    // Data reaches the disk before the rename publishes it; otherwise a power loss
    // right after the rename can leave an empty or truncated file.
    {
        let mut f = File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    let _ = fs::set_permissions(&tmp, meta.permissions());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = std::os::unix::fs::chown(&tmp, Some(meta.uid()), Some(meta.gid()));
    }
    if hard_links(&meta) > 1 {
        let res = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .and_then(|mut f| {
                f.write_all(data)?;
                f.sync_all()
            });
        let _ = fs::remove_file(&tmp);
        return res;
    }
    for attempt in 0..8 {
        match fs::rename(&tmp, path) {
            Ok(()) => {
                sync_dir(parent);
                return Ok(());
            }
            Err(e)
                if cfg!(windows) && e.kind() == io::ErrorKind::PermissionDenied && attempt < 7 =>
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }
    Ok(())
}

/// Compute a standard unified diff (`--- a/...`, `+++ b/...`) with 3 context lines.
pub fn unified_diff(rel: &str, old: &str, new: &str) -> String {
    let d = line_diff(rel, old, new);
    if d.contains("\n@@ ") || old == new {
        return d;
    }
    // Same lines, different text: only the final newline changed. Show it the way
    // `diff -u` does, or the change would be invisible in a dry run.
    let last = old.lines().last().unwrap_or("");
    let n = old.lines().count().max(1);
    let marker = |text: &str| {
        if text.ends_with('\n') {
            ""
        } else {
            "\\ No newline at end of file\n"
        }
    };
    format!(
        "--- a/{rel}\n+++ b/{rel}\n@@ -{n},1 +{n},1 @@\n-{last}\n{}+{last}\n{}",
        marker(old),
        marker(new)
    )
}

fn line_diff(rel: &str, old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let n = old_lines.len();
    let m = new_lines.len();

    // LCS DP when size is reasonable; fallback to single coarse hunk for huge files.
    let mut ops: Vec<(char, usize, usize)> = Vec::new();
    if n.saturating_mul(m) <= 2_000_000 {
        let mut dp = vec![0u32; (n + 1) * (m + 1)];
        let stride = m + 1;
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                dp[i * stride + j] = if old_lines[i] == new_lines[j] {
                    dp[(i + 1) * stride + (j + 1)] + 1
                } else {
                    dp[(i + 1) * stride + j].max(dp[i * stride + (j + 1)])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if old_lines[i] == new_lines[j] {
                ops.push((' ', i, j));
                i += 1;
                j += 1;
            } else if dp[(i + 1) * stride + j] >= dp[i * stride + (j + 1)] {
                ops.push(('-', i, j));
                i += 1;
            } else {
                ops.push(('+', i, j));
                j += 1;
            }
        }
        while i < n {
            ops.push(('-', i, j));
            i += 1;
        }
        while j < m {
            ops.push(('+', i, j));
            j += 1;
        }
    } else {
        let mut pref = 0;
        while pref < n && pref < m && old_lines[pref] == new_lines[pref] {
            ops.push((' ', pref, pref));
            pref += 1;
        }
        let mut suff = 0;
        while suff + pref < n
            && suff + pref < m
            && old_lines[n - 1 - suff] == new_lines[m - 1 - suff]
        {
            suff += 1;
        }
        for idx in pref..(n - suff) {
            ops.push(('-', idx, pref));
        }
        for idx in pref..(m - suff) {
            ops.push(('+', n - suff, idx));
        }
        for k in 0..suff {
            ops.push((' ', n - suff + k, m - suff + k));
        }
    }

    let ctx = 3usize;
    let change_idxs: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter_map(|(k, &(op, _, _))| (op != ' ').then_some(k))
        .collect();
    if change_idxs.is_empty() {
        return String::new();
    }

    let mut hunks: Vec<(usize, usize)> = Vec::new();
    let mut h_start = change_idxs[0].saturating_sub(ctx);
    let mut h_end = (change_idxs[0] + ctx + 1).min(ops.len());
    for &ci in &change_idxs[1..] {
        let s = ci.saturating_sub(ctx);
        let e = (ci + ctx + 1).min(ops.len());
        if s <= h_end {
            h_end = e;
        } else {
            hunks.push((h_start, h_end));
            h_start = s;
            h_end = e;
        }
    }
    hunks.push((h_start, h_end));

    let mut out = format!("--- a/{rel}\n+++ b/{rel}\n");
    for (s, e) in hunks {
        let slice = &ops[s..e];
        let old_start = slice.first().map_or(0, |&(_, oi, _)| oi) + 1;
        let new_start = slice.first().map_or(0, |&(_, _, nj)| nj) + 1;
        let old_cnt = slice.iter().filter(|&&(op, _, _)| op != '+').count();
        let new_cnt = slice.iter().filter(|&&(op, _, _)| op != '-').count();
        out.push_str(&format!(
            "@@ -{old_start},{old_cnt} +{new_start},{new_cnt} @@\n"
        ));
        for &(op, oi, nj) in slice {
            match op {
                ' ' => {
                    out.push(' ');
                    out.push_str(old_lines[oi]);
                    out.push('\n');
                }
                '-' => {
                    out.push('-');
                    out.push_str(old_lines[oi]);
                    out.push('\n');
                }
                '+' => {
                    out.push('+');
                    out.push_str(new_lines[nj]);
                    out.push('\n');
                }
                _ => {}
            }
        }
    }
    out
}
/// Persist a rename by syncing its directory (Unix; best effort, a failure here
/// does not undo the already-visible write).
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(unix)]
fn hard_links(m: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    m.nlink()
}

#[cfg(not(unix))]
fn hard_links(_: &fs::Metadata) -> u64 {
    1
}

/// Why [`write_if_rev`] did not write.
#[derive(Debug)]
pub enum WriteError {
    /// The file's revision is no longer the expected one; `now` is the current one.
    Conflict {
        now: String,
    },
    Io(io::Error),
}

/// Read `path` while holding a shared lock, retrying transient Windows sharing
/// errors so a concurrent `write_if_rev` serialises cleanly instead of causing
/// `ERROR_LOCK_VIOLATION`.
pub fn read_shared(path: &Path) -> io::Result<Vec<u8>> {
    for attempt in 0..16 {
        let mut f = match File::open(path) {
            Ok(f) => f,
            Err(e)
                if cfg!(windows) && e.kind() == io::ErrorKind::PermissionDenied && attempt < 15 =>
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }
            Err(e) => return Err(e),
        };
        f.lock_shared()?;
        if !same_file(&f, path) {
            continue;
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        return Ok(buf);
    }
    Err(io::Error::other("file kept being replaced while reading"))
}

/// Compare-and-swap write: replace `path` with `data` only if its content revision
/// is still `expect_rev`.
///
/// Lancet writers hold an exclusive lock on the file across check-and-write, so
/// concurrent agents serialise: the first wins, the others get `Conflict` instead
/// of silently overwriting it (no lost updates). Writers that bypass lancet (an
/// editor) are not serialised, but a change they made before the check still
/// yields `Conflict`.
pub fn write_if_rev(path: &Path, expect_rev: &str, data: &[u8]) -> Result<(), WriteError> {
    for attempt in 0..16 {
        let mut f = match File::open(path) {
            Ok(f) => f,
            Err(e)
                if cfg!(windows) && e.kind() == io::ErrorKind::PermissionDenied && attempt < 15 =>
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }
            Err(e) => return Err(WriteError::Io(e)),
        };
        f.lock().map_err(WriteError::Io)?;
        // A writer may have renamed a new file over `path` while we waited.
        if !same_file(&f, path) {
            continue;
        }
        let mut cur = Vec::new();
        f.read_to_end(&mut cur).map_err(WriteError::Io)?;
        let now = rev(&cur);
        if now != expect_rev {
            return Err(WriteError::Conflict { now });
        }
        // `f` (and its lock) lives until after the rename.
        return write_atomic(path, data).map_err(WriteError::Io);
    }
    Err(WriteError::Io(io::Error::other(
        "file kept being replaced by other writers",
    )))
}

#[cfg(unix)]
fn same_file(f: &File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (f.metadata(), fs::metadata(path)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// Windows also lets a writer rename over a file we hold open (Rust opens files
/// with FILE_SHARE_DELETE), so compare volume serial + file index like inodes.
#[cfg(windows)]
fn same_file(f: &File, path: &Path) -> bool {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    #[derive(Default)]
    struct ByHandleFileInformation {
        attributes: u32,
        creation_time: [u32; 2],
        last_access_time: [u32; 2],
        last_write_time: [u32; 2],
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(h: *mut c_void, info: *mut ByHandleFileInformation) -> i32;
    }
    let id = |file: &File| {
        let mut info = ByHandleFileInformation::default();
        // SAFETY: valid open handle and a correctly laid out out-parameter.
        let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
        (ok != 0).then_some((
            info.volume_serial_number,
            info.file_index_high,
            info.file_index_low,
        ))
    };
    match File::open(path) {
        Ok(current) => matches!((id(f), id(&current)), (Some(a), Some(b)) if a == b),
        Err(_) => false,
    }
}

#[cfg(not(any(unix, windows)))]
fn same_file(_: &File, _: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(start: usize, end: usize, r: &str) -> Edit {
        Edit {
            start,
            end,
            replacement: r.into(),
        }
    }

    #[test]
    fn plan_applies_back_to_front_and_drops_overlaps() {
        let p = plan(
            "aaa bbb ccc",
            &[e(8, 11, "C"), e(0, 3, "A"), e(1, 2, "x"), e(4, 7, "B")],
        );
        assert_eq!(p.new_src, "A B C");
        assert_eq!((p.applied, p.skipped_overlaps), (3, 1));
        // equal starts: the longer edit wins
        let p = plan("abcdef", &[e(0, 2, "X"), e(0, 4, "Y")]);
        assert_eq!(p.new_src, "Yef");
    }

    #[test]
    fn rev_is_stable_and_content_sensitive() {
        assert_eq!(rev(b""), "2d06800538d394c2"); // XXH3-64 of the empty input
        assert_ne!(rev(b"fn a() {}"), rev(b"fn a() {} "));
    }
}
