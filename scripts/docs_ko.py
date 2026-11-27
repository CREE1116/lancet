"""Korean documentation pages for Lancet (Diátaxis IA)."""

from __future__ import annotations

from docs_ui import callout, cards, code, diagram, h2, h3, table

REPO_DOCS = "https://github.com/CREE1116/lancet/blob/main/docs"


def _intro(_d: str) -> str:
    return f"""
<p><strong>Lancet</strong>은 파일 탐색(<code>fd</code>), 트라이그램 인덱스 기반 검색(<code>rg</code> / LQL), 24개 언어 구조적 AST 검색·치환(<code>ast</code>), 범위·문서 정밀 열람(<code>read</code> / <code>outline</code>), compare-and-swap 편집(<code>edit</code> / <code>patch</code> / <code>rename</code>), 내장 stdio MCP 서버(<code>mcp</code>)를 바이너리 하나에 담은 Rust 도구입니다.</p>

{callout("tip", "시작 전 핵심 규칙 하나", "<p>모든 검색 결과, AST 매치, 읽기 조각, 편집 구간이 <strong>1부터 시작하는 동일한 줄 번호</strong>와 <strong>XXH3-64 <code>rev</code> 해시</strong>를 공유합니다. PDF, 오피스, HWP/HWPX, 압축 파일 내부조차 검색과 <code>read</code>가 같은 줄 번호를 씁니다.</p>",)}

{h2("install", "설치")}
<p>macOS, Linux(glibc 및 정적 musl), Windows용 사전 빌드 바이너리를 제공합니다. Rust 툴체인 없이 바로 설치할 수 있습니다.</p>

{code("macOS / Linux (Homebrew)", "brew tap CREE1116/lancet https://github.com/CREE1116/lancet\nbrew install CREE1116/lancet/lancet", "ko")}
{code("macOS / Linux (릴리스 설치 스크립트)", "curl -fsSL https://raw.githubusercontent.com/CREE1116/lancet/main/install.sh | sh", "ko")}
{code("Windows (PowerShell)", "irm https://raw.githubusercontent.com/CREE1116/lancet/main/install.ps1 | iex", "ko")}
{code("소스 빌드 (Cargo)", "# 전체 바이너리 (~40 MB, Tree-sitter 24개 언어 + PDF/오피스/HWP 어댑터)\ncargo binstall --git https://github.com/CREE1116/lancet lancet\n\n# 텍스트·코드 전용 경량 바이너리 (~4 MB, 문서 어댑터 제외)\ncargo install --git https://github.com/CREE1116/lancet --no-default-features", "ko")}

{h2("walkthrough", "60초 빠른 시작")}
<p><code>.lancet</code> 인덱스는 선택 사항입니다. 인덱스가 없어도 동일하게 동작하며, 기본 <code>auto</code> 모드는 매 쿼리마다 파일 변경 여부를 확인합니다.</p>
{code("터미널", "# 1. 선택: 로컬 트라이그램 인덱스 한 번 빌드\nlancet index .\n\n# 2. 폴더 구조 먼저 확인 후 참조 빈도순 핵심 심볼 찾기\nlancet fd --tree --dirs -d 2 .\nlancet refs --rank '*Service' --kind class src --limit 8\n\n# 3. ripgrep 문법 또는 LQL 논리·근접 쿼리로 검색\nlancet rg -w execute_one src\nlancet 'near(\"gradient\", \"clip\", 30) in function | expand parent'\n\n# 4. 필요한 함수/범위만 읽고 `rev` 확보\nlancet read src/search.rs --symbol execute_one --json\n\n# 5. dry-run diff 확인 후 --rev와 --apply로 원자적 반영\nlancet edit src/search.rs --find 'let limit = 20;' --with 'let limit = 50;' --rev <REV> --apply", "ko")}

{h2("cheat-sheet", "한 화면 치트시트")}
{table(
    ["작업", "명령어", "보장"],
    [
        ["폴더 레이아웃", "<code>lancet fd --tree --dirs -d 2 .</code>", "<code>.gitignore</code> 반영"],
        ["최근/확장자 필터", "<code>lancet fd -t f -e rs --changed-within 7d</code>", "인덱스 가속"],
        ["rg 그대로 쓰기", "<code>lancet rg -w -t rs 'pat' src</code>", "<code>rg</code>와 바이트 동일 + <code>--docs</code>"],
        ["논리·유사 검색", "<code>lancet '(\"a\" OR \"b\") NOT \"c\" in function'</code>", "<code>Scan == Auto</code>"],
        ["파일 뼈대 보기", "<code>lancet outline src/main.rs</code>", "함수·마크다운 제목·시트·압축 목록"],
        ["토큰 제한 읽기", "<code>lancet read f.rs --lines 10:50 --budget-tokens 800</code>", "<code>rev</code> + <code>next_line</code> 반환"],
        ["심볼 단위 읽기", "<code>lancet read f.rs --symbol my_fn</code>", "문서 주석·어트리뷰트 포함"],
        ["문서·압축파일", "<code>lancet read r.pdf -p 2:3</code> · <code>a.zip!app.log</code>", "검색과 동일한 줄 번호"],
        ["참조 랭킹", "<code>lancet refs --rank '*Service' --kind class src</code>", "어휘 참조 횟수 내림차순"],
        ["AST 검색·치환", "<code>lancet ast '$X.unwrap()' src -l rs -r '$X?'</code>", "24개 언어, <code>--apply</code> 전엔 dry-run"],
        ["CAS 다중 편집", "<code>lancet edit f.rs --edits - --rev REV --apply</code>", "원본 좌표 유지, 전부 아니면 무효"],
        ["Unified Diff", "<code>git diff | lancet patch --apply</code>", "사전 검증 + 충돌 시 롤백"],
        ["로컬 이름 변경", "<code>lancet rename f.rs:42 old new --rev REV --apply</code>", "스코프 재해석으로 캡처 차단"],
        ["MCP 서버", "<code>lancet mcp --root .</code>", "11개 도구 stdio JSON-RPC"],
        ["인덱스 점검", "<code>lancet explain 'pat'</code> · <code>lancet status</code>", "필요 트라이그램·후보 비율 출력"],
    ],
)}

{h2("explore-docs", "문서 둘러보기")}
{cards([
    ("시작하기", "기존 도구에서 전환하기", "rg, fd, ast-grep, sed, patch, rga 사용자가 바로 옮겨 탈 수 있는 대조표와 의도적 차이점.", "migration.html"),
    ("실전 가이드", "낯선 코드베이스 탐색", "fd --dirs, refs --rank, outline, batch, budget-tokens 페이지네이션으로 토큰 절약하기.", "guides/explore.html"),
    ("실전 가이드", "안전한 편집·패치·로컬 이름 변경", "--edits로 줄 밀림 방지, --rev 동시성 제어, 다중 파일 patch 및 로컬 스코프 rename.", "guides/editing.html"),
    ("레퍼런스", "CLI·LQL·JSON/MCP 전체 명세", "전체 플래그, LQL EBNF 문법, 24개 AST 언어, MCP 11개 도구 및 오류 복구표.", "reference/cli.html"),
])}
"""


