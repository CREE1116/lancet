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

/// One step of an edit script: keep, delete or insert a line. `old` / `new` are
/// line indexes; for an insert `old` is the next old line, for a delete `new` is
/// the next new line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    Keep,
    Del,
    Ins,
}

/// A unified diff (`--- a/…`, `+++ b/…`, 3 lines of context) exactly as `diff -u`
/// prints it: lines split on `\n` only (a `\r` stays part of its line) and
/// `\ No newline at end of file` after a last line that has no newline.
pub fn unified_diff(rel: &str, old: &str, new: &str) -> String {
    let a: Vec<&str> = old.split_terminator('\n').collect();
    let b: Vec<&str> = new.split_terminator('\n').collect();
    let (n, m) = (a.len(), b.len());
    let a_open = !old.is_empty() && !old.ends_with('\n');
    let b_open = !new.is_empty() && !new.ends_with('\n');
    // A last line without a newline is a different line from the same text with one.
    let eq = |i: usize, j: usize| a[i] == b[j] && (a_open && i + 1 == n) == (b_open && j + 1 == m);
    let ops = edit_script(n, m, &eq);

    const CTX: usize = 3;
    let changes: Vec<usize> = (0..ops.len()).filter(|&k| ops[k].0 != Op::Keep).collect();
    let Some(&first) = changes.first() else {
        return String::new();
    };
    let mut hunks: Vec<(usize, usize)> = Vec::new();
    let (mut hs, mut he) = (first.saturating_sub(CTX), (first + CTX + 1).min(ops.len()));
    for &c in &changes[1..] {
        let (s, e) = (c.saturating_sub(CTX), (c + CTX + 1).min(ops.len()));
        if s <= he {
            he = e;
        } else {
            hunks.push((hs, he));
            (hs, he) = (s, e);
        }
    }
    hunks.push((hs, he));

    let no_newline = "\\ No newline at end of file\n";
    let mut out = format!("--- a/{rel}\n+++ b/{rel}\n");
    for (s, e) in hunks {
        let slice = &ops[s..e];
        let old_cnt = slice.iter().filter(|o| o.0 != Op::Ins).count();
        let new_cnt = slice.iter().filter(|o| o.0 != Op::Del).count();
        // An empty range names the line before it, as `diff -u` does.
        let start = |first: usize, cnt: usize| if cnt == 0 { first } else { first + 1 };
        out.push_str(&format!(
            "@@ -{},{old_cnt} +{},{new_cnt} @@\n",
            start(slice[0].1, old_cnt),
            start(slice[0].2, new_cnt)
        ));
        for &(op, i, j) in slice {
            let (mark, text, open) = match op {
                Op::Keep => (' ', a[i], a_open && i + 1 == n),
                Op::Del => ('-', a[i], a_open && i + 1 == n),
                Op::Ins => ('+', b[j], b_open && j + 1 == m),
            };
            out.push(mark);
            out.push_str(text);
            out.push('\n');
            if open {
                out.push_str(no_newline);
            }
        }
    }
    out
}

/// A shortest edit script from `n` old lines to `m` new lines (Myers' O(ND)
/// algorithm, as `git diff` uses), after trimming the common prefix and suffix.
/// Its cost grows with the size of the change, not of the file.
fn edit_script(n: usize, m: usize, eq: &dyn Fn(usize, usize) -> bool) -> Vec<(Op, usize, usize)> {
    let mut pre = 0;
    while pre < n && pre < m && eq(pre, pre) {
        pre += 1;
    }
    let mut suf = 0;
    while suf + pre < n && suf + pre < m && eq(n - 1 - suf, m - 1 - suf) {
        suf += 1;
    }
    let mut ops: Vec<(Op, usize, usize)> = (0..pre).map(|i| (Op::Keep, i, i)).collect();
    let (an, bm) = (n - pre - suf, m - pre - suf);
    match myers(an, bm, &|i, j| eq(pre + i, pre + j)) {
        Some(mid) => ops.extend(mid.into_iter().map(|(op, i, j)| (op, pre + i, pre + j))),
        // shortcut: past the memory cap (a huge rewrite) the middle is one replacement;
        // still a correct diff, just not the smallest.
        None => {
            ops.extend((0..an).map(|i| (Op::Del, pre + i, pre)));
            ops.extend((0..bm).map(|j| (Op::Ins, pre + an, pre + j)));
        }
    }
    ops.extend((0..suf).map(|k| (Op::Keep, n - suf + k, m - suf + k)));
    ops
}

