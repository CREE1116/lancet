"""Korean documentation pages 8-14: 편집과 에이전트 & 아키텍처와 내부 구조."""

from __future__ import annotations

from docs_ui import callout, code, diagram, h2, h3, table

REPO_DOCS = "https://github.com/CREE1116/lancet/blob/main/docs"


def _edit(d: str) -> str:
    return f"""
<p><code>lancet edit</code>는 <code>lancet read</code>가 반환한 정확한 줄 번호와 <code>rev</code> 해시로 파일을 수정합니다. <code>--apply</code>를 붙이기 전에는 항상 dry-run unified diff만 출력합니다.</p>

{diagram(f"{d}/coordinates.ko.svg", "검색, 읽기, 심볼, 리비전, 편집이 하나의 좌표계를 공유합니다.", "<code>read</code>가 반환하는 줄 번호와 XXH3-64 <code>rev</code>를 <code>edit</code>와 <code>rename</code>이 그대로 소비합니다.")}

{h2("single-hunk", "단일 구간 지정 (--lines, --symbol, --find)")}
<p><strong>대상 1개</strong>와 <strong>연산 1개</strong>를 정확히 짝지어 지정합니다:</p>
<ul>
  <li><strong>대상:</strong> <code>--lines START:END|LINE|START:</code> · <code>--symbol NAME</code> · <code>--find TEXT [--all]</code></li>
  <li><strong>연산:</strong> <code>--with TEXT</code> · <code>--with-file PATH|-</code> · <code>--delete</code> · <code>--insert-before TEXT</code> · <code>--insert-after TEXT</code></li>
</ul>
{code("터미널", "# 1. 줄 범위 교체\nlancet edit src/net.rs --lines 12:15 --with '    timeout_ms: 5000,' \\\n  --rev e70d5a898f122a62 --apply\n\n# 2. AST 심볼 교체 (위쪽 문서 주석과 어트리뷰트도 범위에 포함됨!)\nlancet edit src/net.rs --symbol retry_limit --with-file new_fn.rs --rev <REV> --apply\n\n# 3. 고유 텍스트 치환 (LF와 CRLF 개행 모두 매치)\nlancet edit src/net.rs --find 'let retries = 3;' --with 'let retries = 5;' --rev <REV> --apply\n\n# 4. 모든 일치 항목 일괄 치환 (--all 없으면 비고유 시 거절)\nlancet edit src/net.rs --find 'old_fn(' --with 'new_fn(' --all --rev <REV> --apply", "ko")}

{callout("warn", "자주 겪는 함정 두 가지", "<ul><li><strong><code>--symbol</code> 경계:</strong> <code>read --symbol</code>과 똑같이 선언 위쪽의 <code>///</code> 문서 주석과 <code>#[attr]</code> 어트리뷰트까지 포함합니다. 유지하려면 새 본문에도 함께 넣어야 합니다.</li><li><strong><code>--find</code> 고유성:</strong> <code>--all</code> 없이 0회 또는 2회 이상 등장하면 <code>invalid_query</code>로 거절하고 해당 줄 번호를 출력해 주변 문맥을 넓히도록 안내합니다.</li></ul>")}

{h2("multi-hunk", "원자적 다중 구간 편집 (--edits)")}
<p>한 번 읽은 파일에서 여러 군데를 고칠 때 줄 번호 편집을 두 번 연달아 호출하면 첫 편집 이후 줄 번호가 밀립니다. <code>--edits FILE</code>이나 <code>--edits -</code>(stdin)에 JSON 배열 하나로 전달하면 모든 구간을 <strong>원본 파일 좌표</strong>로 해석하고 겹침을 거부한 뒤 전부 반영하거나 한 줄도 쓰지 않습니다.</p>
{code("터미널", "lancet edit src/net.rs --rev e70d5a898f122a62 --apply --edits - <<'JSON'\n[\n  {\"lines\": \"10:12\", \"with\": \"const RETRIES: u32 = 5;\"},\n  {\"symbol\": \"legacy_helper\", \"delete\": true},\n  {\"find\": \"old_metric(\", \"with\": \"new_metric(\", \"all\": true},\n  {\"lines\": \"40\", \"insert_after\": \"    tracing::info!(\\\"updated\\\");\"}\n]\nJSON", "ko")}
<p>배열의 각 객체는 대상 키(<code>lines</code>, <code>symbol</code>, <code>find</code> 중 1개; <code>find</code>에 한해 <code>all</code> 선택)와 연산 키(<code>with</code>, <code>delete</code>, <code>insert_before</code>, <code>insert_after</code> 중 1개)를 하나씩 가져야 합니다.</p>

{h2("cas-loop", "멀티 에이전트 CAS 동시성 & 구문 게이트")}
{diagram(f"{d}/concurrent-edit.ko.svg", "에이전트 4개 동시 수정 시 일반 덮어쓰기는 800건 중 600건 유실, lancet --rev는 0건 유실.", "배타적 OS 파일 잠금 아래 낡은 rev는 <code>status: \"conflict\"</code>(종료 2)를 내고 0바이트를 씁니다.")}
<ol>
  <li><code>lancet read --json</code>에서 <code>rev</code>를 확보합니다(연속 편집은 직전 응답의 <code>rev_after</code>를 이어 씀).</li>
  <li><code>lancet edit ... --rev &lt;REV&gt; --apply</code>를 실행합니다. 그사이 다른 프로세스가 썼다면 <code>status: "conflict"</code>(종료 <code>2</code>)가 반환됩니다.</li>
  <li><code>conflict</code> 시 <code>lancet read</code>로 최신 본문을 다시 읽어 수정을 재계산한 뒤 새 <code>rev</code>로 재시도하세요.</li>
</ol>
<p><strong>Tree-sitter 구문 게이트:</strong> 편집 전후를 파싱해 구문 오류가 <em>순증가</em>하는 수정만 거부합니다(이미 구문이 깨진 파일도 단계적으로 복구 가능). 의도적으로 중간 상태를 저장할 때만 <code>--allow-syntax-errors</code>를 씁니다.</p>
"""


