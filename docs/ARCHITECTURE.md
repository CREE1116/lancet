# Lancet 제작 방침과 구조

이 문서는 lancet을 고치거나 기능을 더할 때 따르는 기준이다. 코드보다 이 문서가 먼저 바뀌고, 코드는 이 문서를 따라간다.

상태: **확정 (2026-11-28)**. 작업 브랜치 `restructure`.

---

## 0. 목표

**lancet은 기존 도구를 하나로 통합하고 대체한다.** 에이전트와 사람이 파일을 찾고, 읽고, 고칠 때 쓰는 도구(ripgrep, fd, Claude Code·OpenCode의 Read/Grep/Glob/Edit/Write, ast-grep, apply_patch·git apply)를 lancet 하나로 바꿔 쓸 수 있어야 한다.

| lancet | 대체하는 도구 |
|---|---|
| `rg` | ripgrep, Grep |
| `fd` | fd, Glob |
| `read` | Read, cat |
| `edit` | Edit |
| `write` | Write |
| `patch` | apply_patch, git apply |
| `ast` | ast-grep |

기존 도구에는 없지만 에이전트가 파일을 고치는 데 꼭 필요해서 두는 명령: `refs`(정의와 사용처), `rename`(지역 이름 바꾸기), `outline`(파일 구조). 동작에 필요해서 두는 명령: `index`(속도), `mcp`(셸이 없는 클라이언트).

**이 표에 없는 기능은 만들지 않는다.** 새 기능은 (1) 기존 도구의 기능을 대체하거나, (2) 대체한 도구들 사이를 잇는 일(같은 좌표계, 안전한 쓰기)일 때만 넣는다.

## 1. 제품 원칙

기능을 넣을지 말지는 여기에 비춰 판단한다.

1. **정확도는 양보하지 않는다.** 빠르게 하려고 결과를 놓치거나 틀리게 내지 않는다. 인덱스는 후보를 줄이기만 하고, 판정은 항상 실제 엔진이 한다.
2. **모르면 거절한다.** 확신할 수 없는 변경(매크로 안의 이름, 다른 뜻일 수 있는 같은 이름, 그사이 바뀐 파일)은 추측하지 않고 이유를 말하고 멈춘다.
3. **결과는 항상 최신이다.** 인덱스가 있든 없든, 오래됐든 아니든 같은 결과를 낸다.
4. **이미 익숙한 규칙을 따른다.** 에이전트와 사람이 이미 아는 도구(ripgrep, fd, Claude Code·OpenCode의 Read/Edit/Write, git diff, Codex 패치)의 문법과 동작을 그대로 쓴다. lancet만의 문법은 기존 도구에 없는 기능에만 만든다.
5. **하나의 좌표계.** 모든 명령이 같은 경로 표기(넘긴 PATH 기준)와 같은 줄 번호를 쓴다. 한 명령의 출력을 다음 명령에 그대로 넣을 수 있어야 한다.
6. **출력은 결정적이다.** 같은 입력이면 실행할 때마다, 스레드 수와 상관없이 같은 출력이 나온다(경로순).
7. **쓰기는 안전하다.** 내가 읽은 내용이 아니면 쓰지 않고(rev), 여러 파일을 바꿀 때는 전부 바꾸거나 하나도 안 바꾼다.

### 따르는 기존 규칙

| 규칙 | 기준 도구 | lancet |
|---|---|---|
| 플래그, 출력, 종료 코드 | ripgrep 16.0 | `rg` (바이트 단위 동일, 차이 3가지는 `docs/rg-parity.md`) |
| 경로 여러 개 `[PATH]...` | rg, fd, ast-grep | 경로를 받는 모든 명령 |
| 범위 없이 읽으면 2,000줄, 한 줄 2,000자 | Claude Code·OpenCode Read | `read` |
| `old_string` / `new_string` / `replace_all` | Claude Code·OpenCode Edit | `edit` |
| 통합 diff, Codex `*** Begin Patch` | git, Codex | `patch` |
| 종료 코드 0 성공 / 1 없음 / 2 오류·거절 | grep, ripgrep | 모든 명령 |

---

## 2. 구조

### 2.1 지금 구조의 비용 (2026-11 측정)

