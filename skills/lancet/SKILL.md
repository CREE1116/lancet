---
name: lancet
description: Use the `lancet` CLI instead of the built-in Read, Grep, Glob, Edit and Write tools (and instead of rg/grep/fd/cat/sed/ast-grep) for reading, searching and changing files — it takes the same arguments (old_string/new_string/replace_all, offset/limit, globs, ripgrep flags, Codex patches) and adds symbol reads, PDF/Office/HWP reads, references, AST rewrites, safe local renames, and edits to several files at once that never overwrite another agent's change.
---

# Lancet (CLI)

One binary for find → read → change, used **instead of** the built-in file tools. All commands share one coordinate system: `src/x.rs:42` from `rg` is line 42 in `read`, `edit` and `rename`, and paths can be fed straight back in. Results are always fresh; on big repos (≳ 5k files) run `lancet index .` once for speed.

| Instead of | Run |
|---|---|
| **Read** (first 2000 lines unless you give a range) | `lancet read src/x.rs --offset 120 --limit 60` · `lancet read src/x.rs:120-180 src/y.rs:10-40` · `--symbol handle_request` · `report.pdf --page 3` (PDF, Office, HWP too; `--ocr` for scans) |
| **Grep** | `lancet rg -w spin_lock src` (ripgrep's flags and output) |
| **Glob** | fd's syntax: `lancet fd -e ts` · `lancet fd -g '*.test.ts' src` (the pattern is a regex unless `-g`; paths come after it: `lancet fd . src docs`) |
| **Edit** | `lancet edit src/x.rs --old-string 'retries = 3' --new-string 'retries = 5' --apply` (`--replace-all`) |
| **Write** | `lancet write docs/new.md --with-file - --apply <<'EOF' … EOF` |
| apply_patch | `lancet patch - --apply` (unified diff or Codex `*** Begin Patch`) |
| Rename a local | `lancet rename src/x.rs:42 count total --apply` (Rust, Python, JS/TS, Go; checks scopes) |
| Definitions + uses | `lancet refs SearchOptions src` (code only; skips other types' `.name` and same-named locals) |
| Layout | `lancet fd --tree --dirs -d 2 .` · `lancet outline src/server.rs src/db.rs` |
| AST search / rewrite | `lancet ast '$X.unwrap()' src --lang rs` (`--rewrite '$X?' --apply`) |

Images: over MCP `read` returns the image; with the CLI use your own image viewer.

## Editing

- **Edit straight from what you saw.** An `rg` hit or a `read` already shows the text: pass it as `--old-string`. It must occur exactly once (the error lists where it does: add context, or `--replace-all`), so it can't hit the wrong place.
- **The printed diff is the check.** `--apply` prints exactly what changed; you don't need to re-read or re-grep. Leave out `--apply` to preview.
- **Several files in one call:** `lancet edit --edits - --apply` with a JSON array on stdin. Everything is checked first; if anything fails, nothing is written.
  ```json
  [{"path": "src/a.rs", "old_string": "old_call(", "new_string": "new_call(", "replace_all": true},
   {"path": "src/b.rs", "lines": "40", "insert_after": "    log::info!(\"done\");"},
   {"path": "src/c.rs", "content": "pub fn c() {}\n"}]
  ```
- **`--rev`** (from `# PATH rev REV`, the first line `read` prints) is only needed for `--lines`/`--symbol` edits, `write` over an existing file, or when other agents edit the same files: the write is refused if the file changed since.
- `--symbol NAME` spans include doc comments and attributes: keep them in the replacement.
- Edits that add syntax errors are refused: fix the replacement.

## When a command refuses

Exit `0` done, `1` nothing found, `2` refused (message on stderr).
- **"file changed since rev"**: `read` again and redo the change.
- **old_string not unique / not found**: add context, or `--replace-all`.
- **`rename` refused** (macro it can't see into, `eval`, a capture): use `lancet refs NAME` and one `lancet edit --edits` call.

More: [references/edit-patch-rename.md](references/edit-patch-rename.md) (read/edit/write/patch/rename in detail, Claude Code allowlist) · [references/rg-and-ast.md](references/rg-and-ast.md) (`rg` notes, `refs --rank`, AST patterns) · `lancet mcp` for clients without a shell.
