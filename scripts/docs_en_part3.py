"""English documentation pages 15-19: Exhaustive Reference & Troubleshooting."""

from __future__ import annotations

from docs_ui import code, h2, h3, table

REPO_DOCS = "https://github.com/CREE1116/lancet/blob/main/docs"


def _cli(_d: str) -> str:
    return f"""
<p>All subcommands share deterministic exit codes, <code>.gitignore</code> traversal, and structured JSON output when <code>--json</code> is passed. For <code>lancet rg</code>'s 40+ ripgrep flags, see <a href="rg-parity.html"><code>lancet rg</code> Parity Reference</a>.</p>

{h2("exit-codes", "Exit Codes")}
{table(
    ["Code", "Meaning", "Returned JSON <code>status</code>"],
    [
        ["<code>0</code>", "Success / at least one match / edit applied or previewed cleanly", "<code>complete</code>"],
        ["<code>1</code>", "0 matches (or missing index in <code>lancet status</code>)", "<code>empty</code>"],
        ["<code>2</code>", "Bad query/CLI flags, unsupported target/rename, CAS conflict, or I/O error", "<code>invalid_query</code>, <code>unsupported</code>, <code>conflict</code>"],
    ],
    num_cols=[0],
)}

{h2("cmd-search", "lancet <QUERY> & batch & parse & explain")}
{table(
    ["Command & Flag", "Default / Values", "Meaning"],
    [
        ["<code>lancet &lt;QUERY&gt; [PATH]</code>", "<code>PATH = .</code>", "Evaluate LQL query over directory or file"],
        ["<code>--json</code> · <code>--stats</code>", "<code>false</code>", "Machine-readable JSON · execution telemetry on stderr"],
        ["<code>--no-index</code> | <code>--trust-index</code> | <code>--index-dir DIR</code>", "<code>auto</code>, <code>PATH/.lancet</code>", "Override index freshness strategy or location"],
        ["<code>--hidden</code> · <code>--no-ignore</code> · <code>-j THREADS</code> · <code>--ocr</code>", "<code>false</code>, CPUs, <code>false</code>", "Traversal &amp; OCR toggles"],
        ["<code>--max-lines N</code> · <code>--max-cols N</code>", "<code>20</code>, <code>240</code>", "Display cap per matched unit and line"],
        ["<code>lancet batch [PATH] -q Q... [--name N...] [--queries-json JSON]</code>", "—", "Share one index open + walk across multiple LQL queries"],
        ["<code>lancet parse &lt;QUERY&gt;</code>", "—", "Print normalized canonical query + JSON IR"],
        ["<code>lancet explain &lt;QUERY&gt; [PATH]</code>", "—", "Show required trigrams, candidate count &amp; selectivity %"],
    ],
)}

{h2("cmd-discover", "lancet fd & outline & refs & read")}
{table(
    ["Subcommand", "Flags & Arguments"],
    [
        ["<code>lancet fd [PATTERN] [PATH]</code>", "<code>-p/--pattern</code>, <code>-t f|d</code>, <code>-e/--ext</code> (comma/repeatable), <code>--type-class</code>, <code>-g</code> (glob), <code>-r</code> (regex), <code>-s</code> (case), <code>-d/--max-depth</code>, <code>--changed-within DUR</code>, <code>--min-size</code>, <code>--max-size</code>, <code>--tree</code>, <code>--dirs</code>, <code>--limit N</code>, <code>--meta</code>, <code>--no-index</code>, <code>--trust-index</code>, <code>--index-dir</code>, <code>--hidden</code>, <code>--no-ignore</code>, <code>-j</code>, <code>--json</code>, <code>--stats</code>"],
        ["<code>lancet outline [PATH]</code>", "<code>-d/--max-depth</code>, <code>-e/--ext</code>, <code>--limit N</code>, <code>--hidden</code>, <code>--no-ignore</code>, <code>-j</code>, <code>--ocr</code>, <code>--json</code>"],
        ["<code>lancet refs &lt;SYMBOL&gt; [PATH]</code>", "<code>--rank</code>, <code>--kind KIND,...</code> (requires <code>--rank</code>), <code>-l/--lang</code>, <code>-e/--ext</code>, <code>--path GLOB</code>, <code>--all-files</code>, <code>--limit N</code>, <code>--no-index</code>, <code>--trust-index</code>, <code>--index-dir</code>, <code>--hidden</code>, <code>--no-ignore</code>, <code>-j</code>, <code>--json</code>, <code>--stats</code>"],
        ["<code>lancet read &lt;TARGET&gt;</code>", "Inline <code>file:10-50</code> / <code>zip!entry</code>, <code>-l/--lines</code>, <code>-s/--symbol</code>, <code>-p/--page</code>, <code>--sheet</code>, <code>--rows</code>, <code>--cols</code>, <code>-e/--entry</code>, <code>--budget-tokens</code>, <code>--raw</code>, <code>--ocr</code>, <code>--json</code>"],
    ],
)}

{h2("cmd-mutate", "lancet ast, edit, patch, rename, index & mcp")}
{table(
    ["Subcommand", "Flags & Arguments"],
    [
        ["<code>lancet ast &lt;PATTERN&gt; [PATH]</code>", "<code>-l/--lang</code>, <code>-r/--rewrite TEMPLATE</code>, <code>--apply</code>, <code>--path GLOB</code> (repeatable), <code>--limit N</code>, <code>--no-index</code>, <code>--trust-index</code>, <code>--index-dir</code>, <code>--hidden</code>, <code>--no-ignore</code>, <code>-j</code>, <code>--json</code>, <code>--stats</code>"],
        ["<code>lancet edit &lt;PATH&gt;</code>", "Target (one of): <code>--lines</code>, <code>--symbol</code>, <code>--find [--all]</code>, or <code>--edits FILE|-</code> · Operation (one of): <code>--with</code>, <code>--with-file FILE|-</code>, <code>--delete</code>, <code>--insert-before</code>, <code>--insert-after</code> · Safety: <code>--rev REV</code>, <code>--apply</code>, <code>--allow-syntax-errors</code>, <code>--json</code>"],
        ["<code>lancet patch [DIFF|-]</code>", "<code>--root DIR</code> (<code>.</code>), <code>-p/--strip N</code> (<code>1</code>), <code>--apply</code>, <code>--allow-syntax-errors</code>, <code>--json</code>"],
        ["<code>lancet rename &lt;FILE:LINE[:COL]&gt; &lt;OLD&gt; &lt;NEW&gt;</code>", "<code>--rev REV</code>, <code>--apply</code>, <code>--json</code>"],
        ["<code>lancet index [PATH]</code> · <code>status [PATH]</code>", "<code>--rebuild</code> | <code>--compact</code>, <code>--index-dir</code>, <code>--ocr</code>, <code>--hidden</code>, <code>--no-ignore</code>, <code>-j</code>"],
        ["<code>lancet mcp</code>", "<code>--root DIR</code> (<code>.</code>), <code>--read-only</code>, <code>--trust-index</code>, <code>-j/--threads</code>"],
    ],
)}
"""


