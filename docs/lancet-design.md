# Build the index once, share it with every tool — the design of lancet

English · [한국어](lancet-design.ko.md)

> Agents find, read and change files hundreds of times a day. Today, ripgrep, fd, ast-grep, cat, sed and apply_patch each scan the disk from scratch every time. lancet merges these tools into one and **lets every command share one index**, so it gets faster without giving up any accuracy.

This piece explains why lancet is fast (the index, compression, memory), why being fast doesn't make it wrong (conservative filtering and freshness), and why merging the tools is what makes that gain large. Every number is a measurement recorded in the repository (Apple M4 Pro, warm cache).

---

## 1. At a glance

| | Today's tool chain | lancet |
|---|---|---|
| Finding files, text search, structural search, renames | each tool scans the whole disk again | **one** index shared by four commands |
| Rare literal search (29,792 files, 703 MiB) | ripgrep 432 ms | **77.6 ms** (5.6×), `--trust-index` **6.8 ms** (63.5×) |
| Cost of the index | — | 2.9 s to build, **5.9%** of the source (41 MiB), 0.24 s to sync when nothing changed |
| Results | — | **byte-identical to ripgrep** with or without the index (150 parity cases, CI) |
| Searching right after editing files | indexed tools (zoekt) return stale results until reindexed | **fresh immediately**, no daemon |
| Changing several files | half-applied changes, or another agent's edit overwritten | all files or none; conflicts refused |

---

## 2. The index: what it stores, and why

### 2.1 Per file, without positions

lancet's index remembers only **which files contain a given trigram** (three consecutive bytes). It doesn't store which line or byte.

An index that stores positions too (zoekt and others) can answer a query inside the index, but it grows to nearly the size of the source. lancet goes the other way: the index is used **only to narrow down candidate files**, and the final decision is always made by the same regex engine as ripgrep, reading the actual bytes of the file.

- Across 8 codebases the index is **9–30× smaller** than zoekt's, **builds 4–25× faster**, and uses **3.2–5.0× less** peak memory while building.
  - Linux v6.12 (86,680 files): lancet 3.3 s and 134 MB; zoekt 82 s and 2.9 GB.
- Because the final decision is made on the source, results are the same with or without the index. This is not an index that is "fast but sometimes misses".

### 2.2 On-disk format: segments written once and read as they are

The index is a set of immutable segment files (`LANCET04`) plus a table of contents (`manifest.json`).

```text
header (40 B)   "LANCET04" · doc count · key count · section offsets
doc_offs        document record offsets (u32 array)
docs            sorted by path: mtime · size · ctime · inode · flags · path
keys            sorted array of (trigram: u32, offset: u32)
postings        count (varint) + document-number deltas (delta-varint)
```

Three properties follow from this format:

- **Compressed postings.** Document numbers are sorted and only the difference from the previous one is written, as a variable-length integer. The more common a trigram, the smaller the gaps, and they fit in one byte.
  - The index ends up 5.9–23% of the source (crates source: 703 MiB → 41 MiB).
- **Opening costs almost nothing.** Segments are opened with `mmap` and only the 40-byte header is checked. However many documents a segment holds, opening it allocates nothing on the heap.
  - The key table is a sorted fixed-width array, binary-searched in place. There is no deserialization step.
- **Immutable.** A written segment is never modified. Readers need no lock, and several agents can read one index at the same time.
  - In the rare case a query overlaps a sync deleting old segments and can't open the index, that query scans without it, with a warning. It can be slower, never wrong.

### 2.3 LSM: rewrite only what changed

When files change, the big index is not rewritten.

- Only changed files go into a new segment; their old records are just marked dead (tombstones).
- Past 6 segments, segments are merged in tiers (LSM compaction). When 25% or more of the documents are dead (at least 32), those are cleaned up too.
- The table of contents is replaced atomically. A reader during a sync sees either the old table or the new one, whole.

### 2.4 Memory: bounded even on huge repositories

- **Built segment by segment.** One segment holds at most 96 MiB of input and 65,536 documents, so build memory is bounded by the segment size, not the repository size.
  - Reusing build buffers across segments brought the peak memory of a cold Linux build from 1.3 GiB down to about 350 MB.
