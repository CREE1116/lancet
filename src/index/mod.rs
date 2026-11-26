//! Optional on-disk trigram index (`LANCET04`) with zero-allocation mmap
//! lookups and tiered LSM segment compaction.
//!
//! Layout (`<root>/.lancet/` by default):
//! - `manifest.json`: format version, segment list, sorted tombstones.
//! - `seg-NNNNNN.wseg`: immutable zero-copy segment (`LANCET04`, mmap'd at query time).

pub mod req;
pub mod segment;

use crate::walk::{walk, FileEntry, FileFilter, WalkOpts};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub use req::{EvalTrace, Req};
pub use segment::{
    DocMeta, Segment, FLAG_BINARY, FLAG_ERROR, FLAG_OVERSIZE, FLAG_RACY, MAX_INDEX_FILE_BYTES,
};

pub const DIR_NAME: &str = ".lancet";
const FORMAT_VERSION: u32 = 4;
const MANIFEST: &str = "manifest.json";
/// Max input bytes per new segment (bounds memory during build).
const SEGMENT_INPUT_BYTES: u64 = 96 << 20;
/// Max documents per segment so individual segment posting offsets stay compact.
const MAX_DOCS_PER_SEGMENT: usize = 65_536;
/// Trigger tiered LSM compaction when segment count exceeds this bound.
const COMPACT_MAX_SEGMENTS: usize = 6;
/// Trigger tombstone compaction when >= 25% of docs are dead and dead >= 32.
const COMPACT_MIN_DEAD: usize = 32;

pub fn default_dir(root: &Path) -> PathBuf {
    root.join(DIR_NAME)
}

/// Discover an index for `target`: checks `explicit_dir` (if provided),
/// `<target>/.lancet`, and ancestor directories of `target`.
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

    /// Zero-allocation binary search across segments (newest first).
    pub fn lookup_live(&self, rel: &[u8]) -> Option<(usize, u32)> {
        for si in (0..self.segs.len()).rev() {
            if let Some(id) = self.segs[si].find_doc(rel) {
                if !self.is_dead(si, id) {
                    return Some((si, id));
                }
            }
        }
        None
    }

    /// Zero-allocation monotonic cursor lookup across segments for sorted `rel` streams.
    pub fn advance_cursors(&self, cursors: &mut [u32], rel: &[u8]) -> Option<(usize, u32)> {
        let mut found: Option<(usize, u32)> = None;
        for (si, cur) in cursors.iter_mut().enumerate().take(self.segs.len()) {
            if let Some(id) = self.segs[si].advance_cursor(cur, rel) {
                if !self.is_dead(si, id) {
                    found = Some((si, id)); // Newer segment overrides older
                }
            }
        }
        found
    }

    /// Live documents: rel path -> (segment, local id). Retained for compatibility.
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

fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[derive(Debug, Default, Serialize)]
pub struct SyncReport {
    pub files: usize,
    pub unchanged: usize,
    pub added: usize,
    pub removed: usize,
    pub segments: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub compacted: usize,
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
    // Taken before the walk: anything stamped later than this minus the timestamp
    // granularity may still change within the same tick, so it is marked racy.
    let sync_start_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    fs::create_dir_all(dir)?;
    let existing = Index::open(dir)?;
    let had_existing = existing.is_some();

    let entries = walk(root, wopts, &FileFilter::all(), true);
    let racy_cutoff = sync_start_ns.saturating_sub(segment::racy_window_ns(&entries));
    let mut rep = SyncReport {
        files: entries.len(),
        ..SyncReport::default()
    };

