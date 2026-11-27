# `read`, `edit`, `patch`, and `rename` Reference

CLI flags first; the MCP tool (`lancet_read`, …) takes the same options as JSON fields (`--budget-tokens` → `budget_tokens`, `--apply` → `apply: true`).

## 1. `lancet read`

- **Inline targets:** `path/to/file.rs:10-50`, `path/to/file.rs:42`, or `bundle.zip!logs/node.log` (`--entry` also works).
- **Selectors:**
  - `--lines START:END` / `LINE` / `START:` (1-based, inclusive).
  - `--symbol NAME`: resolves the Tree-sitter declaration **including doc comments and attributes** above it.
  - `--page 2` or `--page 2:5`: PDF pages / PPTX slides.
  - `--sheet NAME|1 --rows 1:30 --cols A:E` (or `1:5`): spreadsheets (`.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods`).
  - `--ocr`: fallback via external `tesseract` / `pdftoppm` for scanned PDFs/images.
- **Token budget pagination:** `--budget-tokens 800`; when it reports `truncated` (JSON: `truncated: true`, `next_line`), continue with `--lines NEXT:`.
- **Revision token:** the first line of the output is `# PATH [section] rev REV` (JSON: `rev`), an XXH3-64 of the file's bytes. Documents and archives have none: they are read-only.

---

## 2. `lancet edit`

### Single-Hunk Addressing (target + operation)

Pick **exactly one** target:
- `--lines 10:50` / `42` / `100:`
- `--symbol execute_one` — **spans the doc comments and attributes above it too**
- `--find 'exact text'` (+ `--all`): refused unless it occurs exactly once, and the error lists the lines where it does. `\n` in the text also matches CRLF files.

Pick **exactly one** operation:
- `--with TEXT` / `--with-file FILE` (`-` = stdin)
- `--delete`
- `--insert-before TEXT`
- `--insert-after TEXT`

### Atomic Multi-Hunk `--edits` (JSON file, or `-` for stdin)

`lancet edit FILE --edits - --rev REV --apply <<'JSON'` … `JSON`. All hunks use the **original file's line numbers** (never drift across hunks), must not overlap, and either all land or none do:

```json
[
  {"lines": "10:12", "with": "fn a() {}"},
  {"symbol": "old_fn", "delete": true},
  {"find": "old_call(", "with": "new_call(", "all": true},
  {"lines": "40", "insert_after": "    tracing::info!(\"done\");"}
]
```

Rules: each element requires **exactly one** of `lines`, `symbol`, or `find` (`all` is valid only with `find`) and **exactly one** of `with`, `delete`, `insert_before`, or `insert_after`.

### Safety & Concurrency Guarantees

- **Locked CAS on `--rev`:** takes an exclusive OS file lock, checks `inode` + `rev`, and refuses (`conflict`, exit `2`) with 0 bytes written if anything changed. A preview prints `file is at rev X: re-run with --rev X --apply`; after applying, `rev X -> Y` — pass `Y` to the next edit (JSON: `rev_after`).
- **Tree-sitter syntax gate:** parses before & after; refuses any net increase in syntax errors unless `--allow-syntax-errors`.
- **File identity & crash safety:** preserves symlinks (writes target), permissions, owner/group where OS allows; uses temp-file + fsync + atomic rename + dir fsync for ordinary files, and in-place write for multiply hard-linked files.

---

## 3. `lancet patch`

- Accepts `git diff` or `diff -u` from a file or stdin (`-`).
- Options: `--root DIR`, `-p N` (default `1` strips `a/` and `b/`), `--apply` (default: preview), `--allow-syntax-errors`.
- Handles `git diff -M` renames (with or without hunks), mode changes (`new mode` / `new file mode`), and created/deleted empty files; refuses binary diffs and copies.
- Verifies every hunk in every file (exact match or unique nearest-offset relocation, never fuzzy) before writing; uses locked CAS + best-effort rollback of prior files if a concurrent writer wins mid-apply.

---

## 4. `lancet rename` (Tier-1 Local Scope)

- **CLI:** `lancet rename FILE:LINE[:COL] OLD NEW [--rev REV] [--apply] [--json]`
- **MCP:** `lancet_rename(path, line, col?, old, new, rev?, apply?)`
- **Supported languages:** Rust, Python, JavaScript / TypeScript / TSX, Go.
- **How it works & when it refuses (by design):**
  - Identifies a **local binding** (variable, parameter, closure param, loop variable) at `FILE:LINE[:COL]` (`col` optional when `old` only refers to one binding on that line).
  - Refuses module/top-level declarations, struct fields, methods, exported symbols, macros, JSX tag names, Go composite-literal keys, or dynamic scopes (`eval`/`with`/`locals()`/`global`/`nonlocal`).
  - Re-resolves scopes after substitution and refuses any **shadowing or variable capture** if `new` collides with another visible binding.
  - Comments inside the scope mentioning `old` are left untouched and counted in `comment_mentions`.
  - Uses locked `--rev` CAS writes; preview unless `--apply`.
  - **Fallback when refused:** use `lancet refs <SYMBOL>` + `lancet edit --edits` (multi-hunk) or `lancet ast --rewrite` across the verified sites.
