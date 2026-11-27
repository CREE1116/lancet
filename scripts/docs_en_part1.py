"""English documentation pages 1-7: Getting Started & Search/Discovery."""

from __future__ import annotations

from docs_ui import callout, cards, code, h2, h3, table

REPO_DOCS = "https://github.com/CREE1116/lancet/blob/main/docs"


def _intro(_d: str) -> str:
    return f"""
<p><strong>Lancet</strong> replaces the <code>rg</code> + <code>fd</code> + <code>ast-grep</code> + <code>cat</code> + <code>sed</code> + <code>patch</code> + <code>pdftotext</code> chain with one dependency-free Rust binary. Every command shares the same <code>.lancet</code> trigram index, traversal filter rules, <strong>1-based line coordinates</strong>, and <strong>XXH3-64 <code>rev</code></strong> file revisions.</p>

{callout("tip", "Design contract in one sentence", "<p>The trigram index only decides which files <em>not</em> to open; candidates and live-changed files go through the exact same evaluator as a full scan, and every write runs through locked compare-and-swap.</p>")}

{h2("install", "Installation & Build Variants")}
<p>Prebuilt binaries are published on every release for macOS (Apple Silicon &amp; Intel), Linux (x86_64/arm64 glibc and static musl), and Windows.</p>

{code("macOS / Linux (Homebrew)", "brew tap CREE1116/lancet https://github.com/CREE1116/lancet\nbrew install CREE1116/lancet/lancet")}
{code("macOS / Linux (Prebuilt Release Script)", "curl -fsSL https://raw.githubusercontent.com/CREE1116/lancet/main/install.sh | sh")}
{code("Windows (PowerShell)", "irm https://raw.githubusercontent.com/CREE1116/lancet/main/install.ps1 | iex")}
{code("Cargo (Full vs Lean Code-Only Build)", "# Prebuilt binary via cargo-binstall\ncargo binstall --git https://github.com/CREE1116/lancet lancet\n\n# Compile full build (~40 MB: 24 Tree-sitter grammars + PDF/Office/HWP/archive adapters)\ncargo install --git https://github.com/CREE1116/lancet lancet\n\n# Lean build (~4 MB: pure source/text/code, strips document & archive adapters)\ncargo install --git https://github.com/CREE1116/lancet lancet --no-default-features")}

{h2("walkthrough", "60-Second End-to-End Walkthrough")}
<p>Building <code>.lancet</code> is optional. Every command falls back seamlessly when no index exists, and default <code>auto</code> mode checks file metadata on disk on every query so edited files are never missed.</p>
{code("Terminal", "# 1. Optional: build the local trigram index once\nlancet index .\n\n# 2. Orient with folder-only tree + usage-ranked core symbols\nlancet fd --tree --dirs -d 2 .\nlancet refs --rank '*Service' --kind class src --limit 8\n\n# 3. Search with ripgrep flags or LQL boolean/proximity queries\nlancet rg -w execute_one src\nlancet 'near(\"gradient\", \"clip\", 30) in function | expand parent'\n\n# 4. Read exact symbol or lines and capture its XXH3-64 `rev`\nlancet read src/search.rs --symbol execute_one --json\n\n# 5. Preview dry-run diff, then commit atomically with --rev and --apply\nlancet edit src/search.rs --find 'let limit = 20;' \\\n  --with 'let limit = 50;' --rev <REV> --apply")}

{h2("cheat-sheet", "One-Screen Command Cheat Sheet")}
{table(
    ["Workflow Step", "Command", "Safety / Performance Guarantee"],
    [
        ["Folder layout", "<code>lancet fd --tree --dirs -d 2 .</code>", "Respects <code>.gitignore</code> / <code>.ignore</code>"],
        ["Recent / typed files", "<code>lancet fd -t f -e rs --changed-within 7d</code>", "Units: <code>s</code>, <code>m</code>, <code>h</code>, <code>d</code>, <code>w</code>"],
        ["Drop-in ripgrep", "<code>lancet rg -w -t rs 'pat' src</code>", "Byte-identical to <code>rg</code> + opt-in <code>--docs</code>"],
        ["Boolean / fuzzy LQL", "<code>lancet '(\"a\" OR \"b\") NOT \"c\" in function'</code>", "<code>Scan == Auto</code>"],
        ["Multi-query batch", "<code>lancet batch src -q 'Q1' -q 'Q2'</code>", "One index open &amp; directory walk"],
        ["Skeleton outline", "<code>lancet outline src/main.rs</code>", "Functions, MD headings, sheets, ZIP entries"],
        ["Budgeted read", "<code>lancet read f.rs --lines 10:50 --budget-tokens 800</code>", "Returns <code>rev</code> + <code>next_line</code> cursor"],
        ["Symbol read", "<code>lancet read f.rs --symbol my_fn</code>", "Includes doc comments &amp; attributes"],
        ["Document / archive", "<code>lancet read r.pdf -p 2:3</code> · <code>a.zip!app.log</code>", "1-to-1 line numbers with search; read-only"],
        ["Ranked declarations", "<code>lancet refs --rank '*Service' --kind class src</code>", "Sorted by lexical AST reference count"],
        ["AST search/rewrite", "<code>lancet ast '$X.unwrap()' src -l rs -r '$X?'</code>", "24 languages; dry-run until <code>--apply</code>"],
        ["Atomic multi-edit", "<code>lancet edit f.rs --edits - --rev REV --apply</code>", "Original coordinates, overlap-checked CAS"],
        ["Unified diff patch", "<code>git diff | lancet patch --apply</code>", "All-file pre-check + CAS rollback"],
        ["Local binding rename", "<code>lancet rename f.rs:42 old new --rev REV --apply</code>", "6 languages; scope re-resolution proof"],
        ["MCP stdio server", "<code>lancet mcp --root .</code>", "11 tools, JSON-RPC 2.0, <code>--read-only</code> option"],
        ["Index diagnostics", "<code>lancet explain 'pat'</code> · <code>lancet status</code>", "Trigram requirements &amp; selectivity %"],
    ],
)}

{h2("next-steps", "Where to Go Next")}
{cards([
    ("Getting Started", "Migration & Filtering Rules", "Exact Rosetta Stone from rg/fd/sg/sed/patch plus how .gitignore, globs, and file types evaluate.", "migration.html"),
    ("Search & Discovery", "Explore, LQL, AST & Documents", "Low-context exploration funnel, boolean/proximity DSL, 24-language AST rewrites, PDF/HWP/ZIP reading.", "guides/explore.html"),
    ("Editing & Agents", "Edit, Patch, Rename & MCP", "Locked --rev CAS editing, multi-file patch rollback, tier-1 scope-proven rename, MCP setup.", "guides/edit.html"),
    ("Internals & Reference", "LANCET04, Write Safety & Schemas", "Index segment format, racy-timestamp rules, OS locking, CLI flags, JSON structs, and troubleshooting.", "reference/cli.html"),
])}
"""


