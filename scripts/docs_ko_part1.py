"""Korean documentation pages 1-7: 시작하기 & 탐색과 검색."""

from __future__ import annotations

from docs_ui import callout, cards, code, h2, h3, table

REPO_DOCS = "https://github.com/CREE1116/lancet/blob/main/docs"


def _intro(_d: str) -> str:
    return f"""
<p><strong>Lancet</strong>은 <code>rg</code> + <code>fd</code> + <code>ast-grep</code> + <code>cat</code> + <code>sed</code> + <code>patch</code> + <code>pdftotext</code>를 이어 쓰던 작업을 의존성 없는 단일 Rust 바이너리로 통합합니다. 모든 명령이 같은 <code>.lancet</code> 트라이그램 인덱스, 파일 필터 규칙, <strong>1부터 시작하는 줄 번호</strong>, <strong>XXH3-64 <code>rev</code></strong> 해시를 공유합니다.</p>

{callout("tip", "한 줄로 요약한 설계 원칙", "<p>트라이그램 인덱스는 <em>열지 않을 파일만</em> 걸러냅니다. 후보 파일과 수정된 파일은 풀스캔과 완전히 동일한 평가기로 검증되며, 모든 쓰기는 배타적 잠금 기반 compare-and-swap으로 처리됩니다.</p>")}

{h2("install", "설치 및 빌드 변형")}
<p>릴리스마다 macOS(Apple Silicon·Intel), Linux(x86_64/arm64 glibc 및 정적 musl), Windows용 사전 빌드 바이너리를 제공합니다.</p>

{code("macOS / Linux (Homebrew)", "brew tap CREE1116/lancet https://github.com/CREE1116/lancet\nbrew install CREE1116/lancet/lancet", "ko")}
{code("macOS / Linux (프리빌트 설치 스크립트)", "curl -fsSL https://raw.githubusercontent.com/CREE1116/lancet/main/install.sh | sh", "ko")}
{code("Windows (PowerShell)", "irm https://raw.githubusercontent.com/CREE1116/lancet/main/install.ps1 | iex", "ko")}
{code("Cargo (전체 빌드 vs 코드 전용 경량 빌드)", "# cargo-binstall로 바이너리 바로 받기\ncargo binstall --git https://github.com/CREE1116/lancet lancet\n\n# 전체 기능 소스 빌드 (~40 MB: Tree-sitter 24개 문법 + PDF/오피스/HWP/압축 어댑터)\ncargo install --git https://github.com/CREE1116/lancet lancet\n\n# 경량 빌드 (~4 MB: 문서/압축 어댑터 제외, 순수 코드·텍스트 전용)\ncargo install --git https://github.com/CREE1116/lancet lancet --no-default-features", "ko")}

{h2("walkthrough", "60초 전체 흐름 따라하기")}
<p><code>.lancet</code> 인덱스는 선택 사항입니다. 인덱스가 없어도 모든 명령이 동작하며, 기본 <code>auto</code> 모드는 매 쿼리마다 디스크 메타데이터를 확인해 수정된 파일을 놓치지 않습니다.</p>
{code("터미널", "# 1. 선택: 로컬 트라이그램 인덱스 한 번 빌드\nlancet index .\n\n# 2. 폴더 구조 파악 후 참조 수 순위로 핵심 심볼 찾기\nlancet fd --tree --dirs -d 2 .\nlancet refs --rank '*Service' --kind class src --limit 8\n\n# 3. ripgrep 플래그 또는 LQL 논리·근접 쿼리로 검색\nlancet rg -w execute_one src\nlancet 'near(\"gradient\", \"clip\", 30) in function | expand parent'\n\n# 4. 필요한 심볼이나 줄 범위만 읽고 XXH3-64 `rev` 확보\nlancet read src/search.rs --symbol execute_one --json\n\n# 5. dry-run diff 확인 후 --rev, --apply로 원자적 반영\nlancet edit src/search.rs --find 'let limit = 20;' \\\n  --with 'let limit = 50;' --rev <REV> --apply", "ko")}

{h2("cheat-sheet", "한 화면 명령어 치트시트")}
{table(
    ["작업 단계", "명령어", "보장 / 특징"],
    [
        ["폴더 레이아웃", "<code>lancet fd --tree --dirs -d 2 .</code>", "<code>.gitignore</code> / <code>.ignore</code> 반영"],
        ["최근/확장자 필터", "<code>lancet fd -t f -e rs --changed-within 7d</code>", "단위: <code>s</code>, <code>m</code>, <code>h</code>, <code>d</code>, <code>w</code>"],
        ["rg 그대로 쓰기", "<code>lancet rg -w -t rs 'pat' src</code>", "<code>rg</code>와 바이트 동일 + <code>--docs</code>"],
        ["논리·유사 LQL", "<code>lancet '(\"a\" OR \"b\") NOT \"c\" in function'</code>", "<code>Scan == Auto</code>"],
        ["다중 쿼리 배치", "<code>lancet batch src -q 'Q1' -q 'Q2'</code>", "인덱스·워크 1회 공유"],
        ["뼈대 추출", "<code>lancet outline src/main.rs</code>", "함수·마크다운 제목·시트·압축 목록"],
        ["토큰 예산 읽기", "<code>lancet read f.rs --lines 10:50 --budget-tokens 800</code>", "<code>rev</code> + <code>next_line</code> 커서 반환"],
        ["심볼 읽기", "<code>lancet read f.rs --symbol my_fn</code>", "문서 주석·어트리뷰트 포함"],
        ["문서·압축파일", "<code>lancet read r.pdf -p 2:3</code> · <code>a.zip!app.log</code>", "검색과 1:1 동일 줄 번호; 읽기 전용"],
        ["참조 순위", "<code>lancet refs --rank '*Service' --kind class src</code>", "어휘 AST 참조 수 내림차순"],
        ["AST 검색·치환", "<code>lancet ast '$X.unwrap()' src -l rs -r '$X?'</code>", "24개 언어; <code>--apply</code> 전엔 dry-run"],
        ["원자적 다중 편집", "<code>lancet edit f.rs --edits - --rev REV --apply</code>", "원본 줄 번호 유지, 겹침 검증 CAS"],
        ["Unified Diff", "<code>git diff | lancet patch --apply</code>", "전체 파일 사전 검증 + 경합 롤백"],
        ["로컬 이름 변경", "<code>lancet rename f.rs:42 old new --rev REV --apply</code>", "6개 언어; 스코프 재해석 증명"],
        ["MCP 서버", "<code>lancet mcp --root .</code>", "11개 도구 stdio JSON-RPC, <code>--read-only</code>"],
        ["인덱스 진단", "<code>lancet explain 'pat'</code> · <code>lancet status</code>", "필요 트라이그램·후보 비율 출력"],
    ],
)}

{h2("next-steps", "어디부터 볼까요?")}
{cards([
    ("시작하기", "도구 전환 가이드 & 필터 규칙", "rg/fd/sg/sed/patch 1:1 대조표와 .gitignore, 글롭, 파일 타입 평가 규칙.", "migration.html"),
    ("탐색과 검색", "탐색 · LQL · AST · 문서 읽기", "fd/outline/refs 랭킹, LQL 근접·퍼지, 24개 언어 AST, PDF/HWP/압축파일.", "guides/explore.html"),
    ("편집과 에이전트", "편집 · 패치 · 이름 변경 · MCP", "--rev CAS 동시성 제어, patch 롤백, 스코프 증명 기반 rename, MCP 연동.", "guides/edit.html"),
    ("내부 구조 & 레퍼런스", "LANCET04 · 쓰기 안전성 · 스키마", "바이너리 세그먼트 포맷, racy 검사, OS 잠금, CLI/JSON/MCP 명세 및 문제 해결.", "reference/cli.html"),
])}
"""


