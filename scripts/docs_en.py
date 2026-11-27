"""English documentation pages for Lancet (Diátaxis IA)."""

from __future__ import annotations

from docs_ui import callout, cards, code, diagram, h2, h3, table

REPO_DOCS = "https://github.com/CREE1116/lancet/blob/main/docs"


def _intro(d: str) -> str:
    return f"""
<p><strong>Lancet</strong> is a single Rust binary that unifies discovery (<code>fd</code>), trigram-indexed search (<code>rg</code> / LQL), structural AST search &amp; rewrite (<code>ast</code>), exact range/document reading (<code>read</code> / <code>outline</code>), compare-and-swap editing (<code>edit</code> / <code>patch</code> / <code>rename</code>), and an embedded stdio MCP server (<code>mcp</code>).</p>

{callout("tip", "One rule before you start", "<p>Every search hit, AST match, read slice, and edit hunk agrees on exact <strong>1-based line numbers</strong> and <strong>XXH3-64 <code>rev</code> hashes</strong>. Even PDFs, Office files, HWP/HWPX, and archive entries share the exact same line numbering between search and <code>read</code>.</p>")}

{h2("install", "Installation")}
<p>Install a prebuilt binary on macOS, Linux (glibc and static musl), or Windows — no Rust toolchain needed:</p>

{code("macOS / Linux (Homebrew)", "brew tap CREE1116/lancet https://github.com/CREE1116/lancet\nbrew install CREE1116/lancet/lancet")}
{code("macOS / Linux (Release Installer)", "curl -fsSL https://raw.githubusercontent.com/CREE1116/lancet/main/install.sh | sh")}
{code("Windows (PowerShell)", "irm https://raw.githubusercontent.com/CREE1116/lancet/main/install.ps1 | iex")}
{code("From source (Cargo)", "# Full binary (~40 MB, 24 Tree-sitter grammars + PDF/Office/HWP adapters)\ncargo binstall --git https://github.com/CREE1116/lancet lancet\n\n# Lean text/code-only binary (~4 MB, no document adapters)\ncargo install --git https://github.com/CREE1116/lancet --no-default-features")}

{h2("walkthrough", "60-Second Walkthrough")}
<p>Run this loop in any repository. Building <code>.lancet</code> is optional — commands work identically with or without an index, and <code>auto</code> mode checks disk freshness on every query.</p>
{code("Terminal", "# 1. Optional: build the local trigram index once\nlancet index .\n\n# 2. Map folders first, then rank core symbols by lexical usage\nlancet fd --tree --dirs -d 2 .\nlancet refs --rank '*Service' --kind class src --limit 8\n\n# 3. Search with ripgrep syntax or LQL boolean/proximity queries\nlancet rg -w execute_one src\nlancet 'near(\"gradient\", \"clip\", 30) in function | expand parent'\n\n# 4. Read exact lines or symbols and capture `rev`\nlancet read src/search.rs --symbol execute_one --json\n\n# 5. Preview a dry-run diff, then commit atomically with --rev and --apply\nlancet edit src/search.rs --find 'let limit = 20;' --with 'let limit = 50;' --rev <REV> --apply")}

{h2("cheat-sheet", "One-Screen Cheat Sheet")}
{table(
    ["Job", "Command", "Key Guarantee"],
    [
        ["Folder layout", "<code>lancet fd --tree --dirs -d 2 .</code>", "Respects <code>.gitignore</code>"],
        ["Recent/typed files", "<code>lancet fd -t f -e rs --changed-within 7d</code>", "Index-accelerated"],
        ["Drop-in grep", "<code>lancet rg -w -t rs 'pat' src</code>", "Byte-identical to <code>rg</code> + <code>--docs</code>"],
        ["Boolean/fuzzy search", "<code>lancet '(\"a\" OR \"b\") NOT \"c\" in function'</code>", "<code>Scan == Auto</code>"],
        ["File skeleton", "<code>lancet outline src/main.rs</code>", "Functions, Markdown headings, sheets, ZIPs"],
        ["Exact slice", "<code>lancet read f.rs --lines 10:50 --budget-tokens 800</code>", "Returns <code>rev</code> + <code>next_line</code>"],
        ["Symbol read", "<code>lancet read f.rs --symbol my_fn</code>", "Includes doc comments &amp; attributes"],
        ["Document / ZIP", "<code>lancet read r.pdf -p 2:3</code> · <code>a.zip!app.log</code>", "Shared coordinates with search"],
        ["Usage ranking", "<code>lancet refs --rank '*Service' --kind class src</code>", "Sorted by lexical reference count"],
        ["AST search/rewrite", "<code>lancet ast '$X.unwrap()' src -l rs -r '$X?'</code>", "24 languages; dry-run until <code>--apply</code>"],
        ["CAS edit", "<code>lancet edit f.rs --edits - --rev REV --apply</code>", "Original coordinates, all-or-nothing"],
        ["Unified diff", "<code>git diff | lancet patch --apply</code>", "Pre-verified + race rollback"],
        ["Local scope rename", "<code>lancet rename f.rs:42 old new --rev REV --apply</code>", "Re-resolves scope to prevent capture"],
        ["MCP server", "<code>lancet mcp --root .</code>", "11 tools over stdio JSON-RPC"],
        ["Index inspection", "<code>lancet explain 'pat'</code> · <code>lancet status</code>", "Shows trigrams &amp; candidate %"],
    ],
)}

{h2("explore-docs", "Explore the Documentation")}
{cards([
    ("Getting Started", "Migration Guide", "Coming from rg, fd, ast-grep, sed, patch, or rga — side-by-side equivalents and exact differences.", "migration.html"),
    ("Guides", "Exploring & Budgeted Reads", "Map repos with fd --dirs, rank symbols, inspect outlines, batch queries, and paginate by token budget.", "guides/explore.html"),
    ("Guides", "Safe Editing & Concurrency", "Avoid line drift with --edits, guard races with --rev, apply unified diffs, and use local rename.", "guides/editing.html"),
    ("Reference", "CLI, LQL & JSON/MCP Schemas", "Complete flag tables, LQL formal grammar, 24 AST aliases, all 11 MCP tools, and error recovery.", "reference/cli.html"),
])}
"""


def _migration(_d: str) -> str:
    return f"""
<p>If you already use <code>ripgrep</code>, <code>fd</code>, <code>ast-grep</code>, <code>sed</code>, <code>patch</code>, or <code>rga</code>, you do not have to relearn everything at once. Lancet provides direct drop-in entrypoints alongside tighter guarantees across steps.</p>

{h2("from-rg", "Coming from ripgrep (rg)")}
<p><code>lancet rg</code> shares ripgrep's flag names, Rust <code>regex</code> engine, <code>.gitignore</code> / <code>.ignore</code> / <code>.rgignore</code> rules, exit codes (<code>0</code> hit, <code>1</code> miss, <code>2</code> error), and output formats — tested byte-for-byte against ripgrep 15.2.</p>
{table(
    ["ripgrep Command", "lancet Equivalent", "Notes"],
    [
        ["<code>rg foo src</code>", "<code>lancet rg foo src</code>", "Line numbers on by default; pass <code>-N</code> to hide"],
        ["<code>rg -i -w -t py tok</code>", "<code>lancet rg -i -w -t py tok</code>", "Identical <code>-i</code>/<code>-s</code>/<code>-S</code>, <code>-w</code>, and <code>--type</code> rules"],
        ["<code>rg -g '*.toml' -C 2 pat</code>", "<code>lancet rg -g '*.toml' -C 2 pat</code>", "Supports <code>--passthru</code>, <code>--type-add</code>, <code>-L</code>, <code>--max-depth</code>"],
        ["<code>rg --files -t rs</code>", "<code>lancet rg --files -t rs</code>", "Deterministic path-sorted list"],
        ["<code>rg -P '(?&lt;=a)b'</code>", "<code>lancet 'before(\"a\", \"b\", 1)'</code> or Rust regex", "<code>-P</code> / PCRE2 intentionally omitted (linear-time + sound trigram lowering)"],
        ["<code>rg --pre pdftotext pat</code>", "<code>lancet rg --docs pat</code>", "In-process PDF/Office/HWP/archive extraction without spawning subprocesses"],
    ],
)}

{callout("note", "Intentional differences in lancet rg", "<ul><li><strong>Always path-sorted:</strong> equivalent to <code>rg --sort path</code> (deterministic across threads).</li><li><strong><code>-n</code> enabled by default:</strong> stays on even when stdout is piped; use <code>-N</code> to disable.</li><li><strong>No grouped headings:</strong> always prints <code>path:line:text</code>, with <code>--color auto</code> disabled when piped.</li></ul>")}

{h2("from-fd", "Coming from fd / find")}
{table(
    ["fd / find", "lancet fd", "Difference / Extra Capability"],
    [
        ["<code>fd -e rs -d 2 src</code>", "<code>lancet fd -e rs -d 2 src</code>", "Backed by <code>.lancet</code> index when available"],
        ["<code>fd -t d -d 2</code>", "<code>lancet fd --tree --dirs -d 2 .</code>", "Clean folder hierarchy view without file clutter"],
        ["<code>find . -mtime -7</code>", "<code>lancet fd --changed-within 7d</code>", "Units: <code>s</code>, <code>m</code>, <code>h</code>, <code>d</code>, <code>w</code>"],
        ["<code>find . -name '*.pdf'</code>", "<code>lancet fd --type-class document</code>", "Built-in classes: <code>code</code>, <code>document</code>, <code>spreadsheet</code>, <code>archive</code>, etc."],
        ["<code>fd pat dir1 dir2</code>", "Invoke per root or use LQL <code>path:</code>", "<code>lancet fd</code> takes a single optional <code>[PATH]</code>; use <code>-p pat</code> if pattern looks like a dir"],
    ],
)}

{h2("from-ast", "Coming from ast-grep (sg)")}
{table(
    ["ast-grep", "lancet ast", "Important Nuance"],
    [
        ["<code>sg -p '$X.unwrap()' -l rs</code>", "<code>lancet ast '$X.unwrap()' -l rs</code>", "Uses <code>.lancet</code> trigram prefilter when pattern contains a literal"],
        ["<code>sg -p 'f($$$A)' -r 'g($$$A)' -U</code>", "<code>lancet ast 'f($$$A)' -r 'g($$$A)' --apply</code>", "Dry-run diff by default; <code>--apply</code> writes atomically"],
        ["<code>sg -p 'if ($C) $S'</code>", "<code>lancet ast 'if ($C) $S'</code>", "<strong>Semantic difference:</strong> Lancet matches shape strictly (<code>if</code> without <code>else</code>); <code>if-else</code> is a separate node"],
        ["<code>sg -l cpp ...</code>", "<code>lancet ast ... -l cpp</code>", "Lancet <code>--lang cpp</code> includes <code>.h</code> headers in addition to <code>.cpp</code>/<code>.cc</code>"],
    ],
)}

{h2("from-sed-patch", "Coming from cat / sed / patch / rga")}
{table(
    ["Legacy Chain", "Lancet Replacement", "Failure Mode Eliminated"],
    [
        ["<code>sed -n '10,50p' f.rs</code>", "<code>lancet read f.rs --lines 10:50</code>", "Returns XXH3-64 <code>rev</code> + respects <code>--budget-tokens</code>"],
        ["Two <code>sed -i</code> edits from one <code>rg</code>", "<code>lancet edit f.rs --edits - --rev REV --apply</code>", "All hunks resolve against original lines; never shifts or breaks links/mode"],
        ["<code>patch -p1 &lt; diff</code>", "<code>lancet patch diff --apply</code>", "Pre-verifies every hunk across all files; rolls back on write race"],
        ["<code>rga</code> + <code>pdftotext</code>/<code>pandoc</code>", "<code>lancet 'pat'</code> + <code>lancet read f.pdf -p 2</code>", "Single extraction pipeline so line numbers stay identical"],
    ],
)}

{h2("when-not", "When NOT to Replace Specialized Tools")}
{callout("warn", "Scope boundaries", "<ul><li><strong>Version control:</strong> keep using <code>git</code>; Lancet searches working trees (respecting <code>.gitignore</code>) and consumes <code>git diff</code> via <code>lancet patch</code>.</li><li><strong>Cross-file type-checked refactoring:</strong> keep your LSP (<code>rust-analyzer</code>, <code>tsserver</code>, <code>gopls</code>) when renaming public APIs, struct fields, or traits across files. Lancet provides lexical <code>refs</code> plus single-file scope-proven <code>lancet rename</code>.</li></ul>")}
"""


