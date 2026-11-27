"""English documentation pages 8-14: Editing, Agents & Internals/Benchmarks."""

from __future__ import annotations

from docs_ui import callout, code, diagram, h2, h3, table

REPO_DOCS = "https://github.com/CREE1116/lancet/blob/main/docs"


def _edit(d: str) -> str:
    return f"""
<p><code>lancet edit</code> mutates files using the exact line numbers and <code>rev</code> hashes produced by <code>lancet read</code>. Every call prints a dry-run unified diff unless <code>--apply</code> is specified.</p>

{diagram(f"{d}/coordinates.svg", "Search hits, read slices, symbols, and edits share one coordinate system.", "<code>read</code> returns 1-based line numbers and XXH3-64 <code>rev</code> consumed directly by <code>edit</code> and <code>rename</code>.")}

{h2("single-hunk", "Targeting One Hunk (--lines, --symbol, --find)")}
<p>Choose <strong>exactly one target</strong> and <strong>exactly one operation</strong>:</p>
<ul>
  <li><strong>Targets:</strong> <code>--lines START:END|LINE|START:</code> · <code>--symbol NAME</code> · <code>--find TEXT [--all]</code></li>
  <li><strong>Operations:</strong> <code>--with TEXT</code> · <code>--with-file PATH|-</code> · <code>--delete</code> · <code>--insert-before TEXT</code> · <code>--insert-after TEXT</code></li>
</ul>
{code("Terminal", "# 1. Replace exact line span\nlancet edit src/net.rs --lines 12:15 --with '    timeout_ms: 5000,' \\\n  --rev e70d5a898f122a62 --apply\n\n# 2. Replace declaration by AST symbol (includes doc comments & attributes!)\nlancet edit src/net.rs --symbol retry_limit --with-file new_fn.rs --rev <REV> --apply\n\n# 3. Unique exact text replacement (matches both LF and CRLF line endings)\nlancet edit src/net.rs --find 'let retries = 3;' --with 'let retries = 5;' --rev <REV> --apply\n\n# 4. Replace every match explicitly (--find refuses non-unique unless --all)\nlancet edit src/net.rs --find 'old_fn(' --with 'new_fn(' --all --rev <REV> --apply", "en")}

{callout("warn", "Two common traps to avoid", "<ul><li><strong><code>--symbol</code> boundaries:</strong> matches <code>read --symbol</code> literally — leading <code>///</code> doc comments and <code>#[attr]</code> attributes are part of the span and will be removed if omitted from your replacement.</li><li><strong><code>--find</code> uniqueness:</strong> without <code>--all</code>, matching 0 or &gt;1 times refuses with <code>invalid_query</code> and prints the offending line numbers so you can add surrounding context cleanly.</li></ul>")}

{h2("multi-hunk", "Atomic Multi-Hunk Editing (--edits)")}
<p>Never make two sequential line-numbered edits to one file from a single <code>read</code> — the first edit shifts lines for the second. Instead pass one JSON array via <code>--edits FILE</code> or <code>--edits -</code> (stdin). Lancet resolves every hunk in <strong>original file coordinates</strong>, checks for overlaps, and commits all-or-nothing:</p>
{code("Terminal", "lancet edit src/net.rs --rev e70d5a898f122a62 --apply --edits - <<'JSON'\n[\n  {\"lines\": \"10:12\", \"with\": \"const RETRIES: u32 = 5;\"},\n  {\"symbol\": \"legacy_helper\", \"delete\": true},\n  {\"find\": \"old_metric(\", \"with\": \"new_metric(\", \"all\": true},\n  {\"lines\": \"40\", \"insert_after\": \"    tracing::info!(\\\"updated\\\");\"}\n]\nJSON", "en")}
<p>Each object must provide exactly one target key (<code>lines</code>, <code>symbol</code>, or <code>find</code>; optional <code>all</code> with <code>find</code>) and exactly one operation key (<code>with</code>, <code>delete</code>, <code>insert_before</code>, or <code>insert_after</code>).</p>

{h2("cas-loop", "Multi-Agent Compare-and-Swap & Syntax Gate")}
{diagram(f"{d}/concurrent-edit.svg", "4 concurrent agents: plain writes lose 600/800 updates; lancet --rev loses 0/800.", "Under an exclusive OS file lock, stale revisions exit 2 with <code>status: \"conflict\"</code> and 0 bytes written.")}
<ol>
  <li>Capture <code>rev</code> from <code>lancet read --json</code> (or chain <code>rev_after</code> from the previous <code>lancet edit --json</code>).</li>
  <li>Run <code>lancet edit ... --rev &lt;REV&gt; --apply</code>. If another process wrote in between, Lancet fails with <code>status: "conflict"</code> (exit <code>2</code>).</li>
  <li>On <code>conflict</code>, re-run <code>lancet read</code>, adapt your edit to the updated text, and retry with the new <code>rev</code>.</li>
</ol>
<p><strong>Tree-sitter syntax gate:</strong> Lancet parses supported languages before and after editing and refuses any <em>net increase</em> in syntax errors (so an already broken file can still be fixed step by step). Pass <code>--allow-syntax-errors</code> only when staging partial edits intentionally.</p>
"""