    let (mut manifest, mut seg_docs, entry_matched) = if let Some(ix) = existing {
        let mut manifest = ix.manifest;
        let seg_docs: Vec<usize> = ix.segs.iter().map(|s| s.ndocs()).collect();
        let mut matched = vec![false; entries.len()];

        // Sweep segments newest-first using binary search over `entries` (zero HashMap/String allocs).
        for si in (0..ix.segs.len()).rev() {
            let seg = &ix.segs[si];
            let mut hint = 0usize;
            for id in 0..seg.ndocs() as u32 {
                if !manifest.segments[si].dead.is_empty()
                    && manifest.segments[si].dead.binary_search(&id).is_ok()
                {
                    continue;
                }
                let rel_b = seg.doc_rel_bytes(id);
                // Advance from `hint` using binary search on remaining slice since `id` is also sorted!
                match entries[hint..].binary_search_by(|e| e.rel.as_bytes().cmp(rel_b)) {
                    Ok(off) => {
                        let e_idx = hint + off;
                        hint = e_idx;
                        let e = &entries[e_idx];
                        if !matched[e_idx] && seg.doc_meta(id).matches(e) {
                            matched[e_idx] = true;
                            rep.unchanged += 1;
                        } else {
                            manifest.segments[si].dead.push(id);
                        }
                    }
                    Err(ins) => {
                        hint += ins;
                        manifest.segments[si].dead.push(id);
                        rep.removed += 1;
                    }
                }
            }
        }
        (manifest, seg_docs, matched)
    } else {
        (
            Manifest {
                version: FORMAT_VERSION,
                next_seg: 1,
                segments: vec![],
            },
            Vec::new(),
            vec![false; entries.len()],
        )
    };

    let to_add: Vec<&FileEntry> = entries
        .iter()
        .enumerate()
        .filter_map(|(idx, e)| (!entry_matched[idx]).then_some(e))
        .collect();
    rep.added = to_add.len();

    let mut scratch = segment::BuildScratch::default();
    let mut i = 0;
    while i < to_add.len() {
        let mut bytes = 0u64;
        let mut j = i;
        while j < to_add.len()
            && (j - i) < MAX_DOCS_PER_SEGMENT
            && (j == i || bytes + to_add[j].size <= SEGMENT_INPUT_BYTES)
        {
            bytes += to_add[j].size;
            j += 1;
        }
        let name = format!("seg-{:06}.wseg", manifest.next_seg);
        manifest.next_seg += 1;
        segment::build(
            &dir.join(&name),
            &to_add[i..j],
            ocr,
            racy_cutoff,
            &mut scratch,
        )?;
        manifest.segments.push(SegEntry { name, dead: vec![] });
        seg_docs.push(j - i);
        rep.bytes_indexed += bytes;
        i = j;
    }

    // Drop 100%-dead segments first.
    let mut keep = Vec::new();
    let mut keep_docs = Vec::new();
    for (mut s, n) in manifest.segments.drain(..).zip(seg_docs) {
        s.dead.sort_unstable();
        s.dead.dedup();
        if s.dead.len() >= n {
            let _ = fs::remove_file(dir.join(&s.name));
        } else {
            keep.push(s);
            keep_docs.push(n);
        }
    }
    manifest.segments = keep;

    // Tiered LSM compaction: prevent segment proliferation and reclaim tombstones.
    let total_docs: usize = keep_docs.iter().sum();
    let total_dead: usize = manifest.segments.iter().map(|s| s.dead.len()).sum();
    // A fresh build already produced size-bounded segments, so only compact on incremental syncs.
    let should_compact = had_existing
        && (manifest.segments.len() > COMPACT_MAX_SEGMENTS
            || (manifest.segments.len() >= 2
                && total_dead >= COMPACT_MIN_DEAD
                && total_dead * 4 >= total_docs)
            || (manifest.segments.len() == 1
                && total_dead >= COMPACT_MIN_DEAD
                && total_dead * 2 >= total_docs));

    if should_compact {
        rep.compacted = compact_manifest(dir, &mut manifest, false)?;
    }

    rep.segments = manifest.segments.len();
    write_manifest(dir, &manifest)?;
    rep.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    Ok(rep)
}