def _explore(_d: str) -> str:
    return f"""
<p>The fastest way for a developer or AI agent to burn context tokens and stall is to read whole files blindly. Lancet supports a <strong>5-step funnel</strong> that narrows thousands of files down to the exact lines you need.</p>

{h2("step1", "1. Map Folders Without File Noise")}
<p>Start with directory structure before listing files or running searches:</p>
{code("Terminal", "lancet fd --tree --dirs --max-depth 2 .\nlancet fd --changed-within 2d -t f src")}
<p>Adding <code>--dirs</code> (equivalent to <code>-t d</code>) renders only folders under <code>--tree</code>, giving an architectural map in a couple dozen lines.</p>

{h2("step2", "2. Rank Core Declarations with refs --rank")}
<p>When entering an unfamiliar repository, find which classes, structs, interfaces, or functions are actually central by ranking them by lexical reference count:</p>
{code("Terminal", "# Top 10 most-referenced Service classes\nlancet refs --rank '*Service' --kind class src --limit 10\n\n# Top referenced functions across a directory\nlancet refs --rank '*' --kind function --limit 10 src\n\n# Inspect definitions & call sites (excluding comments and strings)\nlancet refs SearchOptions src --lang rs --json")}
{callout("note", "Understanding shared names in --rank", "<p>Lancet uses the trigram index (for globs with a literal run like <code>Service</code>) and parses declarations across AST grammars. If an entry shows <code>(name shared by N declarations)</code> or <code>definitions &gt; 1</code> in JSON, multiple declarations share that identifier lexically.</p>")}

{h2("step3", "3. Read Skeletons Before Bodies (outline)")}
<p>Use <code>lancet outline</code> to inspect declaration signatures, Markdown heading trees, Excel/ODS sheet names, or archive entries with exact line bounds:</p>
{code("Terminal", "lancet outline src/search.rs\nlancet outline docs/architecture.md\nlancet outline financial_model.xlsx\nlancet outline logs/bundle.zip")}

{h2("step4", "4. Batch Multiple Searches & Refine with Pipes")}
<p>If an agent needs answers to several related questions, <code>lancet batch</code> opens the index and walks the tree <strong>once</strong> for all of them. Combine with LQL pipe stages to compact output:</p>
{code("Terminal", "# One shared index open + tree walk for 3 queries\nlancet batch src \\\n  -q 'near(\"retry\", \"backoff\", 20) in function' \\\n  -q '\"Conflict\" in ext:rs' \\\n  -q 'fuzzy(\"idempotent\", 1) | files'\n\n# Discover top co-occurring terms and enforce a token cap\nlancet '\"trigram\" in ext:rs | terms top:10 | budget tokens:1200' src")}

{h2("step5", "5. Paginate Reads by Token Budget")}
<p>Read only the target symbol (including doc comments and attributes) or a slice bounded by <code>--budget-tokens</code>. When truncated, continue from <code>next_line</code>:</p>
{code("Terminal", "lancet read src/search.rs --symbol execute_one\nlancet read src/search.rs --lines 100:400 --budget-tokens 800 --json")}
{callout("tip", "Measured context savings", "<p>In the <code>why-one-tool</code> suite, targeting lines/symbols/budgets used <strong>5–30× less context</strong> than reading whole files while preserving the exact line numbers and <code>rev</code> token needed for safe editing.</p>")}
"""