def _migration(_d: str) -> str:
    return f"""
<p>이미 <code>ripgrep</code>, <code>fd</code>, <code>ast-grep</code>, <code>sed</code>, <code>patch</code>, <code>rga</code>에 익숙하다면 한 번에 다 바꿀 필요가 없습니다. Lancet은 기존 플래그를 그대로 받는 호환 진입점과 단계 간 이음새 오류를 없애는 안전장치를 함께 제공합니다.</p>

{h2("from-rg", "ripgrep(rg)에서 오셨다면")}
<p><code>lancet rg</code>는 ripgrep의 플래그, Rust <code>regex</code> 엔진, <code>.gitignore</code> / <code>.ignore</code> / <code>.rgignore</code> 규칙, 종료 코드(<code>0</code> 매치, <code>1</code> 없음, <code>2</code> 오류), 출력 형식을 그대로 구현했습니다(ripgrep 15.2와 바이트 단위 검증).</p>
{table(
    ["ripgrep 명령", "lancet 대안", "참고"],
    [
        ["<code>rg foo src</code>", "<code>lancet rg foo src</code>", "줄 번호가 기본 ON; 끄려면 <code>-N</code>"],
        ["<code>rg -i -w -t py tok</code>", "<code>lancet rg -i -w -t py tok</code>", "<code>-i</code>/<code>-s</code>/<code>-S</code>, <code>-w</code>, <code>--type</code> 규칙 동일"],
        ["<code>rg -g '*.toml' -C 2 pat</code>", "<code>lancet rg -g '*.toml' -C 2 pat</code>", "<code>--passthru</code>, <code>--type-add</code>, <code>-L</code>, <code>--max-depth</code> 지원"],
        ["<code>rg --files -t rs</code>", "<code>lancet rg --files -t rs</code>", "경로순으로 결정적 정렬 출력"],
        ["<code>rg -P '(?&lt;=a)b'</code>", "<code>lancet 'before(\"a\", \"b\", 1)'</code> 또는 Rust regex", "<code>-P</code> / PCRE2는 의도적으로 미지원(선형 시간 + 건전한 트라이그램 컴파일 보장)"],
        ["<code>rg --pre pdftotext pat</code>", "<code>lancet rg --docs pat</code>", "외부 프로세스 없이 인프로세스로 PDF/오피스/HWP/압축 검색"],
    ],
)}

{callout("note", "lancet rg의 의도적 차이 3가지", "<ul><li><strong>항상 경로순 정렬:</strong> <code>rg --sort path</code>와 동일하며 스레드 수와 무관하게 결정적입니다.</li><li><strong>줄 번호(<code>-n</code>) 기본 활성화:</strong> 파이프 출력에서도 켜져 있으며 <code>-N</code>으로 끕니다.</li><li><strong>파일명 헤더 없음:</strong> 항상 <code>path:line:text</code> 형식으로 출력하며 <code>--color auto</code>는 파이프 시 색상을 끕니다.</li></ul>",)}

{h2("from-fd", "fd / find에서 오셨다면")}
{table(
    ["fd / find", "lancet fd", "차이점 및 확장 기능"],
    [
        ["<code>fd -e rs -d 2 src</code>", "<code>lancet fd -e rs -d 2 src</code>", "<code>.lancet</code> 인덱스가 있으면 바로 활용"],
        ["<code>fd -t d -d 2</code>", "<code>lancet fd --tree --dirs -d 2 .</code>", "파일 없이 폴더 트리만 깔끔하게 출력"],
        ["<code>find . -mtime -7</code>", "<code>lancet fd --changed-within 7d</code>", "단위: <code>s</code>, <code>m</code>, <code>h</code>, <code>d</code>, <code>w</code>"],
        ["<code>find . -name '*.pdf'</code>", "<code>lancet fd --type-class document</code>", "내장 분류: <code>code</code>, <code>document</code>, <code>spreadsheet</code>, <code>archive</code> 등"],
        ["<code>fd pat dir1 dir2</code>", "경로마다 호출하거나 LQL <code>path:</code> 사용", "<code>lancet fd</code>는 <code>[PATH]</code> 1개만 받음; 패턴이 경로처럼 보이면 <code>-p pat</code> 사용"],
    ],
)}

{h2("from-ast", "ast-grep(sg)에서 오셨다면")}
{table(
    ["ast-grep", "lancet ast", "중요한 차이"],
    [
        ["<code>sg -p '$X.unwrap()' -l rs</code>", "<code>lancet ast '$X.unwrap()' -l rs</code>", "패턴에 리터럴이 있으면 <code>.lancet</code> 트라이그램 프리필터 가속"],
        ["<code>sg -p 'f($$$A)' -r 'g($$$A)' -U</code>", "<code>lancet ast 'f($$$A)' -r 'g($$$A)' --apply</code>", "기본은 dry-run diff; <code>--apply</code>일 때만 원자적 기록"],
        ["<code>sg -p 'if ($C) $S'</code>", "<code>lancet ast 'if ($C) $S'</code>", "<strong>의미 차이:</strong> Lancet은 모양을 엄격히 매치함(<code>if-else</code>는 별개 구문 노드)"],
        ["<code>sg -l cpp ...</code>", "<code>lancet ast ... -l cpp</code>", "Lancet <code>--lang cpp</code>는 <code>.cpp</code>/<code>.cc</code> 외에 <code>.h</code> 헤더도 함께 스캔"],
    ],
)}

{h2("from-sed-patch", "cat / sed / patch / rga에서 오셨다면")}
{table(
    ["기존 도구 사슬", "Lancet 대체", "해결된 실패 사례"],
    [
        ["<code>sed -n '10,50p' f.rs</code>", "<code>lancet read f.rs --lines 10:50</code>", "XXH3-64 <code>rev</code> 반환 + <code>--budget-tokens</code> 상한 지원"],
        ["하나의 <code>rg</code> 결과로 <code>sed -i</code> 2회", "<code>lancet edit f.rs --edits - --rev REV --apply</code>", "모든 hunk가 원본 줄 번호 기준; 심볼릭/하드 링크·권한 유지"],
        ["<code>patch -p1 &lt; diff</code>", "<code>lancet patch diff --apply</code>", "모든 파일 사전 검증, 충돌 시 롤백(반쪽 패치 없음)"],
        ["<code>rga</code> + <code>pdftotext</code>/<code>pandoc</code>", "<code>lancet 'pat'</code> + <code>lancet read f.pdf -p 2</code>", "단일 추출 파이프라인으로 줄 번호 100% 일치"],
    ],
)}

{h2("when-not", "전문 도구를 그대로 써야 할 때")}
{callout("warn", "적용 범위 경계", "<ul><li><strong>버전 관리:</strong> <code>git</code>은 그대로 사용하세요. Lancet은 작업 트리를 검색하고(<code>.gitignore</code> 준수) <code>git diff</code>를 <code>lancet patch</code>로 안전하게 적용합니다.</li><li><strong>여러 파일에 걸친 타입 기반 리팩터링:</strong> 공개 API, 구조체 필드, 메서드 이름 일괄 변경은 LSP(<code>rust-analyzer</code>, <code>tsserver</code>, <code>gopls</code>)를 사용하세요. Lancet은 어휘 기반 <code>refs</code>와 단일 파일 스코프 증명 기반 <code>lancet rename</code>을 제공합니다.</li></ul>",)}
"""


def _explore(_d: str) -> str:
    return f"""
<p>개발자나 AI 에이전트가 컨텍스트 토큰을 낭비하고 길을 잃는 가장 흔한 원인은 파일 전체를 닥치는 대로 읽는 것입니다. Lancet은 수만 개 파일에서 정확히 필요한 줄까지 좁혀 내려가는 <strong>5단계 탐색 깔때기</strong>를 제공합니다.</p>

{h2("step1", "1. 파일 소음 없이 폴더 구조 먼저 파악하기")}
<p>파일 목록이나 검색을 돌리기 전에 디렉터리 골격부터 확인합니다.</p>
{code("터미널", "lancet fd --tree --dirs --max-depth 2 .\nlancet fd --changed-within 2d -t f src", "ko")}
<p><code>--tree</code>에 <code>--dirs</code>(<code>-t d</code>와 동일)를 붙이면 폴더만 계층으로 보여 주어 프로젝트 아키텍처를 수십 줄 안에 파악할 수 있습니다.</p>

{h2("step2", "2. refs --rank로 핵심 선언 순위 찾기")}
<p>낯선 저장소에서는 어떤 클래스·구조체·인터페이스·함수가 실제로 많이 쓰이는지 어휘 참조 수 순위로 바로 뽑아낼 수 있습니다.</p>
{code("터미널", "# 가장 많이 참조되는 *Service 클래스 상위 10개\nlancet refs --rank '*Service' --kind class src --limit 10\n\n# 디렉터리 내 가장 많이 호출되는 함수 상위 10개\nlancet refs --rank '*' --kind function --limit 10 src\n\n# 정의와 참조 위치 상세 조회 (주석·문자열 자동 제외)\nlancet refs SearchOptions src --lang rs --json", "ko")}
{callout("note", "--rank의 중복 이름 표시 이해하기", "<p>글롭에 리터럴(예: <code>Service</code>)이 포함되면 트라이그램 인덱스로 후보를 좁힌 뒤 AST 선언을 수집합니다. 출력에 <code>(name shared by N declarations)</code> 또는 JSON의 <code>definitions &gt; 1</code>이 표시되면 여러 선언이 같은 이름을 공유한다는 뜻입니다.</p>",)}

{h2("step3", "3. 본문 전에 뼈대만 보기 (outline)")}
<p><code>lancet outline</code>으로 코드 선언 시그니처, 마크다운 제목 목차, 엑셀 시트 목록, 압축 파일 내부 경로와 정확한 줄 범위를 먼저 확인하세요.</p>
{code("터미널", "lancet outline src/search.rs\nlancet outline docs/architecture.md\nlancet outline financial_model.xlsx\nlancet outline logs/bundle.zip", "ko")}

{h2("step4", "4. 여러 쿼리 한 번에 실행(batch) & 파이프 정제")}
<p>서로 관련된 여러 질문을 던져야 할 때는 <code>lancet batch</code>로 인덱스와 디렉터리 순회를 <strong>딱 1번만</strong> 열고 모든 쿼리를 평가합니다. LQL 파이프로 출력을 더 줄일 수 있습니다.</p>
{code("터미널", "# 인덱스 열기·워크 1회 공유로 쿼리 3개 동시 실행\nlancet batch src \\\n  -q 'near(\"retry\", \"backoff\", 20) in function' \\\n  -q '\"Conflict\" in ext:rs' \\\n  -q 'fuzzy(\"idempotent\", 1) | files'\n\n# 동시 출현 상위 키워드 추출 및 토큰 상한 적용\nlancet '\"trigram\" in ext:rs | terms top:10 | budget tokens:1200' src", "ko")}

{h2("step5", "5. 토큰 예산(budget-tokens) 기반 페이지네이션 읽기")}
<p>전체 파일 대신 해당 심볼(문서 주석·어트리뷰트 포함)이나 토큰 상한을 지정해 읽으세요. 잘렸다면 반환된 <code>next_line</code>부터 이어서 읽습니다.</p>
{code("터미널", "lancet read src/search.rs --symbol execute_one\nlancet read src/search.rs --lines 100:400 --budget-tokens 800 --json", "ko")}
{callout("tip", "실측 컨텍스트 절감", "<p><code>why-one-tool</code> 실험에서 줄·심볼·토큰 예산을 지정해 읽은 경우 전체 파일 읽기보다 컨텍스트 사용량이 <strong>5~30배</strong> 줄었으며, 안전한 편집에 필요한 줄 번호와 <code>rev</code> 토큰도 그대로 유지했습니다.</p>",)}
"""