def _rg_parity(_d: str) -> str:
    return f"""
<p>Verified byte-for-byte against <code>ripgrep 15.2</code> across 68 edge-case fixtures in <code>bench/rg_parity.py</code> (Unicode case folding, CRLF, missing trailing newline, binary skips, <code>-m</code> with <code>-C</code>, <code>--vimgrep</code>, <code>-L</code>, <code>--max-depth</code>, <code>--max-filesize</code>) and Linux v6.12 full-tree queries.</p>

{h2("matching", "Patterns & Matching")}
{table(
    ["Flag", "Meaning", "Status", "Notes"],
    [
        ["<code>-e</code>, <code>--regexp &lt;PATTERN&gt;</code>", "Search pattern (repeatable)", "<span class='tag-yes'>Supported</span>", "Combined with regex alternation"],
        ["<code>-f</code>, <code>--file &lt;FILE&gt;</code>", "Read patterns per line (<code>-</code> stdin)", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-F</code>, <code>--fixed-strings</code>", "Treat patterns as literals", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-i</code> / <code>-s</code> / <code>-S</code>", "Ignore-case / sensitive / smart-case", "<span class='tag-yes'>Supported</span>", "Last wins; AST-aware <code>-S</code> (e.g. <code>\\W</code> keeps insensitive)"],
        ["<code>-w</code>, <code>--word-regexp</code>", "Surrounded by word boundaries", "<span class='tag-yes'>Supported</span>", "Matches <code>rg</code> boundary behavior (<code>-w '@x'</code> works)"],
        ["<code>-x</code>, <code>--line-regexp</code>", "Match spans whole line", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-v</code>, <code>--invert-match</code>", "Lines not matching pattern", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-U</code>, <code>--multiline-dotall</code>", "Multiline match &amp; <code>.</code> matches <code>\\n</code>", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-P</code>, <code>--pcre2</code>", "Lookarounds &amp; backreferences", "<span class='tag-no'>Unsupported</span>", "Linear-time Rust <code>regex</code> + sound HIR trigram lowering"],
    ],
)}

{h2("output", "Output & Formatting")}
{table(
    ["Flag", "Meaning", "Status", "Notes"],
    [
        ["<code>-o</code>, <code>-c</code>, <code>--count-matches</code>", "Only matches / line count / match count", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-l</code>, <code>--files-without-match</code>, <code>-q</code>", "Matching paths / non-matching / quiet", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-m &lt;N&gt;</code>, <code>-A</code>, <code>-B</code>, <code>-C</code>", "Max matching lines per file &amp; context", "<span class='tag-yes'>Supported</span>", "Trailing context respects <code>-m</code>; emits <code>--</code> separators"],
        ["<code>--passthru</code> (alias <code>--passthrough</code>)", "Print matching &amp; non-matching lines", "<span class='tag-yes'>Supported</span>", "Prints non-matching files too; last of <code>--passthru/-A/-B/-C</code> wins"],
        ["<code>-n</code> / <code>-N</code>", "Show / hide line numbers", "<span class='tag-yes'>Supported</span>", "<strong>ON by default</strong> even when piped"],
        ["<code>-H</code> / <code>-I</code>, <code>--column</code>, <code>--vimgrep</code>", "Filename visibility, 1-based column, vimgrep", "<span class='tag-yes'>Supported</span>", "Last of <code>-H/-I</code> wins"],
        ["<code>--json</code>, <code>-0</code>, <code>-r &lt;TEXT&gt;</code>", "rg JSON Lines, NUL sep, <code>$1</code>/<code>${{name}}</code> replace", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>--color &lt;WHEN&gt;</code>", "<code>auto</code>, <code>always</code>, <code>ansi</code>, <code>never</code>", "<span class='tag-yes'>Supported</span>", "<code>auto</code> colors TTYs only (<code>NO_COLOR</code>/<code>TERM=dumb</code> respected)"],
        ["<code>--sort</code>, <code>--no-heading</code>", "Sort / disable grouped headings", "No-op", "Always path-sorted; never emits headings"],
    ],
)}

{h2("traversal", "File Traversal, Filtering & Lancet Extensions")}
{table(
    ["Flag", "Meaning", "Status", "Notes"],
    [
        ["<code>-g</code>, <code>--iglob</code>, <code>-t</code>, <code>-T</code>, <code>--type-list</code>", "Glob &amp; type filters", "<span class='tag-yes'>Supported</span>", "Positive globs override hidden/ignore like <code>rg</code>"],
        ["<code>--type-add &lt;SPEC&gt;</code>", "<code>NAME:GLOB</code> or <code>NAME:include:A,B</code>", "<span class='tag-yes'>Supported</span>", "Applies to <code>-t</code>, <code>-T</code>, <code>--files</code>, <code>--type-list</code>"],
        ["<code>-a</code>, <code>-u</code>/<code>-uu</code>/<code>-uuu</code>, <code>-.</code>, <code>--no-ignore</code>", "Binary/hidden/ignore overrides", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>-L</code>, <code>--max-depth</code>, <code>--max-filesize</code>", "Follow symlinks, depth limit, size (<code>K/M/G</code>)", "<span class='tag-yes'>Supported</span>", "<code>--trust-index -L</code> falls back to live walk"],
        ["<code>--files</code>, <code>-j/--threads</code>", "Print searched paths, worker count", "<span class='tag-yes'>Supported</span>", ""],
        ["<code>--docs</code>", "Search PDF/Office/HWP/archive text in-process", "<span class='tag-ext'>Extension</span>", "Off by default for strict <code>rg</code> parity"],
        ["<code>--no-index</code>, <code>--trust-index</code>, <code>--index-dir</code>", "Control <code>.lancet</code> index mode", "<span class='tag-ext'>Extension</span>", "Default mode is fresh <code>auto</code>"],
        ["<code>--pre &lt;CMD&gt;</code>", "External per-file preprocessor", "<span class='tag-no'>Unsupported</span>", "Replaced by in-process <code>--docs</code>"],
    ],
)}
"""