def _migration(_d: str) -> str:
    return f"""
<p>기존 작업 습관을 한 번에 바꿀 필요는 없습니다. <code>lancet rg</code>와 <code>lancet patch</code>로 익숙한 플래그와 diff 파이프를 그대로 쓰면서, 도구 사이의 이음새에서 오류가 생기는 지점부터 LQL·AST·<code>--rev</code> CAS를 도입할 수 있습니다.</p>

{h2("from-rg", "ripgrep(rg)에서 오셨다면")}
<p><code>lancet rg</code>는 ripgrep의 플래그, Rust <code>regex</code> 엔진, 무시 파일 순회, 종료 코드(<code>0</code> 매치, <code>1</code> 없음, <code>2</code> 오류), 출력 형식을 그대로 구현했으며 68개 엣지 케이스 픽스처와 Linux 커널 전체 쿼리를 <code>ripgrep 15.2</code>와 바이트 단위로 대조 검증했습니다.</p>
{table(
    ["ripgrep 명령", "lancet 대안", "정확한 동작 차이"],
    [
        ["<code>rg foo src</code>", "<code>lancet rg foo src</code>", "파이프 시에도 <code>-n</code>이 기본 ON; 끄려면 <code>-N</code>"],
        ["<code>rg -i -w -t py tok</code>", "<code>lancet rg -i -w -t py tok</code>", "<code>-i</code>/<code>-s</code>/<code>-S</code> 마지막 플래그 우선; AST 기반 <code>-S</code>(<code>\\W</code> 무시 유지)"],
        ["<code>rg -g '*.toml' --passthru</code>", "<code>lancet rg -g '*.toml' --passthru</code>", "<code>--passthru</code>/<code>-A</code>/<code>-B</code>/<code>-C</code> 마지막 우선; 미일치 파일도 출력"],
        ["<code>rg --type-add 'web:*.tsx'</code>", "<code>lancet rg --type-add 'web:*.tsx' -t web</code>", "<code>name:glob</code>, <code>name:include:a,b</code> 모두 지원"],
        ["<code>rg -P '(?&lt;=a)b'</code>", "<code>lancet 'before(\"a\", \"b\", 1)'</code> 또는 Rust regex", "<code>-P</code>/PCRE2는 선형 시간·건전한 트라이그램 컴파일을 위해 미지원"],
        ["<code>rg --pre pdftotext pat</code>", "<code>lancet rg --docs pat</code>", "외부 프로세스 없이 인프로세스로 PDF·오피스·HWP·압축 검색"],
    ],
)}

{callout("note", "lancet rg의 의도적 차이 3가지", "<ul><li><strong>항상 경로 요소순 정렬:</strong> <code>a/b.txt</code>가 <code>a.txt</code>보다 앞(<code>rg --sort path</code>와 동일).</li><li><strong>줄 번호(<code>-n</code>) 기본 ON:</strong> 파이프·리다이렉션 때도 켜져 있으며 <code>-N</code>으로 끕니다.</li><li><strong>파일명 헤더 없음:</strong> 항상 <code>path:line:text</code> 한 줄 형식이며 <code>--color auto</code>는 TTY에서만 색을 입힙니다.</li></ul>")}

{h2("from-fd", "fd / find에서 오셨다면")}
{table(
    ["fd / find 명령", "lancet fd 대안", "비고 및 추가 기능"],
    [
        ["<code>fd -e rs -d 2 src</code>", "<code>lancet fd -e rs -d 2 src</code>", "<code>.lancet</code> 인덱스 활용; <code>[PATH]</code>는 최대 1개"],
        ["<code>fd -t d -d 2</code>", "<code>lancet fd --tree --dirs -d 2 .</code>", "파일 없이 폴더 트리만 간결하게 출력"],
        ["<code>find . -mtime -7 -type f</code>", "<code>lancet fd -t f --changed-within 7d</code>", "단위: <code>s</code>, <code>m</code>, <code>h</code>, <code>d</code>, <code>w</code>"],
        ["<code>find . -name '*.pdf'</code>", "<code>lancet fd --type-class document</code>", "확장자 나열 없이 9개 의미 클래스로 필터링"],
        ["<code>fd config src</code> (패턴이 폴더명과 같을 때)", "<code>lancet fd -p config src</code>", "<code>-p</code>/<code>--pattern</code>으로 패턴과 경로를 명확히 구분"],
    ],
)}

{h2("from-ast", "ast-grep(sg)에서 오셨다면")}
{table(
    ["ast-grep", "lancet ast", "중요한 차이점"],
    [
        ["<code>sg -p '$X.unwrap()' -l rs</code>", "<code>lancet ast '$X.unwrap()' -l rs</code>", "패턴에 리터럴이 있으면 <code>.lancet</code> 트라이그램 프리필터로 가속"],
        ["<code>sg -p 'f($$$A)' -r 'g($$$A)' -U</code>", "<code>lancet ast 'f($$$A)' -r 'g($$$A)' --apply</code>", "<code>--apply</code> 전엔 dry-run diff; <code>--apply</code>는 <code>--limit</code>과 병행 불가"],
        ["<code>sg -p 'if ($C) $S'</code>", "<code>lancet ast 'if ($C) $S'</code>", "<strong>엄격한 AST 모양 매치:</strong> <code>else</code> 없는 <code>if</code>만 매치(<code>if-else</code>는 별개 노드)"],
        ["<code>sg -l cpp ...</code>", "<code>lancet ast ... -l cpp</code>", "Lancet <code>cpp</code>는 <code>.cpp</code>/<code>.cc</code>뿐 아니라 <code>.h</code> 헤더도 함께 검색"],
    ],
)}

{h2("from-edit", "cat / sed / patch / rga에서 오셨다면")}
{table(
    ["기존 도구 사슬", "Lancet 대체", "해결된 사고"],
    [
        ["<code>sed -n '10,50p' f.rs</code>", "<code>lancet read f.rs --lines 10:50</code>", "XXH3-64 <code>rev</code> 반환 + <code>--budget-tokens</code> 상한"],
        ["<code>rg -n</code> 1회 후 <code>sed -i</code> 2회", "<code>lancet edit f.rs --edits - --rev REV --apply</code>", "모든 구간이 원본 줄 번호 기준; 심볼릭/하드 링크·권한 보존"],
        ["<code>patch -p1 &lt; diff</code>", "<code>lancet patch diff --apply</code>", "모든 파일 사전 검증, 정확 위치 재배치, CAS 경합 롤백"],
        ["<code>rga</code> + <code>pdftotext</code>/<code>pandoc</code>", "<code>lancet 'pat'</code> + <code>lancet read f.pdf -p 2</code>", "같은 추출 파이프라인으로 검색 줄 == 읽기 줄"],
    ],
)}

{h2("when-not", "전문 도구를 그대로 써야 하는 경우")}
{callout("warn", "의도적인 범위 경계", "<ul><li><strong>버전 관리:</strong> <code>git</code>은 그대로 쓰세요. Lancet은 <code>.gitignore</code>를 준수하며 <code>git diff</code>를 <code>lancet patch</code>로 적용합니다.</li><li><strong>여러 파일에 걸친 타입 기반 리팩터링:</strong> 공개 API·필드·메서드·Python 키워드 매개변수의 크로스 파일 이름 변경은 LSP(<code>rust-analyzer</code>, <code>tsserver</code>, <code>gopls</code>)를 쓰세요. Lancet의 <code>rename</code>은 단일 파일 로컬 바인딩만 증명해 변경합니다.</li></ul>")}
"""


