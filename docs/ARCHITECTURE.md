# Lancet: Product Rules and Architecture

English · [한국어](ARCHITECTURE.ko.md)

This document is the standard for changing lancet or adding to it. The document changes first; the code follows it.

Status: **settled (2026-11-28)**. The cleanup and steps 0–5 were done on the `restructure` branch.

---

## 0. Goal

**lancet unifies and replaces existing tools.** The tools agents and people use to find, read and change files (ripgrep, fd, the Read/Grep/Glob/Edit/Write tools of Claude Code and OpenCode, ast-grep, apply_patch and git apply) must be replaceable by lancet alone.

| lancet | Replaces |
|---|---|
| `rg` | ripgrep, Grep |
| `fd` | fd, Glob |
| `read` | Read, cat |
| `edit` | Edit |
| `write` | Write |
| `patch` | apply_patch, git apply |
| `ast` | ast-grep |

Commands no existing tool has but that an agent needs to change files: `refs` (definitions and uses), `rename` (local renames), `outline` (file structure). Commands needed for operation: `index` (speed), `mcp` (clients without a shell).

**Nothing outside this table gets built.** A new feature goes in only if it (1) replaces a feature of an existing tool, or (2) connects the replaced tools (one coordinate system, safe writes).

## 1. Product principles

Whether a feature goes in is judged against these.

1. **Accuracy is never traded.** Nothing is missed or wrong for the sake of speed. The index only narrows candidates; the real engine always decides.
2. **When unsure, refuse.** A change lancet can't be sure of (a name inside a macro, the same name with another meaning, a file that changed meanwhile) is not guessed: lancet says why and stops.
3. **Results are always fresh.** With or without an index, stale or not, the result is the same.
4. **Follow the rules people already know.** Use the syntax and behaviour of tools agents and people already know (ripgrep, fd, the Read/Edit/Write tools of Claude Code and OpenCode, git diff, Codex patches). lancet-only syntax exists only for features no existing tool has.
5. **One coordinate system.** Every command uses the same path spelling (relative to the PATH given) and the same line numbers. One command's output can be fed straight into the next.
6. **Output is deterministic.** The same input gives the same output on every run, whatever the thread count (path order).
7. **Writes are safe.** Nothing is written over content you didn't read (rev), and a change to several files changes all of them or none.

### Existing rules lancet follows

| Rule | Reference tool | lancet |
|---|---|---|
| Flags, output, exit codes | ripgrep 16.0 | `rg` (byte-identical; three differences in `docs/rg-parity.md`) |
| Several paths `[PATH]...` | rg, fd, ast-grep | every command that takes paths |
| 2,000 lines without a range, 2,000 characters per line | Claude Code / OpenCode Read | `read` |
| `old_string` / `new_string` / `replace_all` | Claude Code / OpenCode Edit | `edit` |
| Unified diff, Codex `*** Begin Patch` | git, Codex | `patch` |
| Exit codes 0 done / 1 nothing / 2 error or refusal | grep, ripgrep | every command |

---

## 2. Architecture

### 2.1 Cost of the structure as it was (measured 2026-11)

| Measured | Result | Cost |
|---|---|---|
| Candidate planning (paths → index → freshness → target files) | 4 copies in `rg`, `fd`, `refs`, `ast` → **after step 1, one `scope::plan` (+ `fd`)** | one path fix touched four places |
| Per-language `match Lang::X` branches | 129 (`code.rs` 103, `ast_grep.rs` 22, `rename.rs` 4) → **after step 2, 2 (`LangSpec` table)** | adding or fixing a language touched dozens of places |
| Tree-sitter parsers and caches | 3 (`code.rs`, `ast_grep.rs`, `rename.rs`) → **after step 2, 1** | |
| Parses per file in `refs` | 4 (symbols, function spans, identifiers, scopes each parsed) → **after step 2, 1** | lost performance |
| Write rules | `edit --edits` and `patch` all or nothing, `ast --apply` per file → **after step 3, all through `change::commit`** | safety differed by command |
| Walk → search | search starts after the walk finishes | the two never overlap |
| `rg` output | all results gathered in memory, then printed in path order | memory and first-output latency on big outputs |
| Command list | separate in the top-level help, the MCP description and the skill table → **after step 4, one `commands::COMMANDS`** | drifted with every change (`refs` and `ast` showed a single `[PATH]`) |