- **Big files in chunks.** Files over 64 KiB are folded 64 KiB at a time; whenever the collected keys pass 262,144 they are sorted and deduplicated, so one file's memory is bounded too.
- **Files over 64 MiB are not indexed.** They are marked and read directly on every query: nothing is missed, and the index doesn't bloat.
- **`mmap` when searching.** Files of 4 MiB or more are mapped instead of copied.

---

## 3. How queries stay fast

### 3.1 From a regex to a trigram formula

A query is first turned into a logical formula (`Req`) of "trigrams a matching file must contain".

- `ThreadPoolBuilder` → files that have **all** of the word's trigrams (AND).
- `foo|bar` → files with `foo`'s trigrams **or** with `bar`'s (OR).
- Parts of a regex nothing can be derived from with certainty (`\w+`, character classes …) become **no requirement**.

**The rule is conservatism.** At the slightest doubt, "all files" is returned. The index can only remove candidates; it can never drop a matching file. This is checked by differential fuzzing over random corpora × random edits × random queries.

### 3.2 Intersect the rarest first

- Of the trigrams from one literal, up to 12 are intersected, **shortest posting list first**.
  - A posting list's length is in its first varint, so sorting them needs no decoding.
  - Intersecting a dozen short lists takes microseconds inside L2 cache and removes nearly every false candidate.
  - When the intersection becomes empty, it stops right there. A word that doesn't exist barely touches the disk (ripgrep 430 ms → auto 77 ms → trust 5 ms).
- Conversely, if a single trigram is in 85% or more of the documents, the index is skipped altogether: there is nothing to filter out, and decoding the list would be a loss.

### 3.3 Case-insensitive, and still exact

To filter `-i` searches with the index, indexing and querying must fold case **exactly the same way**.

- lancet follows the Unicode simple case folding the regex engine uses for `(?i)`. Characters in the same folding orbit are mapped to one representative before trigrams are made.
- So tricky characters like `ſ` (long s) and `s`, or `K` (the Kelvin sign) and `k`, are never missed by the index.
- Caseless scripts (Hangul, kana, CJK ideographs) take a fast path straight through.

### 3.4 Fresh without a daemon

The chronic disease of indexed tools is **stale results**. Edit 20 files and search right away with zoekt, and the new text is found in 0 files while the deleted text is found in 20.

lancet solves this without a file-watching daemon.

