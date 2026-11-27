"""Korean documentation pages 15-19: 전체 레퍼런스 & 문제 해결."""

from __future__ import annotations

from docs_ui import code, h2, table

REPO_DOCS = "https://github.com/CREE1116/lancet/blob/main/docs"


def _cli(_d: str) -> str:
    return f"""
<p>모든 하위 명령은 결정적 종료 코드, <code>.gitignore</code> 순회, <code>--json</code> 구조화 출력을 공유합니다. <code>lancet rg</code>의 40여 개 플래그는 <a href="rg-parity.html"><code>lancet rg</code> 호환성 표</a>를 참고하세요.</p>

{h2("exit-codes", "종료 코드")}
{table(
    ["코드", "의미", "JSON <code>status</code> 값"],
    [
        ["<code>0</code>", "성공 / 1건 이상 매치 / 편집·패치 검증 및 적용 완료", "<code>complete</code>"],
        ["<code>1</code>", "0건 매치 (또는 <code>lancet status</code>에서 인덱스 없음)", "<code>empty</code>"],
        ["<code>2</code>", "잘못된 쿼리·플래그, 미지원 대상/이름 변경, CAS 충돌, I/O 오류", "<code>invalid_query</code>, <code>unsupported</code>, <code>conflict</code>"],
    ],
    num_cols=[0],
)}

{h2("cmd-search", "lancet <QUERY> · batch · parse · explain")}
{table(
    ["명령 / 플래그", "기본값 / 값", "설명"],
    [
        ["<code>lancet &lt;QUERY&gt; [PATH]</code>", "<code>PATH = .</code>", "디렉터리 또는 파일에서 LQL 쿼리 평가"],
        ["<code>--json</code> · <code>--stats</code>", "<code>false</code>", "기계 판독용 JSON · stderr 실행 통계"],
        ["<code>--no-index</code> | <code>--trust-index</code> | <code>--index-dir DIR</code>", "<code>auto</code>, <code>PATH/.lancet</code>", "인덱스 최신성 전략 또는 위치 지정"],
        ["<code>--hidden</code> · <code>--no-ignore</code> · <code>-j THREADS</code> · <code>--ocr</code>", "<code>false</code>, 코어 수, <code>false</code>", "숨김/무시 파일 및 OCR 토글"],
        ["<code>--max-lines N</code> · <code>--max-cols N</code>", "<code>20</code>, <code>240</code>", "결과 단위당 표시 줄/열 상한"],
        ["<code>lancet batch [PATH] -q Q... [--name N...] [--queries-json JSON]</code>", "—", "인덱스 열기·워크 1회 공유로 다수 LQL 실행"],
        ["<code>lancet parse &lt;QUERY&gt;</code>", "—", "정규화된 쿼리 및 JSON IR 출력"],
        ["<code>lancet explain &lt;QUERY&gt; [PATH]</code>", "—", "필수 트라이그램, 후보 수, 선택도 % 출력"],
    ],
)}

{h2("cmd-discover", "lancet fd · outline · refs · read")}
{table(
    ["하위 명령", "플래그 및 인자"],
    [
        ["<code>lancet fd [PATTERN] [PATH]</code>", "<code>-p/--pattern</code>, <code>-t f|d</code>, <code>-e/--ext</code> (쉼표/반복), <code>--type-class</code>, <code>-g</code> (글롭), <code>-r</code> (정규식), <code>-s</code> (대소문자 구분), <code>-d/--max-depth</code>, <code>--changed-within DUR</code>, <code>--min-size</code>, <code>--max-size</code>, <code>--tree</code>, <code>--dirs</code>, <code>--limit N</code>, <code>--meta</code>, <code>--no-index</code>, <code>--trust-index</code>, <code>--index-dir</code>, <code>--hidden</code>, <code>--no-ignore</code>, <code>-j</code>, <code>--json</code>, <code>--stats</code>"],
        ["<code>lancet outline [PATH]</code>", "<code>-d/--max-depth</code>, <code>-e/--ext</code>, <code>--limit N</code>, <code>--hidden</code>, <code>--no-ignore</code>, <code>-j</code>, <code>--ocr</code>, <code>--json</code>"],
        ["<code>lancet refs &lt;SYMBOL&gt; [PATH]</code>", "<code>--rank</code>, <code>--kind KIND,...</code> (<code>--rank</code> 필요), <code>-l/--lang</code>, <code>-e/--ext</code>, <code>--path GLOB</code>, <code>--all-files</code>, <code>--limit N</code>, <code>--no-index</code>, <code>--trust-index</code>, <code>--index-dir</code>, <code>--hidden</code>, <code>--no-ignore</code>, <code>-j</code>, <code>--json</code>, <code>--stats</code>"],
        ["<code>lancet read &lt;TARGET&gt;</code>", "인라인 <code>file:10-50</code> / <code>zip!entry</code>, <code>-l/--lines</code>, <code>-s/--symbol</code>, <code>-p/--page</code>, <code>--sheet</code>, <code>--rows</code>, <code>--cols</code>, <code>-e/--entry</code>, <code>--budget-tokens</code>, <code>--raw</code>, <code>--ocr</code>, <code>--json</code>"],
    ],
)}

{h2("cmd-mutate", "lancet ast · edit · patch · rename · index · mcp")}
{table(
    ["하위 명령", "플래그 및 인자"],
    [
        ["<code>lancet ast &lt;PATTERN&gt; [PATH]</code>", "<code>-l/--lang</code>, <code>-r/--rewrite TEMPLATE</code>, <code>--apply</code>, <code>--path GLOB</code> (반복), <code>--limit N</code>, <code>--no-index</code>, <code>--trust-index</code>, <code>--index-dir</code>, <code>--hidden</code>, <code>--no-ignore</code>, <code>-j</code>, <code>--json</code>, <code>--stats</code>"],
        ["<code>lancet edit &lt;PATH&gt;</code>", "대상 (택1): <code>--lines</code>, <code>--symbol</code>, <code>--find [--all]</code>, 또는 <code>--edits FILE|-</code> · 연산 (택1): <code>--with</code>, <code>--with-file FILE|-</code>, <code>--delete</code>, <code>--insert-before</code>, <code>--insert-after</code> · 안전: <code>--rev REV</code>, <code>--apply</code>, <code>--allow-syntax-errors</code>, <code>--json</code>"],
        ["<code>lancet patch [DIFF|-]</code>", "<code>--root DIR</code> (<code>.</code>), <code>-p/--strip N</code> (<code>1</code>), <code>--apply</code>, <code>--allow-syntax-errors</code>, <code>--json</code>"],
        ["<code>lancet rename &lt;FILE:LINE[:COL]&gt; &lt;OLD&gt; &lt;NEW&gt;</code>", "<code>--rev REV</code>, <code>--apply</code>, <code>--json</code>"],
        ["<code>lancet index [PATH]</code> · <code>status [PATH]</code>", "<code>--rebuild</code> | <code>--compact</code>, <code>--index-dir</code>, <code>--ocr</code>, <code>--hidden</code>, <code>--no-ignore</code>, <code>-j</code>"],
        ["<code>lancet mcp</code>", "<code>--root DIR</code> (<code>.</code>), <code>--read-only</code>, <code>--trust-index</code>, <code>-j/--threads</code>"],
    ],
)}
"""