def _filtering(_d: str) -> str:
    return f"""
<p>Lancet은 <code>lancet</code>, <code>rg</code>, <code>fd</code>, <code>outline</code>, <code>refs</code>, <code>ast</code>, <code>index</code> 전역에서 ripgrep의 <code>ignore</code> 크레이트 순회 규칙을 동일하게 적용합니다.</p>

{h2("ignore-files", "무시 규칙 우선순위, 숨김 파일 & 바이너리")}
<ul>
  <li><strong>무시 파일 스택:</strong> <code>.gitignore</code>, Git exclude, <code>.ignore</code>, <code>.rgignore</code>를 준수합니다(<code>--no-ignore</code> 또는 <code>-u</code>로 해제).</li>
  <li><strong>숨김 파일·폴더:</strong> <code>.</code>으로 시작하는 경로(<code>.lancet</code> 포함)는 기본 제외되며 <code>--hidden</code> / <code>-.</code>(<code>lancet rg</code>에선 <code>-uu</code>)로 포함합니다.</li>
  <li><strong>바이너리 판별:</strong> 일반 텍스트 검색은 NUL 바이트가 있는 바이너리를 건너뜁니다(<code>-a</code> / <code>--text</code> / <code>-uuu</code> 또는 문서 어댑터 확장자 제외).</li>
  <li><strong>긍정 글롭 우선:</strong> <code>lancet rg</code>에서 <code>-g '*.lock'</code> 같은 긍정 글롭은 ripgrep과 똑같이 ignore/hidden 설정을 덮어씁니다.</li>
</ul>

{h2("rg-unrestricted", "lancet rg 제한 해제 단계 (-u / -uu / -uuu)")}
{table(
    ["플래그", "동등 옵션", "동작"],
    [
        ["<code>-u</code>", "<code>--no-ignore</code>", "ignore 파일(빌드 산출물, <code>node_modules</code> 등)도 검색"],
        ["<code>-uu</code>", "<code>--no-ignore --hidden</code>", "숨김 파일과 디렉터리도 검색"],
        ["<code>-uuu</code>", "<code>--no-ignore --hidden --text</code>", "바이너리 파일까지 텍스트로 간주해 검색"],
    ],
)}

{h2("globs-types", "글롭, 내장 파일 타입 & --type-add")}
{code("터미널", "# 포함/제외 글롭 (!로 부정; 대소문자 무시 --iglob도 지원)\nlancet rg -g 'src/**' -g '!src/generated/**' 'retry' .\n\n# ripgrep 내장 파일 타입 포함/제외\nlancet rg -t rust -T test 'unsafe' .\n\n# 사용자 정의 타입 추가 또는 기존 타입 병합 (중복 지정 가능)\nlancet rg --type-add 'web:*.{ts,tsx,css}' -t web 'ThemeProvider' .\nlancet rg --type-add 'jvm:include:java,kotlin,scala' -t jvm 'Executor' .\n\n# --type-add가 반영된 타입 목록 확인\nlancet rg --type-add 'proto:*.proto' --type-list", "ko")}

{h2("lql-filters", "LQL & fd 필터 (type:, ext:, path:, --type-class)")}
<p>LQL에서 같은 카테고리 내 여러 값은 <strong>OR</strong>로, 서로 다른 카테고리는 <strong>AND</strong>로 결합됩니다. <code>src/walk.rs</code> 규칙상 <code>path:</code> 값에 글롭 메타문자(<code>*</code>, <code>?</code>, <code>[</code>, <code>{{</code>)가 없으면 <code>dir</code> 자체와 <code>dir/**</code>를 모두 매치합니다.</p>
{code("터미널", "# ext:(...) 내부는 OR, ext:와 path: 사이는 AND\nlancet '\"deadline\" in ext:(rs,toml) path:crates/core'\n\n# 의미 클래스로 최근 수정 문서 찾기\nlancet fd --type-class document,spreadsheet --changed-within 14d", "ko")}
{table(
    ["클래스 (<code>type:</code> / <code>--type-class</code>)", "포함 확장자 / 예시"],
    [
        ["<code>code</code>", "24개 이상 프로그래밍 언어 소스 (<code>.rs</code>, <code>.py</code>, <code>.ts</code>, <code>.go</code>, <code>.c</code>, <code>.cpp</code> 등)"],
        ["<code>document</code>", "<code>.md</code>, <code>.txt</code>, <code>.rst</code>, <code>.pdf</code>, <code>.docx</code>, <code>.pptx</code>, <code>.odt</code>, <code>.hwp</code>, <code>.hwpx</code>, <code>.epub</code>"],
        ["<code>spreadsheet</code>", "<code>.xlsx</code>, <code>.xls</code>, <code>.xlsm</code>, <code>.xlsb</code>, <code>.ods</code>, <code>.csv</code>, <code>.tsv</code>"],
        ["<code>config</code>", "<code>.toml</code>, <code>.yaml</code>, <code>.yml</code>, <code>.ini</code>, <code>.env</code>, <code>.hcl</code>, <code>.tf</code>"],
        ["<code>data</code>", "<code>.json</code>, <code>.jsonc</code>, <code>.xml</code>, <code>.sql</code> 등"],
        ["<code>log</code> · <code>archive</code> · <code>image</code> · <code>other</code>", "<code>.log</code> · <code>.zip/.tar/.gz/.tgz</code> · <code>.png/.jpg/.webp/…</code> · 기타"],
    ],
)}

{h2("bounds", "심볼릭 링크, 깊이 및 파일 크기 제한")}
<ul>
  <li><strong>심볼릭 링크 (<code>-L</code> / <code>--follow</code>):</strong> 기본 비활성. <code>lancet rg -L --trust-index</code>에서는 <code>.lancet</code>에 빠진 링크 대상까지 놓치지 않도록 의도적으로 라이브 순회로 폴백합니다.</li>
  <li><strong>깊이·크기 상한:</strong> <code>--max-depth N</code>(<code>1</code>은 직계 자식만)과 <code>--max-filesize NUM[K|M|G]</code>(<code>rg</code>), <code>--min-size</code>/<code>--max-size</code>(<code>fd</code>)가 라이브 순회와 <code>--trust-index</code> 양쪽에서 모두 적용됩니다.</li>
  <li><strong><code>fd</code> 단일 경로 규칙:</strong> <code>lancet fd [PATTERN] [PATH]</code>는 최대 하나의 <code>PATH</code>만 받으며, 패턴이 폴더명과 같으면 <code>-p PATTERN</code>으로 명시하세요.</li>
</ul>
"""