/// Myers' greedy diff. Step `d` keeps only the furthest-reaching paths of diagonals
/// `-d..=d` for the backtrack, so memory grows with the square of the change, not
/// with the file. `None` past ~4M stored entries (about 2,000 changed lines).
fn myers(n: usize, m: usize, eq: &dyn Fn(usize, usize) -> bool) -> Option<Vec<(Op, usize, usize)>> {
    const MAX_ENTRIES: usize = 4 << 20;
    let max = n + m;
    let off = max as isize + 1;
    let mut v = vec![0isize; 2 * max + 3];
    let at = |k: isize| (k + off) as usize;
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut stored = 0usize;
    for d in 0..=max as isize {
        stored += 2 * d as usize + 1;
        if stored > MAX_ENTRIES {
            return None;
        }
        trace.push(v[at(-d)..=at(d)].to_vec());
        for k in (-d..=d).step_by(2) {
            let mut x = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            let mut y = x - k;
            while (x as usize) < n && (y as usize) < m && eq(x as usize, y as usize) {
                x += 1;
                y += 1;
            }
            v[at(k)] = x;
            if x as usize >= n && y as usize >= m {
                return Some(backtrack(&trace, n, m));
            }
        }
    }
    Some(Vec::new())
}

/// Walks the kept paths back from `(n, m)`. `trace[d][k + d]` is the furthest x on
/// diagonal `k` before step `d`.
fn backtrack(trace: &[Vec<isize>], n: usize, m: usize) -> Vec<(Op, usize, usize)> {
    let (mut x, mut y) = (n as isize, m as isize);
    let mut out = Vec::new();
    for d in (0..trace.len() as isize).rev() {
        let v = |k: isize| trace[d as usize][(k + d) as usize];
        let k = x - y;
        let pk = if k == -d || (k != d && v(k - 1) < v(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let (px, py) = if d == 0 { (0, 0) } else { (v(pk), v(pk) - pk) };
        while x > px && y > py {
            out.push((Op::Keep, (x - 1) as usize, (y - 1) as usize));
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            if x == px {
                out.push((Op::Ins, x as usize, (y - 1) as usize));
            } else {
                out.push((Op::Del, (x - 1) as usize, y as usize));
            }
            (x, y) = (px, py);
        }
    }
    out.reverse();
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

    #[test]
    fn many_changes_in_a_big_file_stay_small_hunks() {
        let old: String = (0..50_000).map(|i| format!("row {i}\n")).collect();
        let mut lines: Vec<String> = old.lines().map(str::to_string).collect();
        for k in 0..300 {
            let at = 100 + k * 160;
            if k % 2 == 0 {
                lines[at] = format!("edited {k}");
            } else {
                lines.insert(at, format!("inserted {k}"));
            }
        }
        let new = lines.join("\n") + "\n";
        let t = std::time::Instant::now();
        let d = unified_diff("f", &old, &new);
        assert!(t.elapsed().as_secs_f64() < 2.0, "{:?}", t.elapsed());
        assert_eq!(d.matches("\n@@ ").count(), 300);
        assert_eq!(
            d.lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                .count(),
            300
        );
    }

    #[test]
    fn big_file_diff_shows_only_changed_lines() {
        let old: String = (0..3000).map(|i| format!("line {i}\n")).collect();
        let new = old
            .replace("line 10\n", "line ten\n")
            .replace("line 2990\n", "line two-nine-nine-zero\n");
        let d = unified_diff("f", &old, &new);
        assert_eq!(d.matches("\n@@ ").count(), 2, "{d}");
        assert_eq!(
            d.lines()
                .filter(|l| l.starts_with('-') || l.starts_with('+'))
                .count(),
            6
        );
        assert!(
            d.contains("@@ -8,7 +8,7 @@\n line 7\n line 8\n line 9\n-line 10\n+line ten\n"),
            "{d}"
        );
        assert!(d.contains("@@ -2988,7 +2988,7 @@\n"), "{d}");
        // Line counts differ and the middle is too big for the LCS: still correct.
        let grown = format!("{new}tail\n").replacen("line 5\n", "", 1);
        let d = unified_diff("f", &old, &grown);
        assert!(d.starts_with("--- a/f\n+++ b/f\n@@ -3,"), "{d}");
    }

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