def _rg_parity(_d: str) -> str:
    return f"""
<p><code>bench/rg_parity.py</code>의 68개 엣지 케이스 픽스처(유니코드 케이스 폴딩, CRLF, 마지막 개행 누락, 바이너리 건너뛰기, <code>-m</code>+<code>-C</code>, <code>--vimgrep</code>, <code>-L</code>, <code>--max-depth</code>, <code>--max-filesize</code>)와 Linux v6.12 전체 쿼리를 <code>ripgrep 15.2</code>와 바이트 단위로 대조 검증했습니다.</p>

{h2("matching", "패턴 & 매칭")}
{table(
    ["플래그", "의미", "상태", "비고"],
    [
        ["<code>-e</code>, <code>--regexp &lt;PATTERN&gt;</code>", "검색 패턴 (중복 지정 가능)", "<span class='tag-yes'>지원</span>", "정규식 선택(alternation)으로 결합"],
        ["<code>-f</code>, <code>--file &lt;FILE&gt;</code>", "파일에서 줄당 하나씩 패턴 읽기 (<code>-</code> stdin)", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-F</code>, <code>--fixed-strings</code>", "리터럴 문자열로 취급", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-i</code> / <code>-s</code> / <code>-S</code>", "대소문자 무시 / 구분 / 스마트", "<span class='tag-yes'>지원</span>", "마지막 플래그 우선; AST 기반 <code>-S</code>(<code>\\W</code> 무시 유지)"],
        ["<code>-w</code>, <code>--word-regexp</code>", "단어 경계로 둘러싸인 매치만", "<span class='tag-yes'>지원</span>", "<code>rg</code> 경계 규칙과 동일(<code>-w '@x'</code> 지원)"],
        ["<code>-x</code>, <code>--line-regexp</code>", "줄 전체에 걸친 매치만", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-v</code>, <code>--invert-match</code>", "매치하지 않는 줄 출력", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-U</code>, <code>--multiline-dotall</code>", "멀티라인 및 <code>.</code>에 <code>\\n</code> 허용", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-P</code>, <code>--pcre2</code>", "전후방 탐색 및 역참조", "<span class='tag-no'>미지원</span>", "선형 시간 Rust <code>regex</code> + 건전한 HIR 트라이그램 컴파일"],
    ],
)}

{h2("output", "출력 & 형식")}
{table(
    ["플래그", "의미", "상태", "비고"],
    [
        ["<code>-o</code>, <code>-c</code>, <code>--count-matches</code>", "매치 부분만 / 줄 수 / 총 매치 수", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-l</code>, <code>--files-without-match</code>, <code>-q</code>", "일치 경로 / 미일치 경로 / 무출력", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-m &lt;N&gt;</code>, <code>-A</code>, <code>-B</code>, <code>-C</code>", "파일당 최대 개수 및 전후 문맥", "<span class='tag-yes'>지원</span>", "후속 문맥도 <code>-m</code> 준수; <code>--</code> 구분선 출력"],
        ["<code>--passthru</code> (별칭 <code>--passthrough</code>)", "일치·비일치 줄 모두 출력", "<span class='tag-yes'>지원</span>", "비일치 파일도 출력; <code>--passthru/-A/-B/-C</code> 중 마지막 우선"],
        ["<code>-n</code> / <code>-N</code>", "줄 번호 표시 / 숨기기", "<span class='tag-yes'>지원</span>", "파이프 시에도 <strong>기본 ON</strong>"],
        ["<code>-H</code> / <code>-I</code>, <code>--column</code>, <code>--vimgrep</code>", "파일명 표시, 1-based 열, vimgrep", "<span class='tag-yes'>지원</span>", "<code>-H/-I</code> 중 마지막 우선"],
        ["<code>--json</code>, <code>-0</code>, <code>-r &lt;TEXT&gt;</code>", "rg JSON Lines, NUL 구분, <code>$1</code>/<code>${{name}}</code> 치환", "<span class='tag-yes'>지원</span>", ""],
        ["<code>--color &lt;WHEN&gt;</code>", "<code>auto</code>, <code>always</code>, <code>ansi</code>, <code>never</code>", "<span class='tag-yes'>지원</span>", "기본 <code>auto</code>는 TTY와 <code>NO_COLOR</code>/<code>TERM=dumb</code> 준수"],
        ["<code>--sort</code>, <code>--no-heading</code>", "정렬, 파일명 헤더 끄기", "No-op", "항상 경로순 정렬; 헤더 사용 안 함"],
    ],
)}

{h2("traversal", "파일 순회, 필터링 & Lancet 확장")}
{table(
    ["플래그", "의미", "상태", "비고"],
    [
        ["<code>-g</code>, <code>--iglob</code>, <code>-t</code>, <code>-T</code>, <code>--type-list</code>", "글롭 및 타입 필터", "<span class='tag-yes'>지원</span>", "긍정 글롭은 ignore/hidden 덮어씀"],
        ["<code>--type-add &lt;SPEC&gt;</code>", "<code>NAME:GLOB</code> 또는 <code>NAME:include:A,B</code>", "<span class='tag-yes'>지원</span>", "<code>-t</code>, <code>-T</code>, <code>--files</code>, <code>--type-list</code> 반영"],
        ["<code>-a</code>, <code>-u</code>/<code>-uu</code>/<code>-uuu</code>, <code>-.</code>, <code>--no-ignore</code>", "바이너리/숨김/무시 해제", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-L</code>, <code>--max-depth</code>, <code>--max-filesize</code>", "심볼릭 링크 추적, 깊이, 크기 (<code>K/M/G</code>)", "<span class='tag-yes'>지원</span>", "<code>--trust-index -L</code>은 라이브 순회로 폴백"],
        ["<code>--files</code>, <code>-j/--threads</code>", "검색 대상 파일 출력, 워커 수", "<span class='tag-yes'>지원</span>", ""],
        ["<code>--docs</code>", "PDF/오피스/HWP/압축파일 인프로세스 검색", "<span class='tag-ext'>확장</span>", "<code>rg</code> 호환성 유지를 위해 기본 OFF"],
        ["<code>--no-index</code>, <code>--trust-index</code>, <code>--index-dir</code>", "<code>.lancet</code> 인덱스 동작 지정", "<span class='tag-ext'>확장</span>", "기본은 항상 최신인 <code>auto</code> 모드"],
        ["<code>--pre &lt;CMD&gt;</code>", "파일별 외부 전처리기", "<span class='tag-no'>미지원</span>", "인프로세스 <code>--docs</code>로 대체"],
    ],
)}
"""


