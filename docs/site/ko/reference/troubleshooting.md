---
nav_title: "문제 해결 · 오류 복구"
title: "문제 해결 & 거절 사유별 복구 매트릭스"
description: "CAS 충돌, --find 비고유, 구문 게이트, rename 거절, patch, OCR, Windows 최신성 증상별 복구표."
lede: "모든 오류와 의도적 거절 사유에 대한 원인 진단과 즉시 복구 절차."
keywords: "문제해결 conflict find 구문오류 rename unsupported patch pcre2 fd windows ocr"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md"
---
Lancet의 모든 거절은 조용히 덮어쓰지 않고 종료 코드 `2`와 구체적인 원인 메시지를 반환합니다. 증상별 해결 절차를 바로 찾으세요.

## 편집 · 패치 · 이름 변경 거절 {#edit-patch-errors}

| 증상 / 오류 | 근본 원인 | 정확한 복구 절차 |
| --- | --- | --- |
| `edit` / `patch` / `rename`에서 `status: "conflict"` | `--rev`를 읽은 뒤 디스크 파일이 변경됨 | `lancet read <FILE> --json`으로 새 내용과 `rev`를 다시 읽어 수정을 재계산해 적용합니다. |
| `edit --find` 거절 (0회 또는 2회 이상 일치) | 리터럴 문자열이 파일 내에서 고유하지 않음 | 오류에 출력된 줄 번호를 보고 `--find` 앞뒤 문맥을 넓히거나 의도한 전체 변경이면 `--all`을 추가합니다. |
| `edit --symbol` 후 문서 주석·어트리뷰트 사라짐 | `--symbol`은 `read --symbol`처럼 위쪽 `///`·`#[attr]`까지 포함함 | `--with`/`--with-file`에 주석과 어트리뷰트를 포함하거나 본문 줄 범위(`--lines`)만 지정하세요. |
| 구문 오류 증가 거절 | 편집·패치 결과 Tree-sitter 구문 오류 수가 늘어남 | dry-run diff에서 닫히지 않은 괄호·따옴표를 점검하고, 의도된 중간 단계일 때만 `--allow-syntax-errors`를 씁니다. |
| `lancet rename` 거절 (`unsupported`) | 최상위/필드/메서드/Python 매개변수/매크로/JSX 태그/섀도잉 | [거절 목록](../guides/rename.md#refusals)을 확인하고 `lancet refs` + `lancet edit --edits`나 LSP를 사용하세요. |
| `lancet patch` 바이너리/복사 diff 거절 | 패치에 `Binary files` 또는 `copy from` 포함 | 텍스트 hunk만 남겨 파이프하세요. |

## 검색 · AST · 순회 오류 {#search-ast-errors}

| 증상 / 오류 | 근본 원인 | 정확한 복구 절차 |
| --- | --- | --- |
| LQL에서 `status: "invalid_query"` | 알 수 없는 필터, 닫히지 않은 따옴표, 연산자 누락 | `lancet parse '<QUERY>'`를 실행해 오류 `pos` 바이트 위치를 확인하세요. |
| `ast --rewrite --apply`가 `--limit` 거절 | 개수 제한과 자동 적용의 조합은 의도치 않은 부분 수정을 유발함 | `--limit`을 빼고 `--path GLOB`로 대상 파일을 좁히세요. |
| `ast 'if ($C) $S'`가 `if ... else` 누락 | Lancet은 `if`와 `if-else` AST 노드를 엄격히 구분함 | `else` 분기까지 포함하는 두 번째 패턴을 함께 실행하세요. |
| `lancet rg -P` 실패 | PCRE2 전후방 탐색·역참조 미지원 | 룩어라운드 없는 Rust `regex`나 LQL `before()` / `NOT`으로 작성하세요. |
| `PATH: No such file or directory` (stderr, 종료 `2`) | `fd`·`search`·`batch`·`refs`·`ast`에 존재하지 않는 경로나 오타(`fd docs/imgaes`) 전달 | 경로 철자를 확인하세요. v0.1.4+부터 없는 경로는 조용한 `empty`(종료 1)가 아니라 명시적 오류(종료 2)를 냅니다. |
| `lancet fd '*.md'` 실행 시 stderr 글롭 안내 출력 | 패턴에 `*`, `?`, `[`가 포함되어 자동으로 글롭(`-g`) 매치됨 | v0.1.3+의 정상 동작입니다. 정규식이면 `-r`, 안내를 숨기려면 `-g`를 명시하세요. |

## 인덱스 · 문서 · MCP 문제 {#index-docs-mcp}

| 증상 / 메시지 | 근본 원인 | 정확한 복구 절차 |
| --- | --- | --- |
| `--trust-index`에서 방금 수정한 파일이 검색 안 됨 | `--trust-index`는 설계상 디스크 stat 검사를 건너뜀 | `--trust-index`를 빼고 `auto` 모드를 쓰거나 `lancet index .`로 동기화하세요. |
| Windows에서 크기·`mtime` 동일한 수정 놓침 | Windows에서 Rust stdlib이 `mtime`과 `size`만 제공함 | 타임스탬프를 되돌리는 도구 실행 후엔 `lancet index .`나 `--no-index`를 쓰세요. |
| `lancet rg`가 PDF / `.docx` 내용을 못 찾음 | `lancet rg`는 `rg` 호환성 때문에 어댑터가 기본 OFF | `lancet rg --docs`를 붙이거나 기본 LQL 검색(`lancet 'query'`)을 쓰세요. |
| 스캔 PDF / 이미지 텍스트가 비어 있음 | 임베디드 텍스트 레이어가 없는 이미지 | `--ocr`을 붙이고 `PATH`에 `pdftoppm`과 `tesseract`가 있는지 확인하세요. |
| MCP 호출이 "this MCP server is read-only"로 실패 | `lancet mcp --read-only`로 실행됨 (`edit`, `patch`, `rename`, `index`, `--apply` 거절) | 편집이 필요하면 `--read-only` 없이 서버를 다시 시작하세요. |