def _lql_ast(_d: str) -> str:
    return f"""
<p>Complete formal specification of <strong>LQL v0.1</strong> and the <strong>24 Tree-sitter AST languages</strong> shared by <code>lancet ast</code> and <code>lancet refs</code>.</p>

{h2("bnf", "LQL v0.1 EBNF Grammar & Precedence")}
{code("EBNF (docs/grammar.md)", """query   := ["find"] expr ("in" scope)* ("|" stage)*
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
         | "budget" ["tokens:"] uint       # uint >= 1""", "en")}
<p><strong>Precedence:</strong> <code>NOT</code> &gt; <code>AND</code> (explicit or implicit) &gt; <code>OR</code>. Unknown keywords or trailing tokens return <code>status: "invalid_query"</code> with a 0-based byte <code>pos</code>.</p>

{h2("semantics", "Matchers, Scopes & Stages Semantics")}
{table(
    ["Construct", "Exact Semantics"],
    [
        ["<code>\"text\"</code> / bare token", "Case-insensitive (Unicode simple case folding) substring match."],
        ["<code>regex(\"pat\")</code>", "Rust <code>regex</code> multi-line mode (<code>^</code>/<code>$</code> per line); case-sensitive unless <code>(?i)</code>."],
        ["<code>fuzzy(\"word\", k)</code>", "Word tokens (<code>[\\p&#123;Alphabetic&#125;\\p&#123;N&#125;_]+</code>) with char Levenshtein distance <code>0 &lt;= k &lt;= 3</code> (default 1); index uses sound q-gram <code>AtLeast</code> threshold."],
        ["<code>symbol(\"name\")</code> / <code>symbol:name</code>", "Exact case-insensitive AST definition (<code>.rs</code>, <code>.py</code>, <code>.js</code>, <code>.ts</code>, <code>.tsx</code>, <code>.go</code>)."],
        ["<code>near(a, b, d)</code> / <code>before(a, b, d)</code>", "Both match inside unit with <code>|tok(b) - tok(a)| &lt;= d</code> (<code>before</code> requires <code>b</code> strictly after <code>a</code>)."],
        ["<code>in file</code> / <code>in block</code> / <code>in line</code>", "Whole file (default), maximal non-blank line run (<code>block</code>), or single line (match <code>[start, end)</code> inside unit)."],
        ["<code>in function</code>", "Tree-sitter function/method AST nodes (<code>.rs</code>, <code>.py</code>, <code>.js</code>, <code>.ts</code>, <code>.tsx</code>, <code>.go</code>); sets <code>symbol</code> on hits."],
        ["<code>type:</code> / <code>ext:</code> / <code>path:</code>", "Values OR within a filter category, AND across categories. 9 <code>type:</code> classes: <code>code</code>, <code>document</code>, <code>spreadsheet</code>, <code>config</code>, <code>log</code>, <code>data</code>, <code>archive</code>, <code>image</code>, <code>other</code>."],
        ["<code>| expand</code> / <code>sort</code> / <code>terms</code> / <code>budget</code>", "<code>expand parent|unit|context:N</code>, deterministic <code>sort count</code>/<code>path</code>, top <code>1..=100</code> non-stopword terms, estimated token cap."],
    ],
)}