def _patch(_d: str) -> str:
    return f"""
<p><code>lancet patch</code>는 여러 파일에 걸친 표준 <code>git diff</code> 및 <code>diff -u</code> 패치에 compare-and-swap, 전체 사전 검증, 경합 롤백, Tree-sitter 구문 검사를 적용합니다.</p>

{h2("usage", "플래그 & 기본 사용법")}
{code("터미널", "# Dry-run 검증 및 diff 미리보기 (기본 동작)\nlancet patch changes.diff\ngit diff | lancet patch\n\n# 하위 디렉터리 기준 적용 (-p 1은 기본값으로 a/, b/ 접두어 제거)\ngit diff | lancet patch --root services/auth -p 1 --apply\n\n# a/, b/ 접두어가 없는 일반 diff -u\ndiff -u old.rs new.rs | lancet patch -p 0 --apply", "ko")}

{h2("exact-reloc", "정확한 Hunk 재배치 vs 퍼지 패치")}
<p>일반 <code>patch -p1</code>은 문맥이 어긋나도 퍼지 매칭으로 덮어쓰거나 일부 파일만 반영합니다. <code>src/patch.rs</code>의 규칙은 다릅니다:</p>
<ul>
  <li><strong>문맥·삭제 줄 완전 일치 필수:</strong> 모든 hunk의 문맥과 <code>-</code> 줄은 파일과 정확히 일치해야 합니다.</li>
  <li><strong>고유한 최근접 오프셋 재배치:</strong> 줄 번호가 밀렸더라도 모든 원본 줄이 <em>단 한 곳</em>에서만 정확히 일치하는 가장 가까운 위치가 있으면 그 오프셋으로 재배치하고 JSON <code>offsets</code>에 기록합니다(<code>0</code> = 정확 일치). 모호하거나 일치하지 않으면 퍼지 추정 대신 거절합니다.</li>
  <li><strong>EOF 개행 마커:</strong> 양쪽의 <code>\\ No newline at end of file</code>을 정확히 보존합니다.</li>
</ul>

{h2("git-headers", "지원 및 거부되는 Diff 구문")}
{table(
    ["Unified Diff 요소", "상태", "<code>lancet patch</code> 동작"],
    [
        ["일반 수정, 다중 hunk 파일", "<span class='tag-yes'>지원</span>", "구문 검사 및 잠금 CAS 쓰기"],
        ["<code>git diff -M</code> 이름 변경 (내용 수정 유무 무관)", "<span class='tag-yes'>지원</span>", "<code>rename from</code> / <code>rename to</code> 처리"],
        ["권한 변경 (<code>new mode</code>, <code>new file mode</code>)", "<span class='tag-yes'>지원</span>", "Unix 파일 권한 비트(<code>chmod</code>) 적용"],
        ["빈 파일 생성 / 삭제", "<span class='tag-yes'>지원</span>", "<code>/dev/null</code> 및 <code>deleted file mode</code> 감지"],
        ["<code>copy from</code> / <code>copy to</code>", "<span class='tag-no'>거부</span>", "어떤 파일도 쓰지 않고 전체 패치 중단"],
        ["<code>Binary files ...</code> / <code>GIT binary patch</code>", "<span class='tag-no'>거부</span>", "디스크를 건드리지 않고 거부"],
    ],
)}

{h2("atomicity", "전체 사전 검증 & 경합 시 롤백")}
<p>어느 한 파일이라도 쓰기 전에 패치 안의 <strong>모든 파일</strong>을 읽고 검증·재배치·구문 검사합니다. 이후 사전 확인한 <code>rev</code>로 잠금 <code>write_if_rev</code>를 순차 실행하며, 도중 다른 쓰기 작업과 경합이 나면 앞서 수정한 파일까지 최선으로 되돌린 뒤 <code>conflict</code>(종료 <code>2</code>)를 반환합니다.</p>
"""


