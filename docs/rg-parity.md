# `lancet rg` — ripgrep Feature Parity

`lancet rg` takes ripgrep's flags and prints ripgrep's output, with ripgrep's Rust `regex` engine, traversal rules (`.gitignore`, `.ignore`, `.rgignore`, `--ignore-file`), decoding (BOM sniffing, `-E`), binary-file handling and exit codes (`0` match, `1` no match, `2` error), on top of Lancet's always-fresh trigram index.

Every flag of ripgrep 15.2's `--help` is listed below: **supported**, **accepted with no effect** (it changes nothing observable in lancet), or **not supported** with the reason.

## Verification

- **Fixture parity (`bench/rg_parity.py`):** 138 cases — every supported flag below plus Unicode case folding, CRLF, missing final newline, UTF-8/UTF-16 BOMs, latin1, NUL-separated data, binary files named and in directories, `.gz`, hidden / ignored / `.ignore` / `.rgignore` / `.git/info/exclude` / `--ignore-file` rules, missing paths — compare stdout and exit code with `ripgrep 15.2` byte for byte (`--stats` timing lines excluded).
- **Real trees:** Linux v6.12, LLVM, rust, kubernetes, VS Code, Django, CPython and Spring queries match `ripgrep 15.2` byte for byte with and without `.lancet` (see [`bench/results/SUITE.md`](../bench/results/SUITE.md)).

---

## Patterns & matching