def _migration(_d: str) -> str:
    return f"""
<p>You can switch incrementally without rewriting muscle memory. Use <code>lancet rg</code> and <code>lancet patch</code> as drop-in replacements for existing flags and diff pipes, then adopt LQL, AST rewrites, and <code>--rev</code> CAS edits where multi-tool seams break.</p>

{h2("from-rg", "Coming from ripgrep (rg)")}
<p><code>lancet rg</code> implements ripgrep's flags, Rust <code>regex</code> engine, ignore traversal, exit codes (<code>0</code> match, <code>1</code> no match, <code>2</code> error), and output formats — tested byte-for-byte against <code>ripgrep 15.2</code> across 68 edge-case fixtures and full Linux kernel runs.</p>
{table(
    ["ripgrep Command", "lancet Equivalent", "Exact Behavioral Note"],
    [
        ["<code>rg foo src</code>", "<code>lancet rg foo src</code>", "<code>-n</code> is ON by default even when piped; pass <code>-N</code> to hide"],
        ["<code>rg -i -w -t py tok</code>", "<code>lancet rg -i -w -t py tok</code>", "Last of <code>-i</code>/<code>-s</code>/<code>-S</code> wins; AST-aware smart case (<code>\\W</code> keeps insensitive)"],
        ["<code>rg -g '*.toml' --passthru</code>", "<code>lancet rg -g '*.toml' --passthru</code>", "Last of <code>--passthru</code>/<code>-A</code>/<code>-B</code>/<code>-C</code> wins; prints unmatched files too"],
        ["<code>rg --type-add 'web:*.tsx'</code>", "<code>lancet rg --type-add 'web:*.tsx' -t web</code>", "Supports <code>name:glob</code> and <code>name:include:a,b</code>"],
        ["<code>rg -P '(?&lt;=a)b'</code>", "<code>lancet 'before(\"a\", \"b\", 1)'</code> or Rust regex", "<code>-P</code>/PCRE2 omitted for linear-time matching &amp; sound trigram lowering"],
        ["<code>rg --pre pdftotext pat</code>", "<code>lancet rg --docs pat</code>", "In-process PDF/Office/HWP/archive extraction instead of subprocesses"],
    ],
)}

{callout("note", "Intentional differences in lancet rg", "<ul><li><strong>Always sorted by path component:</strong> <code>a/b.txt</code> before <code>a.txt</code>, equivalent to <code>rg --sort path</code>.</li><li><strong>Line numbers ON by default:</strong> stays enabled under pipes/redirects unless <code>-N</code> is passed.</li><li><strong>No headings:</strong> output is always <code>path:line:text</code>, with <code>--color auto</code> on TTYs only.</li></ul>")}

{h2("from-fd", "Coming from fd / find")}
{table(
    ["fd / find Command", "lancet fd Equivalent", "Note / Advantage"],
    [
        ["<code>fd -e rs -d 2 src</code>", "<code>lancet fd -e rs -d 2 src</code>", "Uses <code>.lancet</code> when present; single optional <code>[PATH]</code>"],
        ["<code>fd -t d -d 2</code>", "<code>lancet fd --tree --dirs -d 2 .</code>", "Folders-only ASCII tree for compact orientation"],
        ["<code>find . -mtime -7 -type f</code>", "<code>lancet fd -t f --changed-within 7d</code>", "Units: <code>s</code>, <code>m</code>, <code>h</code>, <code>d</code>, <code>w</code>"],
        ["<code>find . -name '*.pdf'</code>", "<code>lancet fd --type-class document</code>", "9 semantic classes instead of listing extensions manually"],
        ["<code>fd config src</code> (if pattern matches dir name)", "<code>lancet fd -p config src</code>", "<code>-p</code>/<code>--pattern</code> disambiguates pattern from path"],
    ],
)}

{h2("from-ast", "Coming from ast-grep (sg)")}
{table(
    ["ast-grep", "lancet ast", "Key Difference"],
    [
        ["<code>sg -p '$X.unwrap()' -l rs</code>", "<code>lancet ast '$X.unwrap()' -l rs</code>", "Accelerated by <code>.lancet</code> trigram prefilter on pattern literals"],
        ["<code>sg -p 'f($$$A)' -r 'g($$$A)' -U</code>", "<code>lancet ast 'f($$$A)' -r 'g($$$A)' --apply</code>", "Dry-run diff until <code>--apply</code>; <code>--limit</code> refused with <code>--apply</code>"],
        ["<code>sg -p 'if ($C) $S'</code>", "<code>lancet ast 'if ($C) $S'</code>", "<strong>Strict AST shape:</strong> matches plain <code>if</code> only; <code>if-else</code> is a separate node"],
        ["<code>sg -l cpp ...</code>", "<code>lancet ast ... -l cpp</code>", "Lancet <code>cpp</code> scans <code>.h</code> headers alongside <code>.cpp</code>/<code>.cc</code>"],
    ],
)}

{h2("from-edit", "Coming from cat / sed / patch / rga")}
{table(
    ["Old Chain", "Lancet Replacement", "What Stops Breaking"],
    [
        ["<code>sed -n '10,50p' f.rs</code>", "<code>lancet read f.rs --lines 10:50</code>", "Returns XXH3-64 <code>rev</code> and respects <code>--budget-tokens</code>"],
        ["Two <code>sed -i</code> calls from one <code>rg -n</code>", "<code>lancet edit f.rs --edits - --rev REV --apply</code>", "Original coordinates for all hunks; preserves symlinks, hard links, mode"],
        ["<code>patch -p1 &lt; diff</code>", "<code>lancet patch diff --apply</code>", "All-file pre-verification, exact relocation, CAS race rollback"],
        ["<code>rga</code> + <code>pdftotext</code>/<code>pandoc</code>", "<code>lancet 'pat'</code> + <code>lancet read f.pdf -p 2</code>", "Shared extraction means search hit line == read line"],
    ],
)}

{h2("when-not", "When NOT to Replace Specialized Tools")}
{callout("warn", "Intentional scope boundaries", "<ul><li><strong>Version control:</strong> keep using <code>git</code>; Lancet respects <code>.gitignore</code> and consumes <code>git diff</code> via <code>lancet patch</code>.</li><li><strong>Cross-file type-checked renames:</strong> use your language's LSP (<code>rust-analyzer</code>, <code>tsserver</code>, <code>gopls</code>) for public APIs, fields, traits, or Python keyword parameters across files. Lancet intentionally restricts <code>rename</code> to scope-proven single-file local bindings.</li></ul>")}
"""