def _editing(d: str) -> str:
    return f"""
<p>줄 번호를 수동으로 계산해 <code>sed</code>를 여러 번 돌리거나 잠금 없이 파일을 덮어쓰면, 줄 밀림이나 에이전트 간 경합으로 변경 사항이 소리 없이 사라집니다. Lancet은 모든 쓰기를 <strong>공유 좌표계 위의 compare-and-swap(CAS)</strong>으로 처리합니다.</p>

{diagram(f"{d}/coordinates.ko.svg", "검색 결과, 읽기 범위, 심볼, 리비전(rev), 편집이 하나의 좌표계를 공유합니다.", "모든 <code>read</code>가 반환하는 줄 번호와 XXH3-64 <code>rev</code>를 <code>edit</code>과 <code>rename</code>이 그대로 사용합니다.")}

{h2("single-hunk", "단일 구간 편집 (--lines, --symbol, --find)")}
<p><strong>대상 하나</strong>(<code>--lines</code>, <code>--symbol</code>, <code>--find</code> 중 택1)와 <strong>연산 하나</strong>(<code>--with</code>, <code>--with-file</code>, <code>--delete</code>, <code>--insert-before</code>, <code>--insert-after</code> 중 택1)를 고릅니다. 기본은 디스크를 건드리지 않는 dry-run diff이며 <code>--apply</code>를 붙일 때만 반영됩니다.</p>
{code("터미널", "# read에서 본 줄 번호 그대로 수정 (--rev로 CAS 검증)\nlancet edit src/net.rs --lines 12:15 --with '    timeout_ms: 5000,' --rev e70d5a898f122a62 --apply\n\n# 심볼 전체 교체 (주의: 위쪽 문서 주석과 어트리뷰트도 범위에 포함됨!)\nlancet edit src/net.rs --symbol retry_limit --with-file new_fn.rs --rev <REV> --apply\n\n# 고유 문자열 치환 (LF/CRLF 모두 매치; 전체 치환은 --all 추가)\nlancet edit src/net.rs --find 'let retries = 3;' --with 'let retries = 5;' --rev <REV> --apply", "ko")}
{callout("warn", "--symbol 범위 주의", "<p><code>edit --symbol</code>은 <code>read --symbol</code>이 보여 주는 것과 똑같이 선언 바로 위의 <code>///</code> 문서 주석과 <code>#[attr]</code> 어트리뷰트까지 포함합니다. 유지하려면 새 내용에 함께 넣으세요.</p>",)}

{h2("multi-hunk", "원자적 다중 구간 편집 (--edits)")}
<p>한 파일에서 여러 곳을 고칠 때는 절대 줄 번호 기반 호출을 따로 이어 붙이지 말고, 하나의 <code>--edits</code> JSON 배열(파일 또는 stdin <code>-</code>)로 묶어 보내세요. 모든 항목은 <strong>처음 읽은 원본 파일의 줄 번호</strong>를 쓰며, 겹침이 있으면 거절되고 전부 성공하거나 한 줄도 쓰지 않습니다.</p>
{code("터미널", "lancet edit src/net.rs --rev e70d5a898f122a62 --apply --edits - <<'JSON'\n[\n  {\"lines\": \"10:12\", \"with\": \"const RETRIES: u32 = 5;\"},\n  {\"symbol\": \"legacy_helper\", \"delete\": true},\n  {\"find\": \"old_metric(\", \"with\": \"new_metric(\", \"all\": true},\n  {\"lines\": \"40\", \"insert_after\": \"    tracing::info!(\\\"updated\\\");\"}\n]\nJSON", "ko")}

{h2("cas-concurrency", "멀티 에이전트 동시성과 conflict 복구")}
{diagram(f"{d}/concurrent-edit.ko.svg", "에이전트 4개가 동시에 한 파일을 고칠 때 일반 쓰기는 800개 중 600개 유실, lancet --rev는 0개 유실.", "배타적 OS 파일 잠금 아래 <code>inode</code>와 <code>rev</code>를 재검증하며, 낡은 리비전은 <code>conflict</code>(종료 2)를 내고 1바이트도 쓰지 않습니다.")}
<ol>
  <li><strong>읽고 <code>rev</code> 보관:</strong> <code>lancet read file --json</code>에서 <code>rev</code>를 얻습니다.</li>
  <li><strong><code>--rev</code>로 반영:</strong> 그사이 다른 에이전트가 먼저 쓰면 <code>status: "conflict"</code>(종료 코드 <code>2</code>)로 거절됩니다.</li>
  <li><strong>연쇄 적용 또는 복구:</strong> 성공 시 반환된 <code>rev_after</code>를 다음 편집에 넘깁니다. <code>conflict</code>가 나면 <code>lancet read</code>로 최신 내용을 다시 읽고 새 <code>rev</code>로 재시도하세요. 충돌을 피하려고 <code>--rev</code>를 빼고 덮어쓰면 안 됩니다.</li>
</ol>
{callout("note", "Tree-sitter 구문 검사 게이트와 파일 정체성", "<ul><li><strong>구문 게이트:</strong> <code>edit</code>, <code>patch</code>, <code>ast --rewrite</code>는 Tree-sitter 구문 오류가 <em>순증가</em>하는 수정을 거부합니다(이미 망가진 파일을 단계적으로 고치는 것은 허용). 의도적인 경우에만 <code>--allow-syntax-errors</code>를 쓰세요.</li><li><strong>파일 정체성:</strong> 심볼릭 링크는 대상 파일에 기록해 링크를 유지하고, 다중 하드 링크 파일은 제자리 갱신하며 권한·소유자를 보존합니다.</li></ul>",)}

{h2("patch", "다중 파일 Unified Diff 적용 (lancet patch)")}
<p><code>lancet patch</code>는 <code>git diff</code> 또는 <code>diff -u</code>를 여러 파일에 걸쳐 원자적으로 적용합니다.</p>
{code("터미널", "# Dry-run 사전 검증\nlancet patch changes.diff\n\n# stdin으로 받아 하위 디렉터리에 적용 (-p 1이 기본으로 a/, b/ 제거)\ngit diff | lancet patch --root subdir -p 1 --apply", "ko")}
<p>이름 변경(<code>git diff -M</code>), 권한 변경(<code>chmod +x</code>), 빈 파일 생성·삭제를 처리합니다. 모든 hunk는 정확히 일치해야 하며(줄 위치가 밀렸을 때는 고유한 최근접 정확 일치 위치로만 재배치, 퍼지 매칭 안 함), 전체 파일을 사전 검증한 뒤 쓰기 경합 발생 시 이전 파일까지 롤백합니다. 바이너리 diff와 복사는 거부합니다.</p>

{h2("rename", "스코프 증명 기반 로컬 이름 변경 (lancet rename)")}
<p><code>lancet rename</code>은 한 파일 내부의 지역 바인딩(변수, 루프 변수, 클로저 매개변수, Python 제외 함수 매개변수)을 <strong>6개 언어</strong>(Rust, Python, JavaScript, TypeScript, TSX, Go)에서 안전하게 바꿉니다.</p>
{code("터미널", "# 해당 줄에 OLD 이름의 바인딩이 하나뿐이면 COL 생략 가능\nlancet rename src/search.rs:42 old_name new_name\nlancet rename src/search.rs:42:9 old_name new_name --rev <REV> --apply", "ko")}
{callout("danger", "lancet rename이 의도적으로 거절하는 경우", "<p>로컬 스코프만으로 안전성을 증명할 수 없으면 추측하지 않고 거절합니다: 모듈/최상위·외부 노출 심볼, 구조체 필드, 메서드, <strong>Python 함수 매개변수</strong>(다른 파일에서 키워드 인자로 호출 가능), 매크로 내부, JSX 태그 이름, Go 구조체 리터럴 키, 동적 스코프(<code>eval</code>/<code>with</code>/<code>locals()</code>/<code>global</code>/<code>nonlocal</code>), 그리고 변경 후 스코프를 재해석했을 때 <strong>섀도잉이나 변수 캡처</strong>가 발생하는 경우.</p><p>스코프 내 주석의 언급은 건드리지 않고 <code>comment_mentions</code> 횟수로 보고합니다. 파일 간 또는 최상위 심볼 변경은 <code>lancet refs</code>로 위치를 찾은 뒤 <code>lancet edit --edits</code>나 LSP를 쓰세요.</p>",)}
"""