def _editing(d: str) -> str:
    return f"""
<p>Hand-written <code>sed</code> commands and unguarded whole-file overwrites fail silently when lines shift or multiple agents touch the same file. Lancet treats every edit as an atomic, syntax-checked <strong>compare-and-swap (CAS)</strong> on shared coordinates.</p>

{diagram(f"{d}/coordinates.svg", "Search hits, read ranges, symbols, and edits share one coordinate system.", "Every read returns line numbers and an XXH3-64 <code>rev</code> hash consumed directly by <code>edit</code> and <code>rename</code>.")}

{h2("single-hunk", "Single-Hunk Edits (--lines, --symbol, --find)")}
<p>Pick <strong>exactly one target</strong> (<code>--lines</code>, <code>--symbol</code>, or <code>--find</code>) and <strong>exactly one operation</strong> (<code>--with</code>, <code>--with-file</code>, <code>--delete</code>, <code>--insert-before</code>, or <code>--insert-after</code>). By default, <code>lancet edit</code> prints a dry-run unified diff without modifying disk; add <code>--apply</code> to commit.</p>
{code("Terminal", "# Edit exact lines read earlier (pass --rev for CAS)\nlancet edit src/net.rs --lines 12:15 --with '    timeout_ms: 5000,' --rev e70d5a898f122a62 --apply\n\n# Replace an entire symbol (NOTE: includes doc comments & attributes above it!)\nlancet edit src/net.rs --symbol retry_limit --with-file new_fn.rs --rev <REV> --apply\n\n# Unique exact-text replacement (matches LF and CRLF; add --all for every match)\nlancet edit src/net.rs --find 'let retries = 3;' --with 'let retries = 5;' --rev <REV> --apply")}
{callout("warn", "Watch the --symbol boundary", "<p><code>edit --symbol</code> spans the exact range that <code>read --symbol</code> displays — including leading <code>///</code> doc comments and <code>#[attr]</code> attributes. Include them in your replacement if you intend to keep them.</p>")}

{h2("multi-hunk", "Atomic Multi-Hunk Edits (--edits)")}
<p>When making multiple changes in one file, never chain separate line-numbered calls blindly — put all hunks into one <code>--edits</code> JSON array (from a file or <code>-</code> for stdin). Every hunk uses the <strong>original file's line numbers</strong>; Lancet resolves all spans first, rejects overlaps, and commits all-or-nothing:</p>
{code("Terminal", "lancet edit src/net.rs --rev e70d5a898f122a62 --apply --edits - <<'JSON'\n[\n  {\"lines\": \"10:12\", \"with\": \"const RETRIES: u32 = 5;\"},\n  {\"symbol\": \"legacy_helper\", \"delete\": true},\n  {\"find\": \"old_metric(\", \"with\": \"new_metric(\", \"all\": true},\n  {\"lines\": \"40\", \"insert_after\": \"    tracing::info!(\\\"updated\\\");\"}\n]\nJSON")}

{h2("cas-concurrency", "Multi-Agent Concurrency & Conflict Recovery")}
{diagram(f"{d}/concurrent-edit.svg", "4 agents competing on one file: plain writes lose 600/800 updates; lancet --rev loses 0/800.", "An exclusive OS lock re-verifies <code>inode</code> and <code>rev</code> before writing; stale revisions fail with <code>conflict</code> (exit 2) and 0 bytes changed.")}
<ol>
  <li><strong>Read &amp; capture <code>rev</code>:</strong> run <code>lancet read file --json</code> and save <code>rev</code>.</li>
  <li><strong>Apply with <code>--rev</code>:</strong> if another writer commits first, your call returns <code>status: "conflict"</code> (exit code <code>2</code>).</li>
  <li><strong>Chain or recover:</strong> on success, pass the returned <code>rev_after</code> to your next edit. On <code>conflict</code>, <code>lancet read</code> the new content, re-compute your change, and retry with the new <code>rev</code>. Never strip <code>--rev</code> to force past a conflict.</li>
</ol>
{callout("note", "Tree-sitter syntax gate & file identity", "<ul><li><strong>Syntax gate:</strong> edits, patches, and AST rewrites refuse any <em>net increase</em> in Tree-sitter syntax errors (so an already broken file can still be fixed incrementally). Pass <code>--allow-syntax-errors</code> only when intentional.</li><li><strong>File identity:</strong> symlinks write to their targets while preserving the symlink; multiply hard-linked files update in-place; permissions and owners are preserved.</li></ul>")}

{h2("patch", "Multi-File Unified Diffs (lancet patch)")}
<p><code>lancet patch</code> applies <code>git diff</code> or <code>diff -u</code> patches atomically across files:</p>
{code("Terminal", "# Dry-run verification\nlancet patch changes.diff\n\n# Apply from stdin, stripping 1 path prefix (default: a/ and b/) from subdir\ngit diff | lancet patch --root subdir -p 1 --apply")}
<p>Handles renames (<code>git diff -M</code>), mode changes (<code>chmod +x</code>), and empty new/deleted files. Every hunk is matched <em>exactly</em> (relocated only if an exact match exists at a unique nearest offset — never fuzzy). Pre-verification runs across all files before any byte is written, with best-effort rollback on mid-apply write races. Binary diffs and copies are refused.</p>

{h2("rename", "Scope-Proven Local Rename (lancet rename)")}
<p><code>lancet rename</code> renames a local binding (variable, loop variable, closure parameter, or non-Python parameter) inside a single file across <strong>6 languages</strong>: Rust, Python, JavaScript, TypeScript, TSX, and Go.</p>
{code("Terminal", "# Col is optional when OLD only resolves to one binding on that line\nlancet rename src/search.rs:42 old_name new_name\nlancet rename src/search.rs:42:9 old_name new_name --rev <REV> --apply")}
{callout("danger", "When lancet rename refuses by design", "<p>Lancet refuses rather than guesses whenever correctness cannot be proven locally: module/top-level or exported symbols, struct fields, methods, <strong>Python function parameters</strong> (callers can pass keyword arguments across files), macros, JSX tag names, Go composite-literal keys, dynamic scopes (<code>eval</code>/<code>with</code>/<code>locals()</code>/<code>global</code>/<code>nonlocal</code>), and any target name that causes <strong>shadowing or variable capture</strong> when scopes are re-resolved post-rename.</p><p>Comments inside the scope mentioning the old name are left untouched and counted in <code>comment_mentions</code>. For cross-file or top-level renames, locate candidates with <code>lancet refs</code> and update explicitly with <code>lancet edit --edits</code> or an LSP.</p>")}
"""


def _documents(_d: str) -> str:
    return f"""
<p>Workspaces rarely hold only source files — specs live in PDFs and Word/HWP files, lookup tables in spreadsheets, and crash logs inside zip/tar bundles. Lancet extracts them <strong>in-process</strong> so search hits and <code>lancet read</code> slices agree on the exact same line numbers.</p>

{h2("formats", "Supported Formats & Selectors")}
{table(
    ["Format / Extension", "In-Process Extractor", "Outline Support", "Read Selector"],
    [
        ["<code>.pdf</code>", "Pure-Rust stream extraction (<code>--ocr</code> optional)", "PDF outline / headings", "<code>--page 2</code> or <code>--page 2:5</code>"],
        ["<code>.docx</code>, <code>.odt</code>, <code>.epub</code>", "XML text &amp; paragraph stream", "Document headings", "<code>--lines START:END</code>"],
        ["<code>.pptx</code>", "Slide XML text extractor", "Slide list", "<code>--page 3</code> or <code>--page 1:4</code>"],
        ["<code>.hwp</code> (5.0 OLE), <code>.hwpx</code>", "Native Korean Hangul document parser", "Section headings", "<code>--lines START:END</code>"],
        ["<code>.xlsx</code>, <code>.xls</code>, <code>.xlsm</code>, <code>.xlsb</code>, <code>.ods</code>", "Spreadsheet cell extractor", "Sheet names + dimensions", "<code>--sheet NAME|1 --rows 1:30 --cols A:E</code>"],
        ["<code>.zip</code>, <code>.tar</code>, <code>.gz</code>, <code>.tgz</code>, <code>.tar.gz</code>", "Streaming archive reader (16 MiB cap)", "Entry path manifest", "<code>bundle.zip!path/inside.log</code> or <code>--entry</code>"],
    ],
)}

{h2("search-docs", "Searching Documents & Archives")}
<p>There is one intentional difference between Lancet's native LQL search and its ripgrep compatibility mode:</p>
<ul>
  <li><strong>Default LQL search (<code>lancet 'query'</code>):</strong> searches code, documents, spreadsheets, and archives automatically (filter with <code>type:document</code>, <code>type:spreadsheet</code>, or <code>type:archive</code>).</li>
  <li><strong><code>lancet rg</code>:</strong> skips binary/document/archive files by default so output stays 100% identical to <code>ripgrep</code>; pass <code>--docs</code> to include them in-process.</li>
</ul>
{code("Terminal", "# Search Korean/English clauses across PDFs and HWP files\nlancet 'near(\"계약\", \"해지\", 15) in type:document' contracts/\n\n# Search inside archives and documents with ripgrep syntax\nlancet rg --docs -i 'timeout' ./artifacts")}

{h2("read-docs", "Coordinate-Exact Reading & Archives")}
{code("Terminal", "# Inspect structure first\nlancet outline q4_report.xlsx\nlancet outline crash_bundle.tar.gz\n\n# Read specific slides, spreadsheet ranges, or archive entries\nlancet read deck.pptx --page 4:6\nlancet read q4_report.xlsx --sheet 1 --rows 1:30 --cols A:E\nlancet read logs/bundle.zip!worker_02/stderr.log --lines 120:180 --budget-tokens 600")}

{h2("ocr-and-safety", "OCR, Safety Caps & Read-Only Contract")}
<ul>
  <li><strong>Scanned PDFs &amp; images (<code>--ocr</code>):</strong> pass <code>--ocr</code> on <code>lancet</code>, <code>read</code>, <code>outline</code>, or <code>index</code> to enable external <code>tesseract</code> / <code>pdftoppm</code> fallback (<code>pdftoppm</code> + <code>tesseract</code> must be on <code>PATH</code>).</li>
  <li><strong>Decompression bomb protection:</strong> archive entries and compressed Office/HWP XML streams are capped (16 MiB per member/stream) so malformed inputs cannot exhaust RAM.</li>
  <li><strong>Read-only guarantee:</strong> binary documents and archive entries never return a <code>rev</code> field in JSON, and <code>lancet edit</code> refuses to write back to them.</li>
  <li><strong>Minimal build without adapters:</strong> build with <code>cargo install --no-default-features</code> to strip document/archive adapters for a ~4 MB code/text-only binary.</li>
</ul>
"""


