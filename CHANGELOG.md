# Changelog

## 0.2.0 — 2026-11-28

lancet's goal is now one thing: replace the tools agents and people use to find, read and change files (ripgrep, fd, the built-in Read/Grep/Glob/Edit/Write tools, ast-grep, apply_patch) with one binary that takes the same arguments. This release removes what didn't serve that goal and rebuilds the internals around it. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the rules and the decisions.

### Breaking changes

| Removed or changed | Use instead |
|---|---|
| The query language (`lancet '…'`): `near()`, `before()`, `fuzzy()`, `in function`, pipes | `lancet rg` for text, `lancet ast` for structure |
| `lancet batch`, `lancet parse`, `lancet explain` | separate `lancet rg` calls (the index is shared anyway) |
| `lancet status` | `lancet index --status` |
| `ast --apply` wrote each file on its own | it writes every rewritten file or none, and refuses rewrites that add syntax errors (`--allow-syntax-errors` overrides) |
| `rename` accepted files containing NUL bytes | refused like in `edit` |
| `lancet fd` had its own syntax: substring patterns, a first argument that was a directory became a path, `-p` meant `--pattern`, `-r`, `--limit`, `--min-size` / `--max-size`, exit 1 when nothing matched | fd's syntax: a regex matched against names (`-g` for globs), paths after the pattern (`lancet fd . src`), `-p` is `--full-path`, `--max-results`, `-S +100ki`, and fd's exit codes (0 even when nothing matched) |

The MCP server's `--read-only` refuses `write` too. The docs site page `reference/lql-and-ast` is now `reference/ast-patterns`.

### The built-in tools' syntax

- `edit --old-string / --new-string / --replace-all / --expected-replacements N`, and the same keys in `--edits` JSON.
- `read --offset N --limit M`; without a range, `read` stops at 2,000 lines and cuts lines over 2,000 characters, like the built-in Read tools.
- `lancet write FILE` creates a file, or replaces one you have read (`--rev`).
- `patch` takes Codex `*** Begin Patch` envelopes as well as unified diffs.
- `read` of an image reports what it is; over MCP the image itself is returned.
- `outline`, `refs` and `ast` take several paths, like `rg`, `fd` and `read`.

### fd parity

- `lancet fd` takes fd 10.5's command line: smart case, `-g` / `-F` / `--exact` / `--and` / `-p`, every `-t` kind, `-e`, `-S`, `--changed-within` / `--changed-before` (durations, dates, `@timestamp`), `-o`, depth, `-E`, `--prune`, `--ignore-contain`, `.fdignore`, `--format`, `-0`, `-a`, `--path-separator`, `--strip-cwd-prefix`, `-C`, `--search-path`, `--max-results`, `-q`. 133 cases are compared with fd 10.5.2 in CI (`docs/fd-parity.md`). `-l`, `-x` and `-X` are refused (use `xargs -0`); lancet keeps `--tree`, `--dirs`, `--type-class`, `--json` and `--trust-index`.
- A Glob-tool pattern such as `src/**/*.ts` gets the fd spelling in the error (`lancet fd -g '*.ts' src`).

### ast-grep

- Rust `match $X { $$$A }` patterns work (they couldn't be parsed).
- A rewrite never deletes a comment: patterns skip comments, so a match holding a comment the template doesn't carry over is left alone (warning; `--apply` refuses).
- Measured against ast-grep 0.51.3 on 34 patterns in six languages: 89.7% of matches agree; every difference is listed in `docs/ast-grep-parity.md`.

### ripgrep parity

- `lancet rg` accepts every ripgrep 16.0 flag it can honour, including `--in`, `--in0`, `--max-bytes` and `--no-pre`. 150 cases are compared with ripgrep 16.0.0 byte for byte in CI.

### Safety

- Every write command (`edit`, `write`, `patch`, `rename`, `ast --rewrite`) reads and writes through one core: documents, binaries and non-UTF-8 files are refused, writes are compare-and-swap, multi-file changes are all or nothing with rollback, and changes that add syntax errors are refused.
- Trust mode no longer answers for files outside the index's scope (hidden or ignored files the index was built without).
- Printed diffs are exact for big files (Myers), keep CRLF and mark a missing final newline, and apply back with `lancet patch` and `patch(1)`.

### Faster

- `refs` parses each file once instead of four times (Go 201 → 115 ms, Rust 734 → 639 ms on the benchmark corpora).
- `refs --rank` no longer does quadratic work: `refs --rank '*'` on all of Linux went from over 27 minutes to 26 s.
- `refs` leaves out other types' members and same-named locals (`refs rev` on lancet: 118 → 23 hits, every real call site kept).

### MCP

- One `lancet` tool that takes the command line, instead of 11 hand-written tools: every CLI feature is available over MCP as it ships.

### Internals

One file planner for `rg`, `refs` and `ast`; language data in one table (129 language branches → 2); one tree-sitter parser per thread; one write core; one command list for the help, MCP and the skill; `main.rs` split per command.