def _rename(_d: str) -> str:
    return f"""
<p><code>lancet rename</code>은 단일 파일 내부의 <strong>1단계(tier-1) 로컬 스코프 심볼 이름 변경</strong>을 수행합니다. 문자열 치환이나 휴리스틱 대신 언어별 어휘 스코프를 해석하고, 변경 후 파일 전체를 재해석해 섀도잉과 변수 캡처가 0건임을 증명할 때만 씁니다.</p>

{h2("usage", "대상 지정 문법 & 지원 6개 언어")}
<p>지원 언어: <strong>Rust</strong>(<code>.rs</code>), <strong>Python</strong>(<code>.py</code>), <strong>JavaScript</strong>(<code>.js</code>), <strong>TypeScript</strong>(<code>.ts</code>), <strong>TSX</strong>(<code>.tsx</code>), <strong>Go</strong>(<code>.go</code>).</p>
{code("터미널", "# Dry-run diff (해당 줄에 OLD 바인딩이 하나뿐이면 COL 생략 가능)\nlancet rename src/search.rs:42 old_name new_name\n\n# 열(1-based byte offset, `lancet rg --column`과 동일) + CAS rev 지정\nlancet rename src/search.rs:42:9 old_name new_name --rev <REV> --apply", "ko")}
<p><code>FILE:LINE[:COL]</code>은 <strong>선언 위치</strong>나 스코프 내부의 <strong>사용 위치</strong> 어느 쪽을 가리켜도 됩니다. 스코프 내 주석의 언급은 바꾸지 않고 JSON <code>comment_mentions</code> 횟수로만 보고합니다.</p>

{h2("how-proof-works", "2패스 스코프 해석 및 섀도잉 증명")}
<ol>
  <li><strong>1패스 (바인딩 수집):</strong> 언어별 호이스팅, 블록/함수 스코프, 클로저 매개변수, 루프 바인더 규칙으로 스코프 트리를 구성해 해당 로컬 선언에 묶인 모든 AST 식별자를 찾습니다.</li>
  <li><strong>2패스 (변경 후 재해석):</strong> 모든 위치를 <code>new</code>로 바꾼 가상 소스에서 파일 스코프를 다시 해석합니다. 어느 한 사용 위치라도 기존과 다른 선언을 가리키면(섀도잉 또는 외부 변수 캡처) <code>unsupported</code>로 거절하고 디스크를 건드리지 않습니다.</li>
</ol>

{h2("refusals", "lancet rename이 의도적으로 거절하는 대상")}
{table(
    ["거절되는 구문", "추측 시 코드가 망가지는 이유"],
    [
        ["모듈 / 최상위 / 외부 노출 심볼", "다른 파일에서 import해 쓸 수 있어 프로젝트 전체 컴파일러/LSP 필요"],
        ["구조체 필드 &amp; 메서드", "리시버를 통해 파일 경계를 넘어 접근됨"],
        ["<strong>Python 함수 매개변수</strong>", "다른 파일 호출자가 키워드 인자(<code>fn(old=1)</code>)로 넘길 수 있음"],
        ["매크로 내부 &amp; JSX 태그명", "일반 변수 바인딩과 다른 매크로 확장·컴포넌트 해석 규칙"],
        ["Go 구조체 리터럴 키 (<code>T{{old: v}}</code>)", "지역 변수 읽기가 아니라 필드 라벨임"],
        ["동적 구문 (<code>eval</code>, <code>with</code>, <code>locals()</code>, <code>global</code>, <code>nonlocal</code>)", "동적 네임스페이스 조회로 정적 스코프 증명이 불가능함"],
        ["섀도잉 / 변수 캡처 발생", "<code>new</code>가 스코프 내 기존 바인딩과 충돌함"],
    ],
)}

{callout("tip", "크로스 파일 또는 거절된 심볼 변경 방법", "<p><code>lancet refs &lt;SYMBOL&gt; src --json</code>으로 정의와 참조를 모두 확인한 뒤 파일별 <code>lancet edit --edits</code> 또는 언어 LSP를 사용하세요.</p>")}
"""