| 측정 | 결과 | 비용 |
|---|---|---|
| 후보 계획(경로 → 인덱스 → 신선도 → 대상 파일) | `rg`, `fd`, `refs`, `ast`에 4벌 → **1단계 후 `scope::plan` 하나 (+ `fd`)** | 경로 처리 하나 고치려면 네 군데 |
| 언어별 `match Lang::X` 분기 | 129곳 (`code.rs` 103, `ast_grep.rs` 22, `rename.rs` 4) → **2단계 후 2곳 (`LangSpec` 표)** | 언어 추가·수정이 수십 군데 |
| tree-sitter 파서·캐시 | 3벌 (`code.rs`, `ast_grep.rs`, `rename.rs`) → **2단계 후 1벌** | |
| `refs`가 파일 하나를 파싱하는 횟수 | 4번 (심볼, 함수 범위, 식별자, 스코프를 각자 파싱) → **2단계 후 1번** | 성능 손실 |
| 쓰기 규칙 | `edit --edits`·`patch`는 전부 아니면 전무, `ast --apply`는 파일마다 → **3단계 후 모두 `change::commit` 하나** | 명령마다 안전성이 다름 |
| 걷기 → 검색 | 걷기를 끝까지 마친 뒤 검색 시작 | 두 단계가 겹치지 않음 |
| `rg` 출력 | 모든 결과를 메모리에 모은 뒤 경로순 출력 | 큰 출력에서 메모리, 첫 출력 지연 |
| 명령 목록 | 최상위 도움말, MCP 설명, 스킬 표에 따로 | 바꿀 때마다 어긋남 (지금도 `refs`·`ast`가 `[PATH]` 하나로 나옴) |

### 2.2 층

위층은 아래층만 쓴다. 같은 층끼리는 서로 부르지 않는다.

```
 인터페이스   cli · mcp · 명령 목록(registry) · 렌더러(human / json / rg 형식 / vimgrep)
     │
 명령        rg  fd  read  outline  refs  ast  edit  write  patch  rename  index
     │        (인자 → 코어 호출 → 결과 값. 출력 형식은 모름)
     │
 코어        Scope ──▶ Pipeline ──▶ Doc ──▶ Syntax          Change
             (어떤 파일)  (병렬·정렬)  (내용)   (코드 구조)     (안전한 쓰기)
     │
 엔진        walk · index(segment, req) · fold · matcher · write(rev, CAS)
             adapters · LangSpec 표 · tree-sitter
```

### 2.3 쓰는 패턴과 이유

패턴은 실제로 바꿀 일이 생기는 곳(연결점)에만 쓴다. 그 밖에서는 함수와 구조체로 충분하다.

#### ① 싼 것부터 거르는 단계식 판정 (성능)

모든 검색 명령의 판정기는 같은 모양이다.

```rust
trait Matcher: Sync {
    fn req(&self) -> Req;               // 1단계: 인덱스로 거를 트라이그램 조건 (모르면 Req::All)
    fn literal(&self) -> Option<&[u8]>; // 2단계: 파일 바이트에서 memmem으로 확인할 리터럴
    fn find(&self, doc: &Doc) -> Vec<Hit>; // 3단계: 실제 판정 (regex, 식별자, AST 패턴)
}
```

- 인덱스(파일 수천 개 → 수십 개) → 리터럴(파싱 없이 바이트 비교) → 실제 판정 순서라, 비싼 tree-sitter 파싱은 리터럴이 들어 있는 파일에서만 일어난다.
- `rg`(regex), `refs`(식별자), `ast`(AST 패턴)가 같은 파이프라인을 탄다. 새 검색 기능은 `Matcher` 하나만 구현하면 인덱스, 신선도, 병렬, 정렬, 여러 경로를 공짜로 얻는다.
- **2단계 결정: trait은 만들지 않음.** 세 명령 모두 이미 이 순서를 따른다(인덱스 조건은 `scope::plan`, 그다음 `rg`는 버퍼 정규식, `refs`·`ast`는 memmem 리터럴 검사 뒤에 파싱). 파일별 처리가 명령마다 달라(rg 출력, refs 분류, ast 재작성) 세 구현을 함께 쓰는 소비자가 없으므로, trait은 이름만 바꾸는 추상화가 된다(2.4). 네 번째 검색 명령이 생기면 다시 판단한다.

#### ② 한 번 파싱하고, 필요한 것만 계산 (성능)

