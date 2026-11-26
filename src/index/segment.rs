//! Zero-copy immutable trigram index segment (`LANCET03`).
//!
//! ```text
//! header (40 B): "LANCET03" ndocs:u32 nkeys:u32 doc_offs_off:u64 keys_off:u64 post_off:u64
//! doc_offs:      (ndocs + 1) x u32 offsets relative to docs_off (docs_off = doc_offs_off + (ndocs + 1) * 4)
//! docs:          per doc (sorted by rel_utf8_bytes): mtime_ns:u64 size:u64 flags:u8 rel_utf8_bytes
//! keys:          nkeys x (key:u32 off:u32) + sentinel_end_off:u32, sorted by key
//! postings:      varint(count) then delta-varint doc ids
//! ```
//!
//! `Segment::open` only validates the 40-byte header — O(1) with zero heap
//! allocations regardless of document count.

use crate::content;
use crate::fold::file_grams_bounded;
use crate::walk::FileEntry;
use anyhow::{bail, Result};
use memmap2::Mmap;
use rayon::prelude::*;
use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

pub const FLAG_BINARY: u8 = 1;
pub const FLAG_ERROR: u8 = 2;
pub const FLAG_OVERSIZE: u8 = 4;
/// Files larger than 64 MiB are marked `FLAG_OVERSIZE` and streamed via mmap at query time.
pub const MAX_INDEX_FILE_BYTES: u64 = 64 << 20;

const MAGIC: &[u8; 8] = b"LANCET03";
const HEADER: usize = 40;
const KEY_ENTRY: usize = 8;

pub struct Segment {
    mmap: Mmap,
    ndocs: usize,
    nkeys: usize,
    doc_offs_off: usize,
    docs_off: usize,
    keys_off: usize,
    post_off: usize,
}

fn put_varint(buf: &mut Vec<u8>, mut v: u32) {
    while v >= 0x80 {
        buf.push((v as u8) | 0x80);
        v >>= 7;
    }
    buf.push(v as u8);
}

fn get_varint(s: &[u8], cur: &mut usize) -> u32 {
    let mut r = 0u32;
    let mut shift = 0;
    loop {
        let b = s[*cur];
        *cur += 1;
        r |= ((b & 0x7f) as u32) << shift;
        if b & 0x80 == 0 {
            return r;
        }
        shift += 7;
    }
}

#[inline]
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

#[inline]
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

/// Trigram keys of a file's folded content using bounded scratch memory.
pub fn file_grams(bytes: &[u8]) -> Vec<u32> {
    // Dedup leaves capacity at ~file length; 2048 of these live per sub-batch.
    let mut g = file_grams_bounded(bytes);
    g.shrink_to_fit();
    g
}