def _patch(_d: str) -> str:
    return f"""
<p><code>lancet patch</code> brings compare-and-swap safety, pre-verification, rollback, and syntax checks to standard <code>git diff</code> and <code>diff -u</code> unified patches across multiple files.</p>

{h2("usage", "Flags & Basic Usage")}
{code("Terminal", "# Dry-run verify and preview resulting diffs (default)\nlancet patch changes.diff\ngit diff | lancet patch\n\n# Apply atomically from subdir with custom prefix strip (-p 1 strips a/ and b/)\ngit diff | lancet patch --root services/auth -p 1 --apply\n\n# Plain diff -u without a/ or b/ prefixes\ndiff -u old.rs new.rs | lancet patch -p 0 --apply", "en")}

{h2("exact-reloc", "Exact Hunk Relocation vs Fuzzy Patching")}
<p>Traditional <code>patch -p1</code> allows fuzzy context mismatches and applies hunks partially per file. In <code>src/patch.rs</code>:</p>
<ul>
  <li><strong>Exact context &amp; removals required:</strong> every hunk's context and <code>-</code> lines must match verbatim.</li>
  <li><strong>Unique nearest-offset relocation:</strong> if line numbers shifted cleanly, Lancet searches for the nearest offset where all old lines match <em>uniquely</em> and records the shift in JSON <code>offsets</code> (<code>0</code> = exact header match). Ambiguous or non-matching hunks are refused — never fuzzy.</li>
  <li><strong>EOF newline markers:</strong> honors <code>\\ No newline at end of file</code> accurately on both old and new sides.</li>
</ul>

{h2("git-headers", "Supported & Refused Diff Constructs")}
{table(
    ["Unified Diff Feature", "Status", "Behavior in <code>lancet patch</code>"],
    [
        ["Normal edits, multi-hunk files", "<span class='tag-yes'>Supported</span>", "Syntax-checked &amp; written via CAS"],
        ["<code>git diff -M</code> renames (with or without edits)", "<span class='tag-yes'>Supported</span>", "Handles <code>rename from</code> / <code>rename to</code> cleanly"],
        ["Mode changes (<code>new mode</code>, <code>new file mode</code>)", "<span class='tag-yes'>Supported</span>", "Applies Unix permission bits (<code>chmod</code>)"],
        ["Created / deleted empty files", "<span class='tag-yes'>Supported</span>", "Detects <code>/dev/null</code> and <code>deleted file mode</code>"],
        ["<code>copy from</code> / <code>copy to</code>", "<span class='tag-no'>Refused</span>", "Aborts entire patch before writing any file"],
        ["<code>Binary files ...</code> / <code>GIT binary patch</code>", "<span class='tag-no'>Refused</span>", "Binary patches are refused without touching disk"],
    ],
)}

{h2("atomicity", "Pre-Verification & Race Rollback")}
<p>Before mutating any file, Lancet reads, verifies, relocates, and Tree-sitter-checks <strong>every file in the patch</strong>. Each modification writes via locked <code>write_if_rev</code> against the pre-checked <code>rev</code>. If another writer wins a race mid-patch, previously written files in the same patch are restored via best-effort rollback and exit code <code>2</code> (<code>conflict</code>) is returned.</p>
"""


