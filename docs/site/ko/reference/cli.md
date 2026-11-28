---
nav_title: "CLI 명령 & 종료 코드"
title: "CLI 명령, 플래그, 종료 코드 레퍼런스"
description: "Lancet의 모든 명령, 플래그와 기본값, 함께 쓰는 종료 코드."
lede: "명령 12개, 종료 코드 한 벌, 좌표계 하나."
keywords: "cli 명령 플래그 종료 코드 0 1 2 conflict invalid_query unsupported"
source: "https://github.com/CREE1116/lancet/blob/main/src/commands.rs"
---
`lancet --help`는 명령 목록을, `lancet COMMAND --help`는 명령별 플래그를 보여 줍니다. 둘 다 이 페이지와 같은 명령 표(`src/commands.rs`)에서 나옵니다. `lancet rg`의 ripgrep 플래그는 [`lancet rg` 호환 레퍼런스](rg-parity.md)를 보세요.

## 종료 코드 {#exit-codes}

| 코드 | 의미 | JSON `status` |
| ---: | --- | --- |
| `0` | 완료: 찾은 것이 있음, 또는 변경을 미리 보거나 반영함 | `complete` |
| `1` | 찾은 것이 없음(오류 아님) | `empty` |
| `2` | 거부 또는 잘못됨: 잘못된 인자, 지원하지 않는 파일, 충돌(`rev` 이후 파일이 바뀜), I/O 오류 | `invalid_query`, `unsupported`, `conflict` |

`--json`이면 모든 명령의 응답에 `status`가 있고, 실패하면 `error`가 있습니다. 검색 명령은 `warnings`도 알려 줍니다.

## 찾기와 검색 {#cmd-search}

| 명령 | 플래그 |
| --- | --- |
| `lancet rg [FLAGS] PATTERN [PATH...]` | ripgrep의 플래그([호환표](rg-parity.md)) + `--docs`(문서·압축 파일 검색) |
| `lancet fd [PATTERN] [PATH...]` | `-p/--pattern`, `-t f\|d`, `-e/--ext`, `--type-class`, `-g/--glob`, `-r/--regex`, `-s/--case-sensitive`, `-d/--max-depth`, `--changed-within DUR`, `--min-size`, `--max-size`, `--tree`, `--dirs`, `--limit N`, `--meta`, `--hidden`, `--no-ignore`, `-j`, `--json` |
| `lancet refs SYMBOL [PATH...]` | `--rank`(SYMBOL은 글롭, 참조가 많은 선언부터), `--kind KIND,...`, `-l/--lang`, `-e/--ext`, `--path GLOB`, `--all-files`, `--limit N`, `--hidden`, `--no-ignore`, `-j`, `--json` |
| `lancet ast PATTERN [PATH...]` | `-l/--lang`, `-r/--rewrite TEMPLATE`, `--apply`, `--allow-syntax-errors`, `--path GLOB`, `--limit N`, `--hidden`, `--no-ignore`, `-j`, `--json` ([패턴 문법](ast-patterns.md)) |

검색 명령은 인덱스가 있으면 자동으로 씁니다. `--no-index`와 `--trust-index`로 모드를 직접 고를 수 있습니다([인덱스와 신선도](../concepts/index-and-freshness.md)).

## 읽기 {#cmd-read}

| 명령 | 플래그 |
| --- | --- |
| `lancet read TARGET...` | 대상: `file`, `file:10-50`, `file:42`, `archive.zip!inner.log`, 한 번에 여러 개 · `-l/--lines`, `--offset N --limit N`, `-s/--symbol`, `-p/--page`, `--sheet`, `--rows`, `--cols`, `-e/--entry`, `--budget-tokens N`, `--raw`, `--ocr`, `--json` |
| `lancet outline [PATH...]` | `-d/--max-depth`, `-e/--ext`, `--limit N`, `--hidden`, `--no-ignore`, `-j`, `--ocr`, `--json` |

`read`는 범위를 주지 않으면 최대 2,000줄을 출력하고, 첫 줄은 `# PATH rev REV`입니다.

## 고치기 {#cmd-change}

모든 변경은 `--apply` 전까지 미리보기입니다. 모든 쓰기는 compare-and-swap이고, 여러 파일 변경은 전부 쓰거나 하나도 안 씁니다([쓰기 안전성](../concepts/write-safety.md)).

| 명령 | 플래그 |
| --- | --- |
| `lancet edit PATH` | 대상: `--old-string TEXT`(`--replace-all`, `--expected-replacements N`), `--lines`, `--symbol` · 연산: `--new-string TEXT`, `--with-file FILE\|-`, `--delete`, `--insert-before`, `--insert-after` · `--edits FILE\|-`(여러 구간·파일 한 번에) · `--rev REV`, `--apply`, `--allow-syntax-errors`, `--json` |
| `lancet write PATH` | `--content TEXT` 또는 `--with-file FILE\|-`, `--rev REV`(있는 파일을 바꿀 때 필수), `--apply`, `--allow-syntax-errors`, `--json` |
| `lancet patch [DIFF\|-]` | unified diff 또는 Codex `*** Begin Patch` · `--root DIR`(`.`), `-p/--strip N`(`1`), `--apply`, `--allow-syntax-errors`, `--json` |
| `lancet rename FILE:LINE[:COL] OLD NEW` | `--rev REV`, `--apply`, `--json` |

## 기반 {#cmd-infra}

| 명령 | 플래그 |
| --- | --- |
| `lancet index [PATH]` | `--status`(인덱스 상태를 JSON으로 출력, 아무것도 바꾸지 않음), `--rebuild`, `--compact`, `--index-dir DIR`, `--hidden`, `--no-ignore`, `--ocr`, `-j` |
| `lancet mcp` | `--root DIR`(`.`), `--read-only`(`edit`, `write`, `patch`, `rename`, `index`, `--apply` 거부), `--trust-index`, `-j` |