def _explore(_d: str) -> str:
    return f"""
<p>파일 전체를 열어 보는 대신 <code>fd</code>, <code>outline</code>, <code>refs</code>로 디렉터리, 선언 골격, 사용 빈도순 핵심 심볼을 먼저 지도화하면 컨텍스트 토큰 소비를 크게 줄일 수 있습니다.</p>

{h2("fd-guide", "빠른 트리 & 메타데이터 탐색 (lancet fd)")}
<p><code>lancet fd</code>는 디스크를 순회하거나 <code>--trust-index</code>로 mmap 된 <code>.lancet</code>에서 워크 없이 즉시 목록을 읽어 옵니다.</p>
{code("터미널", "# 파일 소음 없이 폴더 계층만 출력\nlancet fd --tree --dirs --max-depth 2 .\n\n# 글롭/정규식 매칭 (패턴이 디렉터리명처럼 보이면 -p 사용)\nlancet fd -g '*_test.rs' crates/\nlancet fd -p config -r 'auth|session' src\n\n# 수정 시각 + 크기 조건 (기간: s, m, h, d, w / 크기: b, k, m, g)\nlancet fd -t f --changed-within 48h --min-size 4k --max-size 2m .\n\n# size, mtime_ns 포함 JSON (반환 항목당 stat 1회 비용)\nlancet fd -e rs --meta --limit 25 --json src", "ko")}
{callout("note", "fd에서 -e / --type-class / 크기 필터를 쓸 때", "<p><code>-t</code>를 주지 않은 채 <code>-e</code>, <code>--type-class</code>, <code>--min-size</code>, <code>--max-size</code>를 지정하면 <code>--dirs</code>나 <code>-t d</code>를 함께 주지 않는 한 자동으로 파일만 추립니다.</p>")}

{h2("outline-guide", "코드·문서·시트·압축 뼈대 추출 (lancet outline)")}
<p><code>lancet outline</code>은 AST 선언 시그니처(코드), 마크다운·PDF·PPTX·DOCX·HWP 제목, 엑셀 시트명과 크기, 압축 파일 내부 항목 목록을 줄 번호와 함께 뽑아 줍니다.</p>
{code("터미널", "# 단일 파일 AST 시그니처와 시작/끝 줄 범위\nlancet outline src/search.rs\n\n# 디렉터리 깊이·확장자 제한 요약\nlancet outline src -d 2 -e rs --limit 20 --json\n\n# 비코드 파일 뼈대: 마크다운 목차, 엑셀 시트, tar.gz 내부 파일 목록\nlancet outline docs/spec.md\nlancet outline financial_model.xlsx\nlancet outline logs/crash_bundle.tar.gz", "ko")}

{h2("refs-and-rank", "어휘 AST 참조 & 사용 빈도 랭킹 (lancet refs)")}
<p><code>lancet refs</code>는 24개 Tree-sitter 문법으로 후보 파일을 파싱해 <code>[def]</code> 선언과 <code>[ref]</code> 참조 위치를 분리하고, 감싸는 함수(<code>enclosing</code>)를 기록하며, <strong>주석과 문자열 리터럴을 자동 제외</strong>합니다.</p>
{code("터미널", "# 정의와 모든 어휘 호출·참조 위치 조회\nlancet refs SearchOptions src --lang rs\n\n# 코드 외 파일(Markdown, TOML 등)까지 어휘 매칭으로 포함\nlancet refs SearchOptions . --all-files", "ko")}

{h3("rank-mode", "refs --rank로 핵심 선언 순위 뽑기")}
<p><code>--rank</code>를 주면 첫 인자를 선언 이름 글롭(예: <code>*Service</code>, <code>*Handler</code>)으로 해석해 어휘 참조 횟수 내림차순으로 정렬합니다. 글롭에 리터럴 구간(예: <code>Service</code>)이 있으면 트라이그램 인덱스로 후보를 먼저 좁힙니다.</p>
{code("터미널", "# src/에서 가장 많이 참조되는 *Service 클래스 상위 10개\nlancet refs --rank '*Service' --kind class src --limit 10\n\n# Rust 크레이트 내 핵심 function, struct 순위\nlancet refs --rank '*' --kind function,struct src --limit 15 --json", "ko")}
<p>여러 선언이 같은 이름을 공유할 때는(예: <code>new</code>, <code>run</code>) 터미널에 <code>(name shared by N declarations)</code>가, JSON에 <code>definitions: N</code>이 표시되어 어휘 참조 수가 공유됨을 알려 줍니다.</p>
"""


