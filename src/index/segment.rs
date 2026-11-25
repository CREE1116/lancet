//! Zero-copy immutable trigram index segment (`LANCET03`).
//!
//! ```text
//! header (40 B): "LANCET03" ndocs:u32 nkeys:u32 doc_offs_off:u64 keys_off:u64 post_off:u64
//! doc_offs:      (ndocs + 1) x u32 offsets relative to docs_off (docs_off = doc_offs_off + (ndocs + 1) * 4)
//! docs:          per doc: mtime_ns:u64 size:u64 flags:u8 rel_utf8_bytes
//! keys:          nkeys x (key:u32 off:u32) + sentinel_end_off:u32, sorted by key
//! postings:      varint(count) then delta-varint doc ids
//! ```
//!
//! `Segment::open` only validates the 40-byte header — O(1) with zero heap
//! allocations regardless of document count.

use crate::content;
use crate::fold::{fold_bytes, trigrams};
use crate::walk::FileEntry;
use anyhow::{bail, Result};
use memmap2::Mmap;
use rayon::prelude::*;
use std::borrow::Cow;
use std::fs::{self, File};
use std::path::Path;

pub const FLAG_BINARY: u8 = 1;
pub const FLAG_ERROR: u8 = 2;
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

/// Trigram keys of a file's folded content.
pub fn file_grams(bytes: &[u8]) -> Vec<u32> {
    let mut folded = Vec::new();
    fold_bytes(bytes, &mut folded);
    trigrams(&folded)
}

pub fn build(path: &Path, batch: &[&FileEntry], ocr: bool) -> Result<()> {
    let per: Vec<(u8, Vec<u32>)> = batch
        .par_iter()
        .map(|e| match content::load_searchable(&e.path, ocr) {
            Ok(Some(c)) => (0, file_grams(&c)),
            Ok(None) => (FLAG_BINARY, vec![]),
            Err(_) => (FLAG_ERROR, vec![]),
        })
        .collect();

    let ndocs = batch.len();
    let mut doc_offs: Vec<u8> = Vec::with_capacity((ndocs + 1) * 4);
    let mut docs: Vec<u8> = Vec::new();
    let total: usize = per.iter().map(|p| p.1.len()).sum();
    let mut pairs: Vec<u64> = Vec::with_capacity(total);
    for (id, (e, (flags, grams))) in batch.iter().zip(per).enumerate() {
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
    if docs.len() > u32::MAX as usize {
        bail!("segment docs table exceeded 4 GiB");
    }
    doc_offs.extend_from_slice(&(docs.len() as u32).to_le_bytes());
    pairs.par_sort_unstable();

    let mut keys: Vec<u8> = Vec::new();
    let mut post: Vec<u8> = Vec::new();
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
        put_varint(&mut post, (j - i) as u32);
        let mut prev = 0u32;
        for &p in &pairs[i..j] {
            let id = p as u32;
            put_varint(&mut post, id - prev);
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

    let doc_offs_off = HEADER as u64;
    let docs_off = doc_offs_off + doc_offs.len() as u64;
    let keys_off = docs_off + docs.len() as u64;
    let post_off = keys_off + keys.len() as u64;
    let mut out = Vec::with_capacity(post_off as usize + post.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(ndocs as u32).to_le_bytes());
    out.extend_from_slice(&nkeys.to_le_bytes());
    out.extend_from_slice(&doc_offs_off.to_le_bytes());
    out.extend_from_slice(&keys_off.to_le_bytes());
    out.extend_from_slice(&post_off.to_le_bytes());
    out.extend_from_slice(&doc_offs);
    out.extend_from_slice(&docs);
    out.extend_from_slice(&keys);
    out.extend_from_slice(&post);
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &out)?;
    fs::rename(&tmp, path)?;
    Ok(())
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
    pub fn doc_rel(&self, id: u32) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.doc_record(id)[17..])
    }

    #[inline]
    pub fn doc_meta(&self, id: u32) -> (u64, u64, u8) {
        let r = self.doc_record(id);
        (u64_at(r, 0), u64_at(r, 8), r[16])
    }

    #[inline]
    fn key_at(&self, i: usize) -> u32 {
        u32_at(&self.mmap, self.keys_off + i * KEY_ENTRY)
    }

    /// (absolute offset, byte length) of the posting list for `key`.
    pub fn posting_range(&self, key: u32) -> Option<(usize, usize)> {
        let (mut lo, mut hi) = (0usize, self.nkeys);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let k = self.key_at(mid);
            if k < key {
                lo = mid + 1;
            } else if k > key {
                hi = mid;
            } else {
                let e = self.keys_off + mid * KEY_ENTRY;
                let off = u32_at(&self.mmap, e + 4) as usize;
                let next_off = if mid + 1 < self.nkeys {
                    u32_at(&self.mmap, e + 12) as usize
                } else {
                    u32_at(&self.mmap, e + 8) as usize
                };
                return Some((self.post_off + off, next_off - off));
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