```rust
struct Doc { bytes: Content, kind: DocKind, lines: OnceCell<LineTable>, tree: OnceCell<Option<Tree>>, rev: OnceCell<Rev> }
// Syntax 뷰는 같은 tree에서 필요할 때만 계산하고 캐시한다
doc.syntax()?.symbols()  /  .identifiers(name)  /  .scopes()
```

- `refs`의 파싱이 4번 → 1번이 된다. 줄 표, rev도 처음 필요할 때 한 번만 계산한다.
- tree-sitter 파서는 스레드별로 하나만 둔다(지금 3벌 → 1벌).
- **3단계 결정: 지연 계산하는 `Doc` 구조체는 만들지 않음.** 2단계 뒤로는 어느 명령도 같은 파일의 줄 표·트리·rev를 두 번 계산하지 않아서, 캐시할 소비자가 없다(2.4). 내용 코어는 읽기 쪽 `content`(`read`, `load_searchable`)와 쓰기 쪽 `change::load`(문서·바이너리·UTF-8 검사, 공유 잠금, rev), `change::line_starts`·`eol`로 둔다.

#### ③ 언어는 코드가 아니라 데이터 (수정)

```rust
static LANGS: &[LangSpec] = &[LangSpec {
    name: "rust", exts: &["rs"], grammar: tree_sitter_rust::LANGUAGE,
    symbols: &[("function_item", "function"), ("struct_item", "struct"), ...],
    comment_kinds: &["line_comment", "block_comment"], string_kinds: &[...],
    scopes: Some(&RUST_SCOPES),   // 스코프 해석기가 있는 언어만
}, ...];
```

- 129곳의 `match Lang::X`가 표 조회로 바뀐다. 언어 추가는 표에 한 줄과 테스트.
- 스코프 해석기처럼 언어마다 로직이 진짜 다른 부분만 `trait ScopeRules`로 둔다(Rust, Python, JS/TS, Go 네 개).
- `cfg(feature = "ast")`는 이 표와 tree-sitter를 감싸는 한 곳에만 둔다(지금 5개 파일에 흩어짐).

#### ④ 계획과 실행을 나눈다 (수정, 안전)

모든 쓰기 명령은 부수효과 없는 계획 함수와 한 곳의 실행으로 나뉜다.

```rust
let text = change::load(path)?;          // 읽기: 문서·바이너리·UTF-8 거부, 공유 잠금, rev
// 명령의 계획: 파일을 쓰지 않음. diff와 change::added_syntax_errors 검사까지
let set: Vec<FileChange> = plan(args, text)?;
change::commit(&set)?;                   // 전부 아니면 전무. 실패하면 이미 쓴 파일을 되돌림
```

- **3단계 구현:** `ChangeSet` 타입 대신 `&[FileChange]`(갱신·생성·삭제·권한)와 `change::commit`. 감싸는 타입이 할 일이 없어서다. 문법 검사는 오류 문구가 명령마다 달라(`edit`·`patch`·`rename`·`ast`) 계획 단계에서 `change::added_syntax_errors`를 부르게 했다.

- `edit`, `write`, `patch`, `rename`, `ast --rewrite`가 모두 `ChangeSet`을 만든다. 미리보기(`--apply` 없음)는 `check`까지만, 적용은 `commit`까지.
- 안전 규칙(CAS, 원자적 쓰기, 롤백, 문법 검사)이 한 곳에만 있어서, 명령마다 다를 수 없다.
- **동작 변화:** `ast --apply`도 전부 아니면 전무가 된다.

#### ⑤ 결과 값과 출력 형식을 나눈다 (수정)

명령은 결과 값(`SearchResult`, `ReadResult`, `ChangeResult` …)만 돌려주고, 렌더러가 형식을 정한다.

- 사람용, `--json`, ripgrep 형식(컨텍스트, `--vimgrep`, `--json` 이벤트, `-c`, `-l` …)이 렌더러다. rg의 출력 플래그 수십 개가 명령 로직에 섞이지 않는다.
- `--json`은 모든 명령에서 같은 바깥 구조(`status`, `warnings`, `error`)를 쓴다.
- 오류는 한 타입이다: `Refusal { status, message, hint }`. 종료 코드(0/1/2)는 인터페이스 한 곳에서 status로 정한다.

#### ⑥ 병렬로 처리하고, 순서대로 흘려보낸다 (성능)