def _search(_d: str) -> str:
    return f"""
<p>Lancet은 같은 트라이그램 인덱스와 평가기 위에서 두 가지 검색 문법을 제공합니다: 익숙한 정규식·UNIX 플래그를 쓰는 <code>lancet rg</code>와 논리·근접·퍼지·AST 스코프·파이프를 조합하는 <strong>LQL</strong>입니다.</p>

{h2("rg-vs-lql", "lancet rg vs LQL vs lancet batch 선택 기준")}
{table(
    ["목적", "선택", "이유"],
    [
        ["일반 정규식, 문맥 줄(<code>-A/-B/-C</code>), <code>--vimgrep</code>, <code>-c</code>, <code>-r</code>", "<code>lancet rg</code>", "ripgrep 플래그 그대로, 학습 비용 0"],
        ["다중 항 <code>AND</code>/<code>OR</code>/<code>NOT</code>, <code>near()</code>, <code>fuzzy()</code>, <code>in function</code>, <code>| budget</code>", "<code>lancet '&lt;LQL&gt;'</code>", "복합 질의를 1패스로 처리 + 토큰 예산 제어"],
        ["같은 저장소에 LQL 쿼리 여러 개 연속 실행", "<code>lancet batch</code>", "인덱스 열기 및 디렉터리 순회를 1회로 공유"],
    ],
)}

{h2("lql-recipes", "실전 LQL 쿼리 레시피")}
{code("터미널", "# 1. 한 줄 안에서의 불리언 논리 (공백은 암묵적 AND, OR/NOT 명시)\nlancet '(\"OOM\" OR \"out of memory\") NOT \"CPU\" in line' src/\n\n# 2. Tree-sitter 함수 내부에서의 토큰 근접성 + 부모 함수 전체 펼치기\nlancet 'near(\"retry\", \"backoff\", 25) in function | expand parent | limit 8' src/\n\n# 3. 순서 있는 근접 (A 뒤에 15토큰 이내 B 등장)\nlancet 'before(\"lock\", \"write_atomic\", 15) in function' src/\n\n# 4. 오타 허용 검색 (레벤슈타인 거리 k <= 3, q-gram 인덱스 가속)\nlancet 'fuzzy(\"idempotnt\", 2) | limit 10' src/\n\n# 5. 검색 결과에 함께 많이 나온 키워드 상위 15개 추출 (쿼리 구체화용)\nlancet '\"trigram\" in ext:rs | terms top:15' src/\n\n# 6. 추정 토큰 예산으로 출력 크기 하드 캡\nlancet '\"SearchOptions\" in ext:rs | expand context:3 | budget tokens:1500' src/", "ko")}

{h2("batch-queries", "한 번의 워크로 여러 쿼리 실행 (lancet batch)")}
<p>에이전트가 관련 검색을 3~5회 연달아 돌릴 때는 <code>lancet batch</code>로 인덱스와 디렉터리 목록을 메모리에 공유하세요:</p>
{code("터미널", "# 반복 가능한 -q / --name 플래그\nlancet batch src \\\n  -q '\"Conflict\" in function' --name conflicts \\\n  -q 'near(\"mtime\", \"ctime\", 20)' --name freshness \\\n  -q 'fuzzy(\"compaction\", 1) | files' --name compact\n\n# JSON 객체 또는 배열 직접 전달\nlancet batch src --queries-json '{\"errors\": \"Status::Unsupported\", \"locks\": \"lock_shared OR lock()\"}'", "ko")}

{h2("debug-query", "쿼리 계획 점검 (lancet parse & lancet explain)")}
<p>대형 저장소에 쿼리를 날리기 전에 <code>parse</code>로 정규화된 정규식을 보고, <code>explain</code>으로 추출된 트라이그램과 인덱스 후보 비율을 확인하세요:</p>
{code("터미널", "lancet parse '(\"alpha\" OR \"beta\") NOT NOT \"gamma\" in line | limit 5'\nlancet explain 'near(\"gradient\", \"clip\", 30)' ./repo", "ko")}
<p><code>explain</code>은 <code>trigram_req</code>, <code>files_considered</code>, <code>index_candidates</code>, <code>stale_files</code>, <code>selectivity_pct</code>를 출력해 풀스캔 폴백 여부를 즉시 보여 줍니다.</p>
"""