def _agents_mcp(_d: str) -> str:
    return f"""
<p>AI coding agents can invoke Lancet either through shell commands or natively through its zero-dependency stdio <strong>Model Context Protocol (MCP)</strong> server — paired with the cross-agent <strong>Official Skill</strong> in <code>skills/lancet/</code>.</p>

{h2("mcp-server", "Starting the Built-in MCP Server (lancet mcp)")}
<p><code>lancet mcp</code> speaks newline-delimited JSON-RPC 2.0 over <code>stdin</code>/<code>stdout</code> (supporting MCP revisions <code>2025-03-26</code> and <code>2024-11-05</code>) and executes Lancet's Rust engines directly in-process without async daemon overhead.</p>
{table(
    ["Flag", "Default", "Behavior"],
    [
        ["<code>--root &lt;DIR&gt;</code>", "<code>.</code>", "Default working directory for resolving relative paths in tool calls (<strong>not</strong> a chroot/sandbox; absolute or <code>..</code> paths work like the CLI)"],
        ["<code>--read-only</code>", "<code>false</code>", "Omits <code>lancet_edit</code>, <code>lancet_patch</code>, and <code>lancet_rename</code> from <code>tools/list</code> and refuses writes (<code>lancet_ast</code> <code>apply</code>, mutating <code>lancet_index</code> actions)"],
        ["<code>--trust-index</code>", "<code>false</code>", "Sets default <code>index_mode</code> to <code>trust</code> instead of <code>auto</code> unless overridden per tool call"],
        ["<code>-j, --threads &lt;NUM&gt;</code>", "CPU count", "Rayon worker thread count"],
    ],
)}

{h2("client-configs", "Client Configurations")}
<p>Ready-to-copy templates are also shipped in <a href="https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/mcp-config.json"><code>skills/lancet/references/mcp-config.json</code></a>.</p>

{h3("cfg-claude", "Claude Code")}
{code("Terminal", "# Standard read/write server\nclaude mcp add --transport stdio lancet -- lancet mcp --root .\n\n# Read-only exploration mode\nclaude mcp add --transport stdio lancet -- lancet mcp --root . --read-only")}

{h3("cfg-cursor", "Cursor (.cursor/mcp.json or ~/.cursor/mcp.json)")}
{code("JSON", '{\n  "mcpServers": {\n    "lancet": {\n      "command": "lancet",\n      "args": ["mcp", "--root", "."]\n    }\n  }\n}')}

{h3("cfg-desktop", "Claude Desktop / Windsurf / Antigravity")}
<p>Pass an absolute <code>--root</code> path when the desktop host launches outside your project directory:</p>
{code("JSON", '{\n  "mcpServers": {\n    "lancet": {\n      "command": "lancet",\n      "args": ["mcp", "--root", "/abs/path/to/your/repo"]\n    }\n  }\n}')}

{h3("cfg-codex", "OpenAI Codex (~/.codex/config.toml)")}
{code("TOML", '[mcp_servers.lancet]\ncommand = "lancet"\nargs = ["mcp", "--root", "."]')}

{h2("official-skill", "Installing the Official Agent Skill")}
<p>The cross-agent Skill package (<a href="https://github.com/CREE1116/lancet/tree/main/skills/lancet"><code>skills/lancet/</code></a>) teaches agents LQL syntax, all 24 AST language aliases, compare-and-swap <code>rev</code> workflows, and when to prefer <code>lancet</code> over legacy tools.</p>
{code("Terminal", "# Project-local (.agents universal convention)\nmkdir -p .agents/skills && ln -s \"$(pwd)/skills/lancet\" .agents/skills/lancet\n\n# Claude Code / Codex / Antigravity personal skills\nmkdir -p ~/.claude/skills && cp -R skills/lancet ~/.claude/skills/lancet\nmkdir -p ~/.codex/skills  && cp -R skills/lancet ~/.codex/skills/lancet\nmkdir -p ~/.gemini/config/skills && cp -R skills/lancet ~/.gemini/config/skills/lancet")}

{h2("tools-overview", "11 Tools & 2 Built-in Prompts")}
<p>Every read/search tool treats 0 matches as a normal <code>empty</code> result (<code>isError: false</code>). Write operations default to <code>apply: false</code> (dry-run unified diff). See the full parameter and response schema tables in <a href="../reference/json-and-mcp.html#mcp-tools">JSON &amp; MCP Reference</a>.</p>
<ul>
  <li><strong>Exploration &amp; search:</strong> <code>lancet_fd</code>, <code>lancet_outline</code>, <code>lancet_search</code>, <code>lancet_rg</code>, <code>lancet_refs</code> (including <code>rank: true</code>), <code>lancet_ast</code>, <code>lancet_read</code>.</li>
  <li><strong>Safe mutations:</strong> <code>lancet_edit</code>, <code>lancet_patch</code>, <code>lancet_rename</code>, plus <code>lancet_ast</code> with <code>apply: true</code>.</li>
  <li><strong>Index control:</strong> <code>lancet_index</code> (<code>status</code>, <code>sync</code>, <code>rebuild</code>, <code>compact</code>).</li>
  <li><strong>Built-in prompts:</strong> <code>lancet-explore</code> and <code>lancet-safe-edit</code> via <code>prompts/list</code>.</li>
</ul>
{callout("tip", "Minimal AGENTS.md / CLAUDE.md snippet", "<pre style='margin:6px 0 0;white-space:pre-wrap;font-size:12.5px'>Prefer lancet (lancet_* MCP tools or lancet CLI) over rg/fd/ast-grep/cat/sed for discovery, search, bounded reads, and CAS edits (read --rev -> edit --apply).</pre>")}
"""


def _index_freshness(d: str) -> str:
    return f"""
<p>Traditional indexed search tools fall out of sync until a file watcher catches up, or return approximate results from the index. Lancet takes the opposite design: <strong>the index only decides which files NOT to open</strong>.</p>

{diagram(f"{d}/pipeline.svg", "How an indexed query skips 91% of files on Linux v6.12 while staying exact.", "Measured on Linux v6.12 (86,218 files) for <code>\"spin_lock\"</code> in <code>auto</code> mode: 7,814 candidates verified, 7,073 true matches.")}

{h2("sound-lowering", "Sound Trigram Lowering & Exact Verification")}
<p>Every query — whether LQL, <code>lancet rg</code> regex HIR, <code>lancet ast</code> pattern with a literal, or <code>lancet refs</code> — is lowered into a boolean expression over required <strong>byte trigrams</strong> (plus a sound q-gram threshold <code>AtLeast(k)</code> for <code>fuzzy()</code>). Because any file missing the required trigrams <em>provably</em> cannot match, skipping it never creates false negatives.</p>
<p>Every candidate file returned by the index — plus any modified file detected live — is verified by the <strong>exact same evaluator</strong> used in a scan without an index. That guarantees <strong><code>Scan == Auto</code></strong> by construction.</p>

{h2("freshness", "How auto Mode Detects Edits Without a Daemon")}
{diagram(f"{d}/index-lifecycle.svg", "Index lifecycle: initial build, live stat verification, incremental segments, and compaction.", "Auto mode catches modified and untracked files immediately with zero background daemon.")}
<p>In default <code>auto</code> mode, Lancet walks file metadata once and checks four attributes stored per indexed file:</p>
<ul>
  <li><code>mtime</code> + <code>size</code> + <code>ctime</code> + <code>inode</code> (on Unix/macOS)</li>
  <li>Plus a <strong>racy-timestamp guard</strong>: files modified within the same timestamp window as the index write are always treated as potentially stale and re-scanned live.</li>
</ul>
<p>This catches same-size edits (where length didn't change), restored <code>mtime</code> tricks (caught by <code>ctime</code>/<code>inode</code>), and sub-second edits.</p>

{callout("warn", "Honest Windows Caveat", "<p>Rust's standard library currently exposes only <code>mtime</code> and <code>size</code> stably on Windows (without <code>ctime</code>/<code>inode</code>). On Windows, edits that preserve both file size and <code>mtime</code> are outside metadata detection; run <code>lancet index</code> or <code>--no-index</code> if a tool explicitly backdates timestamps on Windows.</p>")}

{h2("modes", "auto vs --trust-index vs --no-index")}
{table(
    ["Mode", "Disk Metadata Walk?", "Reads Changed Files Live?", "Latency (Linux 86k files)", "When to Use"],
    [
        ["<code>auto</code> (default)", "Yes (~100 ms)", "Yes", "~148 ms (rare literal)", "Interactive coding &amp; AI agents (always fresh)"],
        ["<code>--trust-index</code>", "No", "No", "~40 ms (or 3 ms no-hit)", "Read-only exploration of giant repos when index is known fresh"],
        ["<code>--no-index</code>", "Full scan", "Yes (every file)", "~1,113 ms", "Debugging, baseline parity checks, or write-disabled dirs"],
    ],
)}

{h2("lsm-ops", "LSM Segments, Ancestor Discovery & Compaction")}
<ul>
  <li><strong>On-disk layout:</strong> stored in <code>PATH/.lancet</code> using an immutable LSM segment format (<code>LANCET04</code>) with tombstones and mmap'd posting lists.</li>
  <li><strong>Ancestor resolution:</strong> running a query inside <code>src/query/</code> automatically discovers and uses a parent repo's <code>.lancet</code> index unless overridden with <code>--index-dir</code>.</li>
  <li><strong>Incremental updates &amp; maintenance:</strong> run <code>lancet index .</code> to absorb stale/new files into a new segment, <code>lancet index --compact</code> to merge segments and purge tombstones, or <code>lancet index --rebuild</code> to recreate from scratch.</li>
  <li><strong>Inspect selectivity:</strong> run <code>lancet status</code> to see indexed vs stale counts, or <code>lancet explain '&lt;QUERY&gt;'</code> to view required trigrams and candidate percentages.</li>
</ul>
"""


