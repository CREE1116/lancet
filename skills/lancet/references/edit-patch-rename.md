# `read`, `edit`, `patch`, and `rename` Reference

## 1. `lancet read` & `lancet_read`

- **Inline targets:** `path/to/file.rs:10-50`, `path/to/file.rs:42`, or `bundle.zip!logs/node.log` (`--entry` also works).
- **Selectors:**
  - `--lines START:END` / `LINE` / `START:` (1-based, inclusive).
  - `--symbol NAME`: resolves the Tree-sitter declaration **including doc comments and attributes** above it.
  - `--page 2` or `--page 2:5`: PDF pages / PPTX slides.
  - `--sheet NAME|1 --rows 1:30 --cols A:E` (or `1:5`): spreadsheets (`.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods`).
  - `--ocr`: fallback via external `tesseract` / `pdftoppm` for scanned PDFs/images.
- **Token budget pagination:** pass `budget_tokens: 800` (`--budget-tokens 800`); if `truncated: true`, resume cleanly with `lines: "<next_line>:"`.
- **Revision token:** `rev` (XXH3-64) is returned for plain-text files (documents/archives omit `rev` because they are read-only).

---

## 2. `lancet edit` & `lancet_edit`

### Single-Hunk Addressing (`target` + `op`)

Pick **exactly one** target:
- `lines`: `"10:50"`, `"42"`, `"100:"` (CLI `--lines`)
- `symbol`: `"execute_one"` (CLI `--symbol`) — **remember this spans doc comments/attributes too**
- `find` (+ optional `all: true`): exact substring (CLI `--find` / `--all`). Refused unless unique (`all: false`), listing matching line numbers in the error. `\n` in `find` matches CRLF (`\r\n`) files transparently.

Pick **exactly one** operation:
- `with` (CLI `--with` / `--with-file`)
- `delete: true` (CLI `--delete`)
- `insert_before` (CLI `--insert-before`)
- `insert_after` (CLI `--insert-after`)

### Atomic Multi-Hunk `--edits` JSON Schema

All hunks use the **original file's line numbers** (never drift across hunks), must not overlap, and either all land or none do:

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

- **Locked CAS on `--rev`:** acquires an exclusive OS file lock, checks `inode` + `rev`, and refuses (`conflict`, exit `2`) with 0 bytes written if anything changed. Chain consecutive edits with returned `rev_after`.
- **Tree-sitter syntax gate:** parses before & after; refuses any net increase in syntax errors unless `allow_syntax_errors: true`.
- **File identity & crash safety:** preserves symlinks (writes target), permissions, owner/group where OS allows; uses temp-file + fsync + atomic rename + dir fsync for ordinary files, and in-place write for multiply hard-linked files.

---

## 3. `lancet patch` & `lancet_patch`

- Accepts `git diff` or `diff -u` from a file or stdin (`-` in CLI, `patch` string in MCP).
- Options: `root` (`--root DIR`), `strip` (`-p N`, default `1` strips `a/` and `b/`), `apply` (default `false`), `allow_syntax_errors`.
- Handles `git diff -M` renames (with or without hunks), mode changes (`new mode` / `new file mode`), and created/deleted empty files; refuses binary diffs and copies.
- Verifies every hunk in every file (exact match or unique nearest-offset relocation, never fuzzy) before writing; uses locked CAS + best-effort rollback of prior files if a concurrent writer wins mid-apply.

---

## 4. `lancet rename` & `lancet_rename` (Tier-1 Local Scope)

- **CLI:** `lancet rename FILE:LINE[:COL] OLD NEW [--rev REV] [--apply] [--json]`
- **MCP:** `lancet_rename(path, line, col?, old, new, rev?, apply?)`
- **Supported languages:** Rust, Python, JavaScript / TypeScript / TSX, Go.
- **How it works & when it refuses (by design):**
  - Identifies a **local binding** (variable, parameter, closure param, loop variable) at `FILE:LINE[:COL]` (`col` optional when `old` only refers to one binding on that line).
  - Refuses module/top-level declarations, struct fields, methods, exported symbols, macros, JSX tag names, Go composite-literal keys, or dynamic scopes (`eval`/`with`/`locals()`/`global`/`nonlocal`).
  - Re-resolves scopes after substitution and refuses any **shadowing or variable capture** if `new` collides with another visible binding.
  - Comments inside the scope mentioning `old` are left untouched and counted in `comment_mentions`.
  - Uses locked `--rev` CAS writes; dry-run unless `apply: true`.
  - **Fallback when refused:** use `lancet refs <SYMBOL>` + `lancet edit --edits` (multi-hunk) or `lancet ast --rewrite` across the verified sites.