def _filtering(_d: str) -> str:
    return f"""
<p>Lancet uses ripgrep's <code>ignore</code> crate traversal rules uniformly across <code>lancet</code>, <code>rg</code>, <code>fd</code>, <code>outline</code>, <code>refs</code>, <code>ast</code>, and <code>index</code> so discovery, search, and indexing agree on which files exist.</p>

{h2("ignore-files", "Ignore Precedence, Hidden & Binary Files")}
<ul>
  <li><strong>Ignore stack:</strong> honors <code>.gitignore</code>, global/repo git excludes, <code>.ignore</code>, and <code>.rgignore</code> (unless <code>--no-ignore</code> or <code>-u</code> is set).</li>
  <li><strong>Hidden dotfiles:</strong> files/directories starting with <code>.</code> (including <code>.lancet</code>) are skipped by default; pass <code>--hidden</code> / <code>-.</code> (or <code>-uu</code> in <code>lancet rg</code>) to include them.</li>
  <li><strong>Binary detection:</strong> plain-text search skips binary files containing NUL bytes unless <code>-a</code> / <code>--text</code> / <code>-uuu</code> is passed, or a document/archive adapter handles the extension.</li>
  <li><strong>Positive glob override:</strong> in <code>lancet rg</code>, positive globs like <code>-g '*.lock'</code> override ignore rules and hidden status identically to ripgrep.</li>
</ul>

{h2("rg-unrestricted", "Unrestricted Escalation (-u / -uu / -uuu) in lancet rg")}
{table(
    ["Flag", "Equivalent Flags", "Effect"],
    [
        ["<code>-u</code>", "<code>--no-ignore</code>", "Search ignored files (build artifacts, <code>node_modules</code>, gitignored logs)"],
        ["<code>-uu</code>", "<code>--no-ignore --hidden</code>", "Also search hidden dotfiles and directories"],
        ["<code>-uuu</code>", "<code>--no-ignore --hidden --text</code>", "Also search binary files as raw text"],
    ],
)}

{h2("globs-types", "Globs, Built-in Types & --type-add")}
{code("Terminal", "# Include & exclude globs (! negates; case-insensitive --iglob also supported)\nlancet rg -g 'src/**' -g '!src/generated/**' 'retry' .\n\n# Restrict to / exclude ripgrep built-in file types\nlancet rg -t rust -T test 'unsafe' .\n\n# Define custom types or merge existing ones (repeatable)\nlancet rg --type-add 'web:*.{ts,tsx,css}' -t web 'ThemeProvider' .\nlancet rg --type-add 'jvm:include:java,kotlin,scala' -t jvm 'Executor' .\n\n# Inspect all known definitions (including your --type-add)\nlancet rg --type-add 'proto:*.proto' --type-list", "en")}

{h2("lql-filters", "LQL & fd Filters (type:, ext:, path:, --type-class)")}
<p>In LQL queries, multiple values inside a category are <strong>OR-ed</strong>, while different categories are <strong>AND-ed</strong>. Notice a subtle rule on <code>path:</code> implemented in <code>src/walk.rs</code>: when a <code>path:</code> value has no glob metacharacters (<code>*</code>, <code>?</code>, <code>[</code>, <code>{{</code>), Lancet automatically matches both <code>dir</code> and <code>dir/**</code>.</p>
{code("Terminal", "# OR within ext:(...), AND across type: and path:\nlancet '\"deadline\" in ext:(rs,toml) path:crates/core'\n\n# Discover files by Lancet semantic class\nlancet fd --type-class document,spreadsheet --changed-within 14d", "en")}
{table(
    ["Class (<code>type:</code> / <code>--type-class</code>)", "Covered Extensions / Examples"],
    [
        ["<code>code</code>", "Source files across 24+ programming languages (<code>.rs</code>, <code>.py</code>, <code>.ts</code>, <code>.go</code>, <code>.c</code>, <code>.cpp</code>, …)"],
        ["<code>document</code>", "<code>.md</code>, <code>.txt</code>, <code>.rst</code>, <code>.pdf</code>, <code>.docx</code>, <code>.pptx</code>, <code>.odt</code>, <code>.hwp</code>, <code>.hwpx</code>, <code>.epub</code>"],
        ["<code>spreadsheet</code>", "<code>.xlsx</code>, <code>.xls</code>, <code>.xlsm</code>, <code>.xlsb</code>, <code>.ods</code>, <code>.csv</code>, <code>.tsv</code>"],
        ["<code>config</code>", "<code>.toml</code>, <code>.yaml</code>, <code>.yml</code>, <code>.ini</code>, <code>.env</code>, <code>.hcl</code>, <code>.tf</code>"],
        ["<code>data</code>", "<code>.json</code>, <code>.jsonc</code>, <code>.xml</code>, <code>.sql</code>, etc."],
        ["<code>log</code> · <code>archive</code> · <code>image</code> · <code>other</code>", "<code>.log</code> · <code>.zip/.tar/.gz/.tgz</code> · <code>.png/.jpg/.webp/…</code> · remaining text/files"],
    ],
)}

{h2("bounds", "Symlinks, Depth & File-Size Bounds")}
<ul>
  <li><strong>Symlinks (<code>-L</code> / <code>--follow</code>):</strong> disabled by default. On <code>lancet rg -L --trust-index</code>, Lancet deliberately falls back to a live filesystem walk so symlinked targets absent from <code>.lancet</code> are never skipped.</li>
  <li><strong>Depth &amp; size:</strong> <code>--max-depth N</code> (<code>1</code> = direct children only) and <code>--max-filesize NUM[K|M|G]</code> (in <code>rg</code>) or <code>--min-size</code>/<code>--max-size</code> (in <code>fd</code>) are enforced in both live walks and <code>--trust-index</code>.</li>
  <li><strong>Single-path rule on <code>fd</code>:</strong> <code>lancet fd [PATTERN] [PATH]</code> accepts at most one path; pass <code>-p PATTERN</code> if your pattern happens to match a folder name.</li>
</ul>
"""


