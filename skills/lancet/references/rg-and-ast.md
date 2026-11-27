# `lancet rg` and `lancet ast` Reference

## 1. `lancet rg` (ripgrep Parity & Extensions)

- **Flags:** all of ripgrep 16.0's, with the same output and exit codes, except `-P`/`--pcre2`, `--pre`/`--pre-glob` and `--generate` (refused with a reason); `-z` decompresses `.gz` only. Full table: `docs/rg-parity.md`.
- **Lancet extensions:** `--no-index`, `--trust-index`, `--index-dir DIR`, `--docs` (searches inside PDF, DOCX/PPTX/ODT, HWP/HWPX, EPUB, XLSX/XLS/ODS, ZIP/TAR/GZ/TGZ in-process).
- **Intentional differences:** always sorted by path, `-n` stays ON when piped (`-N` disables), no headings unless `--heading`/`--pretty`.

---

## 2. Discovery & Ranked Declarations (`fd --dirs`, `refs --rank`)

- **Directory-only tree (`lancet fd --tree --dirs`):** lists folders only (same as `-t d`) for compact architecture overviews.
- **Usage-ranked declarations (`lancet refs --rank '<GLOB>' [--kind K,...]`):**
  - Matches declaration names against a glob (e.g. `*Service`), narrows candidate files via the index using the longest literal substring of the glob, and sorts by lexical reference count descending.
  - Each entry includes `references` and `definitions`; when `definitions > 1`, multiple declarations share the same name (lexical count is shared).

---

## 3. `lancet ast` (24 Languages & Metavariables)

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