def _rename(_d: str) -> str:
    return f"""
<p><code>lancet rename</code> performs <strong>tier-1 local-scope symbol renaming</strong> within a single file. Instead of heuristics or plain text substitution, it resolves lexical scopes per language and re-resolves the whole file after substitution to prove zero shadowing or variable capture.</p>

{h2("usage", "Target Syntax & Supported Languages (6)")}
<p>Supported languages: <strong>Rust</strong> (<code>.rs</code>), <strong>Python</strong> (<code>.py</code>), <strong>JavaScript</strong> (<code>.js</code>), <strong>TypeScript</strong> (<code>.ts</code>), <strong>TSX</strong> (<code>.tsx</code>), and <strong>Go</strong> (<code>.go</code>).</p>
{code("Terminal", "# Dry-run diff (COL is optional when OLD only resolves to one binding on LINE)\nlancet rename src/search.rs:42 old_name new_name\n\n# Disambiguate column (1-based byte offset, matching `lancet rg --column`) + CAS rev\nlancet rename src/search.rs:42:9 old_name new_name --rev <REV> --apply", "en")}
<p>You can point <code>FILE:LINE[:COL]</code> at either the <strong>declaration</strong> or <strong>any resolved use</strong> inside the binding's scope. Comments inside the scope mentioning <code>old</code> are left intact and reported in <code>comment_mentions</code>.</p>

{h2("how-proof-works", "Two-Pass Scope & Shadowing Verification")}
<ol>
  <li><strong>Pass 1 (Binding discovery):</strong> builds the scope tree (hoisting, block/function scopes, closure parameters, loop binders) to find every AST identifier bound to that exact local declaration.</li>
  <li><strong>Pass 2 (Post-rename re-resolution):</strong> simulates replacing every occurrence with <code>new</code> and runs scope resolution again on the new source. If any use now resolves to a different declaration (shadowing) or captures a surrounding binding, Lancet refuses with <code>unsupported</code> and writes nothing.</li>
</ol>

{h2("refusals", "What lancet rename Intentionally Refuses (and Why)")}
{table(
    ["Refused Construct", "Why Guessing Would Break Code"],
    [
        ["Module / top-level / exported symbols", "Can be imported across files; requires project-wide compiler/LSP"],
        ["Struct fields &amp; methods", "Accessed outside the local scope via receivers across files"],
        ["<strong>Python function parameters</strong>", "Callers in other files may pass them as keyword arguments (<code>fn(old=1)</code>)"],
        ["Macro bodies &amp; JSX tag names", "Opaque token expansion or component resolution outside normal variable bindings"],
        ["Go composite-literal keys (<code>T{{old: v}}</code>)", "Field labels, not local variable reads"],
        ["Dynamic Python/JS constructs (<code>eval</code>, <code>with</code>, <code>locals()</code>, <code>global</code>, <code>nonlocal</code>)", "Dynamic lookup makes static scope proof unsound"],
        ["Shadowing / variable capture", "<code>new</code> collides with an existing binding in scope"],
    ],
)}

{callout("tip", "Fallback for cross-file or refused symbols", "<p>Inspect every definition and usage site with <code>lancet refs &lt;SYMBOL&gt; src --json</code>, then apply deterministic edits per file using <code>lancet edit --edits</code> or your language's LSP.</p>")}
"""