| Flag | Status | Notes |
|---|---|---|
| `-e`, `--regexp PATTERN` | Supported | Repeatable; combined by alternation |
| `-f`, `--file FILE` | Supported | One pattern per line; `-` reads stdin |
| `-F`, `--fixed-strings` | Supported | |
| `-i` / `-s` / `-S` (`--ignore-case` / `--case-sensitive` / `--smart-case`) | Supported | Last one wins; smart case looks at literals only (`\W` doesn't count) |
| `-w`, `--word-regexp` | Supported | ripgrep's boundaries (`-w '@x'` works) |
| `-x`, `--line-regexp` | Supported | |
| `-v`, `--invert-match` | Supported | |
| `-U`, `--multiline` / `--multiline-dotall` | Supported | |
| `--crlf` | Supported | `$` matches before `\r\n`; lancet-added terminators become `\r\n` |
| `--null-data` | Supported | NUL ends lines (and output lines) |
| `--no-unicode` | Supported | ASCII `\w`, `\d`, `\b`, case folding |
| `-P`, `--pcre2`, `--engine`, `--auto-hybrid-regex`, `--pcre2-version`, `--no-pcre2-unicode` | **Not supported** | Rust `regex` only: matching stays linear-time and the index can derive a sound trigram filter from every pattern. `--engine default` and `--no-pcre2-unicode` are accepted |
| `--dfa-size-limit`, `--regex-size-limit` | Accepted, no effect | lancet's regex size limit is fixed at 256 MiB |

## Output

| Flag | Status | Notes |
|---|---|---|
| `-o`, `--only-matching` | Supported | |
| `-c`, `--count` / `--count-matches` / `--include-zero` | Supported | |
| `-l`, `--files-with-matches` / `--files-without-match` | Supported | |
| `-q`, `--quiet` | Supported | |
| `-m`, `--max-count NUM` | Supported | Trailing context still printed, as in ripgrep |
| `-A` / `-B` / `-C`, `--passthru` | Supported | The last of `--passthru` / `-A` / `-B` / `-C` wins |
| `--context-separator SEP`, `--no-context-separator` | Supported | |
| `--field-match-separator SEP`, `--field-context-separator SEP` | Supported | |
| `-n` / `-N`, `-H` / `-I` (`--with-filename` / `--no-filename`) | Supported | Line numbers are **on by default** (see below) |
| `--column`, `-b`, `--byte-offset` | Supported | `-b` gives the match's offset with `-o` |
| `--vimgrep`, `--json` | Supported | `--json` reports `binary_offset` for binary files |
| `-0`, `--null`, `--path-separator SEP` | Supported | |
| `-r`, `--replace TEXT` | Supported | `$1`, `${name}` |
| `--trim` | Supported | |
| `-M`, `--max-columns NUM`, `--max-columns-preview` | Supported | Measured with the line terminator, as ripgrep does |
| `--heading` / `--no-heading`, `-p`, `--pretty` | Supported | Off by default (see below) |
| `--color WHEN` | Supported | `auto` (default): color on a terminal only; `NO_COLOR` / `TERM=dumb` respected |
| `--colors SPEC` | Accepted, no effect | lancet's colors are fixed |
| `--hyperlink-format`, `--hostname-bin` | Accepted, no effect | No hyperlinks are printed |
| `--stats` | Supported | "bytes searched" for **binary** files counts the whole decoded file; ripgrep counts only what it read before stopping |
| `--sort` / `--sortr` (`path`, `modified`, `accessed`, `created`, `none`), `--sort-files` | Supported | Paths keep the order given on the command line and are sorted within each, as `rg --sort path` does |
| `--line-buffered`, `--block-buffered` | Accepted, no effect | Output is written per file |

## Files & traversal

| Flag | Status | Notes |
|---|---|---|
| `-g`, `--glob`, `--iglob`, `--glob-case-insensitive` | Supported | Positive globs override hidden/ignore rules, as in ripgrep |
| `-t` / `-T`, `--type-add`, `--type-clear`, `--type-list` | Supported | ripgrep's type table |
| `-.`, `--hidden`, `--no-ignore`, `-u` / `-uu` / `-uuu` | Supported | |
| `--ignore-file PATH`, `--no-ignore-files`, `--ignore-file-case-insensitive` | Supported | `--ignore-file` applies even with `--no-ignore`, as in ripgrep |
| `--no-ignore-vcs`, `--no-ignore-dot`, `--no-ignore-parent`, `--no-ignore-global`, `--no-ignore-exclude`, `--no-require-git` | Supported | These (and `--one-file-system`) search with a live walk instead of the index |
| `--no-ignore-messages` | Accepted, no effect | |
| `-L`, `--follow`, `--max-depth`, `--max-filesize`, `--one-file-system` | Supported | |
| `--files` | Supported | |
| `-j`, `--threads` | Supported | |
| `--no-messages` | Supported | Missing paths are reported (unless this flag) and the other paths are still searched; exit `2` |
| `--mmap` / `--no-mmap`, `--no-config` | Accepted, no effect | lancet reads no config file |

## Input

| Flag | Status | Notes |
|---|---|---|
| (default) BOM sniffing | Supported | A UTF-8 BOM is dropped; UTF-16 LE/BE files are transcoded |
| `-E`, `--encoding ENC` | Supported | `auto`, `none`, or any WHATWG label (`latin1`, `sjis`, `utf-16le`, …) |
| `-a`, `--text` | Supported | |
| `--binary` | Supported | Binary files met in directories are searched like named ones: NULs become line breaks, and text output is one `binary file matches (found "\0" byte around offset N)` line |
| `-z`, `--search-zip` | Partly | `.gz` is decompressed in-process. ripgrep's other formats (`.bz2`, `.xz`, `.lz4`, `.zst`, `.Z`, `.br`) need external programs and are not supported. `--docs` reads `.zip`, `.tar`, `.tgz` and documents |
| `--pre COMMAND`, `--pre-glob GLOB` | **Not supported** | lancet doesn't run a process per file; documents and archives are read in-process with `--docs` |
| `--debug`, `--trace` | Accepted, no effect | |
| `-V`, `--version` | Supported | Prints lancet's version |
| `--generate KIND` | **Not supported** | No man page or shell-completion generator yet |

## Lancet additions

| Flag | Description |
|---|---|
| `--docs` | Also search inside `.pdf`, `.docx`, `.pptx`, `.odt`, `.hwp`, `.hwpx`, `.epub`, `.xlsx`/`.xls`/`.ods`, `.zip`, `.tar`, `.gz`, `.tgz` (off by default, so output matches ripgrep) |
| `--no-index` / `--trust-index` / `--index-dir DIR` | Hidden from `--help`. The default uses `.lancet` when present and re-checks changed files, so results never go stale; `--trust-index` skips that check |

---

## Intentional differences

1. **Always sorted by path** (as `rg --sort path`), so output is the same on every run and thread count.
2. **Line numbers on by default**, even when piped, so coordinates survive between an agent's steps; `-N` turns them off.
3. **No headings unless asked** (`--heading` / `--pretty`): output is `path:line:text`.