def _arch_bench(d: str) -> str:
    return f"""
<p>Tools built separately assume a human bridges the gaps between them. When an autonomous agent chains <code>rg</code> → <code>sed</code> → <code>patch</code> → <code>pdftotext</code>, mismatches turn into silent data loss. Lancet replaces the brittle handoffs inside one binary — and publishes exact benchmarks including where competitors are faster.</p>

{h2("failure-modes", "7 Failure Modes Reproduced & Eliminated")}
{diagram(f"{d}/failure-modes.svg", "Seven real-world multi-step failure modes reproduced by bench/failure_modes.py.", "Every row was measured against real tools in <code>bench/failure_modes.py</code>.")}
<ol>
  <li><strong>Line drift across edits:</strong> two sequential <code>sed -i</code> calls from one <code>rg -n</code> hit the wrong line cleanly (exit 0). <code>lancet edit --edits</code> resolves all hunks on original coordinates.</li>
  <li><strong>Silent concurrent overwrites:</strong> 4 concurrent read-modify-write agents lose <strong>600 / 800</strong> updates silently; <code>lancet edit --rev</code> loses <strong>0 / 800</strong>.</li>
  <li><strong>Half-applied patches:</strong> <code>patch -p1</code> with one stale hunk mutates file A and rejects file B; <code>lancet patch</code> verifies all files first and rolls back on write races.</li>
  <li><strong>Comment/string corruption:</strong> <code>sed</code> renames mutate 0.3–11.7% comments/strings; <code>refs</code> and <code>ast</code> ignore them cleanly, and <code>rename</code> re-resolves local scopes.</li>
  <li><strong>Document extraction mismatch:</strong> <code>rga</code> + <code>pandoc</code> agree on 0 / 40 hit lines inside <code>.docx</code>; Lancet shares one extraction pipeline (40 / 40).</li>
  <li><strong>Post-edit index staleness:</strong> immediately after editing 20 files, zoekt finds new text in 0 and stale deleted text in 20; Lancet <code>auto</code> reports 20 and 0.</li>
  <li><strong>Broken links &amp; modes:</strong> <code>sed -i</code> / <code>perl -pi</code> destroy symlinks/hard links and drop mode <code>0640</code>; Lancet writes symlink targets and updates multiply hard-linked files in-place.</li>
</ol>
<p>See the full design essay in <a href="{REPO_DOCS}/why-one-tool.md"><code>docs/why-one-tool.md</code></a> (<a href="{REPO_DOCS}/why-one-tool.ko.md">한국어</a>).</p>

{h2("core-bench", "Core Benchmark Suite (8 Codebases)")}
<p>Apple M4 Pro, warm cache, median of 5 <code>hyperfine -N</code> runs. Full tables: <a href="https://github.com/CREE1116/lancet/blob/main/bench/results/SUITE.md"><code>bench/results/SUITE.md</code></a>.</p>
{diagram(f"{d}/bench-index-footprint.svg", "Index build time, disk size, and peak RAM across 8 codebases: lancet vs zoekt.", "4–25× faster builds, 9–30× smaller disk footprint, 3.2–5.0× lower peak RAM than zoekt.")}
{diagram(f"{d}/bench-workload-profile.svg", "8 text query types on LLVM 19.1 and structural AST search across 8 codebases.", "Every text output was verified byte-for-byte against ripgrep 15.2.")}
{table(
    ["Corpus", "Files", "lancet Index (build · disk · peak)", "zoekt Index (build · disk · peak)", "auto vs rg", "trust vs rg", "AST vs ast-grep"],
    [
        ["Linux v6.12 (C)", "86,680", "3.3 s · 134 MB · 358 MB", "82 s · 2.9 GB · 1.2 GB", "5.8–7.5×", "13–403×", "4.0×"],
        ["LLVM 19.1 (C++)", "146,319", "4.7 s · 122 MB · 280 MB", "53 s · 3.7 GB · 1.3 GB", "6.1–8.0×", "34–786×", "2.3×"],
        ["rust 1.83 (Rust)", "49,964", "1.2 s · 29 MB · 184 MB", "4.9 s · 327 MB · 739 MB", "1.5–9.6×", "2–273×", "1.8×"],
        ["kubernetes 1.31 (Go)", "26,232", "1.0 s · 30 MB · 224 MB", "4.1 s · 537 MB · 1.0 GB", "3.5–5.8×", "9–210×", "1.9×"],
        ["VS Code 1.95 (TS)", "7,708", "0.4 s · 12 MB · 114 MB", "2.4 s · 245 MB · 573 MB", "2.1–3.9×", "4–63×", "2.7×"],
        ["Django 5.1 (Python)", "6,815", "0.2 s · 7 MB · 76 MB", "1.2 s · 95 MB · 302 MB", "2.4–2.9×", "10–60×", "1.4×"],
        ["CPython 3.13 (C/Py)", "4,882", "0.3 s · 10 MB · 116 MB", "3.7 s · 227 MB · 509 MB", "3.0–5.4×", "5–30×", "2.6×"],
        ["Spring 6.1 (Java)", "10,163", "0.3 s · 12 MB · 122 MB", "1.9 s · 146 MB · 395 MB", "2.9–5.3×", "6–95×", "3.1×"],
    ],
    num_cols=[1],
)}

{h2("competitors", "Indexed & Agent Tool Comparison (Honest Tradeoffs)")}
<p>Measured via <a href="https://github.com/CREE1116/lancet/blob/main/bench/competitors_agents.py"><code>bench/competitors_agents.py</code></a> across <code>rg</code>, <code>ast-grep</code>, <code>tgrep</code> (disk &amp; daemon <code>serve</code>), <code>grix</code>, <code>aft</code>, <code>tensor-grep</code>, and <code>probe</code> (<a href="https://github.com/CREE1116/lancet/blob/main/bench/results/COMPETITORS.md"><code>COMPETITORS.md</code></a>):</p>
<ul>
  <li><strong>Where Lancet leads:</strong> <strong>20/20 immediate 0 ms post-edit freshness</strong> with zero daemon (<code>aft</code> watcher lag and serverless <code>tgrep</code> score 0/20 at 0 ms); <strong>1/3–1/7× smaller disk index</strong> than <code>aft</code> / <code>tgrep</code> (141.8 MB vs 368.5 MB / 955.2 MB on Linux); <strong>~1/3× peak build RAM</strong> vs <code>grix</code> (353.5 MB vs 1,001.6 MB on Linux); <strong>20/20 exact <code>rg</code> line-count parity</strong>; fastest literal AST search across all 4 tested corpora.</li>
  <li><strong>Where specialized or daemon tools win:</strong> <code>tgrep serve</code> (resident daemon) beats daemonless <code>lancet auto</code> on <code>regex with literal</code> in Rust (119.2 ms vs 508.7 ms) and LLVM (19.9 ms vs 295.8 ms), and on small literal-free regexes in Django (15.4 ms) / Rust (60.9 ms). <code>grix --no-auto-index</code> is faster than <code>lancet --trust-index</code> on Linux rare literal (11.5 ms vs 43.6 ms). Resident <code>aft</code> (587–1,260 MB RAM) is faster on Rust rare literal (8.5 ms vs 73.5 ms). Literal-free regexes and literal-free AST patterns run near <code>rg</code> (down to 1.4× slower) and <code>ast-grep</code> (0.6–1.0×).</li>
</ul>
"""


def _cli(_d: str) -> str:
    return f"""
<p>Every subcommand shares global traversal semantics (<code>.gitignore</code> / <code>.ignore</code> / <code>.rgignore</code>), deterministic sorting, and structured exit codes.</p>

{h2("exit-codes", "Exit Codes")}
{table(
    ["Code", "Meaning", "Status Values"],
    [
        ["<code>0</code>", "Success / match found", "<code>complete</code>"],
        ["<code>1</code>", "No matches found (or missing index in <code>status</code>)", "<code>empty</code>"],
        ["<code>2</code>", "Invalid query/flags, unsupported op, conflict, or I/O error", "<code>invalid_query</code>, <code>unsupported</code>, <code>conflict</code>"],
    ],
    num_cols=[0],
)}

{h2("subcommands", "All Subcommands & Flags")}
{code("CLI Surface", """lancet <QUERY> [PATH] [--json] [--no-index | --trust-index] [--index-dir DIR] [--ocr]
                      [--hidden] [--no-ignore] [-j THREADS] [--max-lines N] [--max-cols N] [--stats]

lancet rg [FLAGS] [PATTERN] [PATH...] [--docs] [--passthru] [--type-add TYPESPEC]
                      [--no-index | --trust-index] [--index-dir DIR]

lancet fd [PATTERN] [PATH] [-p PATTERN] [-t f|d] [-e EXT] [--type-class CLASS] [-g | -r] [-s]
                           [-d DEPTH] [--changed-within DUR] [--min-size S] [--max-size S]
                           [--tree] [--dirs] [--limit N] [--meta]
                           [--no-index | --trust-index] [--index-dir DIR] [--json] [--stats]

lancet ast <PATTERN> [PATH] [-l LANG] [-r TEMPLATE [--apply]] [--path GLOB] [--limit N]
                            [--no-index | --trust-index] [--index-dir DIR] [--json] [--stats]

lancet read <TARGET> [-l START:END] [-s SYMBOL] [-p PAGE] [--sheet S] [--rows R] [--cols C]
                     [-e ENTRY] [--budget-tokens N] [--raw] [--ocr] [--json]

lancet outline [PATH] [-d DEPTH] [-e EXT] [--limit N] [--hidden] [--no-ignore] [--ocr] [--json]

lancet refs <SYMBOL> [PATH] [--rank [--kind KIND,...]] [-l LANG] [-e EXT] [--path GLOB]
                            [--all-files] [--limit N] [--no-index | --trust-index] [--json] [--stats]

lancet edit <PATH> (--lines RANGE | --symbol SYM | --find TEXT [--all] | --edits FILE)
                   (--with TEXT | --with-file FILE | --delete | --insert-before TEXT | --insert-after TEXT)
                   [--rev REV] [--apply] [--allow-syntax-errors] [--json]

lancet patch [DIFF|-] [--root DIR] [-p N] [--apply] [--allow-syntax-errors] [--json]

lancet rename <FILE:LINE[:COL]> <OLD> <NEW> [--rev REV] [--apply] [--json]

lancet mcp [--root DIR] [--read-only] [--trust-index] [-j THREADS]