def _agents_mcp(_d: str) -> str:
    return f"""
<p>AI coding agents can drive Lancet via shell CLI or its zero-dependency stdio <strong>Model Context Protocol (MCP)</strong> server — paired with the cross-agent <strong>Official Skill</strong> package (<code>skills/lancet/</code>).</p>

{h2("mcp-flags", "Starting lancet mcp")}
<p><code>lancet mcp</code> speaks newline-delimited JSON-RPC 2.0 over <code>stdin</code>/<code>stdout</code> supporting protocol revisions <code>2025-03-26</code> and <code>2024-11-05</code>, invoking Lancet's Rust engines directly in-process.</p>
{table(
    ["Flag", "Default", "Exact Behavior"],
    [
        ["<code>--root &lt;DIR&gt;</code>", "<code>.</code>", "Base directory for relative paths in tool arguments (<strong>not</strong> a chroot/sandbox; absolute or <code>..</code> paths still resolve like the CLI)"],
        ["<code>--read-only</code>", "<code>false</code>", "Removes <code>lancet_edit</code>, <code>lancet_patch</code>, and <code>lancet_rename</code> from <code>tools/list</code> and refuses writes (<code>lancet_ast</code> <code>apply</code>, mutating <code>lancet_index</code> actions)"],
        ["<code>--trust-index</code>", "<code>false</code>", "Sets default <code>index_mode</code> to <code>trust</code> instead of <code>auto</code> unless overridden per tool call"],
        ["<code>-j, --threads &lt;NUM&gt;</code>", "CPU count", "Rayon worker threads"],
    ],
)}

{h2("client-configs", "Ready-to-Copy Client Configurations")}
<p>Full template bundle: <a href="https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/mcp-config.json"><code>skills/lancet/references/mcp-config.json</code></a>.</p>
{h3("claude-code", "Claude Code")}
{code("Terminal", "# Read + CAS write tools\nclaude mcp add --transport stdio lancet -- lancet mcp --root .\n\n# Safe read-only research server\nclaude mcp add --transport stdio lancet -- lancet mcp --root . --read-only", "en")}
{h3("cursor", "Cursor (.cursor/mcp.json or ~/.cursor/mcp.json)")}
{code("JSON", '{\n  "mcpServers": {\n    "lancet": {\n      "command": "lancet",\n      "args": ["mcp", "--root", "."]\n    }\n  }\n}', "en")}
{h3("desktop-agy", "Claude Desktop / Windsurf / Antigravity")}
<p>Pass an absolute <code>--root</code> path when the desktop client launches without your project as CWD:</p>
{code("JSON", '{\n  "mcpServers": {\n    "lancet": {\n      "command": "lancet",\n      "args": ["mcp", "--root", "/abs/path/to/your/repo"]\n    }\n  }\n}', "en")}
{h3("codex", "OpenAI Codex (~/.codex/config.toml)")}
{code("TOML", '[mcp_servers.lancet]\ncommand = "lancet"\nargs = ["mcp", "--root", "."]', "en")}

{h2("skill-install", "Installing the Official Agent Skill")}
<p>The <a href="https://github.com/CREE1116/lancet/tree/main/skills/lancet"><code>skills/lancet/</code></a> directory bundles <code>SKILL.md</code> plus detailed <code>references/</code> for LQL, 24 AST languages, edit/patch/rename CAS rules, and MCP configs.</p>
{code("Terminal", "# Project-local (.agents standard)\nmkdir -p .agents/skills && ln -s \"$(pwd)/skills/lancet\" .agents/skills/lancet\n\n# Personal install (Claude Code / Codex / Antigravity)\nmkdir -p ~/.claude/skills && cp -R skills/lancet ~/.claude/skills/lancet\nmkdir -p ~/.codex/skills  && cp -R skills/lancet ~/.codex/skills/lancet\nmkdir -p ~/.gemini/config/skills && cp -R skills/lancet ~/.gemini/config/skills/lancet", "en")}

{h2("prompts-notes", "Built-in MCP Prompts & AGENTS.md Rule")}
<ul>
  <li><strong><code>prompts/list</code>:</strong> ships <code>lancet-explore</code> (<code>goal</code> argument) and <code>lancet-safe-edit</code> (<code>target</code> argument).</li>
  <li><strong>Empty-match semantics:</strong> read/search tools return <code>isError: false</code> on <code>status: "empty"</code> (0 hits is normal, not an exception).</li>
  <li><strong>Line-boundary byte cap:</strong> <code>lancet_rg</code> caps stdout cleanly on a newline boundary via <code>max_output_bytes</code> (default <code>131072</code> B = 128 KiB, hard clamp <code>256..=4194304</code>).</li>
</ul>
{code("AGENTS.md / CLAUDE.md snippet", "Prefer lancet (lancet_* MCP tools or lancet CLI) instead of rg/fd/ast-grep/cat/sed.\nOrient with fd/outline, locate with rg/search/refs/ast, capture rev via read, apply changes via edit/patch/rename --rev --apply.", "en")}
"""