def _ast(_d: str) -> str:
    return f"""
<p><code>lancet ast</code>는 내장된 24개 Tree-sitter 문법으로 구문 트리를 매치합니다. 공백, 개행, 주석, 문자열 내용 차이를 무시하고 실제 코드 구조만 겨냥합니다.</p>

{h2("metavariables", "메타변수와 구조 동등성 제약")}
<ul>
  <li><strong><code>$VAR</code> (단일 명명 노드):</strong> 이름 있는 AST 노드 1개를 캡처합니다. 같은 이름을 반복하면(예: <code>$X == $X</code>, <code>$X = $X</code>) 두 서브트리의 <strong>구조적 동등성</strong>을 강제합니다.</li>
  <li><strong><code>$_</code> (익명 와일드카드):</strong> 단일 노드에 매치되지만 리라이트 캡처에 저장하지 않습니다.</li>
  <li><strong><code>$$$ARGS</code> (가변 형제 노드):</strong> 0개 이상의 형제 노드(함수 인자, 구조체 필드, 문장 블록)에 매치합니다.</li>
  <li><strong>C 계열 문장 메타변수:</strong> C, C++, Rust, Go, JS/TS/TSX, Java, C# 등에서 <code>if ($C) $S</code>, <code>{{ $$$B }}</code> 같은 문장/블록 패턴이 동작합니다.</li>
</ul>

{h2("recipes", "언어별 AST 검색·치환 레시피")}
{code("터미널", "# Rust: .unwrap()을 ? 연산자로 바꾸는 dry-run 미리보기\nlancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'\n\n# Rust: 가변 인자 panic! 호출 찾기\nlancet ast 'panic!($$$ARGS)' src --lang rs\n\n# Python: 자기 자신과의 중복 비교 탐지 ($X 구조 동등성)\nlancet ast '$X == $X' . --lang py\n\n# Go: 테스트 파일을 제외한 fmt.Errorf 래핑 일괄 치환 미리보기\nlancet ast 'fmt.Errorf($FMT, $ERR)' pkg/ --lang go --path '!*_test.go' --rewrite 'errors.Wrap($ERR, $FMT)'\n\n# TypeScript / TSX: console.log 호출 찾기\nlancet ast 'console.log($$$ARGS)' web/ --lang tsx", "ko")}

{h2("rewrites", "안전한 리라이트와 원자적 --apply")}
<ol>
  <li><strong>먼저 미리보기:</strong> <code>--rewrite TEMPLATE</code>만 주면 디스크를 수정하지 않고 unified diff(<code>mode: "dry-run"</code>)를 출력합니다.</li>
  <li><strong><code>skipped_overlaps</code> 확인:</strong> 한 파일 안에 중첩된 매치(예: 중첩된 <code>$X.unwrap().foo.unwrap()</code>)가 있으면 가장 앞/바깥쪽 편집을 우선 적용하고 겹치는 안쪽 편집은 건너뛰어 오프셋 손상을 막습니다. 안쪽 잔여분은 한 번 더 실행하면 됩니다.</li>
  <li><strong>원자적 기록:</strong> <code>--apply</code>를 추가하면 Tree-sitter 구문 검사 게이트와 <code>write_atomic</code>을 거쳐 반영합니다.</li>
</ol>

{callout("warn", "주의할 제약과 ast-grep과의 차이", "<ul><li><strong><code>--apply</code>는 <code>--limit</code>과 병행 불가:</strong> 일부 개수만 잘린 리라이트가 실수로 반영되지 않도록 <code>--limit</code>을 거절합니다. 대신 <code>--path</code> 글롭으로 대상을 좁히세요.</li><li><strong>엄격한 <code>if</code> 모양:</strong> <code>if ($C) $S</code>는 <code>else</code>가 없는 순수 <code>if</code>만 매치합니다(<code>ast-grep</code>은 <code>if ... else</code>도 함께 매치).</li><li><strong><code>--lang cpp</code>의 <code>.h</code> 포함:</strong> C++ 검색 시 <code>.cpp</code>/<code>.cc</code>/<code>.cxx</code>/<code>.hpp</code>뿐 아니라 <code>.h</code> 헤더 파일도 스캔합니다.</li><li><strong>리터럴 없는 패턴:</strong> 3바이트 이상 리터럴이 없으면 <code>.lancet</code>으로 좁힐 수 없어 해당 언어 파일을 전부 파싱합니다(<code>ast-grep</code>의 0.6~1.0배 속도; 최악의 <code>$$$</code> 백트래킹은 메모이제이션과 단계 예산으로 제한).</li></ul>")}
"""