### 2.2 Layers

Upper layers use only lower layers. Modules in one layer don't call each other.

```
 Interface   cli · mcp · command list (registry) · renderers (human / json / rg format / vimgrep)
     │
 Commands    rg  fd  read  outline  refs  ast  edit  write  patch  rename  index
     │        (arguments → core calls → result values; they don't know the output format)
     │
 Core        Scope ──▶ Pipeline ──▶ Doc ──▶ Syntax          Change
             (which files) (parallel, order) (content) (code structure) (safe writes)
     │
 Engine      walk · index(segment, req) · fold · matcher · write(rev, CAS)
             adapters · LangSpec table · tree-sitter
```

### 2.3 Patterns used, and why

Patterns go only where change actually happens (the seams). Elsewhere, functions and structs are enough.

#### ① Staged checks, cheapest first (performance)

Every search command's matcher has the same shape:

```rust
trait Matcher: Sync {
    fn req(&self) -> Req;               // stage 1: trigram requirement for the index (Req::All if unknown)
    fn literal(&self) -> Option<&[u8]>; // stage 2: literal checked with memmem on the file's bytes
    fn find(&self, doc: &Doc) -> Vec<Hit>; // stage 3: the real match (regex, identifier, AST pattern)
}
```

- Index (thousands of files → dozens) → literal (byte comparison, no parsing) → real match: expensive tree-sitter parsing happens only in files that contain the literal.
- `rg` (regex), `refs` (identifiers) and `ast` (AST patterns) run the same pipeline. A new search feature implements one `Matcher` and gets the index, freshness, parallelism, ordering and several paths for free.
- **Step 2 decision: no trait.** All three commands already follow this order (index requirement in `scope::plan`, then `rg` runs its buffered regex, and `refs` and `ast` parse after a memmem literal check). The per-file work differs by command (rg output, refs classification, ast rewriting), so nothing would consume the three implementations together, and the trait would be renaming for its own sake (2.4). Revisit if a fourth search command appears.

#### ② Parse once, compute only what is needed (performance)

```rust
struct Doc { bytes: Content, kind: DocKind, lines: OnceCell<LineTable>, tree: OnceCell<Option<Tree>>, rev: OnceCell<Rev> }
// Syntax views are computed from the same tree on demand and cached
doc.syntax()?.symbols()  /  .identifiers(name)  /  .scopes()
```

- `refs` goes from 4 parses to 1. The line table and the rev are also computed once, when first needed.
- One tree-sitter parser per thread (3 → 1).
- **Step 3 decision: no lazily computed `Doc` struct.** After step 2 no command computes a file's line table, tree or rev twice, so there is nothing to cache for (2.4). The content core is `content` on the read side (`read`, `load_searchable`) and, on the write side, `change::load` (document, binary and UTF-8 checks, shared lock, rev) with `change::line_starts` and `eol`.

#### ③ Languages are data, not code (maintainability)

```rust
static LANGS: &[LangSpec] = &[LangSpec {
    name: "rust", exts: &["rs"], grammar: tree_sitter_rust::LANGUAGE,
    symbols: &[("function_item", "function"), ("struct_item", "struct"), ...],
    comment_kinds: &["line_comment", "block_comment"], string_kinds: &[...],
    scopes: Some(&RUST_SCOPES),   // only languages with a scope resolver
}, ...];
```