- On every query, each candidate file's `(mtime, size, ctime, inode)` is compared with the index's record. If anything differs, that file is read directly instead of trusting the index.
- **Racy window:** a file changed within the same timestamp tick as the sync could change again in that tick with the same metadata (git's "racy git" problem). Such files are flagged and always checked directly.
- This check takes about 100 ms on 87k files. When even that is too much, `--trust-index` skips it. Trading freshness for speed is a choice the caller should make knowingly, so it is never skipped silently.
- A faster path through macOS FSEvents history was implemented and rejected: events for one path are merged into earlier records, so a change right after a sync could be lost.

---

## 4. Sharing the index multiplies its value

### 4.1 One index, four commands

Building an index costs the same however it's used; the more commands use it, the more it pays.

| Command | Replaces | How it uses the index |
|---|---|---|
| `lancet rg` | ripgrep, Grep | regex → trigram formula; only candidate files are read |
| `lancet fd` | fd, Glob | file listings come straight from the index, no directory walk (`--trust-index` 14 ms vs fd 50 ms) |
| `lancet refs` | finding symbols | candidates are filtered by the identifier's trigrams before any parsing |
| `lancet ast` | ast-grep | candidates are filtered by the pattern's literals before Tree-sitter parsing (`ThreadPoolBuilder::new()`: ast-grep 1,717 ms → 65 ms, trust 14 ms) |

Used separately, ripgrep, fd and ast-grep each scan the disk from the start. Even bolting an index onto each tool gives one index per tool. In lancet, **one index makes all four kinds of query fast.**

### 4.2 It pays for itself quickly

On the crates source tree (29,792 files):

- Building the index: 2.87 s.
- Time saved per rare-literal search: 432 → 77.6 ms, about 0.35 s.
- **About 8 searches pay back the build.** Every `rg`, `fd`, `refs` and `ast` query after that is pure gain.

An agent searches dozens of times per task, and those searches are split across text search, finding files, finding references and structural search — all made faster by the same index. With separate tools, that payback is divided by the number of tools.

### 4.3 What you change is visible at once

After lancet changes a file, the next query's freshness check notices the change and reads that file directly. No reindex command, no daemon. The agent's "change it, then search again" loop is never misled by the index.

### 4.4 Many agents, one index

Segments are immutable and the table of contents is replaced atomically (written to a temporary file, then renamed), so several agents working in one repository read one index at the same time. The index doesn't multiply with the number of agents.

---

## 5. One coordinate system: read and change exactly what you found

Chain tools together and they disagree, because each has its own idea of a "line" and a "file".

- **Documents:** reading the line numbers `rga` found in a `.docx` with `pandoc` matched 0 of 40, because the two tools extract text differently. lancet's search and `read` share one extraction: 40 of 40. It also finds text in `.xlsx` and `.hwpx`, which `rga` doesn't.
- **File scope:** hidden files, `.gitignore` and binary detection differ by tool. Every lancet command uses one walker (ripgrep's `ignore` crate) and one binary rule.
- **Code:** `read --symbol` returns one whole function including its doc comments (median 129–629 B per function). Unlike `cat` (median 0.6–18.7 KB) or a 50-line window (cutting off 1.7–8.3% of functions), it doesn't add round trips.

---

## 6. Fast, and still safe: one write path

The five write commands (`edit`, `write`, `patch`, `rename`, `ast --rewrite`) write through one path only (`src/change.rs`), so the safety rules cannot differ between commands.

- **Compare-and-swap.** A file is written only if the rev `read` returned (an XXH3-64 hash of the file's bytes) still matches. lancet takes an exclusive lock and also checks the inode, so the file can't have been swapped out.
  - In an experiment where 4 agents each added a line to one file 200 times, "read → modify → write the whole file" **lost 600 of 800 edits.** lancet detected 1,199 conflicts, made them retry, and **lost none**.
- **All files or none.** If one file of a multi-file change fails, the files already written are put back. No half-applied refactor and no `.rej` files, unlike `patch -p1`.
- **Syntax check.** A change that adds Tree-sitter errors is refused (`--allow-syntax-errors` overrides).
  - In practice, this stopped a Python rewrite template containing a newline, which broke indentation, from writing 70 files that would no longer parse.
- **File identity kept.** Symlinks and hard links are not broken, and permissions (and owner and group where possible) are kept.

---

## 7. Why lancet

**Nothing to learn.**
`lancet rg` takes ripgrep's flags and output as they are (byte-identical on 150 cases). `edit` takes the `old_string` / `new_string` / `replace_all` of other agents' edit tools, and `patch` takes unified diffs and Codex `apply_patch`. These are formats models already know.

**No loss without the index.**
With `--no-index`, text search runs within a few percent of ripgrep (interleaved: −1% to +5%). The index is pure bonus.

**Agents wander less.**
In an A/B with the same model (big-pickle) on the same 8 tasks, the built-in side got ripgrep and a usage guide of the same length.

| | Correct | Time | Tool calls | Input tokens |
|---|---|---|---|---|
| Built-in tools + rg | 6/6 | 270 s | 105 | 985k |
| lancet | 6/6 | **170 s** | **44** | **725k** |

Finding one function's call sites among same-named methods and local variables went from 20 calls to 4; tracing a value went from 46 to 14. Each task ran once, so there is noise.

**Accuracy is never given up.**
The index only narrows candidates and the decision is made on the source. Freshness is checked every time. Writes refuse conflicts and change all files or none. The only path that can be wrong for the sake of speed is `--trust-index`, and it is on only when the caller names it.

---

## 8. Where lancet doesn't win

Honestly:

- **The first time.** For one or two searches in a repository without an index, it's the same as ripgrep. The gain comes from repetition.
- **Very common patterns.** Short strings in nearly every file (`fn` …) leave the index nothing to filter, so the gain is small (crates source: 508 ms → 441 ms).
- **AST patterns without literals.** A pattern with no fixed text, like `$X == $X`, can't be filtered by the index, so every candidate is parsed.

---

*Sources for the numbers: the README benchmark section, `docs/why-one-tool.md`, `bench/results/`, `bench/agents/`. Measured on Apple M4 Pro, warm cache.*