```
걷기(병렬) ──파일──▶ 판정(rayon) ──(순번, 결과)──▶ 재정렬 버퍼 ──▶ 렌더러 ──▶ stdout
```

- 걷기가 끝나기 전에 판정을 시작한다(지금은 걷기를 다 마치고 시작).
- 결과에 경로 순번을 붙이고, 재정렬 버퍼가 순서대로 내보낸다. 출력은 지금과 똑같이 경로순이면서, 첫 출력이 빨라지고 메모리는 버퍼 크기로 묶인다.
- 경로순은 정렬된 걷기 순서가 있어야 하므로, 걷기 결과에 순번을 매기는 방식은 1단계에서 측정해 보고 정한다. **이득이 측정되지 않으면 넣지 않는다.**
- **1단계 측정 결과: 넣지 않음.** 걷기 스레드에서 바로 검색하는 방식(+6%)과 걷기 → rayon으로 흘려보내는 방식(+12%) 모두 "걷기 후 전체 코어로 검색"보다 느렸다. 숫자는 `bench/results/baseline-step1.md`.

#### ⑦ 명령 목록은 한 곳 (수정)

```rust
static COMMANDS: &[CommandSpec] = &[CommandSpec { name: "refs", usage: "refs SYMBOL [PATH...]", summary: "...", read_only: true }, ...];
```

- 최상위 도움말, MCP 도구 설명, MCP `--read-only` 판정, 스킬 표 검사가 모두 여기서 나온다.
- 테스트 하나가 `skills/lancet/SKILL.md`의 명령 표가 이 목록과 맞는지 확인한다.

#### ⑧ 내용에서 나온 값은 인덱스에 쌓을 수 있다 (나중, 성능)

지금 세그먼트는 파일마다 트라이그램과 메타데이터를 저장하고, 메타데이터로 신선도를 판단한다. 같은 방식으로 파일별 심볼 표를 세그먼트에 저장하면, 큰 저장소에서 `refs --rank`와 `outline`이 파싱 없이 돈다. 신선도 규칙은 그대로라 결과가 늦어질 일은 없다. ②와 ③이 끝난 뒤 측정해 보고 정한다.

### 2.4 하지 않을 것

- 플러그인 체계, 동적 로딩, 비동기 런타임, 데몬. lancet은 매번 새로 뜨는 CLI이고, 그게 장점이다.
- 연결점이 아닌 곳의 trait과 제네릭. 구현이 하나뿐이면 그냥 함수로 둔다.
- 측정 없는 최적화. ⑥, ⑧은 측정해서 이득이 있을 때만.

### 2.5 명령과 코어

| 명령 | Scope / Pipeline | Matcher | Doc | Syntax | Change | 명령만의 부분 |
|---|---|---|---|---|---|---|
| rg | ✓ | regex | decode | | | rg 렌더러 |
| fd | ✓ (파일만) | 경로 glob | | | | 트리 렌더러 |
| read | | | ✓ | symbols | | 범위·페이지·시트 고르기 |
| outline | ✓ (파일만) | | ✓ | symbols | | 목차 |
| refs | ✓ | 식별자 | ✓ | identifiers, scopes | | 정의·사용처 분류, `--rank` |
| ast | ✓ | AST 패턴 | ✓ | tree | ✓ | 재작성 템플릿 |
| edit / write | | | ✓ | | ✓ | 대상 지정 |
| patch | | | ✓ | | ✓ | diff·Codex 파서 |
| rename | | | ✓ | scopes | ✓ | 충돌 검사 |
| index | ✓ (파일만) | | | | | 세그먼트 생성·병합 |

### 2.6 목표 디렉터리

```
src/
  engine/   walk.rs  index/  fold.rs  matcher.rs  write.rs  adapters/  lang/{spec.rs, rust.rs, python.rs, …}
  core/     scope.rs  pipeline.rs  doc.rs  syntax/{mod.rs, symbols.rs, scopes/}  change.rs  refusal.rs
  cmd/      rg.rs  fd.rs  read.rs  outline.rs  refs.rs  ast.rs  edit.rs  write.rs  patch.rs  rename.rs  index.rs
  iface/    commands.rs  cli/  mcp.rs  render/{human.rs, json.rs, rg.rs}
```

---

## 3. 유지보수 방침

### 3.1 변경 규칙