- The 129 `match Lang::X` become table lookups. Adding a language is a table row and a test.
- Only the parts whose logic really differs per language, like scope resolution, get their own code (Rust, Python, JS/TS, Go).
- `cfg(feature = "ast")` wraps this table and tree-sitter in one place (it was spread over five files).

#### ④ Separate planning from doing (maintainability, safety)

Every write command splits into a side-effect-free plan and one place that writes.

```rust
let text = change::load(path)?;          // read: refuse documents, binaries, non-UTF-8; shared lock; rev
// the command's plan: writes nothing; produces the diff and runs change::added_syntax_errors
let set: Vec<FileChange> = plan(args, text)?;
change::commit(&set)?;                   // all or nothing; on failure the files already written are put back
```

- **Step 3 implementation:** `&[FileChange]` (update, create, delete, permissions) and `change::commit` instead of a `ChangeSet` type, which would have had nothing to do. Syntax error messages differ by command (`edit`, `patch`, `rename`, `ast`), so each plan calls `change::added_syntax_errors`.
- `edit`, `write`, `patch`, `rename` and `ast --rewrite` all build `FileChange`s. A preview (no `--apply`) stops after planning; applying goes through `commit`.
- The safety rules (CAS, atomic writes, rollback, syntax check) live in one place, so they cannot differ between commands.
- **Behaviour change:** `ast --apply` became all or nothing.

#### ⑤ Separate result values from output formats (maintainability)

Commands return result values (`SearchResult`, `ReadResult`, `ChangeResult` …); renderers choose the format.

- Human output, `--json` and ripgrep's formats (context, `--vimgrep`, `--json` events, `-c`, `-l` …) are renderers, so rg's dozens of output flags don't leak into command logic.
- `--json` uses the same envelope in every command (`status`, `warnings`, `error`).
- Errors are one type, `Refusal { status, message, hint }`; the exit code (0/1/2) is decided from the status in one place in the interface.
- **Step 4 decisions:**
  - Every command except `rg` already returns a result struct (`*Response`), drawn by `print_*_human` for people and by serde for `--json`. A separate renderer module would only move files, so it wasn't made.
  - `rg`'s output is ripgrep's format itself (compared byte for byte) and is streamed. Gathering values and rendering afterwards would cost memory and first-output latency, so it stays in `rg.rs`.
  - `--json` envelope: every response has `status` and, on failure, `error`; commands that produce warnings (`rg`, `fd`, `refs`, `ast`) have `warnings`. Commands without warnings don't get a field that would always be empty.
  - `Refusal { status, message }` moved to `output.rs` next to `Status` (`hint` had no users and was dropped). Exit codes come from `Status::exit_code` (10 copies → 1). The `(Status, String)` errors inside commands are local values that become the response's `ErrorInfo`, so they stay.

#### ⑥ Process in parallel, stream in order (performance)

```
walk (parallel) ──files──▶ match (rayon) ──(seq, result)──▶ reorder buffer ──▶ renderer ──▶ stdout
```

- Matching starts before the walk finishes (today it starts after).
- Results carry their path sequence number and a reorder buffer emits them in order: output stays in path order, first output comes sooner, and memory is bounded by the buffer.
- Path order needs a sorted walk, so numbering walk results is measured in step 1 first. **No gain measured, no change.**
- **Step 1 result: not adopted.** Searching on the walk threads (+6%) and streaming walk → rayon (+12%) were both slower than "walk, then search on all cores". Numbers in `bench/results/baseline-step1.md`.

#### ⑦ One command list (maintainability)

```rust
// src/commands.rs — columns: name, usage, summary, writes, searches
cmd("refs", "refs SYMBOL [PATH...]", "definitions and references (`--rank` for the most used)", false, true),
```

- The top-level help, the MCP tool description, MCP's `--read-only` check and the skill-table check all come from here.
- A test checks that `skills/lancet/SKILL.md` matches this list.
- **Step 4 implementation:** the help's command list (`commands::help_list`) and MCP's which-command / writes / searches checks read this table. A test checks that the binary's runner table (`RUNNERS` in `main.rs`) has the same names in the same order. The skill check makes sure every command but `mcp` appears in `SKILL.md` as `lancet NAME `.