def _agents_mcp(_d: str) -> str:
    return f"""
<p>AI 코딩 에이전트는 셸 CLI는 물론 의존성 없는 내장 stdio <strong>Model Context Protocol(MCP)</strong> 서버(<code>lancet mcp</code>)와 크로스 에이전트 <strong>공식 스킬</strong>(<code>skills/lancet/</code>)로 Lancet의 11개 엔진을 직접 호출할 수 있습니다.</p>

{h2("mcp-flags", "lancet mcp 실행 플래그")}
<p><code>lancet mcp</code>는 <code>stdin</code>/<code>stdout</code> 위에서 줄바꿈 구분 JSON-RPC 2.0(프로토콜 <code>2025-03-26</code>, <code>2024-11-05</code>)으로 동작하며 별도 데몬 없이 인프로세스로 즉시 응답합니다.</p>
{table(
    ["플래그", "기본값", "정확한 동작"],
    [
        ["<code>--root &lt;DIR&gt;</code>", "<code>.</code>", "도구 호출의 상대 경로 기준 디렉터리 (chroot/샌드박스가 <strong>아님</strong>; CLI처럼 절대 경로나 <code>..</code> 경로도 허용됨)"],
        ["<code>--read-only</code>", "<code>false</code>", "<code>tools/list</code>에서 <code>lancet_edit</code>, <code>lancet_patch</code>, <code>lancet_rename</code>을 제거하고 쓰기(<code>lancet_ast</code> <code>apply</code>, 인덱스 변경)를 거부"],
        ["<code>--trust-index</code>", "<code>false</code>", "개별 호출에서 지정하지 않는 한 기본 <code>index_mode</code>를 <code>trust</code>로 설정"],
        ["<code>-j, --threads &lt;NUM&gt;</code>", "CPU 코어 수", "Rayon 워커 스레드 수"],
    ],
)}

{h2("client-configs", "클라이언트별 설정")}
<p>전체 템플릿: <a href="https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/mcp-config.json"><code>skills/lancet/references/mcp-config.json</code></a>.</p>
{h3("claude-code", "Claude Code")}
{code("터미널", "# 읽기 + 잠금 CAS 쓰기\nclaude mcp add --transport stdio lancet -- lancet mcp --root .\n\n# 읽기 전용 탐색 서버\nclaude mcp add --transport stdio lancet -- lancet mcp --root . --read-only", "ko")}
{h3("cursor", "Cursor (.cursor/mcp.json 또는 ~/.cursor/mcp.json)")}
{code("JSON", '{\n  "mcpServers": {\n    "lancet": {\n      "command": "lancet",\n      "args": ["mcp", "--root", "."]\n    }\n  }\n}', "ko")}
{h3("desktop-agy", "Claude Desktop / Windsurf / Antigravity")}
<p>데스크톱 클라이언트가 저장소 루트를 CWD로 두지 않고 실행한다면 <code>--root</code>에 절대 경로를 넘기세요:</p>
{code("JSON", '{\n  "mcpServers": {\n    "lancet": {\n      "command": "lancet",\n      "args": ["mcp", "--root", "/abs/path/to/your/repo"]\n    }\n  }\n}', "ko")}
{h3("codex", "OpenAI Codex (~/.codex/config.toml)")}
{code("TOML", '[mcp_servers.lancet]\ncommand = "lancet"\nargs = ["mcp", "--root", "."]', "ko")}

{h2("skill-install", "공식 에이전트 스킬 설치")}
<p><a href="https://github.com/CREE1116/lancet/tree/main/skills/lancet"><code>skills/lancet/</code></a>는 <code>SKILL.md</code>와 LQL·24개 AST 언어·edit/patch/rename CAS 규칙·MCP 설정용 <code>references/</code> 문서를 포함합니다.</p>
{code("터미널", "# 프로젝트 로컬 (.agents 공통 규약)\nmkdir -p .agents/skills && ln -s \"$(pwd)/skills/lancet\" .agents/skills/lancet\n\n# 개인 전역 설치 (Claude Code / Codex / Antigravity)\nmkdir -p ~/.claude/skills && cp -R skills/lancet ~/.claude/skills/lancet\nmkdir -p ~/.codex/skills  && cp -R skills/lancet ~/.codex/skills/lancet\nmkdir -p ~/.gemini/config/skills && cp -R skills/lancet ~/.gemini/config/skills/lancet", "ko")}

{h2("prompts-notes", "내장 프롬프트 & AGENTS.md 규칙")}
<ul>
  <li><strong><code>prompts/list</code>:</strong> <code>lancet-explore</code>(<code>goal</code> 인자)와 <code>lancet-safe-edit</code>(<code>target</code> 인자) 제공.</li>
  <li><strong>0건 매치 규칙:</strong> 조회·검색 도구는 <code>status: "empty"</code>일 때도 <code>isError: false</code>를 반환합니다.</li>
  <li><strong>줄 경계 바이트 상한:</strong> <code>lancet_rg</code>는 <code>max_output_bytes</code>(기본 <code>131072</code> B = 128 KiB, 허용 범위 <code>256..=4194304</code>)에 맞춰 줄 경계에서 깔끔히 자릅니다.</li>
</ul>
{code("AGENTS.md / CLAUDE.md 규칙 예시", "rg/fd/ast-grep/cat/sed 대신 lancet(lancet_* MCP 또는 lancet CLI)을 우선 사용합니다.\nfd/outline으로 구조를 잡고 rg/search/refs/ast로 찾은 뒤, read로 rev를 확보해 edit/patch/rename --rev --apply로 수정합니다.", "ko")}
"""