def _explore(_d: str) -> str:
    return f"""
<p>Reading full files burns context tokens fast. Use Lancet's discovery commands — <code>fd</code>, <code>outline</code>, and <code>refs</code> — to turn an unfamiliar tree into a ranked map of exact files, declarations, and line bounds.</p>

{h2("fd-guide", "Fast Tree & Metadata Discovery (lancet fd)")}
<p><code>lancet fd</code> can read from disk or directly from the mmap'd <code>.lancet</code> index when <code>--trust-index</code> is passed (no directory walk at all).</p>
{code("Terminal", "# Compact folders-only architecture map (no file noise)\nlancet fd --tree --dirs --max-depth 2 .\n\n# Regex/glob filename matching (pass -p when PATTERN looks like a dir)\nlancet fd -g '*_test.rs' crates/\nlancet fd -p config -r 'auth|session' src\n\n# Recency + size bounds (durations: s, m, h, d, w; sizes: b, k, m, g)\nlancet fd -t f --changed-within 48h --min-size 4k --max-size 2m .\n\n# JSON with file sizes and mtime_ns (costs 1 stat per returned entry)\nlancet fd -e rs --meta --limit 25 --json src")}
{callout("note", "When -e / --type-class / size filters are set on fd", "<p>If you filter by extension (<code>-e</code>), <code>--type-class</code>, <code>--min-size</code>, or <code>--max-size</code> without specifying <code>-t</code>, <code>lancet fd</code> narrows results to files automatically unless <code>--dirs</code> or <code>-t d</code> is also given.</p>")}

{h2("outline-guide", "Declaration, Heading & Archive Skeletons (lancet outline)")}
<p>Inspect structure before opening lines. <code>lancet outline</code> understands AST declarations (code), Markdown headings, PDF/PPTX/DOCX/HWP headings, spreadsheet sheet names + dimensions, and archive entry manifests:</p>
{code("Terminal", "# Single file AST skeleton with line spans & signatures\nlancet outline src/search.rs\n\n# Directory outline capped to depth & extension\nlancet outline src -d 2 -e rs --limit 20 --json\n\n# Non-code outlines: Markdown TOC, workbook sheets, zip/tar entries\nlancet outline docs/spec.md\nlancet outline financial_model.xlsx\nlancet outline logs/crash_bundle.tar.gz")}

{h2("refs-and-rank", "Lexical AST References & Usage Ranking (lancet refs)")}
<p>Unlike plain <code>rg</code>, <code>lancet refs</code> parses candidate files with Tree-sitter across all 24 supported languages, separates <code>[def]</code> declarations from <code>[ref]</code> identifier sites, records the enclosing symbol, and <strong>excludes comments and string literals</strong>.</p>
{code("Terminal", "# Find definitions and all lexical call/reference sites\nlancet refs SearchOptions src --lang rs\n\n# Also scan non-code files (Markdown, TOML, configs) with lexical matching\nlancet refs SearchOptions . --all-files")}

{h3("rank-mode", "Finding Core Symbols with refs --rank")}
<p>Pass <code>--rank</code> to treat the first argument as a glob pattern over declaration names (for example <code>*Service</code> or <code>*Handler</code>) and sort matching declarations by lexical reference count descending. When the glob contains a literal run (like <code>Service</code> in <code>*Service</code>), Lancet accelerates file selection with the trigram index.</p>
{code("Terminal", "# Most-referenced *Service classes in src/\nlancet refs --rank '*Service' --kind class src --limit 10\n\n# Top functions & structs in a Rust crate\nlancet refs --rank '*' --kind function,struct src --limit 15 --json")}
<p>If several declarations share the same name (e.g. <code>new</code> or <code>run</code>), <code>lancet refs --rank</code> prints <code>(name shared by N declarations)</code> and sets <code>definitions: N</code> in JSON so you know the lexical count aggregates references across overloaded or same-named definitions.</p>
"""