#### ⑧ Values derived from content can be stored in the index (later, performance)

Segments store each file's trigrams and metadata, and freshness is judged from the metadata. Storing each file's symbol table in the segment the same way would let `refs --rank` and `outline` run without parsing on big repositories, with the same freshness rules, so results would never lag. Measured and decided after ② and ③.

- **Step 5 result: not yet.** Measuring it turned up two quadratic bugs in `refs --rank`, now fixed (a comment/string check that walked up the ancestors of every leaf node, and line numbers counted from the start of the file for every symbol). `refs --rank '*'` on all of Linux went from over 27 minutes to 26 s (rustc_middle 0.47 → 0.06 s, same output). What remains is Tree-sitter parsing itself: `refs --rank '*_ops' --kind struct` on all of Linux takes 8.4 s and `outline linux/drivers` 14 s, while the scopes agents actually use (a file, a directory, a pattern in a mid-size repository) take 30–50 ms. Symbol tables in segments mean a format change (`LANCET05`), and storing the per-file token counts `--rank` needs would grow the index a lot. The gain is only for structural queries over a whole huge repository; revisit when agent logs show such queries.

### 2.4 What we won't do

- Plugin systems, dynamic loading, async runtimes, daemons. lancet is a CLI that starts fresh every time, and that is a strength.
- Traits and generics away from the seams. With one implementation, a function is enough.
- Optimizations without measurements. ⑥ and ⑧ go in only with a measured gain.

### 2.5 Commands and cores

| Command | Scope / Pipeline | Matcher | Doc | Syntax | Change | Command's own part |
|---|---|---|---|---|---|---|
| rg | ✓ | regex | decode | | | rg renderer |
| fd | ✓ (files only) | path glob | | | | tree renderer |
| read | | | ✓ | symbols | | choosing ranges, pages, sheets |
| outline | ✓ (files only) | | ✓ | symbols | | table of contents |
| refs | ✓ | identifier | ✓ | identifiers, scopes | | definition / use classification, `--rank` |
| ast | ✓ | AST pattern | ✓ | tree | ✓ | rewrite templates |
| edit / write | | | ✓ | | ✓ | addressing |
| patch | | | ✓ | | ✓ | diff and Codex parsers |
| rename | | | ✓ | scopes | ✓ | conflict checks |
| index | ✓ (files only) | | | | | building and merging segments |

### 2.6 Target directories

Actual layout after step 4: the command interface is split into `src/cli/` (one file per command: clap arguments, `main(args)`, output) and a 76-line `main.rs` (the runner table and the top-level help). The cores (`scope.rs`, `code.rs`, `scopes.rs`, `change.rs`) and the engine still sit directly under `src/`. Moving directories only changes paths, so it was left to step 5 with the docs.

**Step 5 decision: don't move.** The layers already show in the module names and in `lib.rs`'s doc comment; moving into `core/` and `engine/` would change only public paths (`lancet::scope` …), tests and `git blame` (2.4). The picture below stays as a map of the layers.

```
src/
  engine/   walk.rs  index/  fold.rs  matcher.rs  write.rs  adapters/  lang/{spec.rs, rust.rs, python.rs, …}
  core/     scope.rs  pipeline.rs  doc.rs  syntax/{mod.rs, symbols.rs, scopes/}  change.rs  refusal.rs
  cmd/      rg.rs  fd.rs  read.rs  outline.rs  refs.rs  ast.rs  edit.rs  write.rs  patch.rs  rename.rs  index.rs
  iface/    commands.rs  cli/  mcp.rs  render/{human.rs, json.rs, rg.rs}
```

---

## 3. Maintenance rules

### 3.1 How to change things