lancet index [PATH] [--rebuild | --compact] [--index-dir DIR] [--ocr] [--hidden] [--no-ignore] [-j THREADS]
lancet status [PATH] [--index-dir DIR]
lancet parse <QUERY>
lancet explain <QUERY> [PATH] [--index-dir DIR] [--no-index | --trust-index] [--hidden] [--no-ignore]
lancet batch [PATH] (-q <QUERY>... [--name <NAME>...] | --queries-json <JSON>)
                    [--no-index | --trust-index] [--index-dir DIR] [--ocr] [--json]""")}

{h2("rg-parity", "lancet rg — ripgrep Feature Parity Matrix")}
<p>Verified against 68 edge-case fixtures in <code>bench/rg_parity.py</code> and full Linux kernel queries against <code>ripgrep 15.2</code>.</p>

{h3("rg-matching", "Patterns & Matching")}
{table(
    ["Flag", "Meaning", "Status", "Notes"],
    [
        ["<code>-e</code>, <code>--regexp &lt;PAT&gt;</code>", "Search pattern (repeatable)", "<span class='tag-yes'>Supported</span>", "Combined with alternation"],
        ["<code>-f</code>, <code>--file &lt;FILE&gt;</code>", "Read patterns per line (<code>-</code> stdin)", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-F</code>, <code>--fixed-strings</code>", "Treat patterns as literals", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-i</code> / <code>-s</code> / <code>-S</code>", "Ignore-case / case-sensitive / smart-case", "<span class='tag-yes'>Supported</span>", "Last flag wins; AST-aware <code>-S</code> (e.g. <code>\\W</code> stays insensitive)"],
        ["<code>-w</code> / <code>-x</code> / <code>-v</code>", "Word / whole-line / invert match", "<span class='tag-yes'>Supported</span>", "Matches <code>rg</code> word-boundary rules"],
        ["<code>-U</code>, <code>--multiline-dotall</code>", "Multiline match &amp; <code>.</code> matches <code>\\n</code>", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-P</code>, <code>--pcre2</code>", "PCRE2 lookarounds &amp; backrefs", "<span class='tag-no'>Unsupported</span>", "Rust <code>regex</code> only for linear-time guarantee &amp; sound trigram compilation"],
    ],
)}

{h3("rg-output", "Output & Formatting")}
{table(
    ["Flag", "Meaning", "Status", "Notes"],
    [
        ["<code>-o</code>, <code>-c</code>, <code>--count-matches</code>", "Only matching / line count / match count", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-l</code>, <code>--files-without-match</code>, <code>-q</code>", "Matching/non-matching paths, quiet", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-m &lt;N&gt;</code>, <code>-A</code>/<code>-B</code>/<code>-C</code>", "Max count per file &amp; context lines", "<span class='tag-yes'>Supported</span>", "<code>--</code> separator emitted cleanly"],
        ["<code>--passthru</code>", "Print matching &amp; non-matching lines", "<span class='tag-yes'>Supported</span>", "Last of <code>--passthru</code>/<code>-A</code>/<code>-B</code>/<code>-C</code> wins"],
        ["<code>-n</code> / <code>-N</code>", "Show / hide line numbers", "<span class='tag-yes'>Supported</span>", "<strong>ON by default</strong> even when piped"],
        ["<code>-H</code> / <code>-I</code>, <code>--column</code>, <code>--vimgrep</code>", "Filename control, column, vimgrep", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>--json</code>, <code>-0</code>, <code>-r &lt;TEXT&gt;</code>", "rg JSON Lines, NUL sep, <code>$1</code> replace", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>--color &lt;WHEN&gt;</code>", "<code>auto</code>, <code>always</code>, <code>ansi</code>, <code>never</code>", "<span class='tag-yes'>Supported</span>", "Default <code>auto</code> respects TTY, <code>NO_COLOR</code>, <code>TERM=dumb</code>"],
        ["<code>--sort</code>, <code>--no-heading</code>", "Sort order, disable headings", "No-op", "Always path-sorted; headings never used"],
    ],
)}

{h3("rg-files", "File Traversal & Lancet Extensions")}
{table(
    ["Flag", "Meaning", "Status", "Notes"],
    [
        ["<code>-g</code>, <code>--iglob</code>, <code>-t</code>, <code>-T</code>, <code>--type-list</code>", "Glob &amp; type filters", "<span class='tag-yes'>Supported</span>", "Positive globs override ignore/hidden like <code>rg</code>"],
        ["<code>--type-add &lt;SPEC&gt;</code>", "Custom type (<code>name:glob</code> / <code>name:include:a,b</code>)", "<span class='tag-yes'>Supported</span>", "Applies to <code>-t</code>, <code>-T</code>, <code>--files</code>, <code>--type-list</code>"],
        ["<code>-u</code> / <code>-uu</code> / <code>-uuu</code>, <code>-.</code>, <code>--no-ignore</code>", "Unrestricted &amp; hidden/ignore toggles", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-L</code>, <code>--max-depth</code>, <code>--max-filesize</code>", "Follow symlinks, depth, size (K/M/G)", "<span class='tag-yes'>Supported</span>", "<code>--trust-index -L</code> falls back to live walk"],
        ["<code>--files</code>, <code>-j</code>", "List searchable files, thread count", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>--docs</code>", "Search PDFs, Office, HWP, archives in-process", "<span class='tag-ext'>Extension</span>", "Off by default for strict <code>rg</code> parity"],
        ["<code>--no-index</code>, <code>--trust-index</code>, <code>--index-dir</code>", "Control <code>.lancet</code> index mode", "<span class='tag-ext'>Extension</span>", "Default is fresh <code>auto</code> mode"],
        ["<code>--pre &lt;CMD&gt;</code>", "External preprocessor", "<span class='tag-no'>Unsupported</span>", "Replaced by in-process <code>--docs</code>"],
    ],
)}
"""


def _query_ast(_d: str) -> str:
    return f"""
<p>Lancet Query Language (<strong>LQL v0.1</strong>) compiles to a deterministic IR evaluated identically on indexed candidates and full scans. Unknown operators, filters, or trailing tokens never fail silently — they return <code>status: "invalid_query"</code> with an exact byte offset.</p>

{h2("lql-grammar", "LQL v0.1 Formal Grammar")}
{code("EBNF", """query   := ["find"] expr ("in" scope)* ("|" stage)*
expr    := or_expr
or_expr := and_expr ("OR" and_expr)*
and_expr:= unary (("AND")? unary)*          # adjacent terms form implicit AND
unary   := "NOT" unary | primary
primary := string
         | word                            # unquoted single token -> literal
         | "(" expr ")"
         | "regex(" string ")"
         | "fuzzy(" string ["," uint] ")"  # k in 0..=3, default 1
         | "symbol(" string ")" | "symbol:" (string | word)
         | "near(" matcher "," matcher "," uint ")"
         | "before(" matcher "," matcher "," uint ")"

matcher := string | word | "regex(" string ")" | "fuzzy(" string ["," uint] ")"
         | "symbol(" string ")" | "symbol:" (string | word)

scope   := ("line" | "block" | "file" | "function"
         |  "type:" values | "ext:" values | "path:" values)+
values  := value | "(" value ("," value)* ")"

stage   := "limit" uint                    # uint >= 1
         | "count"
         | "files"
         | "group" "file"
         | "expand" ("parent" | "unit" | "context:" uint)
         | "sort" ("count" ["asc" | "desc"] | "path")
         | "terms" ["top:" uint]           # 1..=100, default 15
         | "budget" ["tokens:"] uint       # uint >= 1""")}
<p>Operator precedence is <code>NOT</code> &gt; <code>AND</code> (explicit or implicit) &gt; <code>OR</code>. Run <code>lancet parse '&lt;QUERY&gt;'</code> to view the normalized canonical query and JSON IR.</p>

{h2("lql-semantics", "Matchers, Scopes & Pipe Stages")}
{table(
    ["Construct", "Exact Semantics"],
    [
        ["<code>\"text\"</code> / bare token", "Case-insensitive (Unicode simple case folding) substring match."],
        ["<code>regex(\"pat\")</code>", "Rust <code>regex</code> multi-line mode (<code>^</code>/<code>$</code> match line boundaries). Case-sensitive unless <code>(?i)</code> is set."],
        ["<code>fuzzy(\"word\", k)</code>", "Word tokens within char-level Levenshtein distance <code>0 &lt;= k &lt;= 3</code> (default 1), accelerated by sound q-gram thresholds."],
        ["<code>symbol(\"name\")</code> / <code>symbol:name</code>", "Exact (case-insensitive) Tree-sitter symbol definition match (<code>.rs</code>, <code>.py</code>, <code>.js</code>, <code>.ts</code>, <code>.tsx</code>, <code>.go</code>)."],
        ["<code>near(a, b, d)</code> / <code>before(a, b, d)</code>", "Both match in unit with token distance <code>|tok(b) - tok(a)| &lt;= d</code> (<code>before</code> requires <code>b</code> strictly after <code>a</code>)."],
        ["<code>in file</code> / <code>in block</code> / <code>in line</code>", "<code>file</code> (default), non-blank line runs (<code>block</code>), or single line (match <code>[start, end)</code> must lie inside the unit)."],
        ["<code>in function</code>", "Tree-sitter function/method AST nodes (<code>.rs</code>, <code>.py</code>, <code>.js</code>, <code>.ts</code>, <code>.tsx</code>, <code>.go</code>); sets <code>symbol</code> on hits."],
        ["<code>type:</code> / <code>ext:</code> / <code>path:</code>", "Values inside a category OR; across categories AND. <code>type:</code> supports <code>code</code>, <code>document</code>, <code>spreadsheet</code>, <code>config</code>, <code>log</code>, <code>data</code>, <code>archive</code>, <code>image</code>, <code>other</code>."],
        ["<code>| limit</code> / <code>files</code> / <code>count</code> / <code>group file</code>", "Compact projections and deterministic hit caps."],
        ["<code>| expand</code> / <code>sort</code> / <code>terms</code> / <code>budget</code>", "Expand surrounding parent/context, sort by hit count/path, extract co-occurring tokens, or enforce estimated token cap."],
    ],
)}

