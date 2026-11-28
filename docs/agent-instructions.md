# Agent Search, Inspection & Refactoring Rules (`lancet`)

Use `lancet` as your unified, deterministic CLI replacement for `rg`, `fd`, `ast-grep`, and raw `cat`/`sed` across codebases and document workspaces.
`lancet` is pure Rust, `.gitignore`-aware, token-budgeted, and accelerated by a zero-copy `.lancet` trigram index when present.

## 1. Orient & Discover (`lancet fd` + `lancet outline`)

- Map an unfamiliar directory first instead of blind globbing:
  - `lancet fd --tree --dirs --max-depth 2 .` (folders-only project layout)
  - `lancet fd --tree --max-depth 2 src docs` (`fd` takes several paths)
  - `lancet fd -t f -e rs src --changed-within 7d`
  - `lancet fd --trust-index --type-class document --json`
- Inspect a file's symbol/heading/sheet/archive skeleton before reading the whole file:
  - `lancet outline src/rg.rs`
  - `lancet outline docs/spec.md`
  - `lancet outline report.xlsx`

## 2. Search Code, Docs & Archives (`lancet rg` — replaces `rg` / `rga` / Grep)

- `lancet rg` takes ripgrep's flags and prints ripgrep's output (`-w`, `-i`/`-S`, `-A`/`-B`/`-C`, `-g`, `-t`, `-U`, `--json`, `--vimgrep`, `--files`):
  - `lancet rg -w -t rs 'plan_targets' src`
  - `lancet rg --docs 'learning rate' docs` (also inside PDF, Office, HWP and archives)
  - Uses `.lancet` automatically (always fresh), always sorts output by path, and keeps line numbers (`-n`) on. See [ripgrep parity](./rg-parity.md) for the flag table and differences (unsupported: `-P`/`--pcre2` and `--pre`).
- Structure instead of text: `lancet ast '$X.unwrap()' src --lang rs`; definitions and uses: `lancet refs NAME src`.
- Searches source code, `.pdf`, `.docx`, `.pptx`, `.odt`, `.hwp` (5.0 OLE), `.hwpx`, `.epub`, `.xlsx/.xls/.ods`, and `.zip/.tar.gz` natively in-process.
- Run `lancet index .` once on large repositories (`>5k` files); `Auto` mode re-checks every indexed file's `(mtime, size, ctime, inode)`, plus a check for timestamps too recent to trust, so results are **never stale** (`Scan == Auto`).

## 3. Read Surgically (`lancet read` — replaces `cat`/`sed`/PDF extractors)

- Line numbers match `lancet` search output 1-to-1 across code AND binary documents:
  - `lancet read src/rg.rs --symbol run`
  - `lancet read src/rg.rs:100-160 src/main.rs:1-40` (several targets in one call)
  - `lancet read src/rg.rs:100-160 --budget-tokens 800`
  - `lancet read docs/paper.pdf --page 2:3`
  - `lancet read finance/budget.xlsx --sheet 1 --rows 1:30 --cols A:E`
  - `lancet read logs/bundle.zip!node_0.log --lines 1:50`

## 4. Trace Definitions & References (`lancet refs`)

- Find `[def]` declarations and `[ref]` AST identifier sites (comments & strings automatically excluded):
  - `lancet refs SearchOptions src`
  - `lancet refs plan_targets_with_req src --lang rs --json`
- Rank declarations matching a glob by lexical reference count to locate core types/functions quickly (`definitions > 1` indicates multiple declarations share that name):
  - `lancet refs --rank '*Service' --kind class src --limit 10`
  - `lancet refs --rank '*' --kind function --limit 10 .`

## 5. Structural AST Search & Safe Refactoring (`lancet ast` — replaces `ast-grep`)

Supported languages (24): `rs`, `py`, `js`, `ts`, `tsx`, `go`, `c`, `cpp`, `zig`, `java`, `cs`, `kt`, `scala`, `swift`, `rb`, `php`, `bash`, `lua`, `hcl`, `html`, `css`, `json`, `yaml`, `toml`.

- Match syntax trees (`$VAR` single node, `$_` wildcard, `$$$ARGS` 0+ sibling nodes; statement metavariables like `if ($C) $S` and `{ $$$B }` work in C-like languages):
  - `lancet ast '$X.unwrap()' src --lang rs`
  - `lancet ast 'panic!($$$ARGS)' src --lang rs`
  - `lancet ast '$X == $X' src --lang py`
- **Always preview rewrites first** (default `--rewrite` prints a unified diff without touching disk), then pass `--apply`:
  - `lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'`
  - `lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?' --apply`
  - `--apply` writes every rewritten file or none; a rewrite that adds syntax errors is refused (`--allow-syntax-errors` overrides).

## 6. Edit by Coordinates (`lancet edit` — replaces `sed -i` / hand-written patches)

- Address lines exactly as `lancet read` does, then replace, delete, or insert:
  - `lancet edit src/rg.rs --symbol run --with-file new_fn.rs`
  - `lancet edit src/rg.rs --lines 120:135 --with-file - < patch.txt`
  - `lancet edit src/rg.rs --lines 42 --insert-after '    log::debug!("x");'`
  - `lancet edit src/rg.rs --lines 42:44 --delete`