def _index_freshness(d: str) -> str:
    return f"""
<p>백그라운드 워처가 따라잡기를 기다리거나 인덱스 캐시만으로 근사 결과를 돌려주는 도구와 달리, Lancet은 데몬 없는 불변 <strong>LSM 트라이그램 인덱스(<code>LANCET04</code>)</strong> 위에서 단 하나의 불변식 <strong><code>Scan == Auto</code></strong>를 보장합니다.</p>

{diagram(f"{d}/pipeline.ko.svg", "Linux v6.12에서 인덱스 쿼리가 91%를 건너뛰며 정확한 결과를 내는 과정.", "Linux v6.12(86,218개 파일)에서 <code>\"spin_lock\"</code> <code>auto</code> 모드 실측: 7,814개만 열어(9.1%) 7,073개(8.2%) 정확히 일치.")}

{h2("trigram-lowering", "건전한 트라이그램 컴파일")}
<p>리터럴, 정규식 HIR 분기, AST 리터럴, <code>refs --rank</code> 글롭 리터럴은 필수 바이트 트라이그램 논리식으로, <code>fuzzy()</code>는 건전한 q-gram 하한 <code>AtLeast(k)</code>로 컴파일됩니다. 필수 트라이그램이 없는 파일은 논리적으로 매치할 수 없으므로 건너뛰어도 <strong>누락이 0건</strong>이며, 남은 모든 후보는 풀스캔과 완전히 동일한 평가기로 검증됩니다.</p>

{h2("segment-format", "디스크 LANCET04 바이너리 구조 & 제로카피 mmap")}
<p><code>PATH/.lancet</code>(또는 상위 디렉터리 자동 탐색, <code>--index-dir</code>로 변경 가능) 안에 <code>manifest.json</code>과 불변 세그먼트(<code>seg-NNNNNN.wseg</code>)가 저장됩니다:</p>
{code("LANCET04 바이너리 레이아웃 (src/index/segment.rs)", """header (40 B):  "LANCET04" | ndocs:u32 | nkeys:u32 | doc_offs_off:u64 | keys_off:u64 | post_off:u64
doc_offs:       (ndocs + 1) * u32 오프셋 (docs_off 기준)
docs:           문서별(상대 UTF-8 순): mtime_ns:u64 | size:u64 | ctime_ns:u64 | ino:u64 | flags:u8 | rel_utf8
keys:           nkeys * (key:u32 | off:u32) + sentinel_end_off:u32 (트라이그램 키 순 정렬)
postings:       varint(count) + delta-varint 문서 ID""", "ko")}
<p><code>Segment::open</code>은 40바이트 헤더만 검사하므로 파일 수와 무관하게 <strong>힙 할당 0, O(1)</strong>으로 열립니다. 문서별 플래그는 <code>FLAG_BINARY(1)</code>, <code>FLAG_ERROR(2)</code>, <code>FLAG_OVERSIZE(4, &gt; 64 MiB — 쿼리 시 mmap 스트리밍)</code>, <code>FLAG_RACY(8)</code>를 기록합니다.</p>

{h2("freshness", "4종 메타데이터 + Racy-Timestamp 방어")}
{diagram(f"{d}/index-lifecycle.ko.svg", "인덱스 수명주기: 초기 빌드, 매 쿼리 stat 대조, 증분 세그먼트, 컴팩션.", "auto 모드는 데몬 없이도 방금 수정되거나 새로 생긴 파일을 즉시 감지합니다.")}
<p>왜 <code>(mtime, size)</code>만으로는 불충분할까요? <code>cp -p</code>, <code>tar</code>, 같은 길이 수정은 둘 다 그대로 둘 수 있습니다. Unix/macOS에서 Lancet은 <strong><code>(mtime_ns, size, ctime_ns, ino)</code></strong> 4가지를 모두 기록하고 Git의 <strong>racy-timestamp 창</strong>을 적용합니다:</p>
<ul>
  <li><strong>적응형 racy 창:</strong> 최대 4,096개 파일의 나노초 타임스탬프를 표본 검사해, 세밀한 파일시스템(APFS, ext4, NTFS)에서는 <code>100 ms</code>, 거친 타임스탬프(HFS+, ext3, FAT)에서는 <code>2 s</code> 창 안에 들어온 파일에 <code>FLAG_RACY</code>를 찍어 캐시를 믿지 않고 항상 라이브 검증합니다.</li>
</ul>
{callout("warn", "Windows 플랫폼의 정직한 한계", "<p>Windows에서 Rust 표준 라이브러리는 <code>mtime</code>과 <code>size</code>만 안정적으로 노출합니다(<code>ctime_ns</code>, <code>ino</code>는 <code>0</code>). 따라서 Windows에서 파일 크기와 <code>mtime</code>을 함께 유지한 채 덮어쓰는 경우 <code>lancet index</code> 또는 <code>--no-index</code>를 실행해야 합니다.</p>")}

{h2("compaction", "동작 모드, 증분 동기화 & 자동 컴팩션")}
<ul>
  <li><strong><code>auto</code> (기본값):</strong> 메타데이터 stat 순회(8.7만 파일 약 100 ms) + 바뀐 파일만 라이브 스캔.</li>
  <li><strong><code>--trust-index</code>:</strong> stat 순회를 생략하고 <code>.lancet</code>만 신뢰(Linux 커널 약 40 ms, 0건 시 2~3 ms); <code>lancet index</code> 전의 미반영 수정은 누락될 수 있습니다.</li>
  <li><strong><code>--no-index</code>:</strong> 인덱스를 무시하고 디스크 전체를 직접 스캔합니다.</li>
  <li><strong>LSM 컴팩션:</strong> 증분 <code>lancet index</code>는 새 세그먼트와 삭제 툼스톤을 <code>manifest.json.tmp</code> 원자적 rename으로 반영합니다. 세그먼트가 <code>COMPACT_MAX_SEGMENTS = 6</code>을 넘거나 죽은 문서가 <code>COMPACT_MIN_DEAD = 32</code> 이상이면 자동 병합하며 <code>--compact</code> / <code>--rebuild</code>로 수동 실행도 가능합니다.</li>
</ul>
"""


