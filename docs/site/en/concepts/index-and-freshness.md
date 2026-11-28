---
nav_title: "Index (LANCET04) & Freshness"
title: "The LANCET04 Trigram LSM Index & Freshness Model"
description: "LANCET04 binary layout, zero-copy mmap, sound trigram lowering, (mtime, size, ctime, inode), racy windows, and compaction."
lede: "How Lancet achieves Scan == Auto without a background daemon — down to the 40-byte header and racy-timestamp guard."
keywords: "lancet04 segment mmap trigram q-gram mtime ctime inode racy window flag_racy compact windows"
source: "https://github.com/CREE1116/lancet/blob/main/docs/why-one-tool.md"
---
Unlike search servers that rely on an asynchronous background watcher (lagging behind rapid agent edits) or serve stale cached lines, Lancet uses a daemonless, immutable **LSM trigram index (`LANCET04`)** designed around one invariant: **`Scan == Auto`**.

![How an indexed query skips 91% of files on Linux v6.12 while staying exact.](diagram:pipeline.svg "Measured on Linux v6.12 (86,218 files) for `"spin_lock"` in `auto` mode: 7,814 opened (9.1%), 7,073 matches (8.2%).")

## Sound Trigram Lowering {#trigram-lowering}

Queries are compiled into required byte-trigram boolean trees (literals, regex HIR branches, AST pattern literals, `refs` identifiers, `refs --rank` glob literals). When no requirement can be derived (for example `\w+`), every file is a candidate. Any file missing those trigrams cannot possibly satisfy the predicate, so skipping it produces **zero false negatives**. Every remaining candidate goes through the exact full-scan evaluator.

## On-Disk LANCET04 Format & Zero-Copy mmap {#segment-format}

Located in `PATH/.lancet` (discovered up ancestor directories automatically unless `--index-dir` is set), consisting of `manifest.json` and immutable zero-copy segments (`seg-NNNNNN.wseg`):

```text title="LANCET04 Binary Layout (src/index/segment.rs)"
header (40 B):  "LANCET04" | ndocs:u32 | nkeys:u32 | doc_offs_off:u64 | keys_off:u64 | post_off:u64
doc_offs:       (ndocs + 1) * u32 offsets relative to docs_off
docs:           per doc (sorted by rel UTF-8): mtime_ns:u64 | size:u64 | ctime_ns:u64 | ino:u64 | flags:u8 | rel_utf8
keys:           nkeys * (key:u32 | off:u32) + sentinel_end_off:u32 (sorted by trigram key)
postings:       varint(count) followed by delta-varint doc IDs
```

`Segment::open` checks only the 40-byte header — **O(1) with zero heap allocations** regardless of corpus size. Per-doc flags track `FLAG_BINARY(1)`, `FLAG_ERROR(2)`, `FLAG_OVERSIZE(4, > 64 MiB — streamed via mmap at query time)`, and `FLAG_RACY(8)`.

## 4-Tuple Metadata + Racy-Timestamp Guard {#freshness}

![Index lifecycle: build, per-query live stat check, incremental segments, and compaction.](diagram:index-lifecycle.svg "Auto mode catches modified and untracked files immediately with zero daemon.")

Why is `(mtime, size)` alone broken? Tools like `cp -p`, `tar`, or same-length replacements can leave both `mtime` and `size` unchanged. On POSIX, Lancet tracks **`(mtime_ns, size, ctime_ns, ino)`** and applies Git's **racy-timestamp window**:

- **Adaptive racy window:** inspects up to 4,096 files for sub-second timestamps; uses `100 ms` on fine-grained filesystems (APFS, ext4, NTFS) and `2 s` on coarse filesystems (HFS+, ext3, FAT). Any file touched inside that window gets `FLAG_RACY` and is never trusted from cache without live verification.

> [!WARNING] Honest Windows Caveat
> Rust's stdlib exposes only `mtime` and `size` stably on Windows (`ctime_ns` and `ino` are `0`). On Windows, edits preserving both byte length and `mtime` require `lancet index` or `--no-index`.

## Modes, Incremental Sync & Compaction {#compaction}

- **`auto` (default):** live metadata walk (~100 ms / 87k files) + stale files scanned live. Always exact.
- **`--trust-index`:** skips metadata walk and answers strictly from `.lancet` (~40 ms on Linux kernel, 2–3 ms no-hit); misses unindexed edits until `lancet index` runs.
- **`--no-index`:** bypasses `.lancet` and scans disk directly.
- **LSM compaction:** incremental `lancet index` updates append a segment and tombstones atomically via `manifest.json.tmp` rename; automatic compaction triggers above `COMPACT_MAX_SEGMENTS = 6` or `COMPACT_MIN_DEAD = 32` dead docs (or manually via `lancet index --compact` / `--rebuild`).
