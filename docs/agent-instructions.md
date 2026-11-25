# Agent Search, Inspection & Refactoring Rules (`lancet`)

Use `lancet` as your unified, deterministic CLI replacement for `rg`, `fd`, `ast-grep`, and raw `cat`/`sed` across codebases and document workspaces.
`lancet` is pure Rust, `.gitignore`-aware, token-budgeted, and accelerated by a zero-copy `.lancet` trigram index when present.

## 1. Orient & Discover (`lancet fd` + `lancet outline`)

- Map an unfamiliar directory first instead of blind globbing:
  - `lancet fd --tree --max-depth 2 .`
  - `lancet fd -t f -e rs src --changed-within 7d`
  - `lancet fd --trust-index --type-class document --json`
- Inspect a file's symbol/heading/sheet/archive skeleton before reading the whole file:
  - `lancet outline src/search.rs`
  - `lancet outline docs/spec.md`
  - `lancet outline report.xlsx`

## 2. Search Code, Docs & Archives (`lancet` — replaces `rg` / `rga`)

- Express boolean, proximity, fuzzy, regex, and AST-scoped queries in a single pass:
  - `lancet '"exact_symbol" in ext:rs'`
  - `lancet 'near("gradient", "clip", 30) in function | expand parent'`
  - `lancet '("OOM" OR "out of memory") NOT "CPU" in line'`
  - `lancet 'fuzzy("transmte", 1) | limit 10 | budget tokens:1500'`
- Searches source code, `.pdf`, `.docx`, `.pptx`, `.odt`, `.hwp` (5.0 OLE), `.hwpx`, `.epub`, `.xlsx/.xls/.ods`, and `.zip/.tar.gz` natively in-process.
- Run `lancet index .` once on large repositories (`>5k` files); `Auto` mode verifies `mtime + size` automatically so results are **never stale** (`Scan == Auto`).

## 3. Read Surgically (`lancet read` — replaces `cat`/`sed`/PDF extractors)

- Line numbers match `lancet` search output 1-to-1 across code AND binary documents:
  - `lancet read src/search.rs --symbol execute_one`
  - `lancet read src/search.rs:100-160 --budget-tokens 800`
  - `lancet read docs/paper.pdf --page 2:3`
  - `lancet read finance/budget.xlsx --sheet 1 --rows 1:30 --cols A:E`
  - `lancet read logs/bundle.zip!node_0.log --lines 1:50`

## 4. Trace Definitions & References (`lancet refs`)

- Find `[def]` declarations and `[ref]` AST identifier sites (comments & strings automatically excluded):
  - `lancet refs SearchOptions src`
  - `lancet refs execute_one src --lang rs --json`

## 5. Structural AST Search & Safe Refactoring (`lancet ast` — replaces `ast-grep`)

Supported languages: `rs` (`rust`), `py` (`python`), `js`, `ts`, `tsx`, `go`.

- Match syntax trees (`$VAR` single node, `$_` wildcard, `$$$ARGS` 0+ sibling nodes):
  - `lancet ast '$X.unwrap()' src --lang rs`
  - `lancet ast 'panic!($$$ARGS)' src --lang rs`
  - `lancet ast '$X == $X' src --lang py`
- **Always preview rewrites first** (default `--rewrite` prints a unified diff without touching disk), then pass `--apply`:
  - `lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'`
  - `lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?' --apply`