1. **Features go into a core, not into a command.** Behaviour two or more commands could use lives in a core. The second time the same code is written, it moves to a core on the spot.
2. **One truth per behaviour.** Help text, defaults, limits (2,000 lines …) and language data come from one place: the command list, LangSpec, or a constant.
3. **A behaviour change updates everything in the same commit:** code, tests, help, the skill (`skills/lancet/`) and `docs/`. The docs site (`docs/site/`) is synced separately.
4. **A new flag or command checks principle 4 first.** If an existing tool has the feature, use its name and behaviour.
5. **Problems from real use come first.** Friction seen in agent logs beats speculation.
6. **Performance changes come with numbers.** Nothing goes in on "it should be faster" without before/after measurements.

### 3.2 Verification assets (the spec)

These must keep passing through any structural change.

| Asset | Protects | Where | When |
|---|---|---|---|
| Unit and integration tests | each command's behaviour, index = scan, freshness, fuzzing | `tests/`, in modules | CI (all OSes) |
| rg parity, 150 cases | stdout and exit code byte-identical to ripgrep 16.0 | `bench/rg_parity.py` | CI (ripgrep 16.0.0 pinned) |
| diff ↔ patch round trip | printed diffs apply back with lancet `patch` and `patch(1)` (CRLF, final newline, big files) | `tests/integration.rs` `printed_diffs_apply_back_with_patch` | CI |
| Real-repository comparison | same as rg on 8 repositories, speed | `bench/suite.py` | every step |
| rename oracle (Rust, Python, Go) | after a rename, code still compiles and symbol tables match | `bench/rename_oracle.py` | when `rename` or `Syntax` changes |
| Per-command timings | before/after each step (parse costs included) | `bench/baseline.py`, `bench/results/baseline-*.md` | every step |
| Agent A/B | accuracy, time and round trips of lancet vs the built-in tools in real use | `bench/agents/` | after big changes |

### 3.3 Performance bar