/// Explicitly compact all fragmented/tombstoned segments in `dir` into minimal clean segment(s).
pub fn compact(dir: &Path) -> Result<usize> {
    let Some(ix) = Index::open(dir)? else {
        return Ok(0);
    };
    let Index {
        manifest: mut manifest_owned,
        segs,
        ..
    } = ix;
    drop(segs);
    let manifest = &mut manifest_owned;
    if manifest.segments.len() <= 1 && manifest.segments.iter().all(|s| s.dead.is_empty()) {
        return Ok(0);
    }
    let merged = compact_manifest(dir, manifest, true)?;
    write_manifest(dir, manifest)?;
    Ok(merged)
}

fn compact_manifest(dir: &Path, manifest: &mut Manifest, force_all: bool) -> Result<usize> {
    let opened: Vec<Segment> = manifest
        .segments
        .iter()
        .map(|s| Segment::open(&dir.join(&s.name)))
        .collect::<Result<_>>()?;

    // Choose eligible segments: in automatic tiered mode, preserve untouched giant segments (>64 MiB & <20% dead).
    let mut merge_mask = vec![false; opened.len()];
    for (idx, seg) in opened.iter().enumerate() {
        let dead_n = manifest.segments[idx].dead.len();
        let high_dead = dead_n * 5 >= seg.ndocs().max(1);
        let small = seg.disk_bytes() < (64 << 20) && seg.ndocs() < MAX_DOCS_PER_SEGMENT;
        if force_all || small || high_dead {
            merge_mask[idx] = true;
        }
    }
    let selected_count = merge_mask.iter().filter(|&&b| b).count();
    if selected_count == 0
        || (selected_count == 1
            && !force_all
            && manifest
                .segments
                .iter()
                .zip(&merge_mask)
                .find(|(_, &m)| m)
                .is_some_and(|(s, _)| s.dead.is_empty()))
    {
        // If > COMPACT_MAX_SEGMENTS of large segments, merge the last two at minimum.
        if manifest.segments.len() > COMPACT_MAX_SEGMENTS && manifest.segments.len() >= 2 {
            let n = merge_mask.len();
            merge_mask[n - 2] = true;
            merge_mask[n - 1] = true;
        } else {
            return Ok(0);
        }
    }

    let mut new_entries: Vec<SegEntry> = Vec::new();
    let mut old_files_to_remove: Vec<PathBuf> = Vec::new();
    let mut compacted_count = 0usize;

    // Merge contiguous runs of selected segments in order to preserve oldest-to-newest semantics!
    let mut i = 0;
    while i < opened.len() {
        if !merge_mask[i] {
            new_entries.push(manifest.segments[i].clone());
            i += 1;
            continue;
        }
        let mut j = i;
        let mut live_acc = 0usize;
        while j < opened.len() && merge_mask[j] {
            let seg_live = opened[j]
                .ndocs()
                .saturating_sub(manifest.segments[j].dead.len());
            if j > i && live_acc + seg_live > MAX_DOCS_PER_SEGMENT * 2 {
                break;
            }
            live_acc += seg_live;
            j += 1;
        }
        if j - i == 1 && manifest.segments[i].dead.is_empty() {
            new_entries.push(manifest.segments[i].clone());
            i = j;
            continue;
        }
        let inputs: Vec<(&Segment, &[u32])> = (i..j)
            .map(|k| (&opened[k], manifest.segments[k].dead.as_slice()))
            .collect();
        let out_name = format!("seg-{:06}.wseg", manifest.next_seg);
        manifest.next_seg += 1;
        let ndocs = segment::merge(&dir.join(&out_name), &inputs)?;
        old_files_to_remove.extend(manifest.segments[i..j].iter().map(|s| dir.join(&s.name)));
        compacted_count += j - i;
        if ndocs > 0 {
            new_entries.push(SegEntry {
                name: out_name,
                dead: Vec::new(),
            });
        } else {
            let _ = fs::remove_file(dir.join(&out_name));
        }
        i = j;
    }

    drop(opened);
    manifest.segments = new_entries;
    write_manifest(dir, manifest)?;
    for p in old_files_to_remove {
        let _ = fs::remove_file(p);
    }
    Ok(compacted_count)
}