def _documents(_d: str) -> str:
    return f"""
<p>실제 작업공간에는 소스 코드만 있지 않습니다. 명세서는 PDF나 워드·한글(HWP) 문서에, 설정표는 스프레드시트에, 장애 로그는 zip/tar 압축 안에 있습니다. Lancet은 이를 <strong>외부 도구 없이 인프로세스로 추출</strong>하므로 검색과 <code>lancet read</code>가 정확히 같은 줄 번호를 사용합니다.</p>

{h2("formats", "지원 포맷 및 전용 선택자")}
{table(
    ["포맷 / 확장자", "인프로세스 추출기", "Outline 지원", "Read 선택자"],
    [
        ["<code>.pdf</code>", "Pure-Rust 스트림 추출 (<code>--ocr</code> 선택)", "PDF 목차 / 제목", "<code>--page 2</code> 또는 <code>--page 2:5</code>"],
        ["<code>.docx</code>, <code>.odt</code>, <code>.epub</code>", "XML 본문·문단 추출기", "문서 제목 계층", "<code>--lines START:END</code>"],
        ["<code>.pptx</code>", "슬라이드 XML 텍스트 추출기", "슬라이드 목록", "<code>--page 3</code> 또는 <code>--page 1:4</code>"],
        ["<code>.hwp</code> (5.0 OLE), <code>.hwpx</code>", "한글 문서 네이티브 파서", "섹션 제목", "<code>--lines START:END</code>"],
        ["<code>.xlsx</code>, <code>.xls</code>, <code>.xlsm</code>, <code>.xlsb</code>, <code>.ods</code>", "스프레드시트 셀 추출기", "시트 이름 + 크기", "<code>--sheet NAME|1 --rows 1:30 --cols A:E</code>"],
        ["<code>.zip</code>, <code>.tar</code>, <code>.gz</code>, <code>.tgz</code>, <code>.tar.gz</code>", "스트리밍 압축 리더 (16 MiB 상한)", "내부 경로 목록", "<code>bundle.zip!path/inside.log</code> 또는 <code>--entry</code>"],
    ],
)}

{h2("search-docs", "문서와 압축 파일 검색하기")}
<p>Lancet의 기본 LQL 검색과 <code>ripgrep</code> 호환 모드는 문서 포함 여부에 의도적인 차이가 있습니다.</p>
<ul>
  <li><strong>기본 LQL 검색 (<code>lancet 'query'</code>):</strong> 코드뿐 아니라 문서·스프레드시트·압축 파일까지 자동으로 검색합니다(<code>type:document</code>, <code>type:spreadsheet</code>, <code>type:archive</code> 필터 지원).</li>
  <li><strong><code>lancet rg</code>:</strong> <code>ripgrep</code>과 출력을 바이트까지 맞추기 위해 기본적으로 문서/바이너리를 건너뛰며, <code>--docs</code>를 붙일 때만 인프로세스 문서를 함께 검색합니다.</li>
</ul>
{code("터미널", "# PDF와 HWP 문서 안에서 근접 검색\nlancet 'near(\"계약\", \"해지\", 15) in type:document' contracts/\n\n# ripgrep 문법 그대로 압축파일과 문서까지 검색\nlancet rg --docs -i 'timeout' ./artifacts", "ko")}

{h2("read-docs", "동일 좌표로 문서·압축 파일 읽기")}
{code("터미널", "# 먼저 뼈대/목록 확인\nlancet outline q4_report.xlsx\nlancet outline crash_bundle.tar.gz\n\n# 슬라이드, 셀 범위, 압축 내부 파일 읽기\nlancet read deck.pptx --page 4:6\nlancet read q4_report.xlsx --sheet 1 --rows 1:30 --cols A:E\nlancet read logs/bundle.zip!worker_02/stderr.log --lines 120:180 --budget-tokens 600", "ko")}

{h2("ocr-and-safety", "OCR, 압축 폭탄 방어 및 읽기 전용 보장")}
<ul>
  <li><strong>스캔된 PDF 및 이미지(<code>--ocr</code>):</strong> <code>lancet</code>, <code>read</code>, <code>outline</code>, <code>index</code>에 <code>--ocr</code>을 넘기면 외부 <code>tesseract</code> / <code>pdftoppm</code>으로 텍스트를 추출합니다.</li>
  <li><strong>압축 폭탄 상한:</strong> 압축 파일 멤버와 압축 XML 스트림은 16 MiB 상한으로 제한해 악의적/손상된 파일로 인한 메모리 고갈을 막습니다.</li>
  <li><strong>읽기 전용 보장:</strong> 문서와 압축 항목은 JSON에 <code>rev</code>를 내지 않으며 <code>lancet edit</code>이 수정을 거부합니다.</li>
  <li><strong>문서 어댑터 제외 빌드:</strong> <code>cargo install --no-default-features</code>로 빌드하면 어댑터를 제외한 약 4 MB 순수 텍스트·코드 전용 바이너리가 됩니다.</li>
</ul>
"""


def _agents_mcp(_d: str) -> str:
    return f"""
<p>AI 코딩 에이전트는 셸 CLI로 직접 쓰거나, 의존성 없는 내장 <strong>Model Context Protocol (MCP)</strong> stdio 서버와 <a href="https://github.com/CREE1116/lancet/tree/main/skills/lancet"><code>skills/lancet/</code></a> 공식 스킬 패키지로 연동해 쓸 수 있습니다.</p>

{h2("mcp-server", "내장 MCP 서버 실행 (lancet mcp)")}
<p><code>lancet mcp</code>는 <code>stdin</code>/<code>stdout</code> 위에서 줄바꿈 구분 JSON-RPC 2.0(MCP 버전 <code>2025-03-26</code> / <code>2024-11-05</code>)을 구사하며, 별도 데몬 없이 Lancet의 Rust 엔진을 인프로세스로 즉시 호출합니다.</p>
{table(
    ["플래그", "기본값", "동작"],
    [
        ["<code>--root &lt;DIR&gt;</code>", "<code>.</code>", "도구 호출의 상대 경로 해석 기준 디렉터리 (chroot/샌드박스가 <strong>아님</strong>; CLI처럼 절대 경로나 <code>..</code>도 허용됨)"],
        ["<code>--read-only</code>", "<code>false</code>", "<code>tools/list</code>에서 <code>lancet_edit</code>, <code>lancet_patch</code>, <code>lancet_rename</code>을 숨기고 쓰기(<code>lancet_ast</code> <code>apply</code>, 인덱스 변경)를 거부"],
        ["<code>--trust-index</code>", "<code>false</code>", "개별 호출에서 덮어쓰지 않는 한 기본 <code>index_mode</code>를 <code>trust</code>로 설정"],
        ["<code>-j, --threads &lt;NUM&gt;</code>", "CPU 코어 수", "Rayon 워커 스레드 수"],
    ],
)}

{h2("client-configs", "클라이언트별 설정 예시")}
<p>복사해서 바로 쓸 수 있는 JSON 템플릿은 <a href="https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/mcp-config.json"><code>skills/lancet/references/mcp-config.json</code></a>에도 있습니다.</p>

{h3("cfg-claude", "Claude Code")}
{code("터미널", "# 기본 읽기·쓰기 모드\nclaude mcp add --transport stdio lancet -- lancet mcp --root .\n\n# 읽기 전용 탐색 모드\nclaude mcp add --transport stdio lancet -- lancet mcp --root . --read-only", "ko")}

{h3("cfg-cursor", "Cursor (.cursor/mcp.json 또는 ~/.cursor/mcp.json)")}
{code("JSON", '{\n  "mcpServers": {\n    "lancet": {\n      "command": "lancet",\n      "args": ["mcp", "--root", "."]\n    }\n  }\n}', "ko")}

{h3("cfg-desktop", "Claude Desktop / Windsurf / Antigravity")}
<p>데스크톱 앱이 저장소 밖에서 실행되는 경우 절대 경로로 <code>--root</code>를 지정하세요.</p>
{code("JSON", '{\n  "mcpServers": {\n    "lancet": {\n      "command": "lancet",\n      "args": ["mcp", "--root", "/abs/path/to/your/repo"]\n    }\n  }\n}', "ko")}

{h3("cfg-codex", "OpenAI Codex (~/.codex/config.toml)")}
{code("TOML", '[mcp_servers.lancet]\ncommand = "lancet"\nargs = ["mcp", "--root", "."]', "ko")}

{h2("official-skill", "공식 에이전트 스킬 설치")}
<p><a href="https://github.com/CREE1116/lancet/tree/main/skills/lancet"><code>skills/lancet/</code></a> 패키지는 에이전트에게 LQL 문법, 24개 AST 언어 별칭, <code>rev</code> 기반 CAS 편집 흐름, 기존 도구 대비 선택 기준을 알려 줍니다.</p>
{code("터미널", "# 프로젝트 로컬 (.agents 공통 규약)\nmkdir -p .agents/skills && ln -s \"$(pwd)/skills/lancet\" .agents/skills/lancet\n\n# Claude Code / Codex / Antigravity 개인 설정\nmkdir -p ~/.claude/skills && cp -R skills/lancet ~/.claude/skills/lancet\nmkdir -p ~/.codex/skills  && cp -R skills/lancet ~/.codex/skills/lancet\nmkdir -p ~/.gemini/config/skills && cp -R skills/lancet ~/.gemini/config/skills/lancet", "ko")}

{h2("tools-overview", "11개 도구 & 내장 프롬프트 2종")}
<p>모든 조회·검색 도구는 결과 0건을 정상적인 <code>empty</code>(<code>isError: false</code>)로 처리하며, 쓰기 도구는 기본값이 <code>apply: false</code>(dry-run unified diff)입니다. 전체 파라미터와 스키마는 <a href="../reference/json-and-mcp.html#mcp-tools">JSON · MCP 명세</a>를 참고하세요.</p>
<ul>
  <li><strong>탐색·검색:</strong> <code>lancet_fd</code>, <code>lancet_outline</code>, <code>lancet_search</code>, <code>lancet_rg</code>, <code>lancet_refs</code>(<code>rank: true</code> 포함), <code>lancet_ast</code>, <code>lancet_read</code>.</li>
  <li><strong>안전한 수정:</strong> <code>lancet_edit</code>, <code>lancet_patch</code>, <code>lancet_rename</code>, 그리고 <code>apply: true</code>를 준 <code>lancet_ast</code>.</li>
  <li><strong>인덱스 관리:</strong> <code>lancet_index</code>(<code>status</code>, <code>sync</code>, <code>rebuild</code>, <code>compact</code>).</li>
  <li><strong>내장 프롬프트:</strong> <code>prompts/list</code>의 <code>lancet-explore</code>, <code>lancet-safe-edit</code>.</li>
</ul>
{callout("tip", "AGENTS.md / CLAUDE.md 한 줄 규칙", "<pre style='margin:6px 0 0;white-space:pre-wrap;font-size:12.5px'>탐색·검색·범위 읽기·CAS 편집(read --rev -&gt; edit --apply)에는 rg/fd/ast-grep/cat/sed 대신 lancet(lancet_* MCP 또는 CLI)을 우선 사용한다.</pre>",)}
"""


