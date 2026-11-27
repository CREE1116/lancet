# Query Language, `lancet rg`, and `lancet ast` Reference

## 1. Lancet Query Language (LQL)

Strict parser — unknown operators, filters, types, or trailing tokens return `status: "invalid_query"` with a byte offset (`lancet parse <QUERY>` shows canonical form + IR; `lancet explain <QUERY>` shows required trigrams).

### Grammar & Precedence

- **Precedence:** `NOT` > `AND` (explicit or whitespace-adjacent implicit `AND`) > `OR`.
- **Primaries:**
  - `"text"` or bare `token` — **case-insensitive** (Unicode simple case folding) substring match.
  - `regex("pat")` — Rust `regex` in multi-line mode (`^`/`$` per line), **case-sensitive** by default (add `(?i)` for case-insensitive).
  - `fuzzy("word", k)` — token match within char Levenshtein distance `k` (`0 <= k <= 3`, default `1`), index-accelerated via q-gram threshold.
  - `symbol("name")` / `symbol:name` — exact (case-insensitive) Tree-sitter symbol definition (`.rs`, `.py`, `.js`, `.ts`, `.tsx`, `.go`).
  - `near(m1, m2, d)` / `before(m1, m2, d)` — token distance `<= d` within the unit (`before` requires strictly ascending token order).
- **Scopes (`in ...` / filter terms):**
  - Unit scopes: `in file` (default), `in block` (non-blank line run), `in line`, `in function` (Tree-sitter function/method in `.rs`, `.py`, `.js`, `.ts`, `.tsx`, `.go` — sets `symbol` on hits).
  - File filters: `type:CLASS`, `ext:EXT`, `path:GLOB` (multiple values via `ext:(rs,toml)` are OR-ed; distinct categories are AND-ed).
  - `type:` classes: `code`, `document`, `spreadsheet`, `config`, `log`, `data`, `archive`, `image`, `other`.
- **Pipe stages (`| stage`):**
  - `limit N` (`N >= 1`)
  - `count`
  - `files` (compact path-only result per file)
  - `group file`
  - `expand parent` | `expand unit` | `expand context:N`
  - `sort count [asc|desc]` | `sort path`
  - `terms [top:N]` (`1..=100`, default `15` — top co-occurring tokens)
  - `budget [tokens:]N` (estimated token cap; populates `budget`)

### Multi-Query Batching (`lancet batch`)

Use `lancet batch [PATH] -q 'Q1' -q 'Q2' [--name n1 --name n2]` or `--queries-json '{"q1": "...", "q2": "..."}'` to evaluate multiple LQL queries in one shared index open and directory walk.

---

## 2. `lancet rg` (ripgrep Parity & Extensions)

- **Supported flags:** `-e`/`--regexp`, `-f`/`--file`, `-F`, `-i`/`-s`/`-S` (AST-aware smart case), `-w`, `-x`, `-v`, `-U`, `--multiline-dotall`, `-o`, `-c`, `--count-matches`, `-l`, `--files-without-match`, `-q`, `-m`, `-A`/`-B`/`-C`, `-n`/`-N`, `-H`/`-I`, `--column`, `--vimgrep`, `--json`, `-0`/`--null`, `-r`/`--replace`, `-a`, `-u`/`-uu`/`-uuu`, `-.`/`--hidden`, `--no-ignore`, `-g`/`--glob`, `--iglob`, `-t`/`--type`, `-T`/`--type-not`, `--type-list`, `--type-add NAME:GLOB`, `--passthru`, `-L`/`--follow`, `--max-depth NUM`, `--max-filesize NUM[K|M|G]`, `--files`, `--color auto|always|ansi|never`, `-j`.
- **Lancet extensions:** `--no-index`, `--trust-index`, `--index-dir DIR`, `--docs` (searches inside PDF, DOCX/PPTX/ODT, HWP/HWPX, EPUB, XLSX/XLS/ODS, ZIP/TAR/GZ/TGZ in-process).
- **Intentional differences:** always sorted by path component, `-n` stays ON when piped (`-N` disables), `-P`/`--pcre2` is unsupported (guaranteed linear-time matching + sound trigram HIR lowering).

---

## 3. Discovery & Ranked Declarations (`fd --dirs`, `refs --rank`)

- **Directory-only tree (`lancet fd --tree --dirs` / `lancet_fd(dirs: true, tree: true)`):** lists folders only (same as `-t d`) for compact architecture overviews.
- **Usage-ranked declarations (`lancet refs --rank '<GLOB>' [--kind K...]` / `lancet_refs(rank: true)`):**
  - Matches declaration names against a glob (e.g. `*Service`), narrows candidate files via the index using the longest literal substring of the glob, and sorts by lexical reference count descending.
  - Each entry includes `references` and `definitions`; when `definitions > 1`, multiple declarations share the same name (lexical count is shared).

---

## 4. `lancet ast` (24 Languages & Metavariables)

### `--lang` Identifiers

| Language | `--lang` values | Language | `--lang` values |
|---|---|---|---|
| Rust | `rs`, `rust` | Scala | `scala`, `sc` |
| Python | `py`, `python`, `pyi` | Swift | `swift` |
| JavaScript | `js`, `javascript`, `mjs`, `cjs`, `jsx` | Ruby | `rb`, `ruby` |
| TypeScript | `ts`, `typescript`, `mts`, `cts` | PHP | `php` |
| TSX | `tsx` | Bash | `sh`, `bash`, `zsh`, `shell` |
| Go | `go`, `golang` | Lua | `lua` |
| C | `c` | HCL / Terraform | `hcl`, `tf`, `tfvars`, `terraform` |
| C++ | `cpp`, `c++`, `cxx`, `cc`, `hpp`, `hxx`, `hh` | HTML | `html`, `htm` |
| Zig | `zig` | CSS | `css` |
| Java | `java` | JSON | `json`, `jsonc` |
| C# | `cs`, `csharp`, `c#` | YAML | `yaml`, `yml` |
| Kotlin | `kt`, `kts`, `kotlin` | TOML | `toml` |

### Metavariables & Rewrite Rules

- `$VAR`: matches a single named AST node; repeated `$VAR` (e.g. `$X == $X`) requires structurally equal subtrees.
- `$_`: anonymous single-node wildcard (not captured).
- `$$$ARGS`: matches zero or more sibling AST nodes (bounded memoization with per-node step budget).
- C-like statement metavariables: patterns like `if ($C) $S` and `{ $$$B }` work; `if ($C) $S` distinguishes `if` from `if-else`.
- Rewrite safety: `--rewrite <TEMPLATE>` is dry-run unless `--apply`; `--apply` refuses `--limit`; overlapping nested matches inside one file are skipped during rewrite; `--lang cpp` scans `.h` files as well as `.cpp`/`.cc`.