def _read_docs(_d: str) -> str:
    return f"""
<p><code>lancet read</code>는 소스 코드, PDF, 오피스 문서, 한글(HWP/HWPX), 압축 파일에서 필요한 조각만 꺼내 오며, 검색 출력과 <strong>1:1로 일치하는 줄 번호</strong>를 반환합니다.</p>

{h2("code-reading", "코드 읽기: 줄 범위, 심볼 & 토큰 예산")}
{code("터미널", "# 인라인 줄 범위 (file:10-50, file:42, --lines 100: 모두 지원)\nlancet read src/search.rs:100-160\n\n# Tree-sitter 선언 전체 읽기 (위쪽 문서 주석·어트리뷰트 포함!)\nlancet read src/search.rs --symbol execute_one --json\n\n# 추정 토큰 수로 자르고 다음 이어 읽기 커서(next_line) 받기\nlancet read src/mcp.rs --lines 1:800 --budget-tokens 800 --json\n\n# 파이프/복사용으로 헤더와 줄 번호 거터 생략\nlancet read src/lib.rs --lines 1:40 --raw", "ko")}
<p>모든 일반 텍스트 응답에는 파일 바이트의 16자리 16진수 <strong><code>rev</code></strong>(XXH3-64)가 포함됩니다. 이 값을 <a href="edit.html"><code>lancet edit --rev</code></a>나 <a href="rename.html"><code>lancet rename --rev</code></a>에 그대로 넘기면 됩니다.</p>

{h2("formats", "지원 문서 · 스프레드시트 · 압축 포맷")}
{table(
    ["포맷", "확장자", "Read 선택자", "비고"],
    [
        ["PDF", "<code>.pdf</code>", "<code>--page 2</code> · <code>--page 2:5</code> · <code>--lines</code>", "Pure-Rust 인프로세스 추출; <code>--ocr</code> 선택"],
        ["Word / ODT / EPUB", "<code>.docx</code>, <code>.odt</code>, <code>.epub</code>", "<code>--lines START:END</code>", "XML 문단 스트림 추출"],
        ["PowerPoint", "<code>.pptx</code>", "<code>--page 3</code> · <code>--page 1:4</code>", "슬라이드를 <code>[Slide N]</code> 섹션에 매핑"],
        ["한글 (HWP)", "<code>.hwp</code> (5.0 OLE), <code>.hwpx</code>", "<code>--lines START:END</code>", "네이티브 OLE/XML 추출기"],
        ["스프레드시트", "<code>.xlsx</code>, <code>.xls</code>, <code>.xlsm</code>, <code>.xlsb</code>, <code>.ods</code>", "<code>--sheet S</code> · <code>--rows 1:30</code> · <code>--cols A:E</code>", "<code>--sheet</code>는 시트 이름 또는 1부터 시작하는 번호"],
        ["압축 파일", "<code>.zip</code>, <code>.tar</code>, <code>.gz</code>, <code>.tgz</code>, <code>.tar.gz</code>", "<code>bundle.zip!inner/file.log</code> 또는 <code>--entry</code>", "<code>--lines</code> 및 <code>--budget-tokens</code>와 조합 가능"],
        ["이미지 (<code>--ocr</code> 사용 시)", "<code>.png</code>, <code>.jpg</code>, <code>.jpeg</code>, <code>.tif</code>, <code>.tiff</code>, <code>.bmp</code>, <code>.webp</code>", "<code>--ocr</code>", "<code>PATH</code>의 외부 <code>tesseract</code> 필요"],
    ],
)}

{h2("rg-docs", "비텍스트 문서의 검색 vs 읽기 규칙")}
<ul>
  <li><strong><code>lancet '&lt;LQL&gt;'</code>:</strong> 문서·압축 어댑터를 기본 활성화해 인덱싱·검색합니다(<code>type:document</code>, <code>type:spreadsheet</code>, <code>type:archive</code> 필터).</li>
  <li><strong><code>lancet rg --docs</code>:</strong> <code>lancet rg</code>는 <code>ripgrep</code>과 바이트 일치 출력을 지키기 위해 기본 OFF입니다. 문서를 포함하려면 <code>--docs</code>를 붙이세요.</li>
</ul>
{code("터미널", "# 스프레드시트에서 키워드 위치 확인 후 해당 셀 범위만 읽기\nlancet '\"EBITDA\" in type:spreadsheet' ./finance\nlancet read finance/q4.xlsx --sheet 1 --rows 12:35 --cols A:F\n\n# 압축 파일 목록 확인 후 특정 로그만 스트리밍\nlancet outline artifacts/bundle.zip\nlancet read artifacts/bundle.zip!worker_0/panic.log --lines 1:80 --ocr", "ko")}

{h2("caps-safety", "자원 상한, 패닉 격리 & 읽기 전용 보장")}
<ul>
  <li><strong>하드 상한 (<code>src/adapters.rs</code>):</strong> 입력 파일 최대 <strong>64 MiB</strong>(<code>MAX_INPUT_BYTES</code>), 추출 UTF-8 버퍼 최대 <strong>16 MiB</strong>(<code>MAX_EXTRACTED_BYTES</code>, UTF-8 문자 경계에서 안전 절단), 압축 내부 순회 최대 <strong>500개 항목</strong>(<code>MAX_ARCHIVE_ENTRIES</code>)으로 압축 폭탄을 차단합니다.</li>
  <li><strong>패닉 격리:</strong> 손상된 PDF/문서 파서 패닉은 <code>catch_unwind</code>로 격리해 전체 워커를 죽이지 않고 읽기 불가로 처리합니다.</li>
  <li><strong>읽기 전용 계약:</strong> 문서와 압축 항목은 JSON에 <code>rev</code>를 내지 않으며 <code>lancet edit</code>이 수정을 거부합니다.</li>
  <li><strong>스캔 PDF OCR:</strong> <code>PATH</code>에 <code>pdftoppm</code>과 <code>tesseract</code>가 있을 때 <code>--ocr</code>을 넘기면 광학 문자를 추출합니다.</li>
</ul>
"""