def _lql_ast(_d: str) -> str:
    return f"""
<p><strong>LQL v0.1</strong> 정식 문법과 <code>lancet ast</code>·<code>lancet refs</code>가 공유하는 <strong>Tree-sitter 24개 언어</strong> 명세입니다.</p>

{h2("bnf", "LQL v0.1 EBNF 문법 & 우선순위")}
{code("EBNF (docs/grammar.md)", """query   := ["find"] expr ("in" scope)* ("|" stage)*
expr    := or_expr
or_expr := and_expr ("OR" and_expr)*
and_expr:= unary (("AND")? unary)*          # 공백 인접 항은 암묵적 AND
unary   := "NOT" unary | primary
primary := string
         | word                            # 따옴표 없는 단일 토큰 -> 리터럴
         | "(" expr ")"
         | "regex(" string ")"
         | "fuzzy(" string ["," uint] ")"  # k는 0..=3, 기본 1
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
         | "terms" ["top:" uint]           # 1..=100, 기본 15
         | "budget" ["tokens:"] uint       # uint >= 1""", "ko")}
<p><strong>우선순위:</strong> <code>NOT</code> &gt; <code>AND</code>(명시/암묵) &gt; <code>OR</code>. 알 수 없는 키워드나 여분 토큰은 0부터 시작하는 바이트 <code>pos</code>와 함께 <code>status: "invalid_query"</code>를 반환합니다.</p>