{h2("ast-langs", "24 Supported AST Languages (--lang)")}
<p>Supported by both <code>lancet ast</code> and <code>lancet refs</code>:</p>
{table(
    ["Language", "--lang Aliases", "Language", "--lang Aliases"],
    [
        ["Rust", "<code>rs</code>, <code>rust</code>", "Scala", "<code>scala</code>, <code>sc</code>"],
        ["Python", "<code>py</code>, <code>python</code>, <code>pyi</code>", "Swift", "<code>swift</code>"],
        ["JavaScript", "<code>js</code>, <code>javascript</code>, <code>mjs</code>, <code>cjs</code>, <code>jsx</code>", "Ruby", "<code>rb</code>, <code>ruby</code>"],
        ["TypeScript", "<code>ts</code>, <code>typescript</code>, <code>mts</code>, <code>cts</code>", "PHP", "<code>php</code>"],
        ["TSX", "<code>tsx</code>", "Bash", "<code>sh</code>, <code>bash</code>, <code>zsh</code>, <code>shell</code>"],
        ["Go", "<code>go</code>, <code>golang</code>", "Lua", "<code>lua</code>"],
        ["C", "<code>c</code>", "HCL / Terraform", "<code>hcl</code>, <code>tf</code>, <code>tfvars</code>, <code>terraform</code>"],
        ["C++", "<code>cpp</code>, <code>c++</code>, <code>cxx</code>, <code>cc</code>, <code>hpp</code>, <code>hxx</code>, <code>hh</code>", "HTML", "<code>html</code>, <code>htm</code>"],
        ["Zig", "<code>zig</code>", "CSS", "<code>css</code>"],
        ["Java", "<code>java</code>", "JSON", "<code>json</code>, <code>jsonc</code>"],
        ["C#", "<code>cs</code>, <code>csharp</code>, <code>c#</code>", "YAML", "<code>yaml</code>, <code>yml</code>"],
        ["Kotlin", "<code>kt</code>, <code>kts</code>, <code>kotlin</code>", "TOML", "<code>toml</code>"],
    ],
)}

{h2("ast-metavars", "Metavariable & Rewrite Semantics")}
<ul>
  <li><strong><code>$VAR</code>:</strong> matches a single named AST node; repeating the same name (<code>$X == $X</code>) enforces structural equality.</li>
  <li><strong><code>$_</code>:</strong> anonymous single-node wildcard (not captured into template output).</li>
  <li><strong><code>$$$ARGS</code>:</strong> zero or more sibling AST nodes (bounded by memoization and a per-node step budget against catastrophic backtracking).</li>
  <li><strong>Statement metavariables:</strong> C-like patterns such as <code>if ($C) $S</code> and <code>{{ $$$B }}</code> work; note that <code>if ($C) $S</code> strictly matches <code>if</code> without <code>else</code> (unlike <code>ast-grep</code>).</li>
  <li><strong>Rewrite rules:</strong> <code>--rewrite</code> defaults to dry-run; <code>--apply</code> refuses <code>--limit</code>; nested overlapping matches in one file are skipped and counted in <code>skipped_overlaps</code>; <code>--lang cpp</code> includes <code>.h</code> headers as well as <code>.cpp</code>/<code>.cc</code>.</li>
</ul>
"""


def _json_mcp(_d: str) -> str:
    return f"""
<p>When invoked with <code>--json</code> (or over <code>lancet mcp</code>), every command returns stable, snake_case JSON structs backed directly by Lancet's Rust types.</p>

{h2("status-enum", "Shared Status & Error Envelope")}
<p>Every JSON response carries a <code>status</code> field (and optional <code>error: {{"message": string, "pos"?: number}}</code>):</p>
<ul>
  <li><code>"complete"</code> (exit <code>0</code>) — finished with results or applied/dry-run edit.</li>
  <li><code>"empty"</code> (exit <code>1</code>) — finished cleanly with 0 hits.</li>
  <li><code>"invalid_query"</code> (exit <code>2</code>) — malformed LQL, invalid range/flag, or non-unique <code>--find</code>.</li>
  <li><code>"unsupported"</code> (exit <code>2</code>) — unsupported language, read-only document target, or disallowed rename site.</li>
  <li><code>"conflict"</code> (exit <code>2</code>) — stale <code>--rev</code> or concurrent modification detected under OS file lock; 0 bytes written.</li>
</ul>

{h2("json-schemas", "Command JSON Schemas")}
{code("Response (lancet '<QUERY>' --json)", """{
  "status": "complete",
  "query": "\"spin_lock\" in file",
  "mode": "auto",
  "matched": 7073,
  "matched_is_lower_bound": false,
  "returned": 20,
  "truncated": true,
  "results": [
    {
      "id": "...",
      "path": "kernel/locking/spinlock.c",
      "unit": "file",
      "start_line": 12,
      "end_line": 44,
      "symbol": "raw_spin_lock",
      "lines": [{ "line": 12, "text": "void __lockfunc raw_spin_lock(raw_spinlock_t *lock)" }],
      "more_lines": 0
    }
  ],
  "terms": [{ "term": "spinlock", "count": 42, "files": 19 }],
  "budget": { "limit_tokens": 1500, "used_tokens": 1120, "estimated": true },
  "stats": {
    "files_considered": 86218,
    "files_searched": 7814,
    "files_matched": 7073,
    "binary_skipped": 0,
    "read_errors": 0,
    "index_candidates": 7814,
    "stale_files": 0,
    "elapsed_ms": 141.3
  }
}""")}

{code("ReadResponse & EditResponse & RenameResponse", """// ReadResponse (rev omitted on PDFs, Office, HWP, archives)
{
  "status": "complete",
  "path": "src/net.rs",
  "rev": "e70d5a898f122a62",
  "section": "fn retry_limit",
  "total_lines": 80,
  "start_line": 1,
  "end_line": 4,
  "returned_lines": 4,
  "truncated": false,
  "next_line": null,
  "estimated_tokens": 28,
  "lines": [{ "num": 1, "text": "pub fn retry_limit() -> u32 {" }],
  "elapsed_ms": 0.4
}

// EditResponse (mode: "dry-run" | "applied", hunks in original file lines)
{
  "status": "complete",
  "mode": "applied",
  "path": "src/net.rs",
  "rev": "e70d5a898f122a62",
  "rev_after": "1486bdffbc06c68f",
  "start_line": 2,
  "end_line": 2,
  "hunks": [[2, 2]],
  "applied": true,
  "diff": "--- a/src/net.rs\\n+++ b/src/net.rs\\n...",
  "elapsed_ms": 1.1
}

// RenameResponse (tier: "local-scope")
{
  "status": "complete",
  "mode": "applied",
  "path": "src/search.rs",
  "old": "old_name",
  "new": "new_name",
  "tier": "local-scope",
  "rev": "...",
  "rev_after": "...",
  "declaration": { "line": 40, "col": 9 },
  "sites": [{ "line": 40, "col": 9 }, { "line": 45, "col": 14 }],
  "comment_mentions": 1,
  "applied": true,
  "elapsed_ms": 2.3
}""")}

{code("PatchResponse & RefsResponse & RankResponse & AstResponse", """// PatchResponse: { status, mode, applied, files: [{ path, action, from?, hunks, offsets, rev?, rev_after?, diff }], elapsed_ms }
// RefsResponse:  { status, mode, symbol, definitions: [RefHit], references: [RefHit], total_references, truncated, stats }
// RankResponse:  { status, mode, pattern, entries: [{ name, kind, path, line, references, definitions }], total, truncated, files_parsed, elapsed_ms }
// AstResponse:   { status, mode, lang?, pattern, matched, returned, truncated, matches: [AstMatch], diffs: [FileDiff], stats }""")}

{h2("mcp-tools", "MCP 11-Tool Reference (lancet mcp)")}
<p>Protocol revisions supported: <code>2025-03-26</code>, <code>2024-11-05</code>. Under <code>lancet mcp</code>, <code>status: "empty"</code> still returns <code>isError: false</code>; <code>lancet_rg</code> output caps cleanly at line boundaries via <code>max_output_bytes</code> (default 128 KiB, hard cap 4 MiB).</p>
{table(
    ["MCP Tool", "Role", "Key Parameters & Notes"],
    [
        ["<code>lancet_fd</code>", "Files/dirs discovery", "<code>pattern</code>, <code>path</code>, <code>dirs</code>, <code>tree</code>, <code>exts</code>, <code>type_classes</code>, <code>changed_within</code>, <code>index_mode</code>"],
        ["<code>lancet_search</code>", "LQL query", "<code>query</code>, <code>path</code>, <code>max_lines</code>, <code>max_cols</code>, <code>ocr</code>, <code>index_mode</code>"],
        ["<code>lancet_rg</code>", "ripgrep search", "<code>pattern</code>, <code>paths</code>, <code>docs</code>, <code>passthru</code>, <code>type_add</code>, <code>max_output_bytes</code>"],
        ["<code>lancet_outline</code>", "Skeletons & manifests", "<code>path</code>, <code>max_depth</code>, <code>exts</code>, <code>limit</code>, <code>ocr</code>"],
        ["<code>lancet_read</code>", "Exact slices & docs", "<code>target</code>, <code>lines</code>, <code>symbol</code>, <code>page</code>, <code>sheet</code>, <code>rows</code>, <code>cols</code>, <code>entry</code>, <code>budget_tokens</code>"],
        ["<code>lancet_refs</code>", "Lexical AST refs & rank", "<code>symbol</code>, <code>path</code>, <code>rank</code>, <code>kinds</code>, <code>lang</code>, <code>limit</code>"],
        ["<code>lancet_ast</code>", "AST search & rewrite", "<code>pattern</code>, <code>path</code>, <code>lang</code>, <code>rewrite</code>, <code>apply</code> (refused under <code>--read-only</code>)"],
        ["<code>lancet_edit</code>", "Locked CAS edit", "Omitted under <code>--read-only</code>; supports <code>lines</code>, <code>symbol</code>, <code>find</code>, <code>edits</code>, <code>rev</code>, <code>apply</code>"],
        ["<code>lancet_patch</code>", "Atomic unified diff", "Omitted under <code>--read-only</code>; <code>patch</code>, <code>root</code>, <code>strip</code>, <code>apply</code>, <code>allow_syntax_errors</code>"],
        ["<code>lancet_rename</code>", "Local binding rename", "Omitted under <code>--read-only</code>; <code>path</code>, <code>line</code>, <code>col?</code>, <code>old</code>, <code>new</code>, <code>rev?</code>, <code>apply?</code>"],
        ["<code>lancet_index</code>", "Index management", "<code>action</code> (<code>status</code>, <code>sync</code>, <code>rebuild</code>, <code>compact</code>); mutating actions refused under <code>--read-only</code>"],
    ],
)}