def _index_freshness(d: str) -> str:
    return f"""
<p>기존 인덱스 기반 검색 도구는 백그라운드 워처가 따라잡을 때까지 낡은 결과를 내거나, 인덱스 내용만으로 근사치를 반환하곤 합니다. Lancet은 반대로 설계했습니다: <strong>인덱스는 열지 않아도 되는 파일만 거릅니다.</strong></p>

{diagram(f"{d}/pipeline.ko.svg", "Linux v6.12에서 인덱스 쿼리가 91%의 파일을 건너뛰면서도 정확한 결과를 보장하는 흐름.", "Linux v6.12(86,218개 파일)에서 <code>\"spin_lock\"</code> <code>auto</code> 모드 실측: 7,814개 후보만 열어 7,073개 정확히 일치.")}

{h2("sound-lowering", "건전한 트라이그램 컴파일 & 동일 평가기 검증")}
<p>LQL, <code>lancet rg</code> 정규식 HIR, 리터럴이 있는 <code>lancet ast</code> 패턴, <code>lancet refs</code>까지 모두 필수 <strong>바이트 트라이그램</strong> 논리식(및 <code>fuzzy()</code>용 건전한 q-gram 하한 <code>AtLeast(k)</code>)으로 낮춰집니다. 필수 트라이그램이 없는 파일은 <em>수학적으로</em> 매치할 수 없으므로 건너뛰어도 누락이 발생하지 않습니다.</p>
<p>인덱스에서 걸러진 후보와 수정된 파일은 <strong>인덱스 없는 풀스캔과 완전히 동일한 평가기</strong>로 검증합니다. 따라서 구조적으로 <strong><code>Scan == Auto</code></strong>가 성립합니다.</p>

{h2("freshness", "데몬 없이 auto 모드가 수정을 즉시 감지하는 법")}
{diagram(f"{d}/index-lifecycle.ko.svg", "인덱스 수명주기: 초기 빌드, 매 쿼리 메타데이터 검사, 증분 세그먼트, 컴팩션.", "auto 모드는 상주 데몬 없이도 방금 수정되거나 새로 생긴 파일을 즉시 감지합니다.")}
<p>기본 <code>auto</code> 모드는 파일 메타데이터를 한 번 순회하며 인덱스된 파일마다 다음 4가지 속성을 대조합니다:</p>
<ul>
  <li><code>mtime</code> + <code>size</code> + <code>ctime</code> + <code>inode</code> (Unix/macOS)</li>
  <li>더해 <strong>racy-timestamp 검사</strong>: 인덱스 기록 시각과 같은 타임스탬프 구간에서 바뀐 파일은 무조건 stale로 보고 실시간 스캔합니다.</li>
</ul>
<p>덕분에 길이가 같은 수정(<code>size</code> 동일), <code>mtime</code>을 되돌린 트릭(<code>ctime</code>/<code>inode</code>로 포착), 밀리초 미만 연속 수정도 놓치지 않습니다.</p>

{callout("warn", "Windows 플랫폼의 정직한 한계", "<p>Rust 표준 라이브러리는 Windows에서 <code>ctime</code>과 <code>inode</code>를 안정적으로 노출하지 않고 <code>mtime</code>과 <code>size</code>만 제공합니다. 따라서 Windows에서 파일 크기와 <code>mtime</code>을 동시에 유지한 채 덮어쓰는 특수한 경우에는 <code>lancet index</code> 또는 <code>--no-index</code>가 필요합니다.</p>",)}

{h2("modes", "auto vs --trust-index vs --no-index")}
{table(
    ["모드", "디스크 stat 순회", "수정 파일 실시간 스캔", "지연 (Linux 8.6만 파일)", "추천 용도"],
    [
        ["<code>auto</code> (기본값)", "예 (~100 ms)", "예", "~148 ms (희귀 리터럴)", "실제 개발 &amp; AI 에이전트 작업(항상 최신)"],
        ["<code>--trust-index</code>", "아니오", "아니오", "~40 ms (0건 시 3 ms)", "인덱스가 최신임이 확실한 초대형 저장소 읽기 전용 탐색"],
        ["<code>--no-index</code>", "전체 스캔", "예 (모든 파일)", "~1,113 ms", "디버깅, 기준선 비교, 쓰기 불가 환경"],
    ],
)}

{h2("lsm-ops", "LSM 세그먼트, 상위 인덱스 탐색 & 컴팩션")}
<ul>
  <li><strong>디스크 구조:</strong> <code>PATH/.lancet</code> 안에 불변 LSM 세그먼트(<code>LANCET04</code>), 삭제 툼스톤, mmap 포스팅 리스트로 저장됩니다.</li>
  <li><strong>상위 디렉터리 자동 탐색:</strong> <code>src/query/</code> 안에서 실행해도 상위 저장소 루트의 <code>.lancet</code>를 자동으로 찾아 사용합니다(<code>--index-dir</code>로 재지정 가능).</li>
  <li><strong>증분 갱신 및 유지보수:</strong> <code>lancet index .</code>는 바뀐 파일만 새 세그먼트로 추가하고, <code>lancet index --compact</code>는 세그먼트를 병합하고 툼스톤을 정리하며, <code>--rebuild</code>는 처음부터 다시 만듭니다.</li>
  <li><strong>선택도 진단:</strong> <code>lancet status</code>로 인덱스·stale 파일 수를 확인하고, <code>lancet explain '&lt;QUERY&gt;'</code>로 추출된 트라이그램과 후보 비율을 볼 수 있습니다.</li>
</ul>
"""


def _arch_bench(d: str) -> str:
    return f"""
<p>도구들이 서로 다른 전제로 설계되어 있으면 에이전트가 <code>rg</code> → <code>sed</code> → <code>patch</code> → <code>pdftotext</code>를 잇는 틈에서 조용한 데이터 손실이 터집니다. Lancet은 한 바이너리 안에서 좌표와 잠금을 통합하는 동시에, 경쟁 도구 대비 느린 부분까지 투명하게 공개합니다.</p>

{h2("failure-modes", "직접 재현해 막아낸 7가지 실패 사례")}
{diagram(f"{d}/failure-modes.ko.svg", "bench/failure_modes.py로 재현한 도구 체인의 7가지 실패 사례.", "모든 수치는 <code>bench/failure_modes.py</code>가 실제 도구로 직접 재현한 결과입니다.")}
<ol>
  <li><strong>연속 편집 시 줄 밀림:</strong> <code>rg -n</code> 한 번으로 <code>sed -i</code>를 두 번 치면 두 번째가 잘못된 줄을 고치고도 종료 0을 냅니다. <code>lancet edit --edits</code>는 모든 hunk를 원본 줄 번호로 해석합니다.</li>
  <li><strong>동시 수정 유실:</strong> 4개 에이전트가 읽기→쓰기를 반복하면 일반 파일에서는 <strong>800개 중 600개</strong>가 유실되고, <code>lancet edit --rev</code>는 <strong>800개 중 0개</strong>가 유실됩니다.</li>
  <li><strong>반쪽 적용 패치:</strong> <code>patch -p1</code>은 한 파일만 고치고 실패해도 되돌리지 않습니다. <code>lancet patch</code>는 사전 검증과 쓰기 경합 시 롤백으로 원자성을 지킵니다.</li>
  <li><strong>주석·문자열 오염:</strong> <code>sed</code> 일괄 치환은 주석·문자열까지 0.3~11.7% 바꿉니다. <code>refs</code>와 <code>ast</code>는 이를 제외하며, <code>rename</code>은 로컬 스코프를 재검증합니다.</li>
  <li><strong>문서 좌표 불일치:</strong> <code>.docx</code>에서 <code>rga</code>→<code>pandoc</code> 조합은 40줄 중 0줄만 일치했고, Lancet은 단일 추출 파이프라인으로 40/40 일치했습니다.</li>
  <li><strong>수정 직후 인덱스 지연:</strong> 파일 20개를 고친 직후 zoekt는 새 텍스트를 0건 찾고 지운 텍스트를 20건 찾았지만, Lancet <code>auto</code>는 20건·0건을 정확히 반환했습니다.</li>
  <li><strong>링크·권한 손상:</strong> <code>sed -i</code>/<code>perl -pi</code>는 심볼릭/하드 링크를 끊고 <code>0640</code> 권한을 날리지만, Lancet은 대상 파일과 권한·소유자를 보존합니다.</li>
</ol>
<p>전체 배경 에세이: <a href="{REPO_DOCS}/why-one-tool.ko.md"><code>docs/why-one-tool.ko.md</code></a> (<a href="{REPO_DOCS}/why-one-tool.md">English</a>).</p>

{h2("core-bench", "핵심 벤치마크 스위트 (코드베이스 8개)")}
<p>Apple M4 Pro, 웜 캐시, <code>hyperfine -N</code> 5회 중앙값. 전체 표: <a href="https://github.com/CREE1116/lancet/blob/main/bench/results/SUITE.md"><code>bench/results/SUITE.md</code></a>.</p>
{diagram(f"{d}/bench-index-footprint.ko.svg", "8개 코드베이스에서 lancet과 zoekt의 인덱스 빌드 시간, 디스크 크기, 피크 메모리 비교.", "zoekt 대비 빌드 4~25배 빠르고, 디스크 9~30배 작으며, 피크 RAM 3.2~5.0배 적음.")}
{diagram(f"{d}/bench-workload-profile.ko.svg", "LLVM 19.1의 8개 텍스트 쿼리 유형 및 8개 코드베이스 AST 검색 프로필.", "모든 텍스트 출력은 ripgrep 15.2와 바이트 단위 일치.")}
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

{h2("competitors", "인덱스·에이전트 도구 경쟁 비교 (정직한 트레이드오프)")}
<p><a href="https://github.com/CREE1116/lancet/blob/main/bench/competitors_agents.py"><code>bench/competitors_agents.py</code></a>로 <code>rg</code>, <code>ast-grep</code>, <code>tgrep</code>(디스크·상주 데몬 <code>serve</code>), <code>grix</code>, <code>aft</code>, <code>tensor-grep</code>, <code>probe</code>를 동일 조건에서 측정했습니다 (<a href="https://github.com/CREE1116/lancet/blob/main/bench/results/COMPETITORS.md"><code>COMPETITORS.md</code></a>):</p>
<ul>
  <li><strong>Lancet이 우위인 지점:</strong> 데몬 없이 <strong>수정 직후 0 ms 최신성 20/20</strong>(<code>aft</code> 워처 지연과 서버리스 <code>tgrep</code>는 0/20); <code>aft</code>/<code>tgrep</code> 대비 <strong>디스크 인덱스 1/3~1/7 크기</strong>(Linux 기준 141.8 MB vs 368.5 MB / 955.2 MB); <code>grix</code> 대비 <strong>빌드 피크 RAM 약 1/3</strong>(Linux 353.5 MB vs 1,001.6 MB); <strong><code>rg</code> 줄 수 20/20 완전 일치</strong>; 테스트한 4개 코퍼스 모두에서 가장 빠른 리터럴 AST 검색.</li>
  <li><strong>상주 데몬이나 특화 도구가 더 빠른 지점:</strong> 상주 데몬인 <code>tgrep serve</code>는 Rust(119.2 ms vs 508.7 ms)와 LLVM(19.9 ms vs 295.8 ms)의 <code>regex with literal</code>, 그리고 Django(15.4 ms)/Rust(60.9 ms)의 리터럴 없는 정규식에서 데몬 없는 <code>lancet auto</code>보다 빠릅니다. <code>grix --no-auto-index</code>는 Linux 희귀 리터럴에서 <code>lancet --trust-index</code>보다 빠릅니다(11.5 ms vs 43.6 ms). 상주 <code>aft</code>(587~1,260 MB RAM 점유)는 Rust 희귀 리터럴에서 8.5 ms vs 73.5 ms로 더 빠릅니다. 리터럴 없는 정규식과 AST 패턴은 각각 <code>rg</code>(최대 1.4배 느림), <code>ast-grep</code>(0.6~1.0배) 수준입니다.</li>
</ul>
"""


