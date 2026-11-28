---
nav_title: "Write Safety & File Identity"
title: "Coordinates, OS File Locking & File Identity Preservation"
description: "7 failure modes fixed, read_shared, write_if_rev exclusive locks, write_atomic temp/fsync, symlinks, and nlink > 1 hard links."
lede: "How src/edit.rs guarantees symlinks, hard links, permissions, ownership, and crash-safe atomic writes."
keywords: "failure modes write_atomic write_if_rev read_shared lock_shared hard link nlink symlink uid gid sync_all"
source: "https://github.com/CREE1116/lancet/blob/main/docs/why-one-tool.md"
---
Why build reading, AST parsing, diffing, and writing into one binary? Because across multi-step CLI chains, failures happen *between* tools — and every Lancet write command (`edit`, `write`, `patch`, `rename`, `ast --rewrite`) reads and writes through the same core, `src/change.rs`, on top of the low-level writers in `src/edit.rs`.

![Seven real-world multi-step failure modes reproduced in bench/failure_modes.py.](diagram:failure-modes.svg "Every row was measured directly against ripgrep, BSD sed/patch, pandoc, rga, and zoekt.")

## 7 Failure Modes Reproduced in Production Tools {#seven-failures}

| Tool Chain | Reproduced Failure | How Lancet Eliminates It |
| --- | --- | --- |
| `rg -n` → 2× `sed -i` | Second edit hits shifted line; exits `0` | `edit --edits` resolves all hunks on original lines |
| 4 agents: read → mutate → write | **600 / 800** updates lost silently | `edit --rev`: **0 / 800** lost; losers get `conflict` |
| `patch -p1` with 1 stale hunk | File A patched, File B rejected: half-applied | `patch` pre-verifies all files + rolls back on race |
| `sed` symbol rename | Mutated **0.3–11.7%** comments & strings | `refs`/`ast` skip them; `rename` proves local scope |
| `rga` → `pandoc` on `.docx` | **0 / 40** hit lines matched in reader | **40 / 40**: search & `read` share one extractor |
| `zoekt` after editing 20 files | 0 new, 20 deleted hits shown immediately | `auto` mode: 20 new, 0 deleted immediately |
| `sed -i` / `perl -pi` | Broke symlinks/hard links, lost mode `0640` | Symlinks, `nlink > 1` hard links, mode & owner preserved |

## One change, all files or none {#all-or-nothing}

Each write command first plans its whole change without touching the disk: it reads every file through one loader (documents, archives, binary and non-UTF-8 files are refused), computes the new contents and the diff, and refuses a change that adds Tree-sitter syntax errors. Only then does `change::commit` write the files, each with the compare-and-swap below. If any file fails, for example because another agent changed it meanwhile, the files already written are put back and the command reports `conflict` with nothing applied.

## Shared Read Locks & Exclusive write_if_rev CAS {#locks-cas}

- **`read_shared`:** opens target, takes OS shared lock (`lock_shared()`), verifies `same_file(&f, path)` (retrying up to 16×5 ms on transient Windows sharing violations if renamed mid-open), and computes `rev = XXH3-64(bytes)` as 16 lowercase hex digits.
- **`write_if_rev`:** opens target, acquires an exclusive OS lock (`f.lock()`), re-reads current bytes + checks `inode`/`rev == expect_rev`, and returns `WriteError::Conflict { now }` (exit `2`) with zero bytes touched on mismatch.

## How write_atomic Preserves File Identity & Crash Safety {#atomic-writer}

Instead of naive temp-rename (which breaks symlinks, splits hard links, and resets ownership), `write_atomic` in `src/edit.rs` follows four rules:

1. **Follows symlinks first:** calls `fs::canonicalize(path)` so the symlink itself stays intact and its target file is updated.
2. **Stages & fsyncs temp file:** writes to unique `.{name}.lancet-tmp.{pid}.{seq}` and calls `f.sync_all()` first (if the disk is full, it fails before touching the original) and copies permissions + Unix `uid`/`gid`.
3. **Preserves multiply hard-linked files:** checks `nlink() > 1`; if true, overwrites the existing inode in-place + `sync_all()` instead of renaming (because renaming would unlink and split hard-linked paths).
4. **Atomic publish + dir sync:** for single-link files, renames temp → target and calls `sync_dir(parent)` so readers see either old or new contents atomically, never a torn write.
