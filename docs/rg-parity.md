# `lancet rg` — ripgrep Feature Parity

`lancet rg` provides ripgrep's flags, Rust `regex` engine, `.gitignore` / `.ignore` / `.rgignore` traversal rules, exit codes (`0` match, `1` no match, `2` error), and output formats on top of Lancet's always-fresh trigram index and in-process document/archive extractors.

## Verification

- **Fixture parity (`bench/rg_parity.py`):** 68 edge-case queries (Unicode case folding, CRLF, missing final newline, binary files, hidden/ignored files, nested directories, adjacent word boundaries, `-m` with `-C`, `--vimgrep`, `--files`, `--max-depth`, `--max-filesize`, `-L`/`--follow`, etc.) match `ripgrep 15.2` byte-for-byte in stdout and exit code.
- **Linux kernel parity:** Full-tree queries on Linux kernel v6.12 match `ripgrep 15.2` byte-for-byte both with and without `.lancet` (3.7–8.3× faster with the index).

---

## Flag & Feature Comparison

### Patterns & Matching

| Flag | Meaning | Status | Notes |
|---|---|---|---|
| `-e`, `--regexp <PATTERN>` | Pattern to search (repeatable) | Supported | Combined with alternation |
| `-f`, `--file <FILE>` | Read patterns from file, one per line (`-` for stdin) | Supported | |
| `-F`, `--fixed-strings` | Treat patterns as literal strings | Supported | |
| `-i`, `--ignore-case` | Case-insensitive matching | Supported | Last of `-i`/`-s`/`-S` wins |
| `-s`, `--case-sensitive` | Case-sensitive matching (default) | Supported | Last of `-i`/`-s`/`-S` wins |
| `-S`, `--smart-case` | Case-insensitive unless pattern has an uppercase literal | Supported | AST-aware in regex mode (classes like `\W` don't trigger case sensitivity) |
| `-w`, `--word-regexp` | Match only surrounded by word boundaries | Supported | Matches `ripgrep` boundary behavior (`-w '@x'` works) |
| `-x`, `--line-regexp` | Match only spanning the whole line | Supported | |
| `-v`, `--invert-match` | Print lines that do not match | Supported | |
| `-U`, `--multiline` | Allow matches to span lines | Supported | |
| `--multiline-dotall` | With `-U`, allow `.` to match newlines | Supported | |
| `-P`, `--pcre2` | PCRE2 engine (lookarounds, backreferences) | Unsupported | Uses Rust `regex` for guaranteed linear-time matching (no catastrophic backtracking), sound HIR → trigram index compilation, and zero C PCRE2 dependency |

### Output & Formatting

| Flag | Meaning | Status | Notes |
|---|---|---|---|
| `-o`, `--only-matching` | Print only matched spans | Supported | |
| `-c`, `--count` | Print number of matching lines per file | Supported | |
| `--count-matches` | Print total match count per file | Supported | |
| `-l`, `--files-with-matches` | Print only paths with at least one match | Supported | |
| `--files-without-match` | Print only paths with no matches | Supported | |
| `-q`, `--quiet` | No output; exit `0` on first match | Supported | |
| `-m`, `--max-count <NUM>` | Stop after `NUM` matching lines per file | Supported | Trailing context respects `-m` |
| `-A`, `-B`, `-C` | Lines of after / before / surrounding context | Supported | Context separator `--` emitted properly |
| `-n` / `-N` | Show (`--line-number`) / hide (`--no-line-number`) line numbers | Supported | **Line numbers are ON by default** (see [Intentional Differences](#intentional-differences)) |
| `-H` / `-I` | Always (`--with-filename`) / never (`--no-filename`) print file path | Supported | Last flag wins |
| `--column` | Show 1-based column of first match | Supported | |
| `--vimgrep` | `path:line:col:text` for every match | Supported | |
| `--json` | ripgrep JSON Lines (`begin` / `match` / `context` / `end` / `summary`) | Supported | |
| `-0`, `--null` | NUL byte after file paths | Supported | |
| `-r`, `--replace <TEXT>` | Replace matches in output (`$1`, `${name}`) | Supported | |
| `--sort <SORTBY>` | Sort output | No-op | Output is **always** sorted by path component |
| `--no-heading` | Never print filename headings | No-op | Headings are never used |
| `--color <WHEN>` | Colorize output (`auto`, `always`, `ansi`, `never`) | Supported | `--color auto` (default) colors on TTYs (`NO_COLOR`/`TERM=dumb` respected) and emits plain text when piped |
| `--passthru` | Print both matching and non-matching lines | Unsupported | |

### File Filtering & Traversal

| Flag | Meaning | Status | Notes |
|---|---|---|---|
| `-g`, `--glob <GLOB>` | Include/exclude paths by glob (`!` negates; repeatable) | Supported | Positive globs override hidden/ignore as in ripgrep |
| `--iglob <GLOB>` | Case-insensitive `--glob` | Supported | |
| `-t`, `--type <TYPE>` | Search only files of `TYPE` | Supported | Uses `ignore` crate's ripgrep type table |
| `-T`, `--type-not <TYPE>` | Skip files of `TYPE` | Supported | |
| `--type-list` | Print known file types | Supported | |
| `-a`, `--text` | Search binary files as text | Supported | |
| `-u` / `-uu` / `-uuu` | Unrestricted: no ignore (`-u`), + hidden (`-uu`), + binary (`-uuu`) | Supported | |
| `-.`, `--hidden` | Search hidden files and directories | Supported | |
| `--no-ignore` | Do not respect `.gitignore` / `.ignore` / `.rgignore` | Supported | |
| `--files` | Print files that would be searched | Supported | Respects `-g`, `-t`, `--hidden`, `-u` |
| `-j`, `--threads <NUM>` | Worker thread count | Supported | |
| `-L`, `--follow` | Follow symbolic links | Supported | Follows symlinked files and directories (`--trust-index -L` falls back to live walk so unindexed symlinks are never missed) |
| `--max-depth <NUM>` | Maximum traversal depth | Supported | Enforced in both live walk and `--trust-index` |
| `--max-filesize <NUM>` | Skip files larger than `NUM` (`K`/`M`/`G` suffixes supported) | Supported | Enforced in both live walk and `--trust-index` |
| `-z`, `--search-zip` | Search compressed files via external tools | Not needed | Lancet extracts `.zip`, `.tar`, `.gz`, `.tgz` natively in-process |
| `--pre`, `--type-add` | Preprocessor command / custom file types | Unsupported | |

### Lancet Extensions on `lancet rg`

| Flag / Behavior | Description |
|---|---|
| `--no-index` | Ignore `.lancet` and scan files directly |
| `--trust-index` | Answer from `.lancet` without checking files for changes (mtime, size, ctime, inode) |
| `--index-dir <DIR>` | Custom index directory (default: `PATH/.lancet` or ancestor) |
| `--docs` | Also search inside `.pdf`, `.docx`, `.pptx`, `.odt`, `.hwp`, `.hwpx`, `.epub`, `.xlsx`/`.xls`/`.ods`, `.zip`, `.tar`, `.gz`, `.tgz` in-process. Off by default, so output matches ripgrep (which treats them as binary) |

---

## Intentional Differences

1. **Always path-sorted:** `lancet rg` always sorts output by path component (equivalent to `rg --sort path`, where `a/b.txt` sorts before `a.txt`), making results deterministic across runs and thread counts.
2. **Line numbers enabled by default:** `-n` is on even when stdout is piped or redirected; pass `-N` (`--no-line-number`) to omit line numbers.
3. **Plain output:** `--color` and headings are never emitted; output is always machine-friendly `path:line:text`.