def _write_safety(d: str) -> str:
    return f"""
<p>왜 읽기, AST 파싱, diff, 쓰기를 바이너리 하나에 넣었을까요? CLI 도구 체인에서 사고는 항상 <em>도구와 도구 사이</em>에서 발생하며, Lancet의 모든 쓰기 명령은 <code>src/edit.rs</code>의 단일 안전 경로를 공유하기 때문입니다.</p>

{diagram(f"{d}/failure-modes.ko.svg", "bench/failure_modes.py로 직접 재현한 7가지 도구 체인 사고.", "모든 항목은 실제 ripgrep, BSD sed/patch, pandoc, rga, zoekt로 측정한 결과입니다.")}

{h2("seven-failures", "실제 도구 체인에서 재현된 7가지 사고")}
{table(
    ["도구 체인", "재현된 사고", "Lancet의 해결 방식"],
    [
        ["<code>rg -n</code> → <code>sed -i</code> 2회", "두 번째 수정이 밀린 줄을 고치고 종료 <code>0</code>", "<code>edit --edits</code>가 모든 hunk를 원본 줄 번호로 해석"],
        ["에이전트 4개: 읽기 → 수정 → 쓰기", "<strong>800건 중 600건</strong> 조용히 유실", "<code>edit --rev</code>: <strong>800건 중 0건</strong> 유실; 패자는 <code>conflict</code>"],
        ["낡은 hunk 포함 <code>patch -p1</code>", "A 파일만 바뀌고 B는 거부: 반쪽 패치", "<code>patch</code>: 전 파일 사전 검증 + 경합 시 롤백"],
        ["<code>sed</code> 심볼 이름 변경", "주석·문자열까지 <strong>0.3~11.7%</strong> 오염", "<code>refs</code>/<code>ast</code>는 코드만 대상; <code>rename</code>은 로컬 스코프 증명"],
        ["<code>.docx</code>에 <code>rga</code> → <code>pandoc</code>", "검색 줄과 읽기 줄 <strong>40개 중 0개</strong> 일치", "<strong>40개 중 40개</strong>: 검색·읽기가 같은 추출기 사용"],
        ["파일 20개 수정 직후 <code>zoekt</code>", "새 단어 0건, 지운 단어 20건 계속 출력", "<code>auto</code> 모드: 즉시 새 단어 20건, 삭제 0건"],
        ["<code>sed -i</code> / <code>perl -pi</code>", "심볼릭/하드 링크 파손 및 <code>0640</code> 권한 소실", "심볼릭 링크, 다중 하드 링크(<code>nlink &gt; 1</code>), 권한·소유자 보존"],
    ],
)}

{h2("locks-cas", "공유 읽기 잠금 & 배타적 write_if_rev CAS")}
<ul>
  <li><strong><code>read_shared</code>:</strong> 공유 OS 잠금(<code>lock_shared()</code>)을 잡고 <code>same_file(&amp;f, path)</code>로 파일 교체를 검증한 뒤(Windows 공유 경합 시 최대 16×5 ms 재시도), 16자리 16진수 <code>rev = XXH3-64(bytes)</code>를 계산합니다.</li>
  <li><strong><code>write_if_rev</code>:</strong> 배타적 OS 잠금(<code>f.lock()</code>) 아래 현재 바이트의 <code>inode</code>와 <code>rev == expect_rev</code>를 재확인하며, 불일치 시 1바이트도 쓰지 않고 <code>WriteError::Conflict {{ now }}</code>(종료 <code>2</code>)를 반환합니다.</li>
</ul>

{h2("atomic-writer", "write_atomic의 파일 정체성 및 충돌 안전성")}
<p>단순한 임시 파일 덮어쓰기(<code>rename</code>)는 심볼릭 링크를 일반 파일로 바꾸고 하드 링크를 끊어 버립니다. <code>src/edit.rs</code>의 <code>write_atomic</code>은 4단계를 지킵니다:</p>
<ol>
  <li><strong>심볼릭 링크 먼저 따라가기:</strong> <code>fs::canonicalize(path)</code>로 실제 대상 경로를 얻어 심볼릭 링크 자체는 그대로 유지합니다.</li>
  <li><strong>임시 파일 <code>fsync</code> &amp; 속성 복사:</strong> 고유한 <code>.&#123;name&#125;.lancet-tmp.&#123;pid&#125;.&#123;seq&#125;</code>에 먼저 기록하고 <code>f.sync_all()</code>까지 마친 뒤(디스크가 꽉 차면 원본을 건드리기 전에 실패) 권한과 Unix <code>uid</code>/<code>gid</code>를 복사합니다.</li>
  <li><strong>다중 하드 링크 보존:</strong> <code>nlink() &gt; 1</code>이면 rename 대신 기존 inode를 제자리(in-place)에서 덮어쓰고 <code>sync_all()</code>해 다른 하드 링크 경로가 낡은 내용을 보지 않게 합니다.</li>
  <li><strong>원자적 공개 + 디렉터리 동기화:</strong> 단일 링크 파일은 <code>fs::rename</code> 후 <code>sync_dir(parent)</code>를 호출해 전원 차단 중에도 반쪽 파일을 남기지 않습니다.</li>
</ol>
"""


