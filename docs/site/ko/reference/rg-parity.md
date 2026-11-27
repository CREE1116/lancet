---
nav_title: "lancet rg 호환성 표"
title: "lancet rg — ripgrep 기능 호환성 매트릭스"
description: "매칭, 출력, 순회, --passthru, --type-add, --docs, 의도적 차이까지 전체 플래그 대조표."
lede: "ripgrep 15.2와 바이트 단위로 대조 검증한 전체 플래그 호환성 매트릭스."
keywords: "rg ripgrep 호환성 passthru type-add pcre2 pre docs max-depth max-filesize color"
source: "https://github.com/CREE1116/lancet/blob/main/docs/rg-parity.md"
---
`bench/rg_parity.py`의 68개 엣지 케이스 픽스처(유니코드 케이스 폴딩, CRLF, 마지막 개행 누락, 바이너리 건너뛰기, `-m`+`-C`, `--vimgrep`, `-L`, `--max-depth`, `--max-filesize`)와 Linux v6.12 전체 쿼리를 `ripgrep 15.2`와 바이트 단위로 대조 검증했습니다.

## 패턴 & 매칭 {#matching}

| 플래그 | 의미 | 상태 | 비고 |
| --- | --- | --- | --- |
| `-e`, `--regexp <PATTERN>` | 검색 패턴 (중복 지정 가능) | <span class='tag-yes'>지원</span> | 정규식 선택(alternation)으로 결합 |
| `-f`, `--file <FILE>` | 파일에서 줄당 하나씩 패턴 읽기 (`-` stdin) | <span class='tag-yes'>지원</span> |  |
| `-F`, `--fixed-strings` | 리터럴 문자열로 취급 | <span class='tag-yes'>지원</span> |  |
| `-i` / `-s` / `-S` | 대소문자 무시 / 구분 / 스마트 | <span class='tag-yes'>지원</span> | 마지막 플래그 우선; AST 기반 `-S`(`\W` 무시 유지) |
| `-w`, `--word-regexp` | 단어 경계로 둘러싸인 매치만 | <span class='tag-yes'>지원</span> | `rg` 경계 규칙과 동일(`-w '@x'` 지원) |
| `-x`, `--line-regexp` | 줄 전체에 걸친 매치만 | <span class='tag-yes'>지원</span> |  |
| `-v`, `--invert-match` | 매치하지 않는 줄 출력 | <span class='tag-yes'>지원</span> |  |
| `-U`, `--multiline-dotall` | 멀티라인 및 `.`에 `\n` 허용 | <span class='tag-yes'>지원</span> |  |
| `-P`, `--pcre2` | 전후방 탐색 및 역참조 | <span class='tag-no'>미지원</span> | 선형 시간 Rust `regex` + 건전한 HIR 트라이그램 컴파일 |

## 출력 & 형식 {#output}

| 플래그 | 의미 | 상태 | 비고 |
| --- | --- | --- | --- |
| `-o`, `-c`, `--count-matches` | 매치 부분만 / 줄 수 / 총 매치 수 | <span class='tag-yes'>지원</span> |  |
| `-l`, `--files-without-match`, `-q` | 일치 경로 / 미일치 경로 / 무출력 | <span class='tag-yes'>지원</span> |  |
| `-m <N>`, `-A`, `-B`, `-C` | 파일당 최대 개수 및 전후 문맥 | <span class='tag-yes'>지원</span> | 후속 문맥도 `-m` 준수; `--` 구분선 출력 |
| `--passthru` (별칭 `--passthrough`) | 일치·비일치 줄 모두 출력 | <span class='tag-yes'>지원</span> | 비일치 파일도 출력; `--passthru/-A/-B/-C` 중 마지막 우선 |
| `-n` / `-N` | 줄 번호 표시 / 숨기기 | <span class='tag-yes'>지원</span> | 파이프 시에도 **기본 ON** |
| `-H` / `-I`, `--column`, `--vimgrep` | 파일명 표시, 1-based 열, vimgrep | <span class='tag-yes'>지원</span> | `-H/-I` 중 마지막 우선 |
| `--json`, `-0`, `-r <TEXT>` | rg JSON Lines, NUL 구분, `$1`/`${name}` 치환 | <span class='tag-yes'>지원</span> |  |
| `--color <WHEN>` | `auto`, `always`, `ansi`, `never` | <span class='tag-yes'>지원</span> | 기본 `auto`는 TTY와 `NO_COLOR`/`TERM=dumb` 준수 |
| `--sort`, `--no-heading` | 정렬, 파일명 헤더 끄기 | No-op | 항상 경로순 정렬; 헤더 사용 안 함 |

## 파일 순회, 필터링 & Lancet 확장 {#traversal}

| 플래그 | 의미 | 상태 | 비고 |
| --- | --- | --- | --- |
| `-g`, `--iglob`, `-t`, `-T`, `--type-list` | 글롭 및 타입 필터 | <span class='tag-yes'>지원</span> | 긍정 글롭은 ignore/hidden 덮어씀 |
| `--type-add <SPEC>` | `NAME:GLOB` 또는 `NAME:include:A,B` | <span class='tag-yes'>지원</span> | `-t`, `-T`, `--files`, `--type-list` 반영 |
| `-a`, `-u`/`-uu`/`-uuu`, `-.`, `--no-ignore` | 바이너리/숨김/무시 해제 | <span class='tag-yes'>지원</span> |  |
| `-L`, `--max-depth`, `--max-filesize` | 심볼릭 링크 추적, 깊이, 크기 (`K/M/G`) | <span class='tag-yes'>지원</span> | `--trust-index -L`은 라이브 순회로 폴백 |
| `--files`, `-j/--threads` | 검색 대상 파일 출력, 워커 수 | <span class='tag-yes'>지원</span> |  |
| `--docs` | PDF/오피스/HWP/압축파일 인프로세스 검색 | <span class='tag-ext'>확장</span> | `rg` 호환성 유지를 위해 기본 OFF |
| `--no-index`, `--trust-index`, `--index-dir` | `.lancet` 인덱스 동작 지정 | <span class='tag-ext'>확장</span> | 기본은 항상 최신인 `auto` 모드 |
| `--pre <CMD>` | 파일별 외부 전처리기 | <span class='tag-no'>미지원</span> | 인프로세스 `--docs`로 대체 |