def _search(_d: str) -> str:
    return f"""
<p>Lancet offers two complementary search syntaxes backed by the exact same trigram index and evaluator: <code>lancet rg</code> (regexes and UNIX flags) and <strong>LQL</strong> (structured boolean logic, token proximity, fuzzy matching, AST scopes, and pipes).</p>

{h2("rg-vs-lql", "When to Use lancet rg vs LQL vs lancet batch")}
{table(
    ["Need", "Use", "Why"],
    [
        ["Standard regex, context lines (<code>-A/-B/-C</code>), <code>--vimgrep</code>, <code>-c</code>, <code>-r</code>", "<code>lancet rg</code>", "Drop-in ripgrep flags, zero query-language friction"],
        ["Multi-term <code>AND</code>/<code>OR</code>/<code>NOT</code>, <code>near()</code>, <code>fuzzy()</code>, <code>in function</code>, <code>| budget</code>", "<code>lancet '&lt;LQL&gt;'</code>", "Single-pass compound query with token cap"],
        ["Multiple LQL queries over the same repository", "<code>lancet batch</code>", "Opens <code>.lancet</code> and walks directory entries once"],
    ],
)}

{h2("lql-recipes", "LQL Practical Patterns")}
{code("Terminal", "# 1. Boolean logic in a single line (implicit AND + explicit OR/NOT)\nlancet '(\"OOM\" OR \"out of memory\") NOT \"CPU\" in line' src/\n\n# 2. Word proximity within a Tree-sitter function body + parent expansion\nlancet 'near(\"retry\", \"backoff\", 25) in function | expand parent | limit 8' src/\n\n# 3. Ordered proximity (B appears within 15 tokens AFTER A)\nlancet 'before(\"lock\", \"write_atomic\", 15) in function' src/\n\n# 4. Typo-tolerant search (Levenshtein distance k <= 3, trigram q-gram accelerated)\nlancet 'fuzzy(\"idempotnt\", 2) | limit 10' src/\n\n# 5. Inspect co-occurring keywords for iterative query refinement\nlancet '\"trigram\" in ext:rs | terms top:15' src/\n\n# 6. Hard cap output to estimated token budget\nlancet '\"SearchOptions\" in ext:rs | expand context:3 | budget tokens:1500' src/")}

{h2("batch-queries", "Sharing One Walk Across Many Queries (lancet batch)")}
<p>Agents often run 3–5 exploratory searches back-to-back. <code>lancet batch</code> opens the index and caches the directory walk in memory across every query in the batch:</p>
{code("Terminal", "# Using repeatable -q / --name flags\nlancet batch src \\\n  -q '\"Conflict\" in function' --name conflicts \\\n  -q 'near(\"mtime\", \"ctime\", 20)' --name freshness \\\n  -q 'fuzzy(\"compaction\", 1) | files' --name compact\n\n# Or passing JSON object / array directly\nlancet batch src --queries-json '{\"errors\": \"Status::Unsupported\", \"locks\": \"lock_shared OR lock()\"}'")}

{h2("debug-query", "Inspecting Query Plans (lancet parse & lancet explain)")}
<p>Before running a query on an 80k-file repo, use <code>parse</code> to inspect canonical normalization and <code>explain</code> to verify which trigrams were extracted and how selective the index is:</p>
{code("Terminal", "lancet parse '(\"alpha\" OR \"beta\") NOT NOT \"gamma\" in line | limit 5'\nlancet explain 'near(\"gradient\", \"clip\", 30)' ./repo")}
<p><code>explain</code> prints <code>trigram_req</code>, <code>files_considered</code>, <code>index_candidates</code>, <code>stale_files</code>, and <code>selectivity_pct</code> so you immediately see whether a regex or short token causes a fallback full scan.</p>
"""


