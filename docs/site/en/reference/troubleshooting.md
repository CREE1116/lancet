---
nav_title: "Troubleshooting & Recovery"
title: "Troubleshooting & Refusal Recovery Matrix"
description: "Symptom -> Root Cause -> Exact Fix lookup for CAS conflicts, --find, syntax gates, rename, patch, and OCR."
lede: "Concrete diagnostic and recovery procedures for every error and intentional refusal."
keywords: "troubleshooting conflict find syntax errors rename unsupported patch pcre2 fd windows ocr"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md"
---
Every Lancet refusal returns exit code `2` with a specific diagnostic instead of silently misbehaving. Look up your exact symptom below.

## Editing, Patching & Rename Refusals {#edit-patch-errors}

| Symptom / Error | Root Cause | Exact Fix |
| --- | --- | --- |
| `status: "conflict"` on `edit` / `patch` / `rename` | File bytes on disk changed since `--rev` was read | Run `lancet read <FILE> --json` again, inspect the updated lines, recompute your edit, and pass the new `rev`. |
| `edit --find` refused (found 0 or >1 times) | Literal substring does not occur uniquely | Inspect line numbers listed in the error; expand surrounding context in `--find`, or pass `--all` if replacing all occurrences intentionally. |
| `edit --symbol` removed doc comments or attributes | `--symbol` spans leading `///` and `#[attr]` just like `read --symbol` | Include the doc comments and attributes in `--with` / `--with-file`, or target only the inner body with `--lines`. |
| Syntax error increase refused | Edit or patch increases Tree-sitter error count | Check the preview diff for unclosed braces or typos; pass `--allow-syntax-errors` only if intentionally staging an intermediate edit. |
| `lancet rename` refused (`unsupported`) | Target is top-level/exported, field, method, Python param, macro, JSX tag, or causes shadowing | See [refusal reasons](../guides/rename.md#refusals); fall back to `lancet refs` + `lancet edit --edits` or your LSP. |
| `lancet patch` refuses binary or copy diff | Patch includes `Binary files` / `copy from` | Filter non-binary text hunks cleanly before piping to `lancet patch`. |

## Search, AST & Traversal Errors {#search-ast-errors}

| Symptom / Error | Root Cause | Exact Fix |
| --- | --- | --- |
| `status: "invalid_query"` on LQL | Unknown filter, unclosed quote, or missing operator | Run `lancet parse '<QUERY>'` and inspect the byte offset `pos` in the error. |
| `ast --rewrite --apply` refuses `--limit` | Combining `--limit` with `--apply` risks partial unreviewed mutations | Remove `--limit` or narrow scope explicitly with `--path GLOB`. |
| `ast 'if ($C) $S'` misses `if ... else` | Lancet matches strict AST shapes (`if` vs `if-else` are distinct) | Write a second pattern including the `else` branch if both forms are intended. |
| `lancet rg -P` fails | PCRE2 lookarounds & backreferences are unsupported | Rewrite using Rust `regex` without lookarounds, or LQL `before()` / `NOT`. |
| `PATH: No such file or directory` (exit `2` on stderr) | Passed a nonexistent directory/file to `fd`, `search`, `batch`, `refs`, or `ast` (or typo like `fd docs/imgaes`) | Fix the path spelling; missing paths fail explicitly instead of silently returning `empty` (exit `1`). |
| `lancet fd '*.md'` prints stderr glob notice | Pattern has `*`, `?`, or `[` so Lancet automatically switches to glob mode (`-g`) | Expected in v0.1.3+; pass `-r` for regex or keep `-g` explicitly to silence the note. |

## Index, Documents & MCP Issues {#index-docs-mcp}

| Symptom / Message | Root Cause | Exact Fix |
| --- | --- | --- |
| Query after `lancet index` ignores recent edit under `--trust-index` | `--trust-index` skips metadata stat checks by design | Remove `--trust-index` to use fresh `auto` mode, or run `lancet index .` to sync. |
| Windows misses same-size edit with preserved `mtime` | Rust stdlib on Windows exposes only `mtime` & `size` | Run `lancet index .` or query with `--no-index` after tools that restore timestamps on Windows. |
| `lancet rg` misses text inside PDF / `.docx` | `lancet rg` disables adapters by default for `rg` parity | Pass `lancet rg --docs` (or use default LQL search `lancet 'query'`). |
| Scanned PDF / image yields empty text | No embedded text layer in image/PDF | Pass `--ocr` and verify `pdftoppm` + `tesseract` are installed on `PATH`. |
| MCP call fails with "this MCP server is read-only" | Server started with `--read-only` (refuses `edit`, `patch`, `rename`, `index`, `--apply`) | Restart `lancet mcp` without `--read-only` when edits are intended. |