{h2("langs-24", "24 Tree-sitter AST Languages (--lang)")}
{table(
    ["Language", "<code>--lang</code> Aliases", "Language", "<code>--lang</code> Aliases"],
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
"""


def _json_mcp(_d: str) -> str:
    return f"""
<p>Exact JSON response types serialized from Rust structs in <code>src/</code> and complete parameter schemas for all 11 tools in <code>src/mcp.rs</code>.</p>

{h2("status", "Status Enum & ErrorInfo")}
<ul>
  <li><code>Status</code>: <code>"complete"</code> | <code>"empty"</code> | <code>"invalid_query"</code> | <code>"unsupported"</code> | <code>"conflict"</code></li>
  <li><code>ErrorInfo</code>: <code>{{"message": string, "pos"?: number}}</code> (<code>pos</code> populated on LQL parse failures)</li>
</ul>

{h2("response-structs", "CLI --json Response Payloads")}
{code("Search / Read / Edit / Rename", """// Response (lancet '<QUERY>' --json)
{
  "status": "complete", "query": "\"spin_lock\" in file", "mode": "auto",
  "matched": 7073, "matched_is_lower_bound": false, "returned": 20, "truncated": true,
  "results": [{
    "id": "...", "path": "kernel/locking/spinlock.c", "unit": "file",
    "start_line": 12, "end_line": 44, "symbol": "raw_spin_lock",
    "lines": [{ "line": 12, "text": "...", "context": false }], "more_lines": 0
  }],
  "terms": [{ "term": "spinlock", "count": 42, "files": 19 }],
  "budget": { "limit_tokens": 1500, "used_tokens": 1120, "estimated": true },
  "stats": { "files_considered": 86218, "files_searched": 7814, "files_matched": 7073, "index_candidates": 7814, "stale_files": 0, "elapsed_ms": 141.3 }
}

// ReadResponse (rev omitted on binary documents & archives)
{ "status": "complete", "path": "src/net.rs", "rev": "e70d5a898f122a62", "section": "fn retry_limit", "total_lines": 80, "start_line": 1, "end_line": 4, "returned_lines": 4, "truncated": false, "next_line": null, "estimated_tokens": 28, "lines": [{ "num": 1, "text": "..." }], "elapsed_ms": 0.4 }

// EditResponse (mode: "dry-run" | "applied")
{ "status": "complete", "mode": "applied", "path": "src/net.rs", "rev": "e70d5a898f122a62", "rev_after": "1486bdffbc06c68f", "start_line": 2, "end_line": 2, "hunks": [[2, 2]], "applied": true, "diff": "...", "elapsed_ms": 1.1 }

// RenameResponse (tier: "local-scope")
{ "status": "complete", "mode": "applied", "path": "src/search.rs", "old": "old_n", "new": "new_n", "tier": "local-scope", "rev": "...", "rev_after": "...", "declaration": { "line": 40, "col": 9 }, "sites": [{ "line": 40, "col": 9 }], "comment_mentions": 0, "applied": true, "elapsed_ms": 2.3 }""", "en")}

{code("Fd / Outline / Refs / Rank / Ast / Patch / Explain", """// FdResponse:      { status, mode, total, returned, truncated, entries: [{ path, kind, file_type?, size?, mtime_ns? }], elapsed_ms }
// OutlineResponse: { status, files: [{ path, file_type, total_lines?, items: [{ kind, name, depth, start_line?, end_line?, signature?, detail? }] }], total_items, truncated, elapsed_ms }
// RefsResponse:    { status, mode, symbol, definitions: [RefHit], references: [RefHit], total_references, truncated, stats }
// RankResponse:    { status, mode, pattern, entries: [{ name, kind, path, line, references, definitions }], total, truncated, files_parsed, elapsed_ms }
// AstResponse:     { status, mode, lang?, pattern, matched, returned, truncated, matches: [AstMatch], diffs: [FileDiff], stats }
// PatchResponse:   { status, mode, applied, files: [{ path, action, from?, hunks, offsets, rev?, rev_after?, diff }], elapsed_ms }
// ExplainResponse: { status, canonical?, ir?, trigram_req?, index_exists, files_considered, index_candidates?, stale_files?, selectivity_pct?, grams_available?, grams_decoded?, elapsed_ms }""", "en")}

{h2("mcp-tools", "MCP 11-Tool Schemas (lancet mcp)")}
<p>Protocol revisions: <code>2025-03-26</code>, <code>2024-11-05</code>. JSON-RPC errors: <code>-32700</code> (parse error), <code>-32600</code> (invalid request), <code>-32601</code> (unknown method), <code>-32602</code> (invalid params / unknown prompt). Note: <code>status: "empty"</code> yields <code>isError: false</code>.</p>
{table(
    ["Tool (Required)", "Parameters & Types", "Read-Only / Notes"],
    [
        ["<code>lancet_fd</code>", "<code>path</code>, <code>pattern</code>, <code>pattern_mode</code> (<code>substring|glob|regex</code>), <code>case_sensitive</code>, <code>kinds</code>, <code>dirs</code>, <code>exts</code>, <code>type_classes</code>, <code>max_depth</code>, <code>changed_within</code>, <code>min_size</code>, <code>max_size</code>, <code>tree</code>, <code>limit</code>, <code>index_mode</code> (<code>auto|off|trust</code>), <code>hidden</code>, <code>no_ignore</code>, <code>meta</code>", "<code>readOnlyHint: true</code>"],
        ["<code>lancet_search</code> (<code>query</code>)", "<code>query</code>*, <code>path</code>, <code>max_lines</code> (20), <code>max_cols</code> (240), <code>index_mode</code>, <code>hidden</code>, <code>no_ignore</code>, <code>ocr</code>", "<code>readOnlyHint: true</code>"],
        ["<code>lancet_rg</code> (<code>pattern</code>)", "<code>pattern</code>*, <code>path</code>, <code>fixed_strings</code>, <code>ignore_case</code>, <code>smart_case</code>, <code>word</code>, <code>line_regexp</code>, <code>invert</code>, <code>multiline</code>, <code>multiline_dotall</code>, <code>only_matching</code>, <code>count</code>, <code>files_with_matches</code>, <code>max_count</code>, <code>before</code>, <code>after</code>, <code>context</code>, <code>globs</code>, <code>iglobs</code>, <code>types</code>, <code>types_not</code>, <code>type_add</code>, <code>max_depth</code>, <code>max_filesize</code>, <code>follow</code>, <code>docs</code>, <code>passthru</code>, <code>index_mode</code>, <code>hidden</code>, <code>no_ignore</code>, <code>max_output_bytes</code> (128 KiB..4 MiB)", "<code>readOnlyHint: true</code>; line-boundary truncation"],
        ["<code>lancet_outline</code>", "<code>path</code>, <code>max_depth</code>, <code>exts</code>, <code>limit</code>, <code>hidden</code>, <code>no_ignore</code>, <code>ocr</code>", "<code>readOnlyHint: true</code>"],
        ["<code>lancet_read</code> (<code>target</code>)", "<code>target</code>*, <code>lines</code>, <code>symbol</code>, <code>page</code>, <code>sheet</code>, <code>rows</code>, <code>cols</code>, <code>entry</code>, <code>budget_tokens</code>, <code>ocr</code>", "<code>readOnlyHint: true</code>; returns <code>rev</code> on plain text"],
        ["<code>lancet_refs</code> (<code>symbol</code>)", "<code>symbol</code>*, <code>path</code>, <code>lang</code>, <code>exts</code>, <code>paths</code>, <code>all_files</code>, <code>rank</code>, <code>kinds</code>, <code>limit</code>, <code>index_mode</code>, <code>hidden</code>, <code>no_ignore</code>", "<code>readOnlyHint: true</code>"],
        ["<code>lancet_ast</code> (<code>pattern</code>)", "<code>pattern</code>*, <code>path</code>, <code>lang</code>, <code>rewrite</code>, <code>apply</code> (<code>false</code>), <code>paths</code>, <code>limit</code>, <code>index_mode</code>, <code>hidden</code>, <code>no_ignore</code>", "<code>apply: true</code> refused under <code>--read-only</code>"],
        ["<code>lancet_edit</code> (<code>path</code>)", "<code>path</code>*, <code>lines</code>, <code>symbol</code>, <code>find</code>, <code>all</code>, <code>with</code>, <code>delete</code>, <code>insert_before</code>, <code>insert_after</code>, <code>edits</code> (array), <code>rev</code>, <code>apply</code> (<code>false</code>), <code>allow_syntax_errors</code>", "Omitted under <code>--read-only</code>"],
        ["<code>lancet_patch</code> (<code>patch</code>)", "<code>patch</code>*, <code>root</code>, <code>strip</code> (<code>1</code>), <code>apply</code> (<code>false</code>), <code>allow_syntax_errors</code>", "Omitted under <code>--read-only</code>"],
        ["<code>lancet_rename</code> (<code>path,line,old,new</code>)", "<code>path</code>*, <code>line</code>*, <code>col</code>, <code>old</code>*, <code>new</code>*, <code>rev</code>, <code>apply</code> (<code>false</code>)", "Omitted under <code>--read-only</code>"],
        ["<code>lancet_index</code>", "<code>action</code> (<code>status|sync|rebuild|compact</code>), <code>path</code>, <code>hidden</code>, <code>no_ignore</code>, <code>ocr</code>", "<code>sync</code>/<code>rebuild</code>/<code>compact</code> refused under <code>--read-only</code>"],
    ],
)}
"""


def _troubleshooting(_d: str) -> str:
    return f"""
<p>Every Lancet refusal returns exit code <code>2</code> with a specific diagnostic instead of silently misbehaving. Look up your exact symptom below.</p>

{h2("edit-patch-errors", "Editing, Patching & Rename Refusals")}
{table(
    ["Symptom / Error", "Root Cause", "Exact Fix"],
    [
        ["<code>status: \"conflict\"</code> on <code>edit</code> / <code>patch</code> / <code>rename</code>", "File bytes on disk changed since <code>--rev</code> was read", "Run <code>lancet read &lt;FILE&gt; --json</code> again, inspect the updated lines, recompute your edit, and pass the new <code>rev</code>."],
        ["<code>edit --find</code> refused (found 0 or &gt;1 times)", "Literal substring does not occur uniquely", "Inspect line numbers listed in the error; expand surrounding context in <code>--find</code>, or pass <code>--all</code> if replacing all occurrences intentionally."],
        ["<code>edit --symbol</code> removed doc comments or attributes", "<code>--symbol</code> spans leading <code>///</code> and <code>#[attr]</code> just like <code>read --symbol</code>", "Include the doc comments and attributes in <code>--with</code> / <code>--with-file</code>, or target only the inner body with <code>--lines</code>."],
        ["Syntax error increase refused", "Edit or patch increases Tree-sitter error count", "Check the preview diff for unclosed braces or typos; pass <code>--allow-syntax-errors</code> only if intentionally staging an intermediate edit."],
        ["<code>lancet rename</code> refused (<code>unsupported</code>)", "Target is top-level/exported, field, method, Python param, macro, JSX tag, or causes shadowing", "See <a href='../guides/rename.html#refusals'>refusal reasons</a>; fall back to <code>lancet refs</code> + <code>lancet edit --edits</code> or your LSP."],
        ["<code>lancet patch</code> refuses binary or copy diff", "Patch includes <code>Binary files</code> / <code>copy from</code>", "Filter non-binary text hunks cleanly before piping to <code>lancet patch</code>."],
    ],
)}

{h2("search-ast-errors", "Search, AST & Traversal Errors")}
{table(
    ["Symptom / Error", "Root Cause", "Exact Fix"],
    [
        ["<code>status: \"invalid_query\"</code> on LQL", "Unknown filter, unclosed quote, or missing operator", "Run <code>lancet parse '&lt;QUERY&gt;'</code> and inspect the byte offset <code>pos</code> in the error."],
        ["<code>ast --rewrite --apply</code> refuses <code>--limit</code>", "Combining <code>--limit</code> with <code>--apply</code> risks partial unreviewed mutations", "Remove <code>--limit</code> or narrow scope explicitly with <code>--path GLOB</code>."],
        ["<code>ast 'if ($C) $S'</code> misses <code>if ... else</code>", "Lancet matches strict AST shapes (<code>if</code> vs <code>if-else</code> are distinct)", "Write a second pattern including the <code>else</code> branch if both forms are intended."],
        ["<code>lancet rg -P</code> fails", "PCRE2 lookarounds &amp; backreferences are unsupported", "Rewrite using Rust <code>regex</code> without lookarounds, or LQL <code>before()</code> / <code>NOT</code>."],
        ["<code>lancet fd</code> errors with 2 paths", "<code>lancet fd [PATTERN] [PATH]</code> accepts at most one <code>PATH</code>", "Invoke once per directory, or pass <code>-p PATTERN</code> if the pattern resembles a directory name."],
    ],
)}

{h2("index-docs-mcp", "Index, Documents & MCP Issues")}
{table(
    ["Symptom / Message", "Root Cause", "Exact Fix"],
    [
        ["Query after <code>lancet index</code> ignores recent edit under <code>--trust-index</code>", "<code>--trust-index</code> skips metadata stat checks by design", "Remove <code>--trust-index</code> to use fresh <code>auto</code> mode, or run <code>lancet index .</code> to sync."],
        ["Windows misses same-size edit with preserved <code>mtime</code>", "Rust stdlib on Windows exposes only <code>mtime</code> &amp; <code>size</code>", "Run <code>lancet index .</code> or query with <code>--no-index</code> after tools that restore timestamps on Windows."],
        ["<code>lancet rg</code> misses text inside PDF / <code>.docx</code>", "<code>lancet rg</code> disables adapters by default for <code>rg</code> parity", "Pass <code>lancet rg --docs</code> (or use default LQL search <code>lancet 'query'</code>)."],
        ["Scanned PDF / image yields empty text", "No embedded text layer in image/PDF", "Pass <code>--ocr</code> and verify <code>pdftoppm</code> + <code>tesseract</code> are installed on <code>PATH</code>."],
        ["<code>lancet_edit</code> / <code>patch</code> / <code>rename</code> missing in MCP", "Server started with <code>--read-only</code>", "Restart <code>lancet mcp</code> without <code>--read-only</code> when edits are intended."],
    ],
)}
"""


PAGES_EN_PART3 = [
    {
        "path": "reference/cli.html",
        "group": "reference",
        "nav_title": "CLI Commands & Exit Codes",
        "title": "CLI Subcommands, Flags & Exit Codes Reference",
        "description": "Every Lancet subcommand, flag default, argument rule, and exit code verified against src/main.rs.",
        "lede": "Authoritative per-subcommand syntax and exit codes across all 15 entrypoints.",
        "keywords": "cli subcommands flags exit codes 0 1 2 conflict invalid_query unsupported",
        "source": "https://github.com/CREE1116/lancet/blob/main/src/main.rs",
        "render": _cli,
    },
    {
        "path": "reference/rg-parity.html",
        "group": "reference",
        "nav_title": "lancet rg Parity Matrix",
        "title": "lancet rg — Full ripgrep Parity Matrix",
        "description": "Exhaustive compatibility table covering matching, formatting, traversal, --passthru, --type-add, and Lancet extensions.",
        "lede": "Complete flag-by-flag comparison verified byte-for-byte against ripgrep 15.2.",
        "keywords": "rg ripgrep parity passthru type-add pcre2 pre docs max-depth max-filesize color",
        "source": f"{REPO_DOCS}/rg-parity.md",
        "render": _rg_parity,
    },
    {
        "path": "reference/lql-and-ast.html",
        "group": "reference",
        "nav_title": "LQL & 24 AST Languages",
        "title": "LQL v0.1 Formal Grammar & 24-Language AST Spec",
        "description": "EBNF grammar, precedence, matcher/scope/pipe semantics, 9 type classes, and all 24 --lang aliases.",
        "lede": "Formal EBNF grammar, exact query semantics, and the complete 24-language Tree-sitter alias table.",
        "keywords": "lql bnf grammar precedence near before fuzzy regex symbol expand terms budget lang aliases",
        "source": f"{REPO_DOCS}/grammar.md",
        "render": _lql_ast,
    },
    {
        "path": "reference/json-and-mcp.html",
        "group": "reference",
        "nav_title": "JSON Schemas & MCP Tools",
        "title": "JSON Response Schemas & MCP 11-Tool Reference",
        "description": "Exact JSON response structs and all 11 lancet mcp inputSchema parameters, defaults, and JSON-RPC codes.",
        "lede": "Machine-readable response fields verified against Rust structs and every MCP tool parameter schema.",
        "keywords": "json schema response mcp inputschema lancet_fd lancet_rg lancet_edit max_output_bytes json-rpc",
        "source": f"{REPO_DOCS}/mcp.md",
        "render": _json_mcp,
    },
    {
        "path": "reference/troubleshooting.html",
        "group": "reference",
        "nav_title": "Troubleshooting & Recovery",
        "title": "Troubleshooting & Refusal Recovery Matrix",
        "description": "Symptom -> Root Cause -> Exact Fix lookup for CAS conflicts, --find, syntax gates, rename, patch, and OCR.",
        "lede": "Concrete diagnostic and recovery procedures for every error and intentional refusal.",
        "keywords": "troubleshooting conflict find syntax errors rename unsupported patch pcre2 fd windows ocr",
        "source": "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md",
        "render": _troubleshooting,
    },
]