{h2("semantics", "매처, 스코프 & 파이프 의미론")}
{table(
    ["구문", "정확한 의미"],
    [
        ["<code>\"text\"</code> / 일반 토큰", "대소문자 무시(Unicode simple case folding) 부분 문자열 매치."],
        ["<code>regex(\"pat\")</code>", "멀티라인 Rust <code>regex</code>(줄 단위 <code>^</code>/<code>$</code>); 기본 대소문자 구분(<code>(?i)</code> 지정 가능)."],
        ["<code>fuzzy(\"word\", k)</code>", "단어 토큰(<code>[\\p&#123;Alphabetic&#125;\\p&#123;N&#125;_]+</code>) 중 문자 레벤슈타인 거리 <code>0 &lt;= k &lt;= 3</code>(기본 1); 인덱스는 건전한 q-gram <code>AtLeast</code> 사용."],
        ["<code>symbol(\"name\")</code> / <code>symbol:name</code>", "대소문자 무시 AST 정의 정확 매치(<code>.rs</code>, <code>.py</code>, <code>.js</code>, <code>.ts</code>, <code>.tsx</code>, <code>.go</code>)."],
        ["<code>near(a, b, d)</code> / <code>before(a, b, d)</code>", "단위 내 토큰 거리 <code>|tok(b) - tok(a)| &lt;= d</code>(<code>before</code>는 <code>b</code>가 <code>a</code>보다 엄격히 뒤)."],
        ["<code>in file</code> / <code>in block</code> / <code>in line</code>", "파일 전체(기본), 빈 줄 없는 연속 문단(<code>block</code>), 단일 줄(<code>line</code>; 매치 전체 <code>[start, end)</code>가 단위 안)."],
        ["<code>in function</code>", "Tree-sitter 함수/메서드 AST 노드(<code>.rs</code>, <code>.py</code>, <code>.js</code>, <code>.ts</code>, <code>.tsx</code>, <code>.go</code>); 결과 <code>symbol</code> 설정."],
        ["<code>type:</code> / <code>ext:</code> / <code>path:</code>", "카테고리 내부는 OR, 서로 다른 카테고리는 AND. 9종 <code>type:</code>: <code>code</code>, <code>document</code>, <code>spreadsheet</code>, <code>config</code>, <code>log</code>, <code>data</code>, <code>archive</code>, <code>image</code>, <code>other</code>."],
        ["<code>| expand</code> / <code>sort</code> / <code>terms</code> / <code>budget</code>", "<code>expand parent|unit|context:N</code>, 결정적 <code>sort count</code>/<code>path</code>, 불용어 제외 상위 <code>1..=100</code> 어휘, 추정 토큰 상한."],
    ],
)}