def _benchmarks(d: str) -> str:
    return f"""
<p>Apple M4 Pro(arm64 macOS), 웜 캐시, <code>hyperfine -N</code> 5회 중앙값 기준입니다. <code>auto</code>, <code>--trust-index</code>, <code>--no-index</code>의 모든 텍스트 출력은 <strong><code>ripgrep 15.2</code>와 바이트 단위로 대조</strong>했습니다.</p>

{diagram(f"{d}/bench-index-footprint.ko.svg", "8개 코드베이스에서 lancet과 zoekt의 인덱스 빌드 시간, 디스크 크기, 피크 메모리.", "zoekt 대비 빌드 4~25배 빠름, 디스크 9~30배 작음, 피크 RAM 3.2~5.0배 적음.")}
{diagram(f"{d}/bench-workload-profile.ko.svg", "LLVM 19.1의 8개 텍스트 쿼리 유형과 8개 코드베이스 AST 검색 성능.", "희귀/흔한 리터럴, -i, -w, 리터럴 정규식, 리터럴 없는 정규식, 0건, -c, AST 포함.")}

{h2("suite-table", "핵심 8개 코드베이스 벤치마크 (SUITE.md)")}
{table(
    ["코퍼스", "파일", "lancet 인덱스 (빌드 · 디스크 · 피크)", "zoekt 인덱스 (빌드 · 디스크 · 피크)", "auto 대 rg", "trust 대 rg", "AST 대 ast-grep"],
    [
        ["Linux v6.12 (C)", "86,680", "3.3 s · 134 MB · 358 MB", "82 s · 2.9 GB · 1.2 GB", "5.8~7.5배", "13~403배", "4.0배"],
        ["LLVM 19.1 (C++)", "146,319", "4.7 s · 122 MB · 280 MB", "53 s · 3.7 GB · 1.3 GB", "6.1~8.0배", "34~786배", "2.3배"],
        ["rust 1.83 (Rust)", "49,964", "1.2 s · 29 MB · 184 MB", "4.9 s · 327 MB · 739 MB", "1.5~9.6배", "2~273배", "1.8배"],
        ["kubernetes 1.31 (Go)", "26,232", "1.0 s · 30 MB · 224 MB", "4.1 s · 537 MB · 1.0 GB", "3.5~5.8배", "9~210배", "1.9배"],
        ["VS Code 1.95 (TS)", "7,708", "0.4 s · 12 MB · 114 MB", "2.4 s · 245 MB · 573 MB", "2.1~3.9배", "4~63배", "2.7배"],
        ["Django 5.1 (Python)", "6,815", "0.2 s · 7 MB · 76 MB", "1.2 s · 95 MB · 302 MB", "2.4~2.9배", "10~60배", "1.4배"],
        ["CPython 3.13 (C/Py)", "4,882", "0.3 s · 10 MB · 116 MB", "3.7 s · 227 MB · 509 MB", "3.0~5.4배", "5~30배", "2.6배"],
        ["Spring 6.1 (Java)", "10,163", "0.3 s · 12 MB · 122 MB", "1.9 s · 146 MB · 395 MB", "2.9~5.3배", "6~95배", "3.1배"],
    ],
    num_cols=[1],
)}

{h2("competitors", "인덱스·에이전트 도구 비교 (COMPETITORS.md)")}
<p>재현 스크립트: <a href="https://github.com/CREE1116/lancet/blob/main/bench/competitors_agents.py"><code>bench/competitors_agents.py</code></a> (<a href="https://github.com/CREE1116/lancet/blob/main/bench/results/COMPETITORS.md">전체 원시 표</a>).</p>

{h3("comp-freshness", "수정 직후 0 ms 최신성 & 정확도 (Django .py 20개 수정)")}
{table(
    ["도구 / 모드", "새 토큰 탐지 (0 ms)", "삭제 토큰 잔존 (0 ms)", "20/20 rg 줄 수 일치"],
    [
        ["<code>lancet rg</code> (<code>auto</code>)", "<strong>20 / 20</strong>", "0 / 20", "<strong>20 / 20</strong>"],
        ["<code>lancet rg --trust-index</code>", "0 / 20", "0 / 20", "<strong>20 / 20</strong>"],
        ["<code>grix</code> (auto)", "<strong>20 / 20</strong>", "0 / 20", "19 / 20 (LLVM 리터럴 없는 정규식 2줄 누락)"],
        ["<code>tgrep</code> (서버리스)", "0 / 20", "0 / 20", "<strong>20 / 20</strong>"],
        ["<code>tgrep serve</code> (상주 데몬)", "<strong>20 / 20</strong>", "0 / 20", "<strong>20 / 20</strong>"],
        ["<code>aft</code> (상주 stdio + 워처)", "0 / 20 (~1.5 s 대기 필요)", "0 / 20", "일부 파일 누락 (Rust 49,031 vs 49,964)"],
    ],
)}

{h3("comp-honest", "정직한 요약: 우위와 열위")}
{callout("note", "양쪽 숫자를 가감 없이 확인하세요", "<ul><li><strong>Lancet이 앞서는 영역:</strong> 데몬 없는 20/20 0 ms 최신성; <code>aft</code>/<code>tgrep</code> 대비 <strong>1/3~1/7 인덱스 디스크 크기</strong>(Linux 141.8 MB vs 368.5 MB / 955.2 MB); <code>grix</code> 대비 <strong>약 1/3 피크 빌드 RAM</strong>(Linux 353.5 MB vs 1,001.6 MB); Django·Rust·Linux·LLVM 모두에서 가장 빠른 리터럴 AST 검색; 읽기·편집·패치·이름 변경까지 단일 바이너리 통합.</li><li><strong>상주 데몬이나 trust 전용 도구가 더 빠른 영역:</strong> 상주 데몬 <code>tgrep serve</code>는 Rust(119.2 ms vs 508.7 ms)·LLVM(19.9 ms vs 295.8 ms) <code>regex with literal</code>과 소형 리터럴 없는 정규식(Django 15.4 ms, Rust 60.9 ms)에서 데몬 없는 <code>lancet auto</code>보다 빠릅니다. <code>grix --no-auto-index</code>는 Linux 희귀 리터럴에서 <code>lancet --trust-index</code>보다 빠릅니다(11.5 ms vs 43.6 ms). 상주 <code>aft</code>(RAM 587~1,260 MB)는 Rust 희귀 리터럴(8.5 ms vs 73.5 ms)에서 앞섭니다. 리터럴 없는 정규식과 AST 패턴은 각각 <code>rg</code>(최대 1.4배 느림), <code>ast-grep</code>(0.6~1.0배) 수준입니다.</li></ul>")}
"""


