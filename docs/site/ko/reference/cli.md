---
nav_title: "CLI 명령 · 종료 코드"
title: "CLI 하위 명령, 플래그 & 종료 코드 레퍼런스"
description: "src/main.rs 기준으로 검증된 15개 진입점별 플래그 테이블, 기본값, 종료 코드."
lede: "15개 모든 하위 명령의 정확한 구문, 플래그 기본값, 종료 코드 명세."
keywords: "cli 하위명령 플래그 종료코드 0 1 2 conflict invalid_query unsupported"
source: "https://github.com/CREE1116/lancet/blob/main/src/main.rs"
---
모든 하위 명령은 결정적 종료 코드, `.gitignore` 순회, `--json` 구조화 출력을 공유합니다. `lancet rg`의 40여 개 플래그는 [`lancet rg` 호환성 표](rg-parity.md)를 참고하세요.

## 종료 코드 {#exit-codes}

| 코드 | 의미 | JSON `status` 값 |
| ---: | --- | --- |
| `0` | 성공 / 1건 이상 매치 / 편집·패치 검증 및 적용 완료 | `complete` |
| `1` | 0건 매치 (또는 `lancet status`에서 인덱스 없음) | `empty` |
| `2` | 잘못된 쿼리·플래그, 미지원 대상/이름 변경, CAS 충돌, I/O 오류 | `invalid_query`, `unsupported`, `conflict` |

## lancet <QUERY> · batch · parse · explain {#cmd-search}

| 명령 / 플래그 | 기본값 / 값 | 설명 |
| --- | --- | --- |
| `lancet <QUERY> [PATH]` | `PATH = .` | 디렉터리 또는 파일에서 LQL 쿼리 평가 |
| `--json` · `--stats` | `false` | 기계 판독용 JSON · stderr 실행 통계 |
| `--no-index` \| `--trust-index` \| `--index-dir DIR` | `auto`, `PATH/.lancet` | 인덱스 최신성 전략 또는 위치 지정 |
| `--hidden` · `--no-ignore` · `-j THREADS` · `--ocr` | `false`, 코어 수, `false` | 숨김/무시 파일 및 OCR 토글 |
| `--max-lines N` · `--max-cols N` | `20`, `240` | 결과 단위당 표시 줄/열 상한 |
| `lancet batch [PATH] -q Q... [--name N...] [--queries-json JSON]` | — | 인덱스 열기·워크 1회 공유로 다수 LQL 실행 |
| `lancet parse <QUERY>` | — | 정규화된 쿼리 및 JSON IR 출력 |
| `lancet explain <QUERY> [PATH]` | — | 필수 트라이그램, 후보 수, 선택도 % 출력 |

## lancet fd · outline · refs · read {#cmd-discover}

| 하위 명령 | 플래그 및 인자 |
| --- | --- |
| `lancet fd [PATTERN] [PATH...]` | `-p/--pattern`, `-t f\|d`, `-e/--ext` (쉼표/반복), `--type-class`, `-g` (글롭; `*`/`?`/`[` 시 자동 선택), `-r` (정규식), `-s` (대소문자 구분), `-d/--max-depth`, `--changed-within DUR`, `--min-size`, `--max-size`, `--tree`, `--dirs`, `--limit N`, `--meta`, `--no-index`, `--trust-index`, `--index-dir`, `--hidden`, `--no-ignore`, `-j`, `--json`, `--stats` |
| `lancet outline [PATH]` | `-d/--max-depth`, `-e/--ext`, `--limit N`, `--hidden`, `--no-ignore`, `-j`, `--ocr`, `--json` |
| `lancet refs <SYMBOL> [PATH]` | `--rank`, `--kind KIND,...` (`--rank` 필요), `-l/--lang`, `-e/--ext`, `--path GLOB`, `--all-files`, `--limit N`, `--no-index`, `--trust-index`, `--index-dir`, `--hidden`, `--no-ignore`, `-j`, `--json`, `--stats` |
| `lancet read <TARGET>...` | 다중 타깃 지원 (`file:10-50`, `zip!entry`), `-l/--lines`, `-s/--symbol`, `-p/--page`, `--sheet`, `--rows`, `--cols`, `-e/--entry`, `--budget-tokens`, `--raw`, `--ocr`, `--json` |

## lancet ast · edit · patch · rename · index · mcp {#cmd-mutate}

| 하위 명령 | 플래그 및 인자 |
| --- | --- |
| `lancet ast <PATTERN> [PATH]` | `-l/--lang`, `-r/--rewrite TEMPLATE`, `--apply`, `--path GLOB` (반복), `--limit N`, `--no-index`, `--trust-index`, `--index-dir`, `--hidden`, `--no-ignore`, `-j`, `--json`, `--stats` |
| `lancet edit <PATH>` | 대상 (택1): `--lines`, `--symbol`, `--find [--all]`, 또는 `--edits FILE\|-` · 연산 (택1): `--with`, `--with-file FILE\|-`, `--delete`, `--insert-before`, `--insert-after` · 안전: `--rev REV`, `--apply`, `--allow-syntax-errors`, `--json` |
| `lancet patch [DIFF\|-]` | `--root DIR` (`.`), `-p/--strip N` (`1`), `--apply`, `--allow-syntax-errors`, `--json` |
| `lancet rename <FILE:LINE[:COL]> <OLD> <NEW>` | `--rev REV`, `--apply`, `--json` |
| `lancet index [PATH]` · `status [PATH]` | `--rebuild` \| `--compact`, `--index-dir`, `--ocr`, `--hidden`, `--no-ignore`, `-j` |
| `lancet mcp` | `--root DIR` (`.`), `--read-only`, `--trust-index`, `-j/--threads` |
