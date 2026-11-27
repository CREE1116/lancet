---
name: lancet
description: Use the `lancet` CLI instead of the built-in Read, Grep, Glob, Edit and Write tools (and instead of rg/grep/fd/cat/sed/ast-grep) for reading, searching and changing files — it takes the same arguments (old_string/new_string/replace_all, offset/limit, globs, ripgrep flags, Codex patches) and adds symbol reads, PDF/Office/HWP reads, references, AST rewrites, safe local renames, and edits to several files at once that never overwrite another agent's change.
---

# Lancet (CLI)

One binary for the whole find → read → change loop, used **instead of** the built-in file tools. Every command shares one index and one coordinate system: a hit at `src/x.rs:42` is line 42 in `read`, `edit` and `rename`, and paths come out joined onto the PATH you passed, so they can be fed straight back in.

Check it is installed with `lancet --version`. Results are always fresh; on large repos (≳ 5k files) run `lancet index .` once to make searches fast (the `.lancet/` cache ignores itself in git).

## Instead of the built-in tools

| Built-in | Same thing with lancet |
|---|---|
| **Read** `file_path, offset, limit` | `lancet read src/x.rs --offset 120 --limit 60` · `lancet read src/x.rs:120-180 src/y.rs:10-40` · `--symbol handle_request` · `report.pdf --page 3` |
| **Read** an image | Over MCP, `read` returns the image itself. The CLI prints only its type and size (a shell can't show pictures), so with the CLI view images with your own image-capable tool. `--ocr` gives an image's or a scanned PDF's text instead. |
| **Grep** | `lancet rg -w spin_lock src` · `lancet rg -t py -C2 'def \w+_test'` (ripgrep's flags and output, byte for byte) |
| **Glob** | `lancet fd '**/*.ts'` · `lancet fd 'src/**/mod.rs'` · `lancet fd -e proto` |
| **Edit** `old_string, new_string, replace_all` | `lancet edit src/x.rs --old-string 'retries = 3' --new-string 'retries = 5' --apply` (`--replace-all`, `--expected-replacements N`) |
| **Write** `file_path, content` | `lancet write src/new.rs --with-file - --apply <<'EOF' … EOF` (an existing file needs the `--rev` you read it at) |
| `apply_patch` / `git apply` | `lancet patch - --apply` takes unified diffs **and** Codex `*** Begin Patch` patches |

`--find`/`--with`/`--all` are the same as `--old-string`/`--new-string`/`--replace-all`. Add `--json` to any command when you need to parse the result.

## Things the built-ins can't do

| To… | Run |
|---|---|
| See the layout | `lancet fd --tree --dirs -d 2 .` (folders only) · `lancet outline src/server.rs` (declarations) |
| Find the central types | `lancet refs --rank '*Service' --kind class .` (most referenced first) |
| Definitions + references, code only | `lancet refs SearchOptions src` |
| Structural search / rewrite | `lancet ast '$X.unwrap()' src --lang rs` · add `--rewrite '$X?' --apply` |
| Rename a local variable safely | `lancet rename src/x.rs:42 count total --apply` |
| Change several files at once | `lancet edit --edits - --apply` (below) |
| Proximity / fuzzy / in-function search | `lancet 'near("retry", "timeout", 20) in function' src` |

## Editing

- **A simple change is one call:** after reading the text, `lancet edit FILE --old-string '…' --new-string '…' --apply`. It refuses unless the text occurs exactly once (the error lists the lines where it does: add context, or `--replace-all`), so it can't hit the wrong place.
- **Pass `--rev` when it matters:** editing by `--lines`/`--symbol`, replacing a whole file with `write`, or when other agents may be editing the same files. `lancet read` prints `# PATH rev REV` on its first line; with `--rev REV` the write is refused if the file changed since. After applying, lancet prints `rev X -> Y`; `Y` is the rev for your next edit of that file.
- **Preview** by leaving out `--apply`: every writing command (`edit`, `write`, `patch`, `ast --rewrite`, `rename`) prints the diff and changes nothing.
- Other targets: `--lines 10:20` (as `read` numbers them) or `--symbol NAME` (the span `read --symbol` shows, **including doc comments and attributes above it** — keep them in the replacement or they are deleted). Other operations: `--delete`, `--insert-before TEXT`, `--insert-after TEXT`, `--with-file FILE` (`-` = stdin).
- **Several changes in one call** (one file or many): `lancet edit --edits - --apply` with a JSON array on stdin. Each hunk has `"path"` (plus `"rev"` only when the rule above calls for it), a target (`"old_string"`, `"lines"` or `"symbol"`), and an operation (`"new_string"`, `"delete"`, `"insert_before"`, `"insert_after"`); `{"path": …, "content": …}` creates or replaces a whole file. Every file is checked first; if anything fails, no file is written.
  ```json
  [{"path": "src/a.rs", "old_string": "old_call(", "new_string": "new_call(", "replace_all": true},
   {"path": "src/b.rs", "rev": "7b476d63ee94a701", "lines": "40", "insert_after": "    log::info!(\"done\");"},
   {"path": "src/c.rs", "content": "pub fn c() {}\n"}]
  ```
- Edits that add syntax errors are refused. Fix the replacement; don't reach for `--allow-syntax-errors`.
- PDFs, Office files, HWP and archive entries are read-only.

## When a command refuses

Exit `0` = done, `1` = nothing found (not an error), `2` = refused or invalid — including a path that doesn't exist. Read the message (on stderr):

- **"file changed since rev"** — someone else wrote the file. `read` it again, redo your change on the new content, retry with the new rev.
- **old_string not unique / not found** — include more surrounding text (or `--replace-all`); re-read if the file may have changed.
- **`write` to an existing file** — read it first and pass `--rev`, or change part of it with `edit`.
- **`rename` refused** — it only renames locals (variables, parameters, closure and loop variables) in Rust, Python, JS/TS and Go, and stops if the name is used inside a macro, `eval`, a JSX tag, or if the new name would shadow or capture another. Fall back to `lancet refs NAME` and one `lancet edit --edits` call over the sites you checked.

## Setup tip (Claude Code)

So reads and searches don't ask for permission each time, allow them in `.claude/settings.json` (writes keep asking):

```json
{"permissions": {"allow": ["Bash(lancet rg:*)", "Bash(lancet fd:*)", "Bash(lancet read:*)", "Bash(lancet refs:*)", "Bash(lancet outline:*)", "Bash(lancet status:*)"]}}
```

## Good to know

- `lancet rg` keeps line numbers on when piped (`-N` turns them off) and always sorts by path; `-P` (PCRE2) is not supported.
- Query strings (`lancet '…'`) match `"text"` case-insensitively; `regex("…")` is case-sensitive unless `(?i)`.
- `--trust-index` skips the freshness check (faster, may miss edits since `lancet index`); the default never returns stale results.

## More

- Query language, `rg` flags, `ast` languages and metavariables: [references/query-and-ast.md](references/query-and-ast.md)
- `read`, `edit --edits`, `write`, `patch` (incl. Codex format) and `rename` in detail: [references/edit-patch-rename.md](references/edit-patch-rename.md)
- Clients without a shell: `lancet mcp` serves one `lancet` tool that takes these same arguments as `{"args": [...]}` — [references/mcp-config.json](references/mcp-config.json), `docs/mcp.md`