def _cli(_d: str) -> str:
    return f"""
<p>모든 하위 명령은 <code>.gitignore</code> / <code>.ignore</code> / <code>.rgignore</code> 순회 규칙, 결정적 정렬, 구조화된 종료 코드를 공유합니다.</p>

{h2("exit-codes", "종료 코드")}
{table(
    ["코드", "의미", "상태 값 (JSON <code>status</code>)"],
    [
        ["<code>0</code>", "성공 / 결과 있음", "<code>complete</code>"],
        ["<code>1</code>", "결과 없음 (또는 <code>status</code>에서 인덱스 없음)", "<code>empty</code>"],
        ["<code>2</code>", "잘못된 쿼리·인자, 미지원 동작, 수정 충돌, I/O 오류", "<code>invalid_query</code>, <code>unsupported</code>, <code>conflict</code>"],
    ],
    num_cols=[0],
)}

{h2("subcommands", "전체 하위 명령 & 플래그")}
{code("CLI 전체 목록", """lancet <QUERY> [PATH] [--json] [--no-index | --trust-index] [--index-dir DIR] [--ocr]
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
                    [--no-index | --trust-index] [--index-dir DIR] [--ocr] [--json]""", "ko")}

{h2("rg-parity", "lancet rg — ripgrep 호환성 표")}
<p><code>bench/rg_parity.py</code>의 68개 엣지 케이스 픽스처와 Linux 커널 전체 쿼리를 <code>ripgrep 15.2</code>와 바이트 단위로 대조했습니다.</p>

{h3("rg-matching", "패턴 & 매칭")}
{table(
    ["플래그", "의미", "상태", "비고"],
    [
        ["<code>-e</code>, <code>--regexp &lt;PAT&gt;</code>", "검색 패턴 (중복 지정 가능)", "<span class='tag-yes'>지원</span>", "선택(alternation)으로 결합"],
        ["<code>-f</code>, <code>--file &lt;FILE&gt;</code>", "파일에서 줄당 하나씩 패턴 읽기 (<code>-</code> stdin)", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-F</code>, <code>--fixed-strings</code>", "리터럴 문자열로 취급", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-i</code> / <code>-s</code> / <code>-S</code>", "대소문자 무시 / 구분 / 스마트", "<span class='tag-yes'>지원</span>", "마지막 플래그 우선; AST 기반 <code>-S</code>(<code>\\W</code> 등은 무시 유지)"],
        ["<code>-w</code> / <code>-x</code> / <code>-v</code>", "단어 경계 / 줄 전체 / 반전 매치", "<span class='tag-yes'>지원</span>", "<code>rg</code> 단어 경계 규칙과 동일"],
        ["<code>-U</code>, <code>--multiline-dotall</code>", "여러 줄 매치 및 <code>.</code>에 개행 허용", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-P</code>, <code>--pcre2</code>", "PCRE2 전후방 탐색·역참조", "<span class='tag-no'>미지원</span>", "선형 시간과 건전한 트라이그램 컴파일 보장을 위해 Rust <code>regex</code>만 사용"],
    ],
)}

{h3("rg-output", "출력 & 형식")}
{table(
    ["플래그", "의미", "상태", "비고"],
    [
        ["<code>-o</code>, <code>-c</code>, <code>--count-matches</code>", "매치 부분만 / 매치 줄 수 / 총 매치 수", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-l</code>, <code>--files-without-match</code>, <code>-q</code>", "일치/미일치 파일 경로, 무출력", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-m &lt;N&gt;</code>, <code>-A</code>/<code>-B</code>/<code>-C</code>", "파일당 최대 개수 및 전후 문맥", "<span class='tag-yes'>지원</span>", "<code>--</code> 구분선 포함"],
        ["<code>--passthru</code>", "일치·비일치 줄 모두 출력", "<span class='tag-yes'>지원</span>", "<code>--passthru</code>/<code>-A</code>/<code>-B</code>/<code>-C</code> 중 마지막 옵션 우선"],
        ["<code>-n</code> / <code>-N</code>", "줄 번호 표시 / 숨기기", "<span class='tag-yes'>지원</span>", "파이프 시에도 <strong>기본 ON</strong>"],
        ["<code>-H</code> / <code>-I</code>, <code>--column</code>, <code>--vimgrep</code>", "파일명 제어, 열 번호, vimgrep", "<span class='tag-yes'>지원</span>", ""],
        ["<code>--json</code>, <code>-0</code>, <code>-r &lt;TEXT&gt;</code>", "rg JSON Lines, NUL 구분, <code>$1</code> 치환", "<span class='tag-yes'>지원</span>", ""],
        ["<code>--color &lt;WHEN&gt;</code>", "<code>auto</code>, <code>always</code>, <code>ansi</code>, <code>never</code>", "<span class='tag-yes'>지원</span>", "기본 <code>auto</code>는 TTY와 <code>NO_COLOR</code>/<code>TERM=dumb</code> 준수"],
        ["<code>--sort</code>, <code>--no-heading</code>", "정렬, 헤더 끄기", "No-op", "항상 경로순 정렬; 헤더 사용 안 함"],
    ],
)}

{h3("rg-files", "파일 순회 & Lancet 확장")}
{table(
    ["플래그", "의미", "상태", "비고"],
    [
        ["<code>-g</code>, <code>--iglob</code>, <code>-t</code>, <code>-T</code>, <code>--type-list</code>", "글롭 및 파일 타입 필터", "<span class='tag-yes'>지원</span>", "긍정 글롭은 ignore/hidden을 덮어씀"],
        ["<code>--type-add &lt;SPEC&gt;</code>", "사용자 타입 정의 (<code>name:glob</code> / <code>include:</code>)", "<span class='tag-yes'>지원</span>", "<code>-t</code>, <code>-T</code>, <code>--files</code>, <code>--type-list</code> 반영"],
        ["<code>-u</code> / <code>-uu</code> / <code>-uuu</code>, <code>-.</code>, <code>--no-ignore</code>", "제한 해제 및 숨김·ignore 옵션", "<span class='tag-yes'>지원</span>", ""],
        ["<code>-L</code>, <code>--max-depth</code>, <code>--max-filesize</code>", "심볼릭 링크 추적, 깊이, 크기 (K/M/G)", "<span class='tag-yes'>지원</span>", "<code>--trust-index -L</code>은 라이브 순회로 폴백"],
        ["<code>--files</code>, <code>-j</code>", "검색 대상 파일 나열, 스레드 수", "<span class='tag-yes'>지원</span>", ""],
        ["<code>--docs</code>", "PDF·오피스·HWP·압축파일 인프로세스 검색", "<span class='tag-ext'>확장</span>", "기본 OFF(<code>rg</code> 호환 출력 유지)"],
        ["<code>--no-index</code>, <code>--trust-index</code>, <code>--index-dir</code>", "<code>.lancet</code> 인덱스 동작 제어", "<span class='tag-ext'>확장</span>", "기본값은 항상 최신인 <code>auto</code> 모드"],
        ["<code>--pre &lt;CMD&gt;</code>", "외부 전처리기", "<span class='tag-no'>미지원</span>", "인프로세스 <code>--docs</code>로 대체"],
    ],
)}
"""


def _query_ast(_d: str) -> str:
    return f"""
<p>Lancet Query Language(<strong>LQL v0.1</strong>)는 결정적인 중간 표현(IR)으로 컴파일되어 인덱스 후보와 풀스캔에서 동일하게 평가됩니다. 알 수 없는 연산자나 필터는 조용히 무시하지 않고 정확한 바이트 위치와 함께 <code>status: "invalid_query"</code>를 반환합니다.</p>

{h2("lql-grammar", "LQL v0.1 정식 문법")}
{code("EBNF", """query   := ["find"] expr ("in" scope)* ("|" stage)*
expr    := or_expr
or_expr := and_expr ("OR" and_expr)*
and_expr:= unary (("AND")? unary)*          # 공백으로 인접한 항은 암묵적 AND
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
<p>연산자 우선순위는 <code>NOT</code> &gt; <code>AND</code>(명시적 또는 암묵적) &gt; <code>OR</code>입니다. <code>lancet parse '&lt;QUERY&gt;'</code>로 정규화된 쿼리와 JSON IR을 바로 확인할 수 있습니다.</p>