def _index_freshness(d: str) -> str:
    return f"""
<p>Unlike search servers that rely on an asynchronous background watcher (lagging behind rapid agent edits) or serve stale cached lines, Lancet uses a daemonless, immutable <strong>LSM trigram index (<code>LANCET04</code>)</strong> designed around one invariant: <strong><code>Scan == Auto</code></strong>.</p>

{diagram(f"{d}/pipeline.svg", "How an indexed query skips 91% of files on Linux v6.12 while staying exact.", "Measured on Linux v6.12 (86,218 files) for <code>\"spin_lock\"</code> in <code>auto</code> mode: 7,814 opened (9.1%), 7,073 matches (8.2%).")}

{h2("trigram-lowering", "Sound Trigram Lowering")}
<p>Queries are compiled into required byte-trigram boolean trees (literals, regex HIR branches, AST literals, <code>refs --rank</code> glob literals) or a sound q-gram count threshold <code>AtLeast(k)</code> for <code>fuzzy()</code>. Any file missing those trigrams cannot possibly satisfy the predicate, so skipping it produces <strong>zero false negatives</strong>. Every remaining candidate goes through the exact full-scan evaluator.</p>

{h2("segment-format", "On-Disk LANCET04 Format & Zero-Copy mmap")}
<p>Located in <code>PATH/.lancet</code> (discovered up ancestor directories automatically unless <code>--index-dir</code> is set), consisting of <code>manifest.json</code> and immutable zero-copy segments (<code>seg-NNNNNN.wseg</code>):</p>
{code("LANCET04 Binary Layout (src/index/segment.rs)", """header (40 B):  "LANCET04" | ndocs:u32 | nkeys:u32 | doc_offs_off:u64 | keys_off:u64 | post_off:u64
doc_offs:       (ndocs + 1) * u32 offsets relative to docs_off
docs:           per doc (sorted by rel UTF-8): mtime_ns:u64 | size:u64 | ctime_ns:u64 | ino:u64 | flags:u8 | rel_utf8
keys:           nkeys * (key:u32 | off:u32) + sentinel_end_off:u32 (sorted by trigram key)
postings:       varint(count) followed by delta-varint doc IDs""", "en")}
<p><code>Segment::open</code> checks only the 40-byte header — <strong>O(1) with zero heap allocations</strong> regardless of corpus size. Per-doc flags track <code>FLAG_BINARY(1)</code>, <code>FLAG_ERROR(2)</code>, <code>FLAG_OVERSIZE(4, &gt; 64 MiB — streamed via mmap at query time)</code>, and <code>FLAG_RACY(8)</code>.</p>

{h2("freshness", "4-Tuple Metadata + Racy-Timestamp Guard")}
{diagram(f"{d}/index-lifecycle.svg", "Index lifecycle: build, per-query live stat check, incremental segments, and compaction.", "Auto mode catches modified and untracked files immediately with zero daemon.")}
<p>Why is <code>(mtime, size)</code> alone broken? Tools like <code>cp -p</code>, <code>tar</code>, or same-length replacements can leave both <code>mtime</code> and <code>size</code> unchanged. On POSIX, Lancet tracks <strong><code>(mtime_ns, size, ctime_ns, ino)</code></strong> and applies Git's <strong>racy-timestamp window</strong>:</p>
<ul>
  <li><strong>Adaptive racy window:</strong> inspects up to 4,096 files for sub-second timestamps; uses <code>100 ms</code> on fine-grained filesystems (APFS, ext4, NTFS) and <code>2 s</code> on coarse filesystems (HFS+, ext3, FAT). Any file touched inside that window gets <code>FLAG_RACY</code> and is never trusted from cache without live verification.</li>
</ul>
{callout("warn", "Honest Windows Caveat", "<p>Rust's stdlib exposes only <code>mtime</code> and <code>size</code> stably on Windows (<code>ctime_ns</code> and <code>ino</code> are <code>0</code>). On Windows, edits preserving both byte length and <code>mtime</code> require <code>lancet index</code> or <code>--no-index</code>.</p>")}

{h2("compaction", "Modes, Incremental Sync & Compaction")}
<ul>
  <li><strong><code>auto</code> (default):</strong> live metadata walk (~100 ms / 87k files) + stale files scanned live. Always exact.</li>
  <li><strong><code>--trust-index</code>:</strong> skips metadata walk and answers strictly from <code>.lancet</code> (~40 ms on Linux kernel, 2–3 ms no-hit); misses unindexed edits until <code>lancet index</code> runs.</li>
  <li><strong><code>--no-index</code>:</strong> bypasses <code>.lancet</code> and scans disk directly.</li>
  <li><strong>LSM compaction:</strong> incremental <code>lancet index</code> updates append a segment and tombstones atomically via <code>manifest.json.tmp</code> rename; automatic compaction triggers above <code>COMPACT_MAX_SEGMENTS = 6</code> or <code>COMPACT_MIN_DEAD = 32</code> dead docs (or manually via <code>lancet index --compact</code> / <code>--rebuild</code>).</li>
</ul>
"""