def _ast(_d: str) -> str:
    return f"""
<p><code>lancet ast</code> parses syntax trees with 24 compiled-in Tree-sitter grammars. Unlike plain regexes, patterns match true syntactic structure — ignoring whitespace differences, formatting, comments, and string contents.</p>

{h2("metavariables", "Metavariables & Equality Constraints")}
<ul>
  <li><strong><code>$VAR</code> (named single node):</strong> captures one named AST node. Repeating the same metavariable (e.g., <code>$X == $X</code> or <code>$X = $X</code>) enforces <strong>structural equality</strong> across the two subtrees.</li>
  <li><strong><code>$_</code> (anonymous wildcard):</strong> matches any single named AST node without allocating or storing a named capture.</li>
  <li><strong><code>$$$ARGS</code> (variadic sibling sequence):</strong> matches 0 or more sibling AST nodes (function arguments, struct fields, statement blocks).</li>
  <li><strong>C-like statement metavariables:</strong> in C, C++, Rust, Go, JS/TS/TSX, Java, C#, etc., patterns like <code>if ($C) $S</code> and <code>{{ $$$B }}</code> bind statement and block subtrees.</li>
</ul>

{h2("recipes", "Cross-Language AST Recipes")}
{code("Terminal", "# Rust: preview replacing .unwrap() with ?\nlancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'\n\n# Rust: find variadic panic! invocations\nlancet ast 'panic!($$$ARGS)' src --lang rs\n\n# Python: catch redundant self-comparisons (structural equality on $X)\nlancet ast '$X == $X' . --lang py\n\n# Go: rewrite fmt.Errorf wrapping patterns across non-test files\nlancet ast 'fmt.Errorf($FMT, $ERR)' pkg/ --lang go --path '!*_test.go' --rewrite 'errors.Wrap($ERR, $FMT)'\n\n# TypeScript / TSX: spot console.log calls\nlancet ast 'console.log($$$ARGS)' web/ --lang tsx")}

{h2("rewrites", "Safe Rewrites & Atomic --apply")}
<ol>
  <li><strong>Preview first:</strong> passing <code>--rewrite TEMPLATE</code> prints unified diffs (<code>mode: "dry-run"</code>) without modifying disk.</li>
  <li><strong>Inspect <code>skipped_overlaps</code>:</strong> when one match is nested inside another match in the same file (for example nested <code>$X.unwrap().foo.unwrap()</code>), Lancet keeps the earliest/outermost edit and skips overlapping inner edits cleanly rather than corrupting offsets; check <code>skipped_overlaps</code> and re-run once for inner leftovers.</li>
  <li><strong>Apply atomically:</strong> add <code>--apply</code> to write each file through Lancet's Tree-sitter syntax gate and atomic <code>write_atomic</code> writer.</li>
</ol>

{callout("warn", "Important constraints & ast-grep differences", "<ul><li><strong><code>--apply</code> refuses <code>--limit</code>:</strong> Lancet refuses capped partial rewrites so you don't mutate the first N matches accidentally; narrow with <code>--path</code> globs instead.</li><li><strong>Strict <code>if</code> shape:</strong> <code>if ($C) $S</code> matches plain <code>if</code> without an <code>else</code> branch (unlike <code>ast-grep</code>, which matches <code>if ... else</code> too).</li><li><strong><code>--lang cpp</code> includes <code>.h</code>:</strong> C++ AST searches inspect <code>.h</code> headers in addition to <code>.cpp</code>/<code>.cc</code>/<code>.cxx</code>/<code>.hpp</code>.</li><li><strong>Literal-free patterns:</strong> patterns without any 3+ byte literal cannot prune via <code>.lancet</code> and must parse all language files (~0.6–1.0× <code>ast-grep</code> speed); worst-case <code>$$$</code> backtracking is capped by memoization and a step budget.</li></ul>")}
"""