{h2("lql-semantics", "매처, 스코프 & 파이프 단계 의미론")}
{table(
    ["구문", "정확한 동작"],
    [
        ["<code>\"text\"</code> / 일반 토큰", "대소문자 무시(Unicode simple case folding) 부분 문자열 매치."],
        ["<code>regex(\"pat\")</code>", "멀티라인 모드 Rust <code>regex</code>(<code>^</code>/<code>$</code>가 줄 경계). 기본 대소문자 구분(<code>(?i)</code>로 해제)."],
        ["<code>fuzzy(\"word\", k)</code>", "문자 단위 레벤슈타인 거리 <code>0 &lt;= k &lt;= 3</code>(기본 1) 단어 토큰 매치, 건전한 q-gram 임계값으로 인덱스 가속."],
        ["<code>symbol(\"name\")</code> / <code>symbol:name</code>", "Tree-sitter 심볼 정의 정확 매치(대소문자 무시; <code>.rs</code>, <code>.py</code>, <code>.js</code>, <code>.ts</code>, <code>.tsx</code>, <code>.go</code>)."],
        ["<code>near(a, b, d)</code> / <code>before(a, b, d)</code>", "단위 안에서 토큰 거리 <code>|tok(b) - tok(a)| &lt;= d</code>(<code>before</code>는 <code>b</code>가 반드시 <code>a</code> 뒤)."],
        ["<code>in file</code> / <code>in block</code> / <code>in line</code>", "<code>file</code>(기본), 빈 줄 없는 문단(<code>block</code>), 단일 줄(<code>line</code>; 매치 전체 <code>[start, end)</code>가 단위 안에 있어야 함)."],
        ["<code>in function</code>", "Tree-sitter 함수·메서드 AST 노드(<code>.rs</code>, <code>.py</code>, <code>.js</code>, <code>.ts</code>, <code>.tsx</code>, <code>.go</code>); 결과에 <code>symbol</code> 채움."],
        ["<code>type:</code> / <code>ext:</code> / <code>path:</code>", "같은 카테고리 값은 OR, 서로 다른 카테고리는 AND. <code>type:</code>는 <code>code</code>, <code>document</code>, <code>spreadsheet</code>, <code>config</code>, <code>log</code>, <code>data</code>, <code>archive</code>, <code>image</code>, <code>other</code>."],
        ["<code>| limit</code> / <code>files</code> / <code>count</code> / <code>group file</code>", "간결한 투영 및 결정적 결과 수 제한."],
        ["<code>| expand</code> / <code>sort</code> / <code>terms</code> / <code>budget</code>", "상위 문맥 확장, 횟수/경로 정렬, 동시 출현 어휘 추출, 추정 토큰 상한 적용."],
    ],
)}

{h2("ast-langs", "지원되는 AST 24개 언어 (--lang)")}
<p><code>lancet ast</code>와 <code>lancet refs</code> 모두 아래 별칭을 지원합니다.</p>
{table(
    ["언어", "--lang 별칭", "언어", "--lang 별칭"],
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

{h2("ast-metavars", "메타변수 & 리라이트 규칙")}
<ul>
  <li><strong><code>$VAR</code>:</strong> 명명된 단일 AST 노드에 매치; 같은 이름을 반복하면(<code>$X == $X</code>) 구조가 동일해야 합니다.</li>
  <li><strong><code>$_</code>:</strong> 익명 단일 노드 와일드카드(치환 템플릿에 캡처되지 않음).</li>
  <li><strong><code>$$$ARGS</code>:</strong> 0개 이상의 형제 AST 노드(백트래킹 폭주를 막기 위해 메모이제이션과 단계 예산으로 제한).</li>
  <li><strong>문장 메타변수:</strong> C 계열 언어에서 <code>if ($C) $S</code>, <code>{{ $$$B }}</code>가 동작하며, <code>if ($C) $S</code>는 <code>else</code> 없는 <code>if</code>만 엄격히 매치합니다(<code>ast-grep</code>과 다른 점).</li>
  <li><strong>리라이트 안전 규칙:</strong> <code>--rewrite</code>는 기본적으로 dry-run이며, <code>--apply</code>는 <code>--limit</code>과 함께 쓸 수 없고, 한 파일 내 중첩 매치는 건너뛰어 <code>skipped_overlaps</code>에 기록됩니다. <code>--lang cpp</code>는 <code>.cpp</code>/<code>.cc</code> 외에 <code>.h</code> 헤더도 스캔합니다.</li>
</ul>
"""


def _json_mcp(_d: str) -> str:
    return f"""
<p><code>--json</code> 플래그나 <code>lancet mcp</code>로 호출하면 모든 명령이 Rust 구조체에서 직접 직렬화된 일관된 snake_case JSON을 반환합니다.</p>

{h2("status-enum", "공통 Status & Error 규격")}
<p>모든 JSON 응답에는 <code>status</code> 필드(필요 시 <code>error: {{"message": string, "pos"?: number}}</code>)가 포함됩니다:</p>
<ul>
  <li><code>"complete"</code> (종료 <code>0</code>) — 검색 완료 또는 편집·패치 적용/검증 완료.</li>
  <li><code>"empty"</code> (종료 <code>1</code>) — 오류 없이 결과 0건.</li>
  <li><code>"invalid_query"</code> (종료 <code>2</code>) — 잘못된 LQL, 잘못된 줄 범위, 중복된 <code>--find</code> 등.</li>
  <li><code>"unsupported"</code> (종료 <code>2</code>) — 미지원 언어, 읽기 전용 문서 수정 시도, 거절된 <code>rename</code> 위치.</li>
  <li><code>"conflict"</code> (종료 <code>2</code>) — <code>--rev</code>가 현재 디스크 상태와 달라 OS 파일 잠금 하에 0바이트만 쓰고 중단됨.</li>
</ul>

{h2("json-schemas", "명령별 JSON 응답 스키마")}
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
}""", "ko")}

{code("ReadResponse · EditResponse · RenameResponse", """// ReadResponse (PDF, 오피스, HWP, 압축 항목에서는 rev 생략)
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

// EditResponse (mode: "dry-run" | "applied", hunks는 원본 파일 기준 줄 범위)
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
}""", "ko")}

{code("PatchResponse · RefsResponse · RankResponse · AstResponse", """// PatchResponse: { status, mode, applied, files: [{ path, action, from?, hunks, offsets, rev?, rev_after?, diff }], elapsed_ms }
// RefsResponse:  { status, mode, symbol, definitions: [RefHit], references: [RefHit], total_references, truncated, stats }
// RankResponse:  { status, mode, pattern, entries: [{ name, kind, path, line, references, definitions }], total, truncated, files_parsed, elapsed_ms }
// AstResponse:   { status, mode, lang?, pattern, matched, returned, truncated, matches: [AstMatch], diffs: [FileDiff], stats }""", "ko")}

{h2("mcp-tools", "MCP 11개 도구 상세 (lancet mcp)")}
<p>지원 프로토콜 버전: <code>2025-03-26</code>, <code>2024-11-05</code>. MCP에서는 <code>status: "empty"</code>도 정상(<code>isError: false</code>)이며, <code>lancet_rg</code>는 <code>max_output_bytes</code>(기본 128 KiB, 최대 4 MiB) 기준으로 줄 경계에서 깔끔히 잘립니다.</p>
{table(
    ["MCP 도구", "역할", "주요 인자 및 동작"],
    [
        ["<code>lancet_fd</code>", "파일·폴더 탐색", "<code>pattern</code>, <code>path</code>, <code>dirs</code>, <code>tree</code>, <code>exts</code>, <code>type_classes</code>, <code>changed_within</code>, <code>index_mode</code>"],
        ["<code>lancet_search</code>", "LQL 검색", "<code>query</code>, <code>path</code>, <code>max_lines</code>, <code>max_cols</code>, <code>ocr</code>, <code>index_mode</code>"],
        ["<code>lancet_rg</code>", "ripgrep 호환 검색", "<code>pattern</code>, <code>paths</code>, <code>docs</code>, <code>passthru</code>, <code>type_add</code>, <code>max_output_bytes</code>"],
        ["<code>lancet_outline</code>", "선언·문서 뼈대", "<code>path</code>, <code>max_depth</code>, <code>exts</code>, <code>limit</code>, <code>ocr</code>"],
        ["<code>lancet_read</code>", "정밀 범위·문서 읽기", "<code>target</code>, <code>lines</code>, <code>symbol</code>, <code>page</code>, <code>sheet</code>, <code>rows</code>, <code>cols</code>, <code>entry</code>, <code>budget_tokens</code>"],
        ["<code>lancet_refs</code>", "어휘 AST 참조 &amp; 랭킹", "<code>symbol</code>, <code>path</code>, <code>rank</code>, <code>kinds</code>, <code>lang</code>, <code>limit</code>"],
        ["<code>lancet_ast</code>", "AST 검색·치환", "<code>pattern</code>, <code>path</code>, <code>lang</code>, <code>rewrite</code>, <code>apply</code> (<code>--read-only</code> 시 차단)"],
        ["<code>lancet_edit</code>", "잠금 CAS 편집", "<code>--read-only</code> 시 목록에서 제외; <code>lines</code>, <code>symbol</code>, <code>find</code>, <code>edits</code>, <code>rev</code>, <code>apply</code>"],
        ["<code>lancet_patch</code>", "원자적 Diff 패치", "<code>--read-only</code> 시 목록에서 제외; <code>patch</code>, <code>root</code>, <code>strip</code>, <code>apply</code>, <code>allow_syntax_errors</code>"],
        ["<code>lancet_rename</code>", "로컬 이름 변경", "<code>--read-only</code> 시 목록에서 제외; <code>path</code>, <code>line</code>, <code>col?</code>, <code>old</code>, <code>new</code>, <code>rev?</code>, <code>apply?</code>"],
        ["<code>lancet_index</code>", "인덱스 관리", "<code>action</code> (<code>status</code>, <code>sync</code>, <code>rebuild</code>, <code>compact</code>); 변경 작업은 <code>--read-only</code> 시 차단"],
    ],
)}