1. **기능은 코어에 넣고, 명령에는 넣지 않는다.** 두 명령 이상이 쓸 수 있는 동작은 코어에 둔다. 같은 코드를 두 번째로 쓰게 되면 그 자리에서 코어로 옮긴다.
2. **동작 하나에 진실은 한 곳.** 도움말 문구, 기본값, 한도(2,000줄 등), 언어 정보는 명령 목록·LangSpec·상수 한 곳에서 가져다 쓴다.
3. **동작을 바꾸면 같은 커밋에서 바꾼다:** 코드, 테스트, 도움말, 스킬(`skills/lancet/`), `docs/`. 문서 사이트(`docs/site/`)는 따로 맞춘다.
4. **새 플래그나 명령은 원칙 4를 먼저 확인한다.** 기존 도구에 같은 기능이 있으면 그 이름과 동작을 쓴다.
5. **실제 사용에서 나온 문제를 우선한다.** 에이전트 실행 기록에서 드러난 마찰이 추측보다 앞선다.
6. **성능 변경은 숫자로 들어온다.** 전후 측정 없이 "빨라질 것"으로 넣지 않는다.

### 3.2 검증 자산 (명세 역할)

구조를 바꿀 때 이것들이 그대로 통과해야 한다.

| 자산 | 지키는 것 | 위치 | 언제 |
|---|---|---|---|
| 단위·통합 테스트 | 명령별 동작, 인덱스=스캔, 신선도, 퍼즈 | `tests/`, 모듈 안 | CI (모든 OS) |
| rg 대조 150케이스 | ripgrep 16.0과 stdout·종료 코드가 바이트 단위로 같음 | `bench/rg_parity.py` | CI (ripgrep 16.0.0 고정) |
| diff ↔ patch 왕복 | 출력한 diff를 lancet `patch`와 `patch(1)`이 그대로 적용함 (CRLF, 끝 줄바꿈, 큰 파일) | `tests/integration.rs` `printed_diffs_apply_back_with_patch` | CI |
| 실제 저장소 대조 | 8개 저장소에서 rg와 동일, 속도 | `bench/suite.py` | 단계마다 |
| rename 오라클 (Rust·Python·Go) | 이름 바꾸기 후에도 컴파일·심볼 표가 같음 | `bench/rename_oracle.py` | `rename`·`Syntax`를 바꿀 때 |
| 명령별 시간 | 단계 전후 비교 (파싱 비용 포함) | `bench/baseline.py`, `bench/results/baseline-*.md` | 단계마다 |
| 에이전트 A/B | 실제 사용에서 lancet과 기본 도구의 정확도·시간·왕복 | `bench/agents/` | 큰 변경 뒤 |

### 3.3 성능 기준

- 실제 저장소 대조에서 ripgrep보다 느려지면 안 된다(Linux 트리 기준 지금 193ms, rg 동급).
- 단계마다 `bench/suite.py`로 전후를 비교하고, 5% 넘게 느려지면 원인을 찾는다.
- `refs`·`outline`·`ast`·`read --symbol`은 `bench/baseline.py`로 따로 잰다(②의 효과 확인). 기준값: `bench/results/baseline-step0.md`.

### 3.4 의존성

- 몇 줄로 될 일에 새 크레이트를 넣지 않는다.
- 외부 프로그램을 실행하지 않는다(OCR의 tesseract만 예외, `--ocr`일 때만).

---

## 4. 이행 계획

한 번에 갈아엎지 않는다. 단계마다 3.2의 자산이 모두 통과해야 다음 단계로 간다.