fn write_segment_sections(
    path: &Path,
    ndocs: usize,
    nkeys: u32,
    doc_offs: &[u8],
    docs: &[u8],
    keys: &[u8],
    post: &[u8],
) -> Result<()> {
    let doc_offs_off = HEADER as u64;
    let docs_off = doc_offs_off + doc_offs.len() as u64;
    let keys_off = docs_off + docs.len() as u64;
    let post_off = keys_off + keys.len() as u64;
    // Stream sections straight to disk: a full in-memory copy doubled peak RSS per segment.
    let tmp = path.with_extension("tmp");
    let mut out = std::io::BufWriter::new(File::create(&tmp)?);
    out.write_all(MAGIC)?;
    out.write_all(&(ndocs as u32).to_le_bytes())?;
    out.write_all(&nkeys.to_le_bytes())?;
    out.write_all(&doc_offs_off.to_le_bytes())?;
    out.write_all(&keys_off.to_le_bytes())?;
    out.write_all(&post_off.to_le_bytes())?;
    out.write_all(doc_offs)?;
    out.write_all(docs)?;
    out.write_all(keys)?;
    out.write_all(post)?;
    out.into_inner().map_err(|e| e.into_error())?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Build buffers reused across segments. macOS keeps freed large blocks resident
/// (deferred reclaim), so fresh per-segment buffers made peak RSS grow with corpus size.
#[derive(Default)]
pub struct BuildScratch {
    doc_offs: Vec<u8>,
    docs: Vec<u8>,
    pairs: Vec<u64>,
    keys: Vec<u8>,
    post: Vec<u8>,
}

pub fn build(
    path: &Path,
    batch: &[&FileEntry],
    ocr: bool,
    scratch: &mut BuildScratch,
) -> Result<()> {
    let sorted_owned: Vec<&FileEntry>;
    let entries: &[&FileEntry] = if batch.windows(2).all(|w| w[0].rel <= w[1].rel) {
        batch
    } else {
        let mut v = batch.to_vec();
        v.sort_by(|a, b| a.rel.cmp(&b.rel));
        sorted_owned = v;
        &sorted_owned
    };

    let ndocs = entries.len();
    let BuildScratch {
        doc_offs,
        docs,
        pairs,
        keys,
        post,
    } = scratch;
    doc_offs.clear();
    docs.clear();
    pairs.clear();
    keys.clear();
    post.clear();

    // Process in sub-chunks of 2048 files so per-file gram vectors are freed progressively.
    const SUB_BATCH: usize = 2048;
    let mut base_id = 0usize;
    for sub in entries.chunks(SUB_BATCH) {
        let per: Vec<(u8, Vec<u32>)> = sub
            .par_iter()
            .map(|e| {
                if e.size > MAX_INDEX_FILE_BYTES {
                    return (FLAG_OVERSIZE, Vec::new());
                }
                match content::load_searchable(&e.path, ocr) {
                    Ok(Some(c)) => (0, file_grams(&c)),
                    Ok(None) => (FLAG_BINARY, Vec::new()),
                    Err(_) => (FLAG_ERROR, Vec::new()),
                }
            })
            .collect();

        let add_grams: usize = per.iter().map(|p| p.1.len()).sum();
        pairs.reserve(add_grams);

        for (local_i, (e, (flags, grams))) in sub.iter().zip(per).enumerate() {
            let id = base_id + local_i;
            if docs.len() > u32::MAX as usize {
                bail!("segment docs table exceeded 4 GiB");
            }
            doc_offs.extend_from_slice(&(docs.len() as u32).to_le_bytes());
            docs.extend_from_slice(&e.mtime_ns.to_le_bytes());
            docs.extend_from_slice(&e.size.to_le_bytes());
            docs.push(flags);
            docs.extend_from_slice(e.rel.as_bytes());
            pairs.extend(grams.into_iter().map(|k| ((k as u64) << 32) | id as u64));
        }
        base_id += sub.len();
    }
    if docs.len() > u32::MAX as usize {
        bail!("segment docs table exceeded 4 GiB");
    }
    doc_offs.extend_from_slice(&(docs.len() as u32).to_le_bytes());
    pairs.par_sort_unstable();

    let mut nkeys = 0u32;
    let mut i = 0;
    while i < pairs.len() {
        let key = (pairs[i] >> 32) as u32;
        let mut j = i;
        while j < pairs.len() && (pairs[j] >> 32) as u32 == key {
            j += 1;
        }
        if post.len() > u32::MAX as usize {
            bail!("segment postings buffer exceeded 4 GiB");
        }
        let off = post.len() as u32;
        put_varint(post, (j - i) as u32);
        let mut prev = 0u32;
        for &p in &pairs[i..j] {
            let id = p as u32;
            put_varint(post, id - prev);
            prev = id;
        }
        keys.extend_from_slice(&key.to_le_bytes());
        keys.extend_from_slice(&off.to_le_bytes());
        nkeys += 1;
        i = j;
    }
    if post.len() > u32::MAX as usize {
        bail!("segment postings buffer exceeded 4 GiB");
    }
    keys.extend_from_slice(&(post.len() as u32).to_le_bytes());

    write_segment_sections(path, ndocs, nkeys, doc_offs, docs, keys, post)
}

/// Pure-mmap LSM compaction: merges live documents and posting lists across `inputs`
/// (`(segment, sorted_dead_ids)` ordered oldest-to-newest) without touching workspace files.
pub fn merge(path: &Path, inputs: &[(&Segment, &[u32])]) -> Result<usize> {
    let total_live_est: usize = inputs
        .iter()
        .map(|(s, d)| s.ndocs().saturating_sub(d.len()))
        .sum();
    let mut live_refs: Vec<(&[u8], usize, u32)> = Vec::with_capacity(total_live_est);

    for (in_idx, &(seg, dead)) in inputs.iter().enumerate() {
        for id in 0..seg.ndocs() as u32 {
            if !dead.is_empty() && dead.binary_search(&id).is_ok() {
                continue;
            }
            live_refs.push((seg.doc_rel_bytes(id), in_idx, id));
        }
    }
    // Sort by path ascending, newest segment first on duplicate path.
    live_refs.sort_unstable_by(|a, b| a.0.cmp(b.0).then_with(|| b.1.cmp(&a.1)));
    live_refs.dedup_by(|a, b| a.0 == b.0);

    let ndocs = live_refs.len();
    let mut remap: Vec<Vec<u32>> = inputs
        .iter()
        .map(|(s, _)| vec![u32::MAX; s.ndocs()])
        .collect();
    let mut doc_offs: Vec<u8> = Vec::with_capacity((ndocs + 1) * 4);
    let mut docs: Vec<u8> = Vec::new();

    for (new_id, &(_rel, in_idx, old_id)) in live_refs.iter().enumerate() {
        if docs.len() > u32::MAX as usize {
            bail!("compacted segment docs table exceeded 4 GiB");
        }
        remap[in_idx][old_id as usize] = new_id as u32;
        doc_offs.extend_from_slice(&(docs.len() as u32).to_le_bytes());
        docs.extend_from_slice(inputs[in_idx].0.doc_record(old_id));
    }
    if docs.len() > u32::MAX as usize {
        bail!("compacted segment docs table exceeded 4 GiB");
    }
    doc_offs.extend_from_slice(&(docs.len() as u32).to_le_bytes());
    drop(live_refs);

    // K-way streaming merge over sorted `(key, posting)` tables.
    let mut heap: BinaryHeap<Reverse<(u32, usize, usize)>> =
        BinaryHeap::with_capacity(inputs.len());
    for (in_idx, &(seg, _)) in inputs.iter().enumerate() {
        if seg.nkeys > 0 {
            heap.push(Reverse((seg.key_at(0), in_idx, 0)));
        }
    }

    let mut keys: Vec<u8> = Vec::new();
    let mut post: Vec<u8> = Vec::new();
    let mut nkeys = 0u32;
    let mut merged_ids: Vec<u32> = Vec::new();

    while let Some(&Reverse((cur_key, _, _))) = heap.peek() {
        merged_ids.clear();
        while let Some(&Reverse((k, in_idx, k_idx))) = heap.peek() {
            if k != cur_key {
                break;
            }
            heap.pop();
            let seg = inputs[in_idx].0;
            let range = seg.posting_range_at(k_idx);
            let map_slice = &remap[in_idx];
            let s = &seg.mmap[range.0..range.0 + range.1];
            let mut cur = 0usize;
            let cnt = get_varint(s, &mut cur) as usize;
            let mut old_id = 0u32;
            for _ in 0..cnt {
                old_id += get_varint(s, &mut cur);
                let nid = map_slice[old_id as usize];
                if nid != u32::MAX {
                    merged_ids.push(nid);
                }
            }
            if k_idx + 1 < seg.nkeys {
                heap.push(Reverse((seg.key_at(k_idx + 1), in_idx, k_idx + 1)));
            }
        }

        if !merged_ids.is_empty() {
            merged_ids.sort_unstable();
            merged_ids.dedup();
            if post.len() > u32::MAX as usize {
                bail!("compacted segment postings buffer exceeded 4 GiB");
            }
            let off = post.len() as u32;
            put_varint(&mut post, merged_ids.len() as u32);
            let mut prev = 0u32;
            for &nid in &merged_ids {
                put_varint(&mut post, nid - prev);
                prev = nid;
            }
            keys.extend_from_slice(&cur_key.to_le_bytes());
            keys.extend_from_slice(&off.to_le_bytes());
            nkeys += 1;
        }
    }
    if post.len() > u32::MAX as usize {
        bail!("compacted segment postings buffer exceeded 4 GiB");
    }
    keys.extend_from_slice(&(post.len() as u32).to_le_bytes());

    write_segment_sections(path, ndocs, nkeys, &doc_offs, &docs, &keys, &post)?;
    Ok(ndocs)
}

impl Segment {
    pub fn open(path: &Path) -> Result<Segment> {
        let f = File::open(path)?;
        // SAFETY: segments are immutable once renamed into place.
        let mmap = unsafe { Mmap::map(&f)? };
        if mmap.len() < HEADER || &mmap[..8] != MAGIC {
            bail!(
                "{} is not a lancet v3 segment (run `lancet index --rebuild`)",
                path.display()
            );
        }
        let ndocs = u32_at(&mmap, 8) as usize;
        let nkeys = u32_at(&mmap, 12) as usize;
        let doc_offs_off = u64_at(&mmap, 16) as usize;
        let docs_off = doc_offs_off + (ndocs + 1) * 4;
        let keys_off = u64_at(&mmap, 24) as usize;
        let post_off = u64_at(&mmap, 32) as usize;
        Ok(Segment {
            mmap,
            ndocs,
            nkeys,
            doc_offs_off,
            docs_off,
            keys_off,
            post_off,
        })
    }

    #[inline]
    pub fn ndocs(&self) -> usize {
        self.ndocs
    }

    #[inline]
    fn doc_record(&self, id: u32) -> &[u8] {
        let p = self.doc_offs_off + (id as usize) * 4;
        let start = u32_at(&self.mmap, p) as usize;
        let end = u32_at(&self.mmap, p + 4) as usize;
        &self.mmap[self.docs_off + start..self.docs_off + end]
    }

    #[inline]
    pub fn doc_rel_bytes(&self, id: u32) -> &[u8] {
        &self.doc_record(id)[17..]
    }

    #[inline]
    pub fn doc_rel(&self, id: u32) -> Cow<'_, str> {
        String::from_utf8_lossy(self.doc_rel_bytes(id))
    }

    #[inline]
    pub fn doc_meta(&self, id: u32) -> (u64, u64, u8) {
        let r = self.doc_record(id);
        (u64_at(r, 0), u64_at(r, 8), r[16])
    }

    /// Zero-allocation binary search over the sorted mmap document table.
    pub fn find_doc(&self, target_rel: &[u8]) -> Option<u32> {
        let (mut lo, mut hi) = (0usize, self.ndocs);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.doc_rel_bytes(mid as u32).cmp(target_rel) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid as u32),
            }
        }
        None
    }

    /// Galloping monotonic cursor advance for sorted traversal (O(1) amortized, zero heap alloc).
    pub fn advance_cursor(&self, cur: &mut u32, target_rel: &[u8]) -> Option<u32> {
        let n = self.ndocs as u32;
        let mut pos = *cur;
        if pos >= n {
            return None;
        }
        match self.doc_rel_bytes(pos).cmp(target_rel) {
            std::cmp::Ordering::Equal => return Some(pos),
            std::cmp::Ordering::Greater => return None,
            std::cmp::Ordering::Less => {}
        }
        // Gallop exponentially to bracket `target_rel`
        let mut step = 1u32;
        let mut lo = pos + 1;
        while lo < n && self.doc_rel_bytes(lo) < target_rel {
            pos = lo;
            lo = lo.saturating_add(step).min(n);
            step = step.saturating_mul(2);
        }
        let mut left = pos;
        let mut right = lo;
        while left < right {
            let mid = left + (right - left) / 2;
            if self.doc_rel_bytes(mid) < target_rel {
                left = mid + 1;
            } else {
                right = mid;
            }
        }
        *cur = left;
        if left < n && self.doc_rel_bytes(left) == target_rel {
            Some(left)
        } else {
            None
        }
    }

    #[inline]
    fn key_at(&self, i: usize) -> u32 {
        u32_at(&self.mmap, self.keys_off + i * KEY_ENTRY)
    }

    #[inline]
    fn posting_range_at(&self, idx: usize) -> (usize, usize) {
        let e = self.keys_off + idx * KEY_ENTRY;
        let off = u32_at(&self.mmap, e + 4) as usize;
        let next_off = if idx + 1 < self.nkeys {
            u32_at(&self.mmap, e + 12) as usize
        } else {
            u32_at(&self.mmap, e + 8) as usize
        };
        (self.post_off + off, next_off - off)
    }

    /// (absolute offset, byte length) of the posting list for `key`.
    pub fn posting_range(&self, key: u32) -> Option<(usize, usize)> {
        let (mut lo, mut hi) = (0usize, self.nkeys);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let k = self.key_at(mid);
            if k < key {
                lo = mid + 1;
            } else if k > key {
                hi = mid;
            } else {
                return Some(self.posting_range_at(mid));
            }
        }
        None
    }

    /// Read only the document count varint from a posting list in O(1).
    #[inline]
    pub fn peek_count(&self, (off, _len): (usize, usize)) -> usize {
        let mut cur = 0;
        get_varint(&self.mmap[off..], &mut cur) as usize
    }

    pub fn decode(&self, (off, len): (usize, usize)) -> Vec<u32> {
        let s = &self.mmap[off..off + len];
        let mut cur = 0;
        let n = get_varint(s, &mut cur) as usize;
        let mut out = Vec::with_capacity(n);
        let mut id = 0u32;
        for _ in 0..n {
            id += get_varint(s, &mut cur);
            out.push(id);
        }
        out
    }

    pub fn disk_bytes(&self) -> usize {
        self.mmap.len()
    }
}