- Never slower than ripgrep on the real-repository comparison (Linux tree: 193 ms today, on par with rg).
- Compare before/after with `bench/suite.py` each step; more than 5% slower means finding the cause.
- `refs`, `outline`, `ast` and `read --symbol` are timed separately with `bench/baseline.py` (to see ②'s effect). Baseline: `bench/results/baseline-step0.md`.

### 3.4 Dependencies

- No new crate for what a few lines can do.
- No external programs (the one exception is tesseract for OCR, only with `--ocr`).

---

## 4. Migration plan

No big-bang rewrite. Every step must pass all the assets in 3.2 before the next one.

| Step | What | Done when |
|---|---|---|
| Cleanup (done) | Removed the query language and `parse`, `explain`, `batch`; `status` → `index --status`. The 7 engine tests that went through the query evaluator (index = scan, incremental sync, LSM merge, freshness, fuzzing, document conversion, damaged documents) moved to `rg` and stay. | all tests and 138 rg parity cases pass |
| 0. Baseline (done) | Brought the rename oracle (`bench/rename_oracle.py`), the diff ↔ patch round-trip test and the agent A/B (`bench/agents/`) into the repository. Moved rg parity to ripgrep 16.0, 150 cases in CI (implemented `--in`, `--in0`, `--max-bytes`, `--no-pre`). Baseline in `bench/results/baseline-step0.md`; README performance tables re-measured. Fixed bugs found on the way in diff (final newline, CRLF, big files → Myers) and trust mode (requests outside the index's scope). | 150 parity cases pass in CI |
| 1. Scope (done) | Merged candidate planning for `rg`, `refs` and `ast` into one `scope::plan` (several paths, shown paths `AsGiven`/`Clean`, dedup, index / freshness / trust decisions). `fd` lists directories too and reads listings straight from the index, so it stays separate. ⑥ not adopted: both variants measured slower. "12% slower than rg without the index" was a timing artifact (interleaved: −1% to +5%). | planning code 4 → 1 (+`fd`), all assets pass, baseline unchanged |
| 2. LangSpec + Syntax (done) | Parse once with `code::Syntax` (refs parses 4 → 1: Go 201 → 115 ms, Rust 734 → 639 ms), tree-sitter parsers 3 → 1 (`code::parse_tree`), language data in the `LangSpec` table (language branches 129 → 2; the two left are logic), scope resolver split into `scopes.rs`. No `Matcher` trait (see ①). The `Doc` core moved to step 3, which handles the read paths. | outline, refs --rank and ast outputs identical to step 0 in 7 languages; rename oracle passes |
| 3. Change (done) | The five write commands (`edit`, `write`, `patch`, `rename`, `ast --rewrite`) read through `change::load` and write through `change::commit`; write/rollback code 4 → 1. Behaviour changes: `ast --apply` is all or nothing, rewrites that add syntax errors are refused (`--allow-syntax-errors`), `rename` refuses binaries too. No lazy `Doc` struct (see ②). Fixed a rename-oracle false alarm on files Python 3.9 can't parse (`match`). Cost: `ast --rewrite` previews parse each changed file once more (42 Rust files: 35 → 69 ms); in exchange, a Python template with a newline that broke indentation in 70 files is refused instead of written (`bench/results/baseline-step3.md`). | `ast --apply` all or nothing, all assets pass |
| 4. Interface (done) | One command list `commands::COMMANDS` (help, MCP, runner table, skill check); `main.rs` 2,250 lines → 76 + per-command modules in `src/cli/`; exit codes in `Status::exit_code`; `Refusal` moved to `output.rs`. No renderer module and no new `--json` fields (see ⑤). Fixed a help example pointing at the deleted `src/search.rs`. | help, MCP and skill come from one list; all assets pass |
| 5. Finish (done) | Brought the docs site (`docs/site/`, 38 pages in two languages) in line with today's commands: removed the query language, `batch`, `explain` and `status`; the search guide is about `lancet rg`, the LQL reference became the AST pattern reference, the CLI reference lists the 12 commands. English `ARCHITECTURE.md` and `lancet-design.md` (Korean in `.ko.md`). ⑧ measured: not adopted (see ⑧); fixed the quadratic `refs --rank` bugs instead. Directories not moved (2.6). | site builds with 0 broken links, all assets pass |

Why this order: step 1 (Scope) had the most duplication and underlies the others. Step 2 had the surest performance gain. Step 3 changes behaviour, so it came after the structure settled.

---

## 5. Decisions (2026-11-28)

1. **The query language (`lancet '…'`) and `parse`, `explain` and `batch` were removed.** No existing tool they replace (section 0), and not used once in 40-odd agent logs. Regex search is `rg`, structural search is `ast`. Proximity and fuzzy search went with them. The internal representation (`Expr` → `Req`) keeps only `Match` and `And`, in the engine.
2. **`status` became `index --status`.**
3. **`ast --apply` became all or nothing (step 3),** like `apply_patch` and `git apply`. Rewrites that add syntax errors are refused, as in `edit`.
4. **English is the default for docs (step 5).** The Korean version of this document is `ARCHITECTURE.ko.md`.
5. **The restructure ships as 0.2.0, not 1.0.** It removes commands and changes `ast --apply`, so the minor version goes up. 1.0 is a promise that the interface won't change, so it waits for the criteria below.

### 1.0 criteria

1.0 ships when all of these hold and a period of real use needed no breaking change:

- Commands, flags and the `--json` schema are frozen and documented, with a test that fails when the schema changes.
- CI passes on Linux, macOS and Windows, rg parity included.
- The agent A/B (`bench/agents/`) is repeated across several models and runs, and lancet is at least as accurate as the built-in tools.
- Every command is timed on the largest corpora (Linux, LLVM) and none has a pathological case like the `refs --rank` one fixed in step 5.