{h2("langs-24", "Tree-sitter 24개 지원 언어 (--lang)")}
{table(
    ["언어", "<code>--lang</code> 별칭", "언어", "<code>--lang</code> 별칭"],
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
<p><code>src/</code>의 실제 Rust 직렬화 구조체와 <code>src/mcp.rs</code>에 정의된 MCP 11개 도구의 전체 파라미터 스키마입니다.</p>

{h2("status", "Status 열거형 & ErrorInfo")}
<ul>
  <li><code>Status</code>: <code>"complete"</code> | <code>"empty"</code> | <code>"invalid_query"</code> | <code>"unsupported"</code> | <code>"conflict"</code></li>
  <li><code>ErrorInfo</code>: <code>{{"message": string, "pos"?: number}}</code> (<code>pos</code>는 LQL 파싱 오류 시 설정)</li>
</ul>

{h2("response-structs", "CLI --json 응답 구조체")}
{code("Search · Read · Edit · Rename", """// Response (lancet '<QUERY>' --json)
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

// ReadResponse (바이너리 문서·압축 파일에서는 rev 생략)
{ "status": "complete", "path": "src/net.rs", "rev": "e70d5a898f122a62", "section": "fn retry_limit", "total_lines": 80, "start_line": 1, "end_line": 4, "returned_lines": 4, "truncated": false, "next_line": null, "estimated_tokens": 28, "lines": [{ "num": 1, "text": "..." }], "elapsed_ms": 0.4 }

// EditResponse (mode: "dry-run" | "applied")
{ "status": "complete", "mode": "applied", "path": "src/net.rs", "rev": "e70d5a898f122a62", "rev_after": "1486bdffbc06c68f", "start_line": 2, "end_line": 2, "hunks": [[2, 2]], "applied": true, "diff": "...", "elapsed_ms": 1.1 }

// RenameResponse (tier: "local-scope")
{ "status": "complete", "mode": "applied", "path": "src/search.rs", "old": "old_n", "new": "new_n", "tier": "local-scope", "rev": "...", "rev_after": "...", "declaration": { "line": 40, "col": 9 }, "sites": [{ "line": 40, "col": 9 }], "comment_mentions": 0, "applied": true, "elapsed_ms": 2.3 }""", "ko")}

{code("Fd · Outline · Refs · Rank · Ast · Patch · Explain", """// FdResponse:      { status, mode, total, returned, truncated, entries: [{ path, kind, file_type?, size?, mtime_ns? }], elapsed_ms }
// OutlineResponse: { status, files: [{ path, file_type, total_lines?, items: [{ kind, name, depth, start_line?, end_line?, signature?, detail? }] }], total_items, truncated, elapsed_ms }
// RefsResponse:    { status, mode, symbol, definitions: [RefHit], references: [RefHit], total_references, truncated, stats }
// RankResponse:    { status, mode, pattern, entries: [{ name, kind, path, line, references, definitions }], total, truncated, files_parsed, elapsed_ms }
// AstResponse:     { status, mode, lang?, pattern, matched, returned, truncated, matches: [AstMatch], diffs: [FileDiff], stats }
// PatchResponse:   { status, mode, applied, files: [{ path, action, from?, hunks, offsets, rev?, rev_after?, diff }], elapsed_ms }
// ExplainResponse: { status, canonical?, ir?, trigram_req?, index_exists, files_considered, index_candidates?, stale_files?, selectivity_pct?, grams_available?, grams_decoded?, elapsed_ms }""", "ko")}

{h2("mcp-tools", "MCP 11개 도구 inputSchema 명세")}
<p>지원 프로토콜 버전: <code>2025-03-26</code>, <code>2024-11-05</code>. JSON-RPC 오류 코드: <code>-32700</code>(파싱 오류), <code>-32600</code>(잘못된 요청), <code>-32601</code>(미지원 메서드), <code>-32602</code>(잘못된 인자/프롬프트). <code>status: "empty"</code>는 <code>isError: false</code>입니다.</p>
{table(
    ["도구 (*필수 인자)", "파라미터 및 타입", "Read-Only / 비고"],
    [
        ["<code>lancet_fd</code>", "<code>path</code>, <code>pattern</code>, <code>pattern_mode</code> (<code>substring|glob|regex</code>), <code>case_sensitive</code>, <code>kinds</code>, <code>dirs</code>, <code>exts</code>, <code>type_classes</code>, <code>max_depth</code>, <code>changed_within</code>, <code>min_size</code>, <code>max_size</code>, <code>tree</code>, <code>limit</code>, <code>index_mode</code> (<code>auto|off|trust</code>), <code>hidden</code>, <code>no_ignore</code>, <code>meta</code>", "<code>readOnlyHint: true</code>"],
        ["<code>lancet_search</code> (<code>query*</code>)", "<code>query*</code>, <code>path</code>, <code>max_lines</code> (20), <code>max_cols</code> (240), <code>index_mode</code>, <code>hidden</code>, <code>no_ignore</code>, <code>ocr</code>", "<code>readOnlyHint: true</code>"],
        ["<code>lancet_rg</code> (<code>pattern*</code>)", "<code>pattern*</code>, <code>path</code>, <code>fixed_strings</code>, <code>ignore_case</code>, <code>smart_case</code>, <code>word</code>, <code>line_regexp</code>, <code>invert</code>, <code>multiline</code>, <code>multiline_dotall</code>, <code>only_matching</code>, <code>count</code>, <code>files_with_matches</code>, <code>max_count</code>, <code>before</code>, <code>after</code>, <code>context</code>, <code>globs</code>, <code>iglobs</code>, <code>types</code>, <code>types_not</code>, <code>type_add</code>, <code>max_depth</code>, <code>max_filesize</code>, <code>follow</code>, <code>docs</code>, <code>passthru</code>, <code>index_mode</code>, <code>hidden</code>, <code>no_ignore</code>, <code>max_output_bytes</code> (128 KiB..4 MiB)", "<code>readOnlyHint: true</code>; 줄 경계 절단"],
        ["<code>lancet_outline</code>", "<code>path</code>, <code>max_depth</code>, <code>exts</code>, <code>limit</code>, <code>hidden</code>, <code>no_ignore</code>, <code>ocr</code>", "<code>readOnlyHint: true</code>"],
        ["<code>lancet_read</code> (<code>target*</code>)", "<code>target*</code>, <code>lines</code>, <code>symbol</code>, <code>page</code>, <code>sheet</code>, <code>rows</code>, <code>cols</code>, <code>entry</code>, <code>budget_tokens</code>, <code>ocr</code>", "<code>readOnlyHint: true</code>; 텍스트 시 <code>rev</code> 반환"],
        ["<code>lancet_refs</code> (<code>symbol*</code>)", "<code>symbol*</code>, <code>path</code>, <code>lang</code>, <code>exts</code>, <code>paths</code>, <code>all_files</code>, <code>rank</code>, <code>kinds</code>, <code>limit</code>, <code>index_mode</code>, <code>hidden</code>, <code>no_ignore</code>", "<code>readOnlyHint: true</code>"],
        ["<code>lancet_ast</code> (<code>pattern*</code>)", "<code>pattern*</code>, <code>path</code>, <code>lang</code>, <code>rewrite</code>, <code>apply</code> (<code>false</code>), <code>paths</code>, <code>limit</code>, <code>index_mode</code>, <code>hidden</code>, <code>no_ignore</code>", "<code>--read-only</code> 시 <code>apply: true</code> 거절"],
        ["<code>lancet_edit</code> (<code>path*</code>)", "<code>path*</code>, <code>lines</code>, <code>symbol</code>, <code>find</code>, <code>all</code>, <code>with</code>, <code>delete</code>, <code>insert_before</code>, <code>insert_after</code>, <code>edits</code> (배열), <code>rev</code>, <code>apply</code> (<code>false</code>), <code>allow_syntax_errors</code>", "<code>--read-only</code> 시 목록 제외"],
        ["<code>lancet_patch</code> (<code>patch*</code>)", "<code>patch*</code>, <code>root</code>, <code>strip</code> (<code>1</code>), <code>apply</code> (<code>false</code>), <code>allow_syntax_errors</code>", "<code>--read-only</code> 시 목록 제외"],
        ["<code>lancet_rename</code> (<code>path*,line*,old*,new*</code>)", "<code>path*</code>, <code>line*</code>, <code>col</code>, <code>old*</code>, <code>new*</code>, <code>rev</code>, <code>apply</code> (<code>false</code>)", "<code>--read-only</code> 시 목록 제외"],
        ["<code>lancet_index</code>", "<code>action</code> (<code>status|sync|rebuild|compact</code>), <code>path</code>, <code>hidden</code>, <code>no_ignore</code>, <code>ocr</code>", "<code>--read-only</code> 시 변경 작업 거절"],
    ],
)}
"""


def _troubleshooting(_d: str) -> str:
    return f"""
<p>Lancet의 모든 거절은 조용히 덮어쓰지 않고 종료 코드 <code>2</code>와 구체적인 원인 메시지를 반환합니다. 증상별 해결 절차를 바로 찾으세요.</p>

{h2("edit-patch-errors", "편집 · 패치 · 이름 변경 거절")}
{table(
    ["증상 / 오류", "근본 원인", "정확한 복구 절차"],
    [
        ["<code>edit</code> / <code>patch</code> / <code>rename</code>에서 <code>status: \"conflict\"</code>", "<code>--rev</code>를 읽은 뒤 디스크 파일이 변경됨", "<code>lancet read &lt;FILE&gt; --json</code>으로 새 내용과 <code>rev</code>를 다시 읽어 수정을 재계산해 적용합니다."],
        ["<code>edit --find</code> 거절 (0회 또는 2회 이상 일치)", "리터럴 문자열이 파일 내에서 고유하지 않음", "오류에 출력된 줄 번호를 보고 <code>--find</code> 앞뒤 문맥을 넓히거나 의도한 전체 변경이면 <code>--all</code>을 추가합니다."],
        ["<code>edit --symbol</code> 후 문서 주석·어트리뷰트 사라짐", "<code>--symbol</code>은 <code>read --symbol</code>처럼 위쪽 <code>///</code>·<code>#[attr]</code>까지 포함함", "<code>--with</code>/<code>--with-file</code>에 주석과 어트리뷰트를 포함하거나 본문 줄 범위(<code>--lines</code>)만 지정하세요."],
        ["구문 오류 증가 거절", "편집·패치 결과 Tree-sitter 구문 오류 수가 늘어남", "dry-run diff에서 닫히지 않은 괄호·따옴표를 점검하고, 의도된 중간 단계일 때만 <code>--allow-syntax-errors</code>를 씁니다."],
        ["<code>lancet rename</code> 거절 (<code>unsupported</code>)", "최상위/필드/메서드/Python 매개변수/매크로/JSX 태그/섀도잉", "<a href='../guides/rename.html#refusals'>거절 목록</a>을 확인하고 <code>lancet refs</code> + <code>lancet edit --edits</code>나 LSP를 사용하세요."],
        ["<code>lancet patch</code> 바이너리/복사 diff 거절", "패치에 <code>Binary files</code> 또는 <code>copy from</code> 포함", "텍스트 hunk만 남겨 파이프하세요."],
    ],
)}

{h2("search-ast-errors", "검색 · AST · 순회 오류")}
{table(
    ["증상 / 오류", "근본 원인", "정확한 복구 절차"],
    [
        ["LQL에서 <code>status: \"invalid_query\"</code>", "알 수 없는 필터, 닫히지 않은 따옴표, 연산자 누락", "<code>lancet parse '&lt;QUERY&gt;'</code>를 실행해 오류 <code>pos</code> 바이트 위치를 확인하세요."],
        ["<code>ast --rewrite --apply</code>가 <code>--limit</code> 거절", "개수 제한과 자동 적용의 조합은 의도치 않은 부분 수정을 유발함", "<code>--limit</code>을 빼고 <code>--path GLOB</code>로 대상 파일을 좁히세요."],
        ["<code>ast 'if ($C) $S'</code>가 <code>if ... else</code> 누락", "Lancet은 <code>if</code>와 <code>if-else</code> AST 노드를 엄격히 구분함", "<code>else</code> 분기까지 포함하는 두 번째 패턴을 함께 실행하세요."],
        ["<code>lancet rg -P</code> 실패", "PCRE2 전후방 탐색·역참조 미지원", "룩어라운드 없는 Rust <code>regex</code>나 LQL <code>before()</code> / <code>NOT</code>으로 작성하세요."],
        ["<code>lancet fd</code>에 경로 2개 전달 시 오류", "<code>lancet fd [PATTERN] [PATH]</code>는 최대 1개 <code>PATH</code>만 허용", "디렉터리마다 나눠 호출하거나, 패턴이 폴더명과 같다면 <code>-p PATTERN</code>을 쓰세요."],
    ],
)}

{h2("index-docs-mcp", "인덱스 · 문서 · MCP 문제")}
{table(
    ["증상 / 메시지", "근본 원인", "정확한 복구 절차"],
    [
        ["<code>--trust-index</code>에서 방금 수정한 파일이 검색 안 됨", "<code>--trust-index</code>는 설계상 디스크 stat 검사를 건너뜀", "<code>--trust-index</code>를 빼고 <code>auto</code> 모드를 쓰거나 <code>lancet index .</code>로 동기화하세요."],
        ["Windows에서 크기·<code>mtime</code> 동일한 수정 놓침", "Windows에서 Rust stdlib이 <code>mtime</code>과 <code>size</code>만 제공함", "타임스탬프를 되돌리는 도구 실행 후엔 <code>lancet index .</code>나 <code>--no-index</code>를 쓰세요."],
        ["<code>lancet rg</code>가 PDF / <code>.docx</code> 내용을 못 찾음", "<code>lancet rg</code>는 <code>rg</code> 호환성 때문에 어댑터가 기본 OFF", "<code>lancet rg --docs</code>를 붙이거나 기본 LQL 검색(<code>lancet 'query'</code>)을 쓰세요."],
        ["스캔 PDF / 이미지 텍스트가 비어 있음", "임베디드 텍스트 레이어가 없는 이미지", "<code>--ocr</code>을 붙이고 <code>PATH</code>에 <code>pdftoppm</code>과 <code>tesseract</code>가 있는지 확인하세요."],
        ["MCP 목록에 <code>lancet_edit</code> / <code>patch</code> / <code>rename</code> 없음", "<code>lancet mcp --read-only</code>로 실행됨", "편집이 필요하면 <code>--read-only</code> 없이 서버를 다시 시작하세요."],
    ],
)}
"""


PAGES_KO_PART3 = [
    {
        "path": "reference/cli.html",
        "group": "reference",
        "nav_title": "CLI 명령 · 종료 코드",
        "title": "CLI 하위 명령, 플래그 & 종료 코드 레퍼런스",
        "description": "src/main.rs 기준으로 검증된 15개 진입점별 플래그 테이블, 기본값, 종료 코드.",
        "lede": "15개 모든 하위 명령의 정확한 구문, 플래그 기본값, 종료 코드 명세.",
        "keywords": "cli 하위명령 플래그 종료코드 0 1 2 conflict invalid_query unsupported",
        "source": "https://github.com/CREE1116/lancet/blob/main/src/main.rs",
        "render": _cli,
    },
    {
        "path": "reference/rg-parity.html",
        "group": "reference",
        "nav_title": "lancet rg 호환성 표",
        "title": "lancet rg — ripgrep 기능 호환성 매트릭스",
        "description": "매칭, 출력, 순회, --passthru, --type-add, --docs, 의도적 차이까지 전체 플래그 대조표.",
        "lede": "ripgrep 15.2와 바이트 단위로 대조 검증한 전체 플래그 호환성 매트릭스.",
        "keywords": "rg ripgrep 호환성 passthru type-add pcre2 pre docs max-depth max-filesize color",
        "source": f"{REPO_DOCS}/rg-parity.md",
        "render": _rg_parity,
    },
    {
        "path": "reference/lql-and-ast.html",
        "group": "reference",
        "nav_title": "LQL 문법 · AST 24개 언어",
        "title": "LQL v0.1 정식 문법 & 24개 언어 AST 명세",
        "description": "EBNF 문법, 연산자 우선순위, 매처/스코프/파이프 의미론, 9종 type: 클래스, 24개 --lang 별칭.",
        "lede": "EBNF 정식 문법, 정확한 쿼리 의미론, Tree-sitter 24개 언어 별칭 표.",
        "keywords": "lql bnf 문법 우선순위 near before fuzzy regex symbol expand terms budget lang 별칭",
        "source": f"{REPO_DOCS}/grammar.md",
        "render": _lql_ast,
    },
    {
        "path": "reference/json-and-mcp.html",
        "group": "reference",
        "nav_title": "JSON 스키마 · MCP 11개 도구",
        "title": "JSON 응답 스키마 & MCP 11개 도구 레퍼런스",
        "description": "Rust 구조체와 일치하는 모든 JSON 출력 스키마, MCP 11개 도구 inputSchema, JSON-RPC 오류 코드.",
        "lede": "Rust 구조체 기준 기계 판독용 JSON 응답 필드와 MCP 도구별 전체 파라미터 명세.",
        "keywords": "json 스키마 응답 mcp inputschema lancet_fd lancet_rg lancet_edit max_output_bytes json-rpc",
        "source": f"{REPO_DOCS}/mcp.md",
        "render": _json_mcp,
    },
    {
        "path": "reference/troubleshooting.html",
        "group": "reference",
        "nav_title": "문제 해결 · 오류 복구",
        "title": "문제 해결 & 거절 사유별 복구 매트릭스",
        "description": "CAS 충돌, --find 비고유, 구문 게이트, rename 거절, patch, OCR, Windows 최신성 증상별 복구표.",
        "lede": "모든 오류와 의도적 거절 사유에 대한 원인 진단과 즉시 복구 절차.",
        "keywords": "문제해결 conflict find 구문오류 rename unsupported patch pcre2 fd windows ocr",
        "source": "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md",
        "render": _troubleshooting,
    },
]