- **Exact text instead of line numbers** (like a `str_replace` edit tool): `--find TEXT` changes that text, which may be part of a line or span several lines. It must occur exactly once, otherwise the edit is refused and the error lists the lines where it occurs — add surrounding text until it is unique, or pass `--all` to change every occurrence. `\n` in the text also matches CRLF files.
  - `lancet edit src/x.rs --find 'let retries = 3;' --with 'let retries = 5;' --rev <REV> --apply`
  - In `--edits`: `{"find": "old_name(", "with": "new_name(", "all": true}`
- `--symbol` covers the doc comments and attributes above the declaration, exactly as `read --symbol` shows them; include them in the replacement or they are removed.
- **Several changes to one file → one call.** Put them in `--edits` (JSON file or `-` for stdin), all using the line numbers you *read*; lancet resolves every hunk against the original file, so earlier hunks never shift later ones. Overlapping hunks are refused; either all hunks land or none do.
  - `lancet edit src/x.rs --rev <REV> --apply --edits - <<'JSON'`
    `[{"lines": "10:12", "with": "..."}, {"symbol": "old_fn", "delete": true}, {"lines": "40", "insert_after": "..."}]`
    `JSON`
- **Always pass the `rev` you read** (`lancet read --json` → `"rev"`): `--rev <REV>` makes the edit fail with `conflict` if the file changed since. Chain edits with the returned `rev_after`.
- **Several agents on one repository:** the read → `--rev` → `--apply` cycle is a compare-and-swap. lancet locks the file while it re-checks the rev and writes, so when two agents edit the same revision exactly one wins and the other gets `conflict` (exit 2) instead of overwriting it. On `conflict`: `lancet read` again, re-apply your change to the new content, retry with the new `rev`. Never retry blindly with `--allow-syntax-errors` or without `--rev`.
- Default is a dry-run diff; add `--apply` to write atomically. Edits that **increase** the number of Tree-sitter syntax errors are refused (a file that is already broken can still be repaired step by step); `--allow-syntax-errors` overrides. PDFs, Office files, HWP, and archive entries are read-only.

## 7. Apply Unified Diffs (`lancet patch` — replaces `patch` / `git apply`)

- Apply a unified diff (`git diff` or `diff -u`) from a file or stdin (`-`):
  - `lancet patch changes.diff` (dry-run preview)
  - `git diff | lancet patch --apply`
  - `lancet patch changes.diff --root subdir -p 0 --apply`
- Handles what `git diff` prints: renames (`git diff -M`, with or without content changes), mode changes (`chmod +x`), new and deleted files including empty ones. Binary changes and copies are refused, and nothing in the patch is written.
- Flags: `--root <DIR>` (working directory for patch paths, default `.`), `-p N` / `--strip N` (leading path components to strip, default `1` for `a/` and `b/`), `--apply`, `--allow-syntax-errors`, `--json`.
- **Exactness & atomicity:** Every hunk's context and removed lines must match exactly (relocates to the nearest unique exact offset if line numbers shifted; never fuzzy). All files in the patch are verified before anything is written, writes use locked compare-and-swap with rollback if a concurrent write wins a race, and any net increase in Tree-sitter syntax errors is refused unless `--allow-syntax-errors` is set. Default is dry-run unless `--apply` is passed.

## 8. Rename Local Bindings (`lancet rename`)

- Rename a variable, closure parameter, loop variable, or supported function parameter inside a single file with scope proof:
  - `lancet rename src/rg.rs:42 old_name new_name` (dry-run diff)
  - `lancet rename src/rg.rs:42:9 old_name new_name --rev <REV> --apply`
- **Supported languages (6):** Rust (`rs`), Python (`py`), JavaScript (`js`), TypeScript (`ts`), TSX (`tsx`), and Go (`go`).
- **Safety by design:** Refuses top-level/module/exported symbols, struct fields, methods, Python parameters (keyword-arg callers can cross files), macros, JSX tag names, Go composite-literal keys, dynamic scopes (`eval`/`with`/`locals()`/`global`/`nonlocal`), and any rename whose post-rename scope re-resolution detects shadowing or variable capture.
- When `lancet rename` refuses a cross-file or top-level symbol, inspect occurrences with `lancet refs <SYMBOL>` and apply explicit updates with `lancet edit --edits` or `lancet ast --rewrite`.

## 9. MCP Server & Official Skill

- **Built-in stdio MCP server:** Run `lancet mcp --root .` (add `--read-only` to hide mutating tools from `tools/list` and block disk writes). See [`./mcp.md`](./mcp.md) for all 11 tools and ready-to-paste client configs (Claude Code, Cursor, Codex, Antigravity, Claude Desktop).
- **Official Skill package:** Install [`../skills/lancet/SKILL.md`](../skills/lancet/SKILL.md) and its `references/` files into `~/.claude/skills/`, `~/.codex/skills/`, `~/.gemini/config/skills/`, or `.agents/skills/`.
