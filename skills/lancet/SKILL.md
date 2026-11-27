---
name: lancet
description: Search code and documents, inspect exact AST symbols, line ranges, PDF/PPTX pages, spreadsheet cells, and archive entries, and apply CAS-safe edits, unified diffs, or scope-checked local renames via `lancet` CLI or `lancet_*` MCP tools. Use whenever navigating an unfamiliar codebase, searching text/regex/AST/references, reading token-budgeted slices, or making race-safe code changes.
---

# Lancet Playbook

Lancet runs identically through **MCP (`lancet_*`)** and **CLI (`lancet ...`)** on one trigram index (`LANCET04`) and one coordinate system across source code, 24 AST languages, PDF, Office (DOCX/PPTX/XLSX), HWP/HWPX, and archives.

## 1. Tool Selection Matrix

| Task | MCP Tool | CLI Command |
|---|---|---|
| Map folders/files or tree view | `lancet_fd` (`dirs: true`) | `lancet fd [--tree -d 2 --dirs] [PAT] [PATH]` |
| Declarations, Markdown headings, sheet/zip outline | `lancet_outline` | `lancet outline [PATH] [-d N]` |
| Boolean, proximity, fuzzy, AST-scoped query | `lancet_search` | `lancet '<QUERY>' [PATH]` |
| Multi-query single-walk batch | — | `lancet batch [PATH] -q ...` / `--queries-json` |
| `ripgrep`-style regex / line grep | `lancet_rg` | `lancet rg [FLAGS] PATTERN [PATH...]` |
| Structural AST search & rewrite | `lancet_ast` | `lancet ast '<PAT>' [PATH] [-l LANG] [-r TPL]` |
| Lexical `[def]` + `[ref]` or ranked declarations | `lancet_refs` (`rank: true`) | `lancet refs [--rank --kind K] <SYM\|GLOB> [PATH]` |
| Surgical line / symbol / page / sheet / archive read | `lancet_read` | `lancet read <TARGET> [--symbol\|-l\|-p\|--budget-tokens]` |
| CAS-safe line / symbol / `find` / multi-hunk edit | `lancet_edit` | `lancet edit <PATH> (--lines\|--symbol\|--find\|--edits)` |
| Atomic multi-file unified diff | `lancet_patch` | `lancet patch [DIFF\|-] [--root DIR] [-p N]` |
| Tier-1 local-binding rename (`FILE:LINE[:COL]`) | `lancet_rename` | `lancet rename <FILE:LINE[:COL]> <OLD> <NEW>` |
| Build / inspect / sync `.lancet` index | `lancet_index` | `lancet index\|status\|explain [PATH]` |

## 2. Core 4-Step Workflow

1. **Orient first** (never dump whole files or run blind recursive globs):
   - `lancet_fd` (`tree: true`, `max_depth: 2`, optionally `dirs: true`) and `lancet_outline` on target files/dirs.
   - To find central symbols in an unfamiliar repo, rank declarations by usage with `lancet_refs(symbol="*Service", rank=true, kinds=["class"])` / `lancet refs --rank '*Service' --kind class`.
2. **Locate exact coordinates**:
   - Use `lancet_search` (`'near("a","b",20) in function'`), `lancet_rg`, `lancet_refs`, or `lancet_ast`.
   - When running several exploratory queries, use CLI `lancet batch` to share one directory walk and index open.
3. **Inspect surgically & capture `rev`**:
   - Read by `--symbol`, `--lines`, `--page`, `--sheet`/`--rows`/`--cols`, or `archive.zip!entry`.
   - On large files pass `budget_tokens` (e.g. `800`); if `truncated: true`, resume at `next_line` via `lines: "<next_line>:"`.
   - Record `"rev"` (XXH3-64) returned by `lancet_read` before mutating.
4. **Preview first, then apply atomically**:
   - `edit`, `patch`, `ast --rewrite`, and `rename` all default to **dry-run** (`apply: false`). Inspect the unified diff, then re-run with `apply: true` and `rev`.

## 3. Critical Gotchas & Rules

- **`--symbol` includes doc comments & attributes:** `read --symbol` shows doc comments/attributes above the declaration; `edit --symbol --with` replaces that entire span, so keep them in the replacement or they are deleted.
- **LQL case & scope limits:** `"literal"` in `lancet '<QUERY>'` is case-insensitive (Unicode fold), whereas `regex("...")` is case-sensitive unless `(?i)`. `in function` and `symbol:` support 6 languages (`.rs`, `.py`, `.js`, `.ts`, `.tsx`, `.go`); `ast` and `refs` support 24.
- **`lancet ast` nuances:** `--apply` refuses `--limit` (no partial rewrites). `--lang cpp` scans `.h` headers too. Statement metavariables (`if ($C) $S`, `{ $$$B }` in C-like languages) distinguish `if` from `if-else`.
- **`lancet rg` defaults:** Line numbers (`-n`) stay **ON even when piped** (pass `-N` for plain `rg` pipes); `--color auto` colors only on interactive TTYs; output is always path-sorted; `--docs` is OFF by default; `-P`/`--pcre2` is unsupported by design.
- **`lancet patch` scope:** Handles `git diff -M` renames, mode changes, and empty files; refuses binary changes and copies; never applies fuzzy hunks.
- **`lancet rename` scope (tier 1):** Renames **local bindings only** (variables, parameters, closures, loops) in one file for Rust, Python, JS/TS/TSX, and Go, picked by `FILE:LINE[:COL] OLD NEW`. Refuses non-local/opaque constructs and capture/shadowing rather than guessing; skips comments (`comment_mentions`).
- **Documents & archives are read-only:** `edit`, `patch`, `ast --rewrite`, and `rename` operate on plain-text UTF-8 files only.
- **Index freshness:** On repos > 5k files run `lancet index .` (`lancet_index(action="sync")`). `auto` mode checks `(mtime, size, ctime, inode)` + racy timestamps so results stay fresh without a daemon. Use `trust` (`--trust-index`) only when skipping disk checks is acceptable.

## 4. Exit Codes & Error Recovery

| CLI Exit | `status` | MCP `isError` | Meaning & Next Action |
|---|---|---|---|
| `0` | `complete` | `false` | Success (captures/results/diff returned) |
| `1` | `empty` | `false` | Clean 0-hit result (or missing index in `status`) — **not an error** |
| `2` | `conflict` | `true` | File changed since `rev` (0 bytes written) → re-run `read`, adapt change, retry with new `rev` |
| `2` | `invalid_query` | `true` | Syntax/arg error or non-unique `find` → fix query or expand `find` context (or `all: true`) |
| `2` | `unsupported` | `true` | Syntax-error increase blocked edit, binary diff, or non-local `rename` → fix replacement or fall back |

- **Syntax gate refusal:** Fix the broken replacement syntax; never retry blindly with `allow_syntax_errors: true`.
- **`rename` refusal on non-local/macro/member:** Fall back to `lancet_refs` + `lancet_edit` (`edits` array) or `lancet_ast` (`rewrite`).

## 5. Deep References

- LQL grammar, `batch`, `rg` flags, and all 24 AST languages: [`references/query-and-ast.md`](references/query-and-ast.md)
- `read`, `edit` (`--edits`/`--find`), `patch`, and tier-1 `rename` schemas & caveats: [`references/edit-patch-rename.md`](references/edit-patch-rename.md)
- Ready-to-copy MCP client JSON configs: [`references/mcp-config.json`](references/mcp-config.json)
