---
name: lancet
description: Search, read and change code and documents with the `lancet` CLI — ripgrep-compatible search on an index, Tree-sitter AST search/rewrite (24 languages), exact reads by symbol, line range, PDF page, sheet or archive entry, and rev-guarded edits, patches and local renames that never overwrite another agent's change. Use instead of rg/grep/fd/cat/sed/ast-grep when exploring an unfamiliar codebase, finding definitions and references, reading only the part of a file you need, or editing files that other agents may also be editing.
---

# Lancet (CLI)

One binary for the whole find → read → change loop. Every command shares one index and one coordinate system: a hit at `src/x.rs:42` is line 42 in `read`, `edit` and `rename`, and paths come out joined onto the PATH you passed, so they can be fed straight back in.

Check it is installed with `lancet --version`. Searches are always fresh; on large repos (≳ 5k files) run `lancet index .` once to make them fast.

## Pick the command

| To… | Run |
|---|---|
| See the layout | `lancet fd --tree --dirs -d 2 .` (folders only) · `lancet fd --tree -d 2 src docs` (several paths) |
| Find files by name | `lancet fd config src` (substring) · `lancet fd -e proto` · `lancet fd '*_test.go'` (glob) |
| Grep (same flags and output as ripgrep) | `lancet rg -w spin_lock src` · `lancet rg -t py -C2 'def \w+_test'` |
| Boolean / proximity / fuzzy / in-function search | `lancet '"OOM" AND "batch" NOT "CPU" in line' src` · `lancet 'near("retry", "timeout", 20) in function' src` |
| Skeleton of a file or folder | `lancet outline src/server.rs` · `lancet outline docs/spec.pdf` |
| Definitions + references (code only, not comments/strings) | `lancet refs SearchOptions src` |
| The most used classes / functions | `lancet refs --rank '*Service' --kind class .` |
| Structural search / rewrite | `lancet ast '$X.unwrap()' src --lang rs` · add `--rewrite '$X?'` |
| Read exactly what you need | `lancet read src/x.rs --symbol handle_request` · `lancet read src/x.rs:120-180 src/y.rs:10-40` (several at once) · `lancet read report.pdf --page 3` |
| Change lines, a symbol or exact text | `lancet edit src/x.rs --find 'retries = 3' --with 'retries = 5' --rev REV --apply` |
| Apply a diff | `git diff | lancet patch --apply` · `lancet patch fix.diff` (preview) |
| Rename a local variable safely | `lancet rename src/x.rs:42 count total --rev REV --apply` |

Add `--json` to any command when you need to parse the result.

## The loop

1. **Orient.** `fd --tree --dirs`, `outline`, and `refs --rank` before opening files. Don't `cat` whole files.
2. **Locate.** `rg`, a query, `refs` or `ast` give `path:line`.
3. **Read and note the `rev`.** `lancet read` prints `# path [section] rev 0d83f19f5e4c1931` on its first line. On long files add `--budget-tokens 800`; when it says `truncated`, continue with `--lines NEXT:`.
4. **Preview, then apply with that rev.** `edit`, `patch`, `ast --rewrite` and `rename` print a diff and change nothing until `--apply`. The preview says which rev to pass: `file is at rev X: re-run with --rev X --apply`. After applying it prints `rev X -> Y`; use `Y` for your next edit of that file.

## Editing

- `edit` targets one of `--lines 10:20` (as `read` numbers them), `--symbol NAME` (the span `read --symbol` shows, **including doc comments and attributes above it** — keep them in the replacement or they are deleted), or `--find 'exact text'` (must occur once; the error lists the lines if it doesn't — add context, or pass `--all`).
- The operation is one of `--with TEXT`, `--with-file FILE` (`-` = stdin), `--delete`, `--insert-before TEXT`, `--insert-after TEXT`.
- **Several changes to one file = one call:** `lancet edit FILE --edits - --rev REV --apply` with a JSON array on stdin, e.g. `[{"lines": "10:12", "with": "…"}, {"find": "old(", "with": "new(", "all": true}]`. All hunks use the line numbers you read; they never shift each other.
- Edits that add syntax errors are refused. Fix the replacement; don't reach for `--allow-syntax-errors`.
- PDFs, Office files, HWP and archive entries are read-only.

## When a command refuses

Exit `0` = done, `1` = nothing found (not an error), `2` = refused or invalid — including a path that doesn't exist. Read the message (on stderr):

- **`conflict` / "file changed since rev"** — someone else wrote the file. `read` it again, redo your change on the new content, retry with the new rev. Never retry without `--rev`.
- **`rename` refused** — it only renames locals (variables, parameters, closure and loop variables) in Rust, Python, JS/TS and Go, and stops if the name is used inside a macro, `eval`, a JSX tag, or if the new name would shadow or capture another. Fall back to `lancet refs NAME` and one `lancet edit --edits` call over the sites you checked.
- **`--find` not unique** — include more surrounding text, or `--all` if every occurrence should change.

## Good to know

- `lancet rg` keeps line numbers on when piped (`-N` turns them off) and always sorts by path; `-P` (PCRE2) is not supported.
- Query strings (`lancet '…'`) match `"text"` case-insensitively; `regex("…")` is case-sensitive unless `(?i)`. `in function` / `symbol:` work for Rust, Python, JS, TS and Go; `ast` and `refs` cover 24 languages.
- `--trust-index` skips the freshness check (faster, may miss edits since `lancet index`); the default never returns stale results.

## More

- Query language, `rg` flags, `ast` languages and metavariables: [references/query-and-ast.md](references/query-and-ast.md)
- `read`, `edit --edits`, `patch` and `rename` in detail: [references/edit-patch-rename.md](references/edit-patch-rename.md)
- The same tools over MCP (`lancet mcp`), for clients without a shell: [references/mcp-config.json](references/mcp-config.json) and `docs/mcp.md`
