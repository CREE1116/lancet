---
nav_title: "Troubleshooting & Recovery"
title: "Troubleshooting & Refusal Recovery Matrix"
description: "Symptom -> Root Cause -> Exact Fix lookup for CAS conflicts, --old-string, syntax gates, rename, patch, and OCR."
lede: "Concrete diagnostic and recovery procedures for every error and intentional refusal."
keywords: "troubleshooting conflict find syntax errors rename unsupported patch pcre2 fd windows ocr"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md"
---
Every Lancet refusal returns exit code `2` with a specific diagnostic instead of silently misbehaving. Look up your exact symptom below.

## Editing, Patching & Rename Refusals {#edit-patch-errors}

| Symptom / Error | Root Cause | Exact Fix |
| --- | --- | --- |
| `status: "conflict"` on `edit` / `patch` / `rename` | File bytes on disk changed since `--rev` was read | Run `lancet read <FILE> --json` again, inspect the updated lines, recompute your edit, and pass the new `rev`. |
| `edit --old-string` refused (found 0 or >1 times) | The text does not occur exactly once | Inspect the line numbers listed in the error; add surrounding context to `--old-string`, or pass `--replace-all` to change every occurrence. |
| `edit --symbol` removed doc comments or attributes | `--symbol` spans leading `///` and `#[attr]` just like `read --symbol` | Include the doc comments and attributes in `--with` / `--with-file`, or target only the inner body with `--lines`. |
| Syntax error increase refused | Edit or patch increases Tree-sitter error count | Check the preview diff for unclosed braces or typos; pass `--allow-syntax-errors` only if intentionally staging an intermediate edit. |
| `lancet rename` refused (`unsupported`) | Target is top-level/exported, field, method, Python param, macro, JSX tag, or causes shadowing | See [refusal reasons](../guides/rename.md#refusals); fall back to `lancet refs` + `lancet edit --edits` or your LSP. |
| `lancet patch` refuses binary or copy diff | Patch includes `Binary files` / `copy from` | Filter non-binary text hunks cleanly before piping to `lancet patch`. |

## Search, AST & Traversal Errors {#search-ast-errors}

| Symptom / Error | Root Cause | Exact Fix |
| --- | --- | --- |
| `ast --rewrite --apply` refuses `--limit` | Combining `--limit` with `--apply` risks partial unreviewed mutations | Remove `--limit` or narrow scope explicitly with `--path GLOB`. |
| `ast 'if ($C) $S'` misses `if ... else` | Lancet matches strict AST shapes (`if` vs `if-else` are distinct) | Write a second pattern including the `else` branch if both forms are intended. |
| `lancet rg -P` fails | PCRE2 lookarounds & backreferences are unsupported | Rewrite without lookarounds (match the wider text and filter, or use `lancet ast` for structure). |
| `PATH: No such file or directory` (exit `2` on stderr) | Passed a nonexistent directory/file to `rg`, `refs`, or `ast` | Fix the path spelling; missing paths fail explicitly instead of silently returning `empty` (exit `1`). `fd` reports `Search path "…" is not a directory.` with fd's exit code `1`. |
| `lancet fd '*.md'` fails with a regex error | As in fd, the pattern is a regex; `*.md` isn't one | Use `lancet fd -g '*.md'` or `lancet fd -e md`. A Glob-tool pattern like `src/**/*.ts` becomes `lancet fd -g '*.ts' src`. |

## Index, Documents & MCP Issues {#index-docs-mcp}

| Symptom / Message | Root Cause | Exact Fix |
| --- | --- | --- |
| Query after `lancet index` ignores recent edit under `--trust-index` | `--trust-index` skips metadata stat checks by design | Remove `--trust-index` to use fresh `auto` mode, or run `lancet index .` to sync. |
| Windows misses same-size edit with preserved `mtime` | Rust stdlib on Windows exposes only `mtime` & `size` | Run `lancet index .` or query with `--no-index` after tools that restore timestamps on Windows. |
| `lancet rg` misses text inside PDF / `.docx` | `lancet rg` disables adapters by default for `rg` parity | Pass `lancet rg --docs`. |
| Scanned PDF / image yields empty text | No embedded text layer in image/PDF | Pass `--ocr` and verify `pdftoppm` + `tesseract` are installed on `PATH`. |
| MCP call fails with "this MCP server is read-only" | Server started with `--read-only` (refuses `edit`, `patch`, `rename`, `index`, `--apply`) | Restart `lancet mcp` without `--read-only` when edits are intended. |