def _write_safety(d: str) -> str:
    return f"""
<p>Why build reading, AST parsing, diffing, and writing into one binary? Because across multi-step CLI chains, failures happen <em>between</em> tools — and every Lancet write command shares the exact same path in <code>src/edit.rs</code>.</p>

{diagram(f"{d}/failure-modes.svg", "Seven real-world multi-step failure modes reproduced in bench/failure_modes.py.", "Every row was measured directly against ripgrep, BSD sed/patch, pandoc, rga, and zoekt.")}

{h2("seven-failures", "7 Failure Modes Reproduced in Production Tools")}
{table(
    ["Tool Chain", "Reproduced Failure", "How Lancet Eliminates It"],
    [
        ["<code>rg -n</code> → 2× <code>sed -i</code>", "Second edit hits shifted line; exits <code>0</code>", "<code>edit --edits</code> resolves all hunks on original lines"],
        ["4 agents: read → mutate → write", "<strong>600 / 800</strong> updates lost silently", "<code>edit --rev</code>: <strong>0 / 800</strong> lost; losers get <code>conflict</code>"],
        ["<code>patch -p1</code> with 1 stale hunk", "File A patched, File B rejected: half-applied", "<code>patch</code> pre-verifies all files + rolls back on race"],
        ["<code>sed</code> symbol rename", "Mutated <strong>0.3–11.7%</strong> comments &amp; strings", "<code>refs</code>/<code>ast</code> skip them; <code>rename</code> proves local scope"],
        ["<code>rga</code> → <code>pandoc</code> on <code>.docx</code>", "<strong>0 / 40</strong> hit lines matched in reader", "<strong>40 / 40</strong>: search &amp; <code>read</code> share one extractor"],
        ["<code>zoekt</code> after editing 20 files", "0 new, 20 deleted hits shown immediately", "<code>auto</code> mode: 20 new, 0 deleted immediately"],
        ["<code>sed -i</code> / <code>perl -pi</code>", "Broke symlinks/hard links, lost mode <code>0640</code>", "Symlinks, <code>nlink &gt; 1</code> hard links, mode &amp; owner preserved"],
    ],
)}

{h2("locks-cas", "Shared Read Locks & Exclusive write_if_rev CAS")}
<ul>
  <li><strong><code>read_shared</code>:</strong> opens target, takes OS shared lock (<code>lock_shared()</code>), verifies <code>same_file(&amp;f, path)</code> (retrying up to 16×5 ms on transient Windows sharing violations if renamed mid-open), and computes <code>rev = XXH3-64(bytes)</code> as 16 lowercase hex digits.</li>
  <li><strong><code>write_if_rev</code>:</strong> opens target, acquires an exclusive OS lock (<code>f.lock()</code>), re-reads current bytes + checks <code>inode</code>/<code>rev == expect_rev</code>, and returns <code>WriteError::Conflict {{ now }}</code> (exit <code>2</code>) with zero bytes touched on mismatch.</li>
</ul>

{h2("atomic-writer", "How write_atomic Preserves File Identity & Crash Safety")}
<p>Instead of naive temp-rename (which breaks symlinks, splits hard links, and resets ownership), <code>write_atomic</code> in <code>src/edit.rs</code> follows four rules:</p>
<ol>
  <li><strong>Follows symlinks first:</strong> calls <code>fs::canonicalize(path)</code> so the symlink itself stays intact and its target file is updated.</li>
  <li><strong>Stages &amp; fsyncs temp file:</strong> writes to unique <code>.&#123;name&#125;.lancet-tmp.&#123;pid&#125;.&#123;seq&#125;</code> and calls <code>f.sync_all()</code> first (if the disk is full, it fails before touching the original) and copies permissions + Unix <code>uid</code>/<code>gid</code>.</li>
  <li><strong>Preserves multiply hard-linked files:</strong> checks <code>nlink() &gt; 1</code>; if true, overwrites the existing inode in-place + <code>sync_all()</code> instead of renaming (because renaming would unlink and split hard-linked paths).</li>
  <li><strong>Atomic publish + dir sync:</strong> for single-link files, renames temp → target and calls <code>sync_dir(parent)</code> so readers see either old or new contents atomically, never a torn write.</li>
</ol>
"""