| 단계 | 내용 | 끝나는 조건 |
|---|---|---|
| 정리 (완료) | 쿼리 언어와 `parse`·`explain`·`batch` 삭제, `status` → `index --status`. 쿼리 실행기로 검사하던 엔진 테스트 7개(인덱스=스캔, 증분 갱신, LSM 병합, 신선도, 퍼즈, 문서 변환, 손상 문서)는 `rg`로 옮겨 유지. | 모든 테스트, rg 대조 138케이스 통과 |
| 0. 기준선 (완료) | rename 오라클(`bench/rename_oracle.py`), diff ↔ patch 왕복 테스트, 에이전트 A/B(`bench/agents/`)를 저장소로 옮김. rg 대조를 ripgrep 16.0으로 올려 150케이스를 CI에서 실행(`--in`, `--in0`, `--max-bytes`, `--no-pre` 구현). 기준값 `bench/results/baseline-step0.md`, README 성능 표 재측정. 그 과정에서 diff(끝 줄바꿈, CRLF, 큰 파일 → Myers)와 trust 모드(인덱스 범위 밖 요청) 버그를 고침. | CI에서 대조 150케이스 통과 |
| 1. Scope (완료) | `rg`·`refs`·`ast`의 후보 계획을 `scope::plan` 하나로 합침(경로 여러 개, 표시 경로 `AsGiven`/`Clean`, 중복 제거, 인덱스·신선도·trust 판단). `fd`는 디렉터리까지 나열하고 인덱스에서 목록을 바로 읽는 다른 방식이라 따로 둠. ⑥은 두 방식 모두 측정상 느려서 넣지 않음. "인덱스 없이 rg보다 12% 느림"은 측정 시점 차이였음(번갈아 재면 -1%~+5%). | 계획 코드 4벌 → 1벌(+`fd`), 모든 자산 통과, 기준값과 같음 |
| 2. LangSpec + Syntax (완료) | `code::Syntax`로 한 번 파싱(refs 파싱 4 → 1: Go 201 → 115 ms, Rust 734 → 639 ms), tree-sitter 파서 3벌 → 1벌(`code::parse_tree`), 언어 정보를 `LangSpec` 표로(언어 분기 129 → 2, 남은 둘은 로직), 스코프 해석기를 `scopes.rs`로 분리. `Matcher` trait은 만들지 않음(① 참고). `Doc` 코어는 읽기 경로를 다루는 3단계로 넘김. | 7개 언어에서 outline·refs --rank·ast 출력이 0단계와 같음, rename 오라클 통과 |
| 3. Change (완료) | 쓰기 명령 다섯 개(`edit`, `write`, `patch`, `rename`, `ast --rewrite`)가 `change::load`로 읽고 `change::commit`으로 씀. 쓰기·되돌리기 코드 4벌 → 1벌. 동작 변화: `ast --apply`는 전부 아니면 전무, 문법 오류를 늘리는 rewrite는 거부(`--allow-syntax-errors`), `rename`도 바이너리 거부. 지연 `Doc` 구조체는 만들지 않음(② 참고). rename 오라클이 Python 3.9로 못 읽는 파일(`match` 문)을 틀림으로 세던 오탐을 고침. 비용: `ast --rewrite` 미리보기가 바뀌는 파일마다 한 번 더 파싱해서 느려짐(Rust 42개 파일 35 → 69 ms). 대신 줄바꿈 있는 Python 템플릿이 들여쓰기를 깨뜨린 70개 파일을 쓰던 것을 거부함(`bench/results/baseline-step3.md`). | `ast --apply` 전부 아니면 전무, 모든 자산 통과 |
| 4. 인터페이스 | 명령 목록, 렌더러, `Refusal`, `main.rs`를 명령별로 나누기. | 도움말·MCP·스킬이 같은 목록에서 나옴 |
| 5. 마무리 | 영어판 문서, 문서 사이트(`docs/site/`) 맞추기. ⑧ 검토. | |

순서 이유: 1단계(Scope)가 중복이 가장 많고 다른 단계의 바탕이다. 2단계는 성능 이득이 가장 확실하다. 3단계는 동작 변화가 있어서 구조가 안정된 뒤에 한다.

---

## 5. 결정 (2026-11-28)

1. **쿼리 언어(`lancet '…'`)와 `parse`, `explain`, `batch`는 지웠다.** 대체하는 기존 도구가 없고(0장), 에이전트 실행 기록 40여 개에서 한 번도 쓰이지 않았다. 정규식 검색은 `rg`, 구조 검색은 `ast`가 맡는다. 근접·퍼지 검색은 같이 없어졌다. 내부 표현(`Expr` → `Req`)은 `Match`와 `And`만 남겨 엔진에 둔다.
2. **`status`는 `index --status`가 됐다.**
3. **`ast --apply`는 전부 아니면 전무가 됐다(3단계).** `apply_patch`·`git apply`와 같다. 문법 오류를 늘리는 rewrite도 `edit`처럼 거부한다.
4. **문서는 이 작업이 끝날 때 영어판을 기본으로 한다.** 이 문서는 그때까지 한국어로 둔다.