def _read_docs(_d: str) -> str:
    return f"""
<p><code>lancet read</code> extracts exact slices from source code, PDFs, Office suites, Korean HWP/HWPX documents, and compressed archives — returning the <strong>exact same 1-based line numbers</strong> reported by search.</p>

{h2("code-reading", "Reading Code: Lines, Symbols & Token Budgets")}
{code("Terminal", "# Inline line range (supports file:10-50, file:42, --lines 100:)\nlancet read src/search.rs:100-160\n\n# Exact Tree-sitter declaration (INCLUDES leading doc comments & attributes)\nlancet read src/search.rs --symbol execute_one --json\n\n# Cap response by estimated tokens and paginate cleanly via `next_line`\nlancet read src/mcp.rs --lines 1:800 --budget-tokens 800 --json\n\n# Omit header/gutter for raw copy/piping\nlancet read src/lib.rs --lines 1:40 --raw")}
<p>Every plain-text response includes a 16-hex <strong><code>rev</code></strong> (XXH3-64 hash of disk bytes) in the header comment and JSON field. Pass this token straight to <a href="edit.html"><code>lancet edit --rev</code></a> or <a href="rename.html"><code>lancet rename --rev</code></a>.</p>

{h2("formats", "Supported Documents, Spreadsheets & Archives")}
{table(
    ["Format", "Extensions", "Read Selectors", "Notes"],
    [
        ["PDF", "<code>.pdf</code>", "<code>--page 2</code> · <code>--page 2:5</code> · <code>--lines</code>", "In-process Pure-Rust extraction; <code>--ocr</code> optional"],
        ["Word / ODT / EPUB", "<code>.docx</code>, <code>.odt</code>, <code>.epub</code>", "<code>--lines START:END</code>", "XML paragraph stream extraction"],
        ["PowerPoint", "<code>.pptx</code>", "<code>--page 3</code> · <code>--page 1:4</code>", "Maps slides to <code>[Slide N]</code> sections"],
        ["Hangul (HWP)", "<code>.hwp</code> (5.0 OLE), <code>.hwpx</code>", "<code>--lines START:END</code>", "Native binary OLE &amp; XML Hangul extraction"],
        ["Spreadsheets", "<code>.xlsx</code>, <code>.xls</code>, <code>.xlsm</code>, <code>.xlsb</code>, <code>.ods</code>", "<code>--sheet S</code> · <code>--rows 1:30</code> · <code>--cols A:E</code>", "<code>--sheet</code> takes sheet name or 1-based index"],
        ["Archives", "<code>.zip</code>, <code>.tar</code>, <code>.gz</code>, <code>.tgz</code>, <code>.tar.gz</code>", "<code>bundle.zip!inner/file.log</code> or <code>--entry</code>", "Pair with <code>--lines</code> &amp; <code>--budget-tokens</code>"],
        ["Images (with <code>--ocr</code>)", "<code>.png</code>, <code>.jpg</code>, <code>.jpeg</code>, <code>.tif</code>, <code>.tiff</code>, <code>.bmp</code>, <code>.webp</code>", "<code>--ocr</code>", "Requires external <code>tesseract</code> on <code>PATH</code>"],
    ],
)}

{h2("rg-docs", "Searching vs Reading Non-Text Files")}
<ul>
  <li><strong><code>lancet '&lt;LQL&gt;'</code>:</strong> indexes and searches document/archive adapters automatically (filter with <code>type:document</code>, <code>type:spreadsheet</code>, or <code>type:archive</code>).</li>
  <li><strong><code>lancet rg --docs</code>:</strong> adapters are OFF by default in <code>lancet rg</code> so plain runs stay byte-identical to <code>ripgrep</code>; pass <code>--docs</code> to include extracted document and archive text in <code>lancet rg</code>.</li>
</ul>
{code("Terminal", "# Find hitting sheet/row first, then slice exact columns\nlancet '\"EBITDA\" in type:spreadsheet' ./finance\nlancet read finance/q4.xlsx --sheet 1 --rows 12:35 --cols A:F\n\n# List archive entries via outline, then stream just one log\nlancet outline artifacts/bundle.zip\nlancet read artifacts/bundle.zip!worker_0/panic.log --lines 1:80 --ocr")}

{h2("caps-safety", "Safety Caps, Panic Isolation & Read-Only Contract")}
<ul>
  <li><strong>Hard resource limits (<code>src/adapters.rs</code>):</strong> input file size capped at <strong>64 MiB</strong> (<code>MAX_INPUT_BYTES</code>), extracted UTF-8 buffer capped at <strong>16 MiB</strong> (<code>MAX_EXTRACTED_BYTES</code>) safely truncated on UTF-8 char boundaries, and archive traversal capped at <strong>500 entries</strong> (<code>MAX_ARCHIVE_ENTRIES</code>) to defeat decompression bombs.</li>
  <li><strong>Panic isolation:</strong> malformed third-party PDF/document panics are caught inside <code>catch_unwind</code> and treated cleanly as unreadable rather than crashing search/index workers.</li>
  <li><strong>Read-only by contract:</strong> binary documents and archive members omit <code>rev</code> in JSON, and <code>lancet edit</code> refuses to write back to them.</li>
  <li><strong>Scanned PDFs:</strong> pass <code>--ocr</code> (requires <code>pdftoppm</code> + <code>tesseract</code> on <code>PATH</code>) on <code>lancet</code>, <code>read</code>, <code>outline</code>, or <code>index</code>.</li>
</ul>
"""