def _benchmarks(d: str) -> str:
    return f"""
<p>Measured on Apple M4 Pro (arm64 macOS), warm filesystem cache, median of 5 <code>hyperfine -N</code> runs. Text search outputs across <code>auto</code>, <code>--trust-index</code>, and <code>--no-index</code> were verified <strong>byte-for-byte against <code>ripgrep 15.2</code></strong>.</p>

{diagram(f"{d}/bench-index-footprint.svg", "Index build time, disk size, and peak RAM across 8 codebases: lancet vs zoekt.", "4–25× faster index builds, 9–30× smaller disk footprint, 3.2–5.0× lower peak RAM than zoekt.")}
{diagram(f"{d}/bench-workload-profile.svg", "Full workload profile across 8 text queries on LLVM 19.1 and AST search across 8 codebases.", "Covers rare/common literals, -i, -w, literal regex, literal-free regex, no-hit, -c, and Tree-sitter AST.")}

{h2("suite-table", "Core 8-Codebase Benchmark Suite (SUITE.md)")}
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

{h2("competitors", "Indexed & Agent Tool Comparison (COMPETITORS.md)")}
<p>Reproducible harness: <a href="https://github.com/CREE1116/lancet/blob/main/bench/competitors_agents.py"><code>bench/competitors_agents.py</code></a> (<a href="https://github.com/CREE1116/lancet/blob/main/bench/results/COMPETITORS.md">full raw tables</a>).</p>

{h3("comp-freshness", "Post-Edit 0 ms Freshness & Parity (20 edited .py files in Django)")}
{table(
    ["Tool / Mode", "Finds New (0 ms)", "Finds Deleted (0 ms)", "20/20 rg Match-Count Parity"],
    [
        ["<code>lancet rg</code> (<code>auto</code>)", "<strong>20 / 20</strong>", "0 / 20", "<strong>20 / 20</strong>"],
        ["<code>lancet rg --trust-index</code>", "0 / 20", "0 / 20", "<strong>20 / 20</strong>"],
        ["<code>grix</code> (auto)", "<strong>20 / 20</strong>", "0 / 20", "19 / 20 (missed 2 lines on LLVM no-lit regex)"],
        ["<code>tgrep</code> (serverless)", "0 / 20", "0 / 20", "<strong>20 / 20</strong>"],
        ["<code>tgrep serve</code> (daemon)", "<strong>20 / 20</strong>", "0 / 20", "<strong>20 / 20</strong>"],
        ["<code>aft</code> (resident stdio + watcher)", "0 / 20 (needs ~1.5 s settle)", "0 / 20", "Under-indexed (~49,031 vs 49,964 files on Rust)"],
    ],
)}

{h3("comp-honest", "Honest Summary: Wins & Losses")}
{callout("note", "Read both sides carefully", "<ul><li><strong>Where Lancet leads:</strong> 20/20 0 ms daemonless freshness; <strong>1/3–1/7× smaller disk index</strong> than <code>aft</code>/<code>tgrep</code> (141.8 MB vs 368.5 MB / 955.2 MB on Linux); <strong>~1/3× peak build RAM</strong> vs <code>grix</code> (353.5 MB vs 1,001.6 MB on Linux); fastest literal AST search across Django, Rust, Linux, and LLVM; one binary covering read + edit + patch + rename.</li><li><strong>Where daemon or trust-only tools win:</strong> <code>tgrep serve</code> (resident daemon) beats daemonless <code>lancet auto</code> on Rust (119.2 ms vs 508.7 ms) and LLVM (19.9 ms vs 295.8 ms) <code>regex with literal</code>, and small literal-free regexes (Django 15.4 ms, Rust 60.9 ms). <code>grix --no-auto-index</code> is faster than <code>lancet --trust-index</code> on Linux rare literal (11.5 ms vs 43.6 ms). Resident <code>aft</code> (587–1,260 MB RAM) is faster on Rust rare literal (8.5 ms vs 73.5 ms). Literal-free regexes and literal-free AST patterns run near <code>rg</code> (down to 1.4× slower) and <code>ast-grep</code> (0.6–1.0×).</li></ul>")}
"""


