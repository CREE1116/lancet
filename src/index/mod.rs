//! Optional on-disk trigram index.
//!
//! Layout (`<root>/.weft/` by default):
//! - `manifest.json`: format version, segment list, sorted tombstones.
//! - `seg-NNNNNN.wseg`: immutable zero-copy segment (`LANCET03`, mmap'd at query time).
//!
//! Sync is incremental: unchanged files (same mtime + size) are kept, changed
//! and new files go into new segments, old versions are tombstoned.
//! At query time files whose mtime/size differ from the index are scanned
//! directly, so results are never stale.

pub mod req;
pub mod segment;

use crate::walk::{walk, FileEntry, FileFilter, WalkOpts};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub use req::{EvalTrace, Req};
pub use segment::{Segment, FLAG_BINARY, FLAG_ERROR};

pub const DIR_NAME: &str = ".lancet";
const FORMAT_VERSION: u32 = 3;
const MANIFEST: &str = "manifest.json";
/// Max input bytes per new segment (bounds memory during build).
const SEGMENT_INPUT_BYTES: u64 = 192 << 20;

pub fn default_dir(root: &Path) -> PathBuf {
    root.join(DIR_NAME)
}

/// Discover an index for `target`: checks `explicit_dir` (if provided),
/// `<target>/.weft`, and ancestor directories of `target`.
/// Returns `(Index, rel_prefix)` where `rel_prefix` is the `/`-separated
/// path from the index root to `target` (empty if `target` is the index root).
pub fn discover(target: &Path, explicit_dir: Option<&Path>) -> Result<Option<(Index, String)>> {
    if let Some(d) = explicit_dir {
        return Ok(Index::open(d)?.map(|ix| (ix, String::new())));
    }
    let local_dir = default_dir(target);
    if local_dir.join(MANIFEST).exists() {
        if let Some(ix) = Index::open(&local_dir)? {
            return Ok(Some((ix, String::new())));
        }
    }
    let abs_target = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let mut cur = abs_target.parent();
    while let Some(anc) = cur {
        let candidate = default_dir(anc);
        if candidate.join(MANIFEST).exists() {
            if let Some(ix) = Index::open(&candidate)? {
                let prefix = match abs_target.strip_prefix(anc) {
                    Ok(r) if !r.as_os_str().is_empty() => r.to_string_lossy().replace('\\', "/"),
                    _ => String::new(),
                };
                return Ok(Some((ix, prefix)));
            }
        }
        cur = anc.parent();
    }
    Ok(None)
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Manifest {
    pub version: u32,
    pub next_seg: u32,
    pub segments: Vec<SegEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SegEntry {
    pub name: String,
    pub dead: Vec<u32>,
}

pub struct Index {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub segs: Vec<Segment>,
}

impl Index {
    pub fn open(dir: &Path) -> Result<Option<Index>> {
        let mp = dir.join(MANIFEST);
        if !mp.exists() {
            return Ok(None);
        }
        let mut manifest: Manifest = serde_json::from_slice(&fs::read(&mp)?)
            .with_context(|| format!("reading {}", mp.display()))?;
        if manifest.version != FORMAT_VERSION {
            bail!(
                "index format v{} is not supported (expected v{FORMAT_VERSION}); run `lancet index --rebuild`",
                manifest.version
            );
        }
        let mut segs = Vec::with_capacity(manifest.segments.len());
        for s in &mut manifest.segments {
            if !s.dead.is_sorted() {
                s.dead.sort_unstable();
                s.dead.dedup();
            }
            segs.push(Segment::open(&dir.join(&s.name))?);
        }
        Ok(Some(Index {
            dir: dir.to_path_buf(),
            manifest,
            segs,
        }))
    }

    #[inline]
    pub fn is_dead(&self, seg: usize, id: u32) -> bool {
        let d = &self.manifest.segments[seg].dead;
        !d.is_empty() && d.binary_search(&id).is_ok()
    }

    /// Live documents: rel path -> (segment, local id).
    pub fn live_map(&self) -> HashMap<Cow<'_, str>, (usize, u32)> {
        let mut m = HashMap::with_capacity(self.live_count());
        for (si, seg) in self.segs.iter().enumerate() {
            for id in 0..seg.ndocs() as u32 {
                if !self.is_dead(si, id) {
                    m.insert(seg.doc_rel(id), (si, id));
                }
            }
        }
        m
    }

    pub fn live_count(&self) -> usize {
        self.segs
            .iter()
            .enumerate()
            .map(|(i, s)| {
                s.ndocs()
                    .saturating_sub(self.manifest.segments[i].dead.len())
            })
            .sum()
    }

    pub fn dead_count(&self) -> usize {
        self.manifest.segments.iter().map(|s| s.dead.len()).sum()
    }
}

#[derive(Debug, Default, Serialize)]
pub struct SyncReport {
    pub files: usize,
    pub unchanged: usize,
    pub added: usize,
    pub removed: usize,
    pub segments: usize,
    pub bytes_indexed: u64,
    pub elapsed_ms: f64,
}

fn write_manifest(dir: &Path, m: &Manifest) -> Result<()> {
    let tmp = dir.join(format!("{MANIFEST}.tmp"));
    fs::write(&tmp, serde_json::to_vec_pretty(m)?)?;
    fs::rename(&tmp, dir.join(MANIFEST))?;
    Ok(())
}

/// Remove existing index files in `dir` and build a fresh index.
pub fn rebuild(root: &Path, dir: &Path, wopts: &WalkOpts) -> Result<SyncReport> {
    rebuild_with_ocr(root, dir, wopts, false)
}

/// Remove existing index files in `dir` and build a fresh index, optionally running OCR.
pub fn rebuild_with_ocr(
    root: &Path,
    dir: &Path,
    wopts: &WalkOpts,
    ocr: bool,
) -> Result<SyncReport> {
    if dir.exists() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == MANIFEST
                || name == format!("{MANIFEST}.tmp")
                || (name.starts_with("seg-") && (name.ends_with(".wseg") || name.ends_with(".tmp")))
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    sync_with_ocr(root, dir, wopts, ocr)
}

/// Create or incrementally update the index for `root` in `dir`.
pub fn sync(root: &Path, dir: &Path, wopts: &WalkOpts) -> Result<SyncReport> {
    sync_with_ocr(root, dir, wopts, false)
}

/// Create or incrementally update the index for `root` in `dir`, optionally running OCR.
pub fn sync_with_ocr(root: &Path, dir: &Path, wopts: &WalkOpts, ocr: bool) -> Result<SyncReport> {
    let t0 = Instant::now();
    fs::create_dir_all(dir)?;
    let mut manifest = Manifest {
        version: FORMAT_VERSION,
        next_seg: 1,
        segments: vec![],
    };
    let mut seg_docs = Vec::new();
    let mut live: HashMap<String, (usize, u32, u64, u64)> = HashMap::new();
    if let Some(ix) = Index::open(dir)? {
        seg_docs = ix.segs.iter().map(|s| s.ndocs()).collect();
        for (si, seg) in ix.segs.iter().enumerate() {
            for id in 0..seg.ndocs() as u32 {
                if !ix.is_dead(si, id) {
                    let (mt, sz, _) = seg.doc_meta(id);
                    live.insert(seg.doc_rel(id).into_owned(), (si, id, mt, sz));
                }
            }
        }
        manifest = ix.manifest;
    }
    finish_sync(
        root,
        dir,
        wopts,
        ocr,
        manifest,
        seg_docs,
        live,
        t0,
        SyncReport::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_sync(
    root: &Path,
    dir: &Path,
    wopts: &WalkOpts,
    ocr: bool,
    mut manifest: Manifest,
    mut seg_docs: Vec<usize>,
    live: HashMap<String, (usize, u32, u64, u64)>,
    t0: Instant,
    mut rep: SyncReport,
) -> Result<SyncReport> {
    let entries = walk(root, wopts, &FileFilter::all(), true);
    rep.files = entries.len();
    let mut seen: HashSet<&str> = HashSet::with_capacity(entries.len());
    let mut to_add: Vec<&FileEntry> = Vec::new();
    for e in &entries {
        seen.insert(e.rel.as_str());
        match live.get(&e.rel) {
            Some(&(_, _, mt, sz)) if mt == e.mtime_ns && sz == e.size => rep.unchanged += 1,
            Some(&(si, id, _, _)) => {
                manifest.segments[si].dead.push(id);
                to_add.push(e);
            }
            None => to_add.push(e),
        }
    }
    for (rel, &(si, id, _, _)) in &live {
        if !seen.contains(rel.as_str()) {
            manifest.segments[si].dead.push(id);
            rep.removed += 1;
        }
    }
    rep.added = to_add.len();

    let mut i = 0;
    while i < to_add.len() {
        let mut bytes = 0u64;
        let mut j = i;
        while j < to_add.len() && (j == i || bytes + to_add[j].size <= SEGMENT_INPUT_BYTES) {
            bytes += to_add[j].size;
            j += 1;
        }
        let name = format!("seg-{:06}.wseg", manifest.next_seg);
        manifest.next_seg += 1;
        segment::build(&dir.join(&name), &to_add[i..j], ocr)?;
        manifest.segments.push(SegEntry { name, dead: vec![] });
        seg_docs.push(j - i);
        rep.bytes_indexed += bytes;
        i = j;
    }

    // Drop segments whose documents are all dead.
    let mut keep = Vec::new();
    for (s, n) in manifest.segments.drain(..).zip(seg_docs) {
        let mut s = s;
        s.dead.sort_unstable();
        s.dead.dedup();
        if s.dead.len() >= n {
            let _ = fs::remove_file(dir.join(&s.name));
        } else {
            keep.push(s);
        }
    }
    manifest.segments = keep;
    rep.segments = manifest.segments.len();
    write_manifest(dir, &manifest)?;
    rep.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    Ok(rep)
}