PAGES_KO_PART2 = [
    {
        "path": "guides/edit.html",
        "group": "edit",
        "nav_title": "안전한 편집 & CAS",
        "title": "줄·심볼·텍스트 찾기 & 다중 구간 CAS 편집",
        "description": "lancet edit --lines, --symbol, --find, 원자적 --edits, XXH3-64 --rev CAS, Tree-sitter 구문 게이트.",
        "lede": "여러 구간을 고쳐도 줄 번호가 밀리지 않고, 여러 에이전트가 동시에 수정해도 덮어쓰기가 발생하지 않습니다.",
        "keywords": "edit edits find symbol lines insert-before insert-after rev rev_after conflict cas allow-syntax-errors",
        "source": f"{REPO_DOCS}/agent-instructions.md",
        "render": _edit,
    },
    {
        "path": "guides/patch.html",
        "group": "edit",
        "nav_title": "원자적 다중 파일 패치",
        "title": "원자적 다중 파일 패치 (lancet patch)",
        "description": "git diff/diff -u 패치의 정확한 hunk 재배치, rename/권한 지원, 경합 시 롤백.",
        "lede": "반쪽만 적용된 패치를 남기지 않고 모든 파일과 hunk를 사전 검증한 뒤 원자적으로 반영합니다.",
        "keywords": "patch unified diff git diff -p strip root rename mode 롤백 binary copy",
        "source": "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md",
        "render": _patch,
    },
    {
        "path": "guides/rename.html",
        "group": "edit",
        "nav_title": "로컬 스코프 이름 변경",
        "title": "스코프 증명 기반 로컬 이름 변경 (lancet rename)",
        "description": "Rust, Python, JS, TS, TSX, Go에서 2패스 스코프·섀도잉 검증으로 로컬 바인딩만 안전하게 변경.",
        "lede": "모호하거나 파일 경계를 넘는 심볼은 추측 대신 거절하고, 섀도잉·변수 캡처가 없음을 증명한 뒤에만 기록합니다.",
        "keywords": "rename 로컬 스코프 python 매개변수 섀도잉 캡처 comment_mentions refs 폴백",
        "source": "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md",
        "render": _rename,
    },
    {
        "path": "guides/agents-mcp.html",
        "group": "edit",
        "nav_title": "AI 에이전트 · MCP · 스킬",
        "title": "AI 에이전트, 내장 MCP 서버 & 공식 스킬",
        "description": "Claude Code, Cursor, Codex, Antigravity, Claude Desktop용 lancet mcp 설정과 skills/lancet 설치.",
        "lede": "--read-only, --root 지원 줄바꿈 JSON-RPC stdio 서버로 11개 도구를 연결하고 공식 스킬을 설치합니다.",
        "keywords": "mcp server root read-only claude cursor codex antigravity skill agents.md max_output_bytes prompts",
        "source": f"{REPO_DOCS}/mcp.md",
        "render": _agents_mcp,
    },
    {
        "path": "concepts/index-and-freshness.html",
        "group": "internals",
        "nav_title": "인덱스(LANCET04) & 최신성",
        "title": "LANCET04 트라이그램 LSM 인덱스와 최신성 모델",
        "description": "LANCET04 바이너리 포맷, 제로카피 mmap, 건전한 트라이그램 컴파일, 4종 메타데이터, racy 창, 컴팩션.",
        "lede": "상주 데몬 없이도 40바이트 헤더 O(1) mmap과 racy-timestamp 방어로 Scan == Auto를 보장하는 원리.",
        "keywords": "lancet04 segment mmap 트라이그램 q-gram mtime ctime inode racy window flag_racy compact windows",
        "source": f"{REPO_DOCS}/why-one-tool.ko.md",
        "render": _index_freshness,
    },
    {
        "path": "concepts/write-safety.html",
        "group": "internals",
        "nav_title": "쓰기 안전성 & 파일 정체성",
        "title": "좌표 통일, OS 파일 잠금 & 파일 정체성 보존",
        "description": "7가지 실패 사례 해결, read_shared, write_if_rev 배타적 잠금, write_atomic, 심볼릭·다중 하드 링크 보존.",
        "lede": "src/edit.rs가 심볼릭 링크, nlink > 1 하드 링크, 권한, 소유자, 크래시 안전 쓰기를 보장하는 내부 구조.",
        "keywords": "실패사례 write_atomic write_if_rev read_shared lock_shared 하드링크 nlink 심볼릭링크 uid gid sync_all",
        "source": f"{REPO_DOCS}/why-one-tool.ko.md",
        "render": _write_safety,
    },
    {
        "path": "concepts/benchmarks.html",
        "group": "internals",
        "nav_title": "벤치마크 & 경쟁 비교",
        "title": "벤치마크 & 정직한 경쟁 도구 비교",
        "description": "8개 코드베이스 SUITE.md 수치(rg/zoekt/ast-grep) 및 COMPETITORS.md(tgrep/grix/aft/tensor-grep/probe) 결과.",
        "lede": "실제 저장소에서 ripgrep, zoekt, ast-grep, tgrep, grix, aft, probe와 비교해 앞서는 점과 뒤지는 점을 모두 공개합니다.",
        "keywords": "벤치마크 suite competitors zoekt tgrep grix aft tensor-grep probe linux llvm rust 최신성",
        "source": "https://github.com/CREE1116/lancet/blob/main/bench/results/COMPETITORS.md",
        "render": _benchmarks,
    },
]