PAGES_EN_PART2 = [
    {
        "path": "guides/edit.html",
        "group": "edit",
        "nav_title": "Safe Editing & CAS",
        "title": "Line, Symbol, Find & Multi-Hunk CAS Editing",
        "description": "Use lancet edit with --lines, --symbol, --find, atomic --edits, XXH3-64 --rev CAS, and Tree-sitter syntax verification.",
        "lede": "Zero line drift across multi-hunk edits and zero silent overwrites when multiple agents edit concurrently.",
        "keywords": "edit edits find symbol lines insert-before insert-after rev rev_after conflict cas allow-syntax-errors",
        "source": f"{REPO_DOCS}/agent-instructions.md",
        "render": _edit,
    },
    {
        "path": "guides/patch.html",
        "group": "edit",
        "nav_title": "Atomic Multi-File Patch",
        "title": "Atomic Multi-File Patching (lancet patch)",
        "description": "Apply git diff and diff -u patches across files with exact hunk relocation, rename/mode support, and race rollback.",
        "lede": "Never leave a half-applied patch on disk: verify every hunk across every file before writing a byte.",
        "keywords": "patch unified diff git diff -p strip root rename mode rollback binary copy",
        "source": "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md",
        "render": _patch,
    },
    {
        "path": "guides/rename.html",
        "group": "edit",
        "nav_title": "Local Scope Rename",
        "title": "Scope-Proven Local Rename (lancet rename)",
        "description": "Rename local bindings across Rust, Python, JS, TS, TSX, and Go with two-pass scope and shadowing verification.",
        "lede": "Refuses ambiguous or cross-file bindings rather than guessing, and proves zero shadowing or capture before writing.",
        "keywords": "rename local scope python parameter shadowing capture comment_mentions refs fallback",
        "source": "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md",
        "render": _rename,
    },
    {
        "path": "guides/agents-mcp.html",
        "group": "edit",
        "nav_title": "AI Agents, MCP & Skill",
        "title": "AI Agents, Built-in MCP Server & Official Skill",
        "description": "Configure lancet mcp across Claude Code, Cursor, Codex, Antigravity, and Claude Desktop, plus install skills/lancet.",
        "lede": "11 tools over newline-delimited stdio JSON-RPC with --read-only, --root, and the cross-agent Official Skill.",
        "keywords": "mcp server root read-only claude cursor codex antigravity skill agents.md max_output_bytes prompts",
        "source": f"{REPO_DOCS}/mcp.md",
        "render": _agents_mcp,
    },
    {
        "path": "concepts/index-and-freshness.html",
        "group": "internals",
        "nav_title": "Index (LANCET04) & Freshness",
        "title": "The LANCET04 Trigram LSM Index & Freshness Model",
        "description": "LANCET04 binary layout, zero-copy mmap, sound trigram lowering, (mtime, size, ctime, inode), racy windows, and compaction.",
        "lede": "How Lancet achieves Scan == Auto without a background daemon — down to the 40-byte header and racy-timestamp guard.",
        "keywords": "lancet04 segment mmap trigram q-gram mtime ctime inode racy window flag_racy compact windows",
        "source": f"{REPO_DOCS}/why-one-tool.md",
        "render": _index_freshness,
    },
    {
        "path": "concepts/write-safety.html",
        "group": "internals",
        "nav_title": "Write Safety & File Identity",
        "title": "Coordinates, OS File Locking & File Identity Preservation",
        "description": "7 failure modes fixed, read_shared, write_if_rev exclusive locks, write_atomic temp/fsync, symlinks, and nlink > 1 hard links.",
        "lede": "How src/edit.rs guarantees symlinks, hard links, permissions, ownership, and crash-safe atomic writes.",
        "keywords": "failure modes write_atomic write_if_rev read_shared lock_shared hard link nlink symlink uid gid sync_all",
        "source": f"{REPO_DOCS}/why-one-tool.md",
        "render": _write_safety,
    },
    {
        "path": "concepts/benchmarks.html",
        "group": "internals",
        "nav_title": "Benchmarks & Competitors",
        "title": "Benchmarks & Honest Competitor Comparison",
        "description": "8-codebase SUITE.md numbers vs ripgrep/zoekt/ast-grep plus COMPETITORS.md results vs tgrep, grix, aft, tensor-grep, probe.",
        "lede": "Every win and every loss measured on real repositories against ripgrep, zoekt, ast-grep, tgrep, grix, aft, and probe.",
        "keywords": "benchmarks suite competitors zoekt tgrep grix aft tensor-grep probe linux llvm rust freshness",
        "source": "https://github.com/CREE1116/lancet/blob/main/bench/results/COMPETITORS.md",
        "render": _benchmarks,
    },
]
