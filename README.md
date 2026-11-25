# Lancet

[![CI](https://github.com/CREE1116/lancet/actions/workflows/ci.yml/badge.svg)](https://github.com/CREE1116/lancet/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**Surgical code, AST, and multi-format document search, inspection, and refactoring CLI for AI agents and developers.**

[한국어 문서 (Korean README)](./README.ko.md) · [Query Language Spec](./docs/grammar.md) · [Agent Instructions (`AGENTS.md`)](./docs/agent-instructions.md)

---

## Why Lancet?

AI coding agents and developers routinely juggle four disconnected tools — `rg` for text, `fd` for files, `ast-grep` for syntax-aware refactoring, and external extractors (`pdftotext`, `pandoc`, `unzip`) for binary documents — none of which share an index, line numbering, or token budgets.

**Lancet** unifies the entire **"discover → search → inspect → refactor"** loop into a single deterministic Rust binary backed by an optional zero-copy `.lancet` trigram index:

- **`lancet <QUERY>`** — Boolean (`AND`, `OR`, `NOT`), word-proximity (`near`, `before`), typo-tolerant Levenshtein `fuzzy("term", k)`, multi-line `regex(...)`, and tree-sitter `in function` / `symbol(...)` search with strict token budgeting (`| budget tokens:1500`).
- **`lancet fd`** — `.gitignore`-aware file & folder explorer with `--tree`, size/recency filters, and sub-millisecond mmap `--trust-index` lookup.
- **`lancet ast`** — Tree-sitter structural search and atomic `--rewrite` refactoring (`$VAR`, `$_`, `$$$ARGS`) across Rust, Python, JS, TS, TSX, and Go — accelerated by automatic trigram candidate pruning.
- **`lancet read`** — Surgical reader for exact line ranges, AST symbols (`--symbol`), PDF/PPTX pages (`--page`), spreadsheet slices (`--sheet`, `--rows`, `--cols`), and archive entries (`archive.zip!file.log`), sharing **1-to-1 line parity** with search hits.
- **`lancet outline`** — Instant declaration skeletons for source code, heading hierarchies for Markdown, sheet schemas for Excel/ODS, and entry manifests for ZIP/TAR archives.
- **`lancet refs`** — Index-accelerated lexical AST `[def]` + `[ref]` lookup that automatically ignores comments and string literals.
- **Zero-Stale Guarantee (`Scan == Auto`)** — Every indexed candidate is verified by the exact scan evaluator, and files modified since indexing (`mtime + size`) are re-scanned automatically. Indexed results are **mathematically identical** to a full scan.

---

## Benchmarks (Apple M4 Pro, 12-core, 24 GB RAM)

Reproducible via [`bench/run_bench.py`](./bench/run_bench.py), [`bench/bench_competitors.py`](./bench/bench_competitors.py), and [`bench/official_suite.sh`](./bench/official_suite.sh).
Code corpus: `28,352` files (`452.6 MiB` local Cargo crate sources). Cold index build: **`1.17 s`** (`36.3 MiB`, **`8.0%`** of corpus size); incremental no-op sync: **`64.6 ms`**.

### 1. Code & Boolean/Proximity Search (`lancet` vs `ripgrep 15.2.0`)

| Query (`28,352` files, `452.6 MiB`) | `rg 15.2` | `lancet` (scan) | `lancet` (auto index) | `lancet` (`--trust-index`) |
|---|---:|---:|---:|---:|
| Rare literal (`ThreadPoolBuilder`, 32 hits) | 307.7 ms | 310.6 ms | **60.5 ms (5.1×)** | **5.1 ms (60.3×)** |
| Medium literal (`unwrap_or_else`, 582 hits) | 311.3 ms | 314.6 ms | **65.4 ms (4.8×)** | **13.0 ms (23.9×)** |
| Boolean (`"unsafe" AND "transmute" NOT "miri"`) | 469.8 ms¹ | **304.8 ms** | **62.6 ms (7.5×)** | **10.1 ms (46.5×)** |
| Anchored regex (`pub unsafe fn \w+\(`) | 307.4 ms | 317.9 ms | **80.5 ms (3.8×)** | **26.9 ms (11.4×)** |
| Proximity (`near("unsafe", "transmute", 15)`) | *unsupported* | 307.1 ms | **65.9 ms** | **13.6 ms** |
| Typo fuzzy (`fuzzy("transmte", 1)`) | *unsupported* | 334.7 ms | **156.7 ms** | **107.4 ms** |
| Shared 6-query batch (`lancet batch`) | ~1,850 ms | 1,570.6 ms | **93.1 ms** | **30.7 ms (5.1 ms/q)** |

*¹ `rg` requires chaining two separate passes for file-level `AND`.*

### 2. File & Directory Discovery (`lancet fd` vs `fd 10.5.2` vs `rg --files` vs `find`)

| Workload (`28,352` files) | POSIX `find` | `rg --files` | `fd 10.5` | `lancet fd` (scan) | `lancet fd --trust-index` |
|---|---:|---:|---:|---:|---:|
| List all files (`28,352`) | 176.0 ms | 47.5 ms | 49.8 ms | 60.6 ms | **14.4 ms (3.5× vs `fd`)** |
| Extension + name (`-e toml -p Cargo`) | — | — | 47.1 ms | 52.1 ms | **6.6 ms (7.1× vs `fd`)** |
| Size filter (`>= 100 KiB`, 637 files) | 169.1 ms | — | 65.4 ms | **53.3 ms** | **5.6 ms (11.7× vs `fd`)** |

### 3. Structural AST Search (`lancet ast` vs `ast-grep 0.51.3`)

| Rust AST Pattern (`28,352` files) | `ast-grep 0.51` | `lancet ast` (scan) | `lancet ast` (auto index) | `lancet ast` (`--trust-index`) |
|---|---:|---:|---:|---:|
| `std::mem::transmute($X)` (19 hits) | 1,764.7 ms | **251.1 ms (7.0×)** | **176.6 ms (10.0×)** | **95.3 ms (18.5×)** |
| `$X.unwrap_or_else($F)` (963 hits) | 1,649.2 ms | **268.6 ms (6.1×)** | **245.7 ms (6.7×)** | **153.0 ms (10.8×)** |
| `unreachable!($$$ARGS)` (1,893 hits) | 1,782.2 ms | **484.8 ms (3.7×)** | **526.7 ms (3.4×)** | **343.3 ms (5.2×)** |
| `ThreadPoolBuilder::new()` (75 hits) | 1,716.7 ms | **241.5 ms (7.1×)** | **65.2 ms (26.3×)** | **13.9 ms (123.5×)** |

### 4. Multi-Format Documents & Archives (`lancet` vs `ripgrep-all 0.10.10`)

Measured over 120 mixed binary documents (`.pdf`, `.docx`, `.xlsx`, `.hwpx`, `.zip`). `lancet` extracts formats in-process in pure Rust with bomb-safe byte caps (`MAX_EXTRACTED_BYTES = 16 MiB`):

| Document Format & Query | `rga` (cold) | `rga` (zstd cached) | `lancet` (scan) | `lancet` (auto) | `lancet` (`--trust-index`) |
|---|---:|---:|---:|---:|---:|
| PDF (`QUARTERLY_REVENUE_ANOMALY`) | 263.5 ms | 67.3 ms | **10.6 ms** | **6.9 ms** | **3.4 ms** |
| DOCX (`SLA_INDEMNITY_CLAUSE`) | 262.0 ms | 67.5 ms | **10.7 ms** | **7.6 ms** | **4.1 ms** |
| XLSX (`H100_CLUSTER_CAPEX`) | *0 hits (unsupported)* | *0 hits* | **11.3 ms** | **7.5 ms** | **3.6 ms** |
| ZIP logs (`RAFT_QUORUM_STALL`) | 274.8 ms | 67.3 ms | **12.2 ms** | **7.1 ms** | **3.5 ms** |
| HWPX (`국가클라우드보안인증검토`) | *0 hits (unsupported)* | *0 hits* | **11.2 ms** | **8.1 ms** | **3.7 ms** |

---

## Installation

```bash
# From source (Rust 1.88+)
cargo install --git https://github.com/CREE1116/lancet

# Minimal pure-text/code build (disables tree-sitter & binary document adapters)
cargo install --git https://github.com/CREE1116/lancet --no-default-features
```

---

## Quick Tour

### 1. Orient & Outline (`fd` + `outline`)

```bash
# Compact .gitignore-aware workspace tree
lancet fd --tree --max-depth 2 .

# Structural declaration skeleton of a source file, Markdown doc, Excel workbook, or ZIP
lancet outline src/search.rs
lancet outline finance/q4_budget.xlsx
```

### 2. Search Code & Binary Documents (`lancet`)

```bash
# Optional: build/update the .lancet trigram index once for sub-100ms queries
lancet index .

# Boolean, proximity, fuzzy, and AST-scoped queries
lancet '("OOM" OR "out of memory") NOT "CPU" in line'
lancet 'near("gradient", "clip", 30) in function | expand parent | limit 5' src
lancet 'fuzzy("accumlation", 2) in type:(document, config)'
lancet '"SLA_INDEMNITY" in type:document | budget tokens:1200'
```

### 3. Surgical Inspection (`read` + `refs`)

```bash
# Read an exact function/struct by symbol name (includes doc comments)
lancet read src/search.rs --symbol execute_one

# Read PDF pages, Excel cell slices, or inner archive logs without extracting to disk
lancet read docs/architecture.pdf --page 2:4
lancet read finance/q4_budget.xlsx --sheet 1 --rows 1:20 --cols A:D
lancet read archives/cluster_logs.zip!node_0.log --lines 1:50

# Find AST symbol definitions + call/reference sites (comments & strings excluded)
lancet refs SearchOptions src
```

### 4. Safe Structural AST Search & Refactoring (`ast`)

```bash
# Match syntax trees across rs, py, js, ts, tsx, go
lancet ast '$X.unwrap()' src --lang rs
lancet ast '$A == $A' src --lang py

# Preview unified diff (dry-run by default), then apply atomically
lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'
lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?' --apply
```

---

## Using Lancet with AI Coding Agents

Drop [`docs/agent-instructions.md`](./docs/agent-instructions.md) into your project's `AGENTS.md`, `CLAUDE.md`, or system rules so agents automatically replace multi-tool shell chains (`rg` + `fd` + `ast-grep` + `cat` + Python PDF scripts) with structured, token-budgeted `lancet` calls.

---

## Architecture

```text
                       ┌───────────────────────────────────────────┐
                       │          .lancet/ (LANCET03 mmap)         │
                       │  40B header + doc meta + delta-varint 3g  │
                       └─────────────────────┬─────────────────────┘
                                             │ sound candidate narrowing
                                             │ (q-gram lemma + AST literals)
                                             ▼
┌──────────────┐   ┌──────────────────┐   ┌────────────────────────┐   ┌──────────────────┐
│ lancet <LQL> │──▶│ Strict IR Parser │──▶│ Candidate Planner      │──▶│ Exact Evaluator  │
│ lancet fd    │   │ & AST Compiler   │   │ Scan == Auto == Trust* │   │ SIMD + TreeSitter│
│ lancet ast   │   └──────────────────┘   └────────────────────────┘   │ + Pure-Rust Docs │
│ lancet refs  │                                                       └────────┬─────────┘
│ lancet read  │◀─────────────────── 1-to-1 line parity ────────────────────────┘
└──────────────┘
```

- **Zero silent approximations**: malformed queries, unknown flags, or unparseable AST patterns fail fast with exact diagnostics; `--rewrite` never mutates files without `--apply`.
- **Supported document & archive formats** (`adapters` feature, enabled by default): `.pdf`, `.docx`, `.pptx`, `.odt`, `.hwp` (5.0 OLE/CFB), `.hwpx`, `.epub`, `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods`, `.zip`, `.tar`, `.gz`, `.tgz`/`.tar.gz` (plus opt-in `--ocr` via `tesseract`/`pdftoppm`).

---

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE))
- MIT License ([LICENSE-MIT](./LICENSE-MIT))

at your option.