PAGES_KO_PART1 = [
    {
        "path": "index.html",
        "group": "start",
        "nav_title": "설치 · 빠른 시작",
        "title": "Lancet 공식 문서 — 설치 · 빠른 시작 · 치트시트",
        "description": "macOS, Linux, Windows 설치, 60초 워크스루, 15개 하위 명령 한 화면 치트시트.",
        "lede": "탐색, 트라이그램 인덱스 검색, 24개 언어 AST 치환, 정밀 읽기, CAS 편집을 하나의 Rust 바이너리로 통합합니다.",
        "keywords": "설치 homebrew curl windows cargo binstall no-default-features 치트시트 빠른시작",
        "source": "https://github.com/CREE1116/lancet/blob/main/README.ko.md",
        "render": _intro,
    },
    {
        "path": "migration.html",
        "group": "start",
        "nav_title": "rg·fd·sg에서 전환",
        "title": "기존 도구(rg, fd, ast-grep, sed, patch, rga)에서 전환하기",
        "description": "1:1 명령 대조표, 의도적 차이, git·LSP를 그대로 유지해야 하는 경계 안내.",
        "lede": "ripgrep·fd·ast-grep의 익숙한 사용감을 유지하면서 줄 밀림과 덮어쓰기 사고를 없앱니다.",
        "keywords": "전환 마이그레이션 ripgrep rg fd ast-grep sg sed patch rga pcre2 lsp 차이",
        "source": f"{REPO_DOCS}/rg-parity.md",
        "render": _migration,
    },
    {
        "path": "filtering.html",
        "group": "start",
        "nav_title": "무시 규칙 · 글롭 · 타입",
        "title": "무시 규칙, 글롭, 파일 타입 & 순회 경계",
        "description": ".gitignore/.ignore/.rgignore 우선순위, -u/-uu/-uuu, --glob, --type-add, type:, -L, --max-depth.",
        "lede": "검색, fd, outline, refs, ast, index가 모두 공유하는 일관된 파일 필터링 및 순회 규칙.",
        "keywords": "gitignore rgignore hidden binary -u -uu -uuu glob type-add max-depth max-filesize 심볼릭링크",
        "source": f"{REPO_DOCS}/rg-parity.md",
        "render": _filtering,
    },
    {
        "path": "guides/explore.html",
        "group": "search",
        "nav_title": "파일 탐색 · 아웃라인 · 랭킹",
        "title": "파일 탐색, 아웃라인 & 심볼 사용 빈도 랭킹",
        "description": "lancet fd --dirs --tree, outline, refs, refs --rank로 낯선 저장소의 핵심을 빠르게 파악하는 법.",
        "lede": "파일 전체를 열어 보기 전에 디렉터리, 선언 골격, 참조 빈도순 핵심 심볼 지도를 먼저 만듭니다.",
        "keywords": "fd tree dirs changed-within meta outline refs rank kinds 선언 랭킹",
        "source": f"{REPO_DOCS}/agent-instructions.md",
        "render": _explore,
    },
    {
        "path": "guides/search.html",
        "group": "search",
        "nav_title": "텍스트 검색 · LQL · 배치",
        "title": "텍스트 검색: lancet rg, LQL & 다중 쿼리 배치",
        "description": "lancet rg vs LQL 선택 기준, near/before/fuzzy/파이프 실전 패턴, lancet batch, parse, explain.",
        "lede": "논리·근접·퍼지·AST 스코프 질의와 한 번의 워크를 공유하는 다중 쿼리 배치 실행.",
        "keywords": "lql search near before fuzzy terms budget batch parse explain 선택도",
        "source": f"{REPO_DOCS}/grammar.md",
        "render": _search,
    },
    {
        "path": "guides/ast.html",
        "group": "search",
        "nav_title": "구조적 AST 검색 · 치환",
        "title": "구조적 AST 검색 & 안전한 치환 (24개 언어)",
        "description": "$VAR, $_, $$$ARGS, 문장 메타변수, 구조 동등성, dry-run diff, 중첩 건너뛰기(skipped_overlaps).",
        "lede": "트라이그램 프리필터와 dry-run diff 위에서 24개 Tree-sitter 언어의 구문 트리를 검색·변환합니다.",
        "keywords": "ast tree-sitter 메타변수 rewrite apply limit skipped_overlaps if else cpp 헤더",
        "source": "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/query-and-ast.md",
        "render": _ast,
    },
    {
        "path": "guides/read-and-documents.html",
        "group": "search",
        "nav_title": "정밀 읽기 · PDF · HWP",
        "title": "정밀 읽기, PDF·오피스·한글(HWP)·압축 파일 & OCR",
        "description": "줄 범위, 심볼, 토큰 예산 읽기, PDF/PPTX 페이지, 엑셀 셀, HWP/HWPX, ZIP/TAR 항목과 16 MiB 상한.",
        "lede": "코드와 문서·압축 파일까지 1:1 동일한 줄 좌표를 공유하며 16 MiB 압축 폭탄 상한으로 안전하게 읽습니다.",
        "keywords": "read budget-tokens next_line pdf docx pptx hwp hwpx xlsx zip tar ocr 16 mib 압축폭탄",
        "source": f"{REPO_DOCS}/agent-instructions.md",
        "render": _read_docs,
    },
]