PAGES_EN_PART1 = [
    {
        "path": "index.html",
        "group": "start",
        "nav_title": "Installation & Quickstart",
        "title": "Lancet Documentation — Installation & Cheat Sheet",
        "description": "Install Lancet on macOS, Linux, or Windows, run a 60-second walkthrough, and reference all 15 subcommands.",
        "lede": "One Rust binary for discovery, trigram-indexed search, 24-language AST rewrites, exact reads, and CAS edits.",
        "keywords": "install homebrew curl windows cargo binstall no-default-features cheat sheet walkthrough",
        "source": "https://github.com/CREE1116/lancet/blob/main/README.md",
        "render": _intro,
    },
    {
        "path": "migration.html",
        "group": "start",
        "nav_title": "Migration from rg, fd, sg",
        "title": "Coming from rg, fd, ast-grep, sed, patch & rga",
        "description": "Side-by-side Rosetta Stone, intentional differences, and when to keep git or an LSP.",
        "lede": "Keep familiar flags from ripgrep, fd, and ast-grep while eliminating line drift and silent overwrites.",
        "keywords": "migration ripgrep rg fd ast-grep sg sed patch rga pcre2 lsp differences",
        "source": f"{REPO_DOCS}/rg-parity.md",
        "render": _migration,
    },
    {
        "path": "filtering.html",
        "group": "start",
        "nav_title": "Ignore Rules, Globs & Types",
        "title": "Ignore Rules, Globs, File Types & Traversal Bounds",
        "description": "How .gitignore, .ignore, .rgignore, -u/-uu/-uuu, --glob, --type-add, type:, -L, and --max-depth work.",
        "lede": "Consistent traversal and file classification rules shared across search, fd, outline, refs, ast, and index.",
        "keywords": "gitignore rgignore hidden binary unrestricted -u -uu -uuu glob type-add max-depth max-filesize symlinks",
        "source": f"{REPO_DOCS}/rg-parity.md",
        "render": _filtering,
    },
    {
        "path": "guides/explore.html",
        "group": "search",
        "nav_title": "Files, Outlines & Ranking",
        "title": "Discovering Files, Outlines & Symbol Usage Ranking",
        "description": "Use lancet fd --dirs --tree, lancet outline, lancet refs, and lancet refs --rank to map repositories quickly.",
        "lede": "Turn an unfamiliar repository into a usage-ranked map of directories, declarations, and sheets before opening full files.",
        "keywords": "fd tree dirs changed-within meta outline refs rank kinds declarations definitions",
        "source": f"{REPO_DOCS}/agent-instructions.md",
        "render": _explore,
    },
    {
        "path": "guides/search.html",
        "group": "search",
        "nav_title": "Text Search, LQL & Batch",
        "title": "Text Search: lancet rg, LQL & Multi-Query Batching",
        "description": "Choose between lancet rg, LQL proximity/fuzzy/boolean operators, pipe stages, lancet batch, parse, and explain.",
        "lede": "Express boolean, proximity, fuzzy, and AST-scoped queries or run multiple queries over one shared walk.",
        "keywords": "lql search near before fuzzy terms budget batch parse explain selectivity",
        "source": f"{REPO_DOCS}/grammar.md",
        "render": _search,
    },
    {
        "path": "guides/ast.html",
        "group": "search",
        "nav_title": "AST Search & Rewrite",
        "title": "Structural AST Search & Safe Rewrites (24 Languages)",
        "description": "Pattern matching with $VAR, $_, $$$ARGS, C-like statements, structural equality, dry-run diffs, and overlap skipping.",
        "lede": "Match and transform syntax trees across 24 Tree-sitter languages with trigram prefiltering and dry-run diffs.",
        "keywords": "ast tree-sitter metavariables rewrite apply limit skipped_overlaps if else cpp header",
        "source": "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/query-and-ast.md",
        "render": _ast,
    },
    {
        "path": "guides/read-and-documents.html",
        "group": "search",
        "nav_title": "Reads, PDF, Office & HWP",
        "title": "Exact Reads, PDFs, Office, HWP, Archives & OCR",
        "description": "Read lines, symbols, token-budget slices, PDF/PPTX pages, spreadsheets, HWP/HWPX, ZIP/TAR entries, and scanned OCR.",
        "lede": "Shared 1-to-1 line coordinates across code and documents, with 16 MiB decompression caps and read-only safety.",
        "keywords": "read budget-tokens next_line pdf docx pptx hwp hwpx xlsx zip tar ocr 16 mib decompression",
        "source": f"{REPO_DOCS}/agent-instructions.md",
        "render": _read_docs,
    },
]