{h2("troubleshooting", "문제 해결 및 거절 사유별 복구법")}
{table(
    ["오류 / 거절", "원인", "정확한 복구 방법"],
    [
        ["<code>status: \"conflict\"</code> (종료 2)", "<code>rev</code>를 읽은 뒤 다른 작업이 파일을 수정함", "<code>lancet read</code>로 최신 내용과 새 <code>rev</code>를 다시 읽어 변경을 재계산해 적용합니다."],
        ["<code>--find</code> 거절 (비고유)", "찾을 문자열이 0회 또는 2회 이상 등장", "오류에 출력된 줄 번호를 확인하고 주변 문맥을 더 넓히거나 의도한 경우 <code>--all</code>을 추가합니다."],
        ["구문 오류 증가 거절", "편집·패치 후 Tree-sitter 구문 오류가 늘어남", "dry-run diff에서 괄호/따옴표 누락을 점검하고, 단계적 수정일 때만 <code>--allow-syntax-errors</code>를 씁니다."],
        ["<code>lancet rename</code> 거절", "전역/필드/Python 매개변수, 매크로·<code>eval</code>, 섀도잉", "오류 사유를 확인하고, 파일 간 심볼은 <code>lancet refs</code> + <code>lancet edit --edits</code> 또는 LSP를 씁니다."],
        ["<code>ast --rewrite --apply</code> + <code>--limit</code>", "일부만 잘라낸 치환은 실수로 적용할 수 없음", "<code>--limit</code>을 빼거나 <code>--path</code> 글롭으로 대상 파일을 좁힌 뒤 <code>--apply</code>하세요."],
        ["<code>lancet rg -P</code> 오류", "PCRE2 전후방 탐색·역참조 미지원", "룩어라운드 없는 Rust <code>regex</code>나 LQL <code>before()</code> / <code>NOT</code>으로 표현하세요."],
        ["<code>lancet fd</code> 경로 2개 오류", "<code>fd</code>는 <code>[PATH]</code> 인자를 최대 1개만 받음", "디렉터리마다 나눠 실행하거나 저장소 루트에서 <code>-p</code>/글롭 필터를 사용하세요."],
    ],
)}
"""


PAGES_KO = [
    {
        "path": "index.html",
        "group": "start",
        "nav_title": "소개 · 설치 · 치트시트",
        "title": "Lancet 공식 문서 — 소개 · 설치 · 치트시트",
        "description": "Lancet 설치 방법, 60초 빠른 시작, 한 화면 CLI 치트시트 안내.",
        "lede": "하나의 좌표계 위에서, 인덱스로 훨씬 빠르게 코드와 문서를 찾고 읽고 고칩니다.",
        "keywords": "설치 homebrew cargo curl windows 치트시트 개요",
        "source": "https://github.com/CREE1116/lancet/blob/main/README.ko.md",
        "render": _intro,
    },
    {
        "path": "migration.html",
        "group": "start",
        "nav_title": "rg·fd·sg·sed에서 전환",
        "title": "기존 도구(rg, fd, ast-grep, sed, patch, rga)에서 전환하기",
        "description": "ripgrep, fd, ast-grep, sed, patch, rga 사용자용 1:1 대조표와 의도적 차이, 도구 선택 경계.",
        "lede": "ripgrep, fd, ast-grep의 익숙한 사용감을 그대로 가져오면서 통일된 좌표계와 무손실 편집을 더합니다.",
        "keywords": "전환 마이그레이션 ripgrep rg fd ast-grep sg sed patch rga pcre2 차이",
        "source": f"{REPO_DOCS}/rg-parity.md",
        "render": _migration,
    },
    {
        "path": "guides/explore.html",
        "group": "guides",
        "nav_title": "낯선 코드베이스 탐색",
        "title": "컨텍스트를 아끼며 낯선 코드베이스 탐색하기",
        "description": "fd --dirs, refs --rank, outline, batch, budget-tokens 페이지네이션을 활용한 5단계 탐색 워크플로우.",
        "lede": "파일 전체를 읽어 토큰을 낭비하지 않고 다섯 단계로 핵심 코드에 정확히 도달하는 법.",
        "keywords": "탐색 fd dirs tree refs rank outline batch budget-tokens 페이지네이션",
        "source": f"{REPO_DOCS}/agent-instructions.md",
        "render": _explore,
    },
    {
        "path": "guides/editing.html",
        "group": "guides",
        "nav_title": "안전한 편집·패치·이름 변경",
        "title": "안전한 편집, 다중 구간 CAS, 패치 & 로컬 이름 변경",
        "description": "edit --find/--symbol/--edits, 멀티 에이전트 rev CAS, patch 롤백, 스코프 증명 기반 local rename.",
        "lede": "여러 군데를 고쳐도 줄 번호가 밀리지 않고, 여러 에이전트가 동시에 써도 덮어쓰기가 발생하지 않습니다.",
        "keywords": "편집 edit edits find symbol rev conflict cas patch rename 로컬 스코프",
        "source": f"{REPO_DOCS}/agent-instructions.md",
        "render": _editing,
    },
    {
        "path": "guides/documents.html",
        "group": "guides",
        "nav_title": "PDF·오피스·HWP·압축파일",
        "title": "PDF, 오피스, 한글(HWP) & 압축 파일 검색·열람",
        "description": "PDF, DOCX, PPTX, HWP/HWPX, XLSX/ODS, EPUB, ZIP, TAR의 인프로세스 추출과 공유 줄 번호 좌표.",
        "lede": "검색과 읽기가 같은 인프로세스 추출기를 써서 문서·압축 파일 내부에서도 줄 번호가 그대로 맞습니다.",
        "keywords": "pdf docx pptx odt hwp hwpx epub xlsx xls ods zip tar gz ocr 문서",
        "source": f"{REPO_DOCS}/grammar.md",
        "render": _documents,
    },
    {
        "path": "guides/agents-mcp.html",
        "group": "guides",
        "nav_title": "AI 에이전트·MCP·공식 스킬",
        "title": "AI 에이전트 연동: 내장 MCP 서버 & 공식 스킬",
        "description": "Claude Code, Cursor, Claude Desktop, Antigravity, Codex용 lancet mcp 설정과 skills/lancet 설치.",
        "lede": "줄바꿈 구분 stdio JSON-RPC 서버로 11개 도구를 연결하거나 공식 스킬을 두 줄로 설치하세요.",
        "keywords": "mcp claude cursor codex antigravity 스킬 skill read-only root 에이전트",
        "source": f"{REPO_DOCS}/mcp.md",
        "render": _agents_mcp,
    },
    {
        "path": "concepts/index-and-freshness.html",
        "group": "concepts",
        "nav_title": "인덱스 & 최신성 모델",
        "title": ".lancet 트라이그램 LSM 인덱스와 최신성 보장",
        "description": "건전한 트라이그램 컴파일, (mtime, size, ctime, inode)+racy 검사, LSM 컴팩션으로 Scan == Auto를 보장하는 원리.",
        "lede": "상주 워처 데몬 없이도 Scan == Auto를 보장하는 이유와 --trust-index의 올바른 쓰임새.",
        "keywords": "인덱스 트라이그램 lsm auto trust-index no-index 최신성 ctime inode racy windows",
        "source": f"{REPO_DOCS}/why-one-tool.ko.md",
        "render": _index_freshness,
    },
    {
        "path": "concepts/architecture-and-benchmarks.html",
        "group": "concepts",
        "nav_title": "아키텍처 & 벤치마크",
        "title": "아키텍처, 실패 사례 방지 & 정직한 벤치마크",
        "description": "7가지 도구 체인 실패 사례, 8개 코드베이스 벤치마크, tgrep/grix/aft와의 가감 없는 경쟁 비교.",
        "lede": "8개 저장소 실측 데이터와 ripgrep, zoekt, ast-grep, tgrep, grix, aft 대비 장단점을 가감 없이 공개합니다.",
        "keywords": "벤치마크 suite competitors zoekt tgrep grix aft 실패 사례 아키텍처 성능",
        "source": "https://github.com/CREE1116/lancet/blob/main/bench/results/COMPETITORS.md",
        "render": _arch_bench,
    },
    {
        "path": "reference/cli.html",
        "group": "reference",
        "nav_title": "CLI & lancet rg 레퍼런스",
        "title": "전체 CLI & lancet rg 호환성 레퍼런스",
        "description": "모든 하위 명령, 플래그, 종료 코드, ripgrep 15.2 기능 대조표.",
        "lede": "전체 하위 명령 구문, 종료 코드, ripgrep 호환성 매트릭스.",
        "keywords": "cli 플래그 하위명령 종료코드 rg 호환성 passthru type-add",
        "source": f"{REPO_DOCS}/rg-parity.md",
        "render": _cli,
    },
    {
        "path": "reference/query-and-ast.html",
        "group": "reference",
        "nav_title": "LQL 문법 & AST 24개 언어",
        "title": "Lancet Query Language(LQL v0.1) & 24개 언어 AST 명세",
        "description": "EBNF 정식 문법, 연산자 우선순위, 스코프, 파이프 단계, 24개 언어 별칭, AST 메타변수 규칙.",
        "lede": "EBNF 정식 문법, 매처·스코프 의미론, 24개 --lang 별칭, AST 메타변수 규칙.",
        "keywords": "lql 문법 near before fuzzy symbol regex ast 메타변수 언어",
        "source": f"{REPO_DOCS}/grammar.md",
        "render": _query_ast,
    },
    {
        "path": "reference/json-and-mcp.html",
        "group": "reference",
        "nav_title": "JSON 스키마 · MCP · 문제 해결",
        "title": "JSON 출력 스키마, MCP 11개 도구 명세 & 문제 해결",
        "description": "Rust 구조체 기반 실제 JSON 응답 스키마, 11개 MCP 도구, 오류·거절 사유별 복구 가이드.",
        "lede": "기계 판독용 JSON 스키마, MCP 도구 인벤토리, 거절·오류별 즉시 복구 절차.",
        "keywords": "json 스키마 응답 mcp 도구 status conflict 문제해결 복구",
        "source": f"{REPO_DOCS}/mcp.md",
        "render": _json_mcp,
    },
]