{h2("troubleshooting", "Troubleshooting & Refusal Recovery")}
{table(
    ["Error / Refusal", "Cause", "Exact Recovery"],
    [
        ["<code>status: \"conflict\"</code> (exit 2)", "File changed on disk since <code>rev</code> was read", "Run <code>lancet read</code> again, recompute your edit against the new lines, and retry with the fresh <code>rev</code>."],
        ["<code>--find</code> refused (non-unique)", "Literal appeared 0 or &gt;1 times", "Inspect line numbers printed in the error; expand surrounding context in <code>--find</code> or add <code>--all</code> if intentional."],
        ["Syntax error increase refused", "Edit or patch increased Tree-sitter error count", "Check the preview diff for missing braces/quotes; pass <code>--allow-syntax-errors</code> only if intentionally staging a partial fix."],
        ["<code>lancet rename</code> refused", "Top-level/field/Python param, macro/<code>eval</code>, or shadowing", "Check error message; for non-local/cross-file symbols use <code>lancet refs</code> + <code>lancet edit --edits</code> or your LSP."],
        ["<code>ast --rewrite --apply</code> with <code>--limit</code>", "Partial capped rewrites cannot be applied blindly", "Drop <code>--limit</code> or narrow scope with <code>--path</code> globs before <code>--apply</code>."],
        ["<code>lancet rg -P</code> error", "PCRE2 lookarounds/backreferences unsupported", "Use Rust <code>regex</code> without lookarounds, or rewrite via LQL <code>before()</code> / <code>NOT</code>."],
        ["<code>lancet fd</code> usage error on 2 paths", "<code>fd</code> accepts at most one <code>[PATH]</code> argument", "Invoke once per directory or search from repo root with <code>-p</code> / glob filters."],
    ],
)}
"""


PAGES_EN = [
    {
        "path": "index.html",
        "group": "start",
        "nav_title": "Introduction & Cheat Sheet",
        "title": "Lancet Documentation — Introduction & Cheat Sheet",
        "description": "Install Lancet, run a 60-second walkthrough, and keep the one-screen CLI cheat sheet handy.",
        "lede": "Find, read, and change code and documents on one coordinate system, at index speed.",
        "keywords": "install homebrew cargo curl windows cheat sheet overview",
        "source": "https://github.com/CREE1116/lancet/blob/main/README.md",
        "render": _intro,
    },
    {
        "path": "migration.html",
        "group": "start",
        "nav_title": "Migration from rg, fd, sg, sed",
        "title": "Coming from rg, fd, ast-grep, sed, patch & rga",
        "description": "Side-by-side Rosetta Stone, intentional behavioral differences, and when to keep specialized tools.",
        "lede": "Keep the mental models you already know from ripgrep, fd, and ast-grep — with unified coordinates and zero-loss editing.",
        "keywords": "migration ripgrep rg fd ast-grep sg sed patch rga pcre2 differences",
        "source": f"{REPO_DOCS}/rg-parity.md",
        "render": _migration,
    },
    {
        "path": "guides/explore.html",
        "group": "guides",
        "nav_title": "Exploring Codebases",
        "title": "Exploring Unfamiliar Codebases with Low Context",
        "description": "5-step exploration funnel using fd --dirs, refs --rank, outline, batch, and budget-tokens pagination.",
        "lede": "Orient cleanly in five calls instead of burning context tokens on blind whole-file reads.",
        "keywords": "explore fd dirs tree refs rank outline batch budget-tokens pagination",
        "source": f"{REPO_DOCS}/agent-instructions.md",
        "render": _explore,
    },
    {
        "path": "guides/editing.html",
        "group": "guides",
        "nav_title": "Safe Editing, Patch & Rename",
        "title": "Safe Editing, Multi-Hunk CAS, Patching & Local Rename",
        "description": "Coordinate-safe edit --find/--symbol/--edits, multi-agent rev compare-and-swap, patch rollback, and tier-1 rename.",
        "lede": "Zero line drift across multi-hunk edits, zero silent overwrites across concurrent agents, and scope-proven local renames.",
        "keywords": "edit edits find symbol rev conflict cas patch rename local scope tree-sitter",
        "source": f"{REPO_DOCS}/agent-instructions.md",
        "render": _editing,
    },
    {
        "path": "guides/documents.html",
        "group": "guides",
        "nav_title": "PDFs, Office, HWP & Archives",
        "title": "Searching & Reading PDFs, Office, HWP & Archives",
        "description": "In-process extraction for PDF, DOCX, PPTX, HWP/HWPX, XLSX/ODS, EPUB, ZIP, and TAR with shared line numbers.",
        "lede": "One in-process extraction pipeline so a search hit in a PDF, Word file, or tarball opens at the exact same line.",
        "keywords": "pdf docx pptx odt hwp hwpx epub xlsx xls ods zip tar gz ocr",
        "source": f"{REPO_DOCS}/grammar.md",
        "render": _documents,
    },
    {
        "path": "guides/agents-mcp.html",
        "group": "guides",
        "nav_title": "AI Agents, MCP & Skill",
        "title": "Integrating AI Agents: MCP Server & Official Skill",
        "description": "Setup lancet mcp for Claude Code, Cursor, Claude Desktop, Antigravity, and Codex, plus skills/lancet.",
        "lede": "Expose all 11 engines over newline-delimited stdio JSON-RPC or install the cross-agent Skill in two commands.",
        "keywords": "mcp claude cursor codex antigravity skill read-only root agents",
        "source": f"{REPO_DOCS}/mcp.md",
        "render": _agents_mcp,
    },
    {
        "path": "concepts/index-and-freshness.html",
        "group": "concepts",
        "nav_title": "Index & Freshness Model",
        "title": "The .lancet Trigram LSM Index & Freshness",
        "description": "How sound trigram lowering, (mtime, size, ctime, inode) + racy checks, and LSM compaction guarantee Scan == Auto.",
        "lede": "Why Lancet guarantees Scan == Auto without a background watcher daemon — and when --trust-index makes sense.",
        "keywords": "index trigram lsm auto trust-index no-index freshness ctime inode racy windows",
        "source": f"{REPO_DOCS}/why-one-tool.md",
        "render": _index_freshness,
    },
    {
        "path": "concepts/architecture-and-benchmarks.html",
        "group": "concepts",
        "nav_title": "Architecture & Benchmarks",
        "title": "Architecture, Failure Modes & Honest Benchmarks",
        "description": "7 failure modes fixed, 8-codebase benchmarks vs rg/zoekt/ast-grep, and honest comparison against tgrep/grix/aft.",
        "lede": "Measured across 8 repositories and compared honestly against ripgrep, zoekt, ast-grep, tgrep, grix, and aft.",
        "keywords": "benchmarks suite competitors zoekt tgrep grix aft failure modes performance",
        "source": "https://github.com/CREE1116/lancet/blob/main/bench/results/COMPETITORS.md",
        "render": _arch_bench,
    },
    {
        "path": "reference/cli.html",
        "group": "reference",
        "nav_title": "CLI & lancet rg Reference",
        "title": "Complete CLI & lancet rg Parity Reference",
        "description": "Every subcommand, flag, exit code, and full ripgrep feature parity matrix.",
        "lede": "Exhaustive subcommand syntax, exit codes, and ripgrep 15.2 compatibility matrix.",
        "keywords": "cli flags subcommands exit codes rg parity passthru type-add",
        "source": f"{REPO_DOCS}/rg-parity.md",
        "render": _cli,
    },
    {
        "path": "reference/query-and-ast.html",
        "group": "reference",
        "nav_title": "LQL & 24 AST Languages",
        "title": "Lancet Query Language (LQL v0.1) & 24-Language AST Spec",
        "description": "Formal EBNF grammar, operator precedence, scopes, pipe stages, 24 AST language aliases, and metavariables.",
        "lede": "Formal EBNF grammar, matcher/scope semantics, 24 --lang aliases, and AST metavariable rules.",
        "keywords": "lql grammar near before fuzzy symbol regex ast metavariables languages",
        "source": f"{REPO_DOCS}/grammar.md",
        "render": _query_ast,
    },
    {
        "path": "reference/json-and-mcp.html",
        "group": "reference",
        "nav_title": "JSON Schemas, MCP & Recovery",
        "title": "JSON Output Schemas, MCP 11-Tool Spec & Troubleshooting",
        "description": "Exact JSON response payloads verified against Rust structs, all 11 MCP tools, and error recovery recipes.",
        "lede": "Machine-readable schemas, MCP tool inventory, and concrete recovery recipes for every refusal.",
        "keywords": "json schema response mcp tools status conflict troubleshooting recovery",
        "source": f"{REPO_DOCS}/mcp.md",
        "render": _json_mcp,
    },
]
