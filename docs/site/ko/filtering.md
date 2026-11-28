---
nav_title: "무시 규칙 · 글롭 · 타입"
title: "무시 규칙, 글롭, 파일 타입 & 순회 경계"
description: ".gitignore/.ignore/.rgignore 우선순위, -u/-uu/-uuu, --glob, --type-add, --type-class, -L, --max-depth."
lede: "검색, fd, outline, refs, ast, index가 모두 공유하는 일관된 파일 필터링 및 순회 규칙."
keywords: "gitignore rgignore hidden binary -u -uu -uuu glob type-add max-depth max-filesize 심볼릭링크"
source: "https://github.com/CREE1116/lancet/blob/main/docs/rg-parity.md"
---
Lancet은 `lancet`, `rg`, `fd`, `outline`, `refs`, `ast`, `index` 전역에서 ripgrep의 `ignore` 크레이트 순회 규칙을 동일하게 적용합니다.

## 무시 규칙 우선순위, 숨김 파일 & 바이너리 {#ignore-files}

- **무시 파일 스택:** `.gitignore`, Git exclude, `.ignore`, `.rgignore`를 준수합니다(`--no-ignore` 또는 `-u`로 해제).
- **숨김 파일·폴더:** `.`으로 시작하는 경로(`.lancet` 포함)는 기본 제외되며 `--hidden` / `-.`(`lancet rg`에선 `-uu`)로 포함합니다.
- **바이너리 판별:** 일반 텍스트 검색은 NUL 바이트가 있는 바이너리를 건너뜁니다(`-a` / `--text` / `-uuu` 또는 문서 어댑터 확장자 제외).
- **긍정 글롭 우선:** `lancet rg`에서 `-g '*.lock'` 같은 긍정 글롭은 ripgrep과 똑같이 ignore/hidden 설정을 덮어씁니다.

## lancet rg 제한 해제 단계 (-u / -uu / -uuu) {#rg-unrestricted}

| 플래그 | 동등 옵션 | 동작 |
| --- | --- | --- |
| `-u` | `--no-ignore` | ignore 파일(빌드 산출물, `node_modules` 등)도 검색 |
| `-uu` | `--no-ignore --hidden` | 숨김 파일과 디렉터리도 검색 |
| `-uuu` | `--no-ignore --hidden --text` | 바이너리 파일까지 텍스트로 간주해 검색 |

## 글롭, 내장 파일 타입 & --type-add {#globs-types}

```bash title="터미널"
# 포함/제외 글롭 (!로 부정; 대소문자 무시 --iglob도 지원)
lancet rg -g 'src/**' -g '!src/generated/**' 'retry' .

# ripgrep 내장 파일 타입 포함/제외
lancet rg -t rust -T test 'unsafe' .

# 사용자 정의 타입 추가 또는 기존 타입 병합 (중복 지정 가능)
lancet rg --type-add 'web:*.{ts,tsx,css}' -t web 'ThemeProvider' .
lancet rg --type-add 'jvm:include:java,kotlin,scala' -t jvm 'Executor' .

# --type-add가 반영된 타입 목록 확인
lancet rg --type-add 'proto:*.proto' --type-list
```

## 파일 클래스 (fd --type-class, --path) {#lql-filters}

`lancet fd --type-class`는 파일을 종류로 고르고, `refs`·`ast`의 `--path GLOB`은 검색을 맞는 경로로 좁힙니다. `--path` 값에 글롭 문자(`*`, `?`, `[`, `{`)가 없으면 `dir` 자체와 `dir/**`를 모두 매치합니다.

```bash title="터미널"
# 구조 검색을 크레이트 하나로 좁히기
lancet ast 'deadline($$$A)' . --lang rs --path crates/core

# 의미 클래스로 최근 수정 문서 찾기
lancet fd --type-class document,spreadsheet --changed-within 14d
```

| 클래스 (`--type-class`) | 포함 확장자 / 예시 |
| --- | --- |
| `code` | 24개 이상 프로그래밍 언어 소스 (`.rs`, `.py`, `.ts`, `.go`, `.c`, `.cpp` 등) |
| `document` | `.md`, `.txt`, `.rst`, `.pdf`, `.docx`, `.pptx`, `.odt`, `.hwp`, `.hwpx`, `.epub` |
| `spreadsheet` | `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods`, `.csv`, `.tsv` |
| `config` | `.toml`, `.yaml`, `.yml`, `.ini`, `.env`, `.hcl`, `.tf` |
| `data` | `.json`, `.jsonc`, `.xml`, `.sql` 등 |
| `log` · `archive` · `image` · `other` | `.log` · `.zip/.tar/.gz/.tgz` · `.png/.jpg/.webp/…` · 기타 |

## 심볼릭 링크, 깊이 및 파일 크기 제한 {#bounds}

- **심볼릭 링크 (`-L` / `--follow`):** 기본 비활성. `lancet rg -L --trust-index`에서는 `.lancet`에 빠진 링크 대상까지 놓치지 않도록 의도적으로 라이브 순회로 폴백합니다.
- **깊이·크기 상한:** `--max-depth N`(`1`은 직계 자식만)과 `--max-filesize NUM[K|M|G]`(`rg`), `--min-size`/`--max-size`(`fd`)가 라이브 순회와 `--trust-index` 양쪽에서 모두 적용됩니다.
- **`fd` 다중 경로 & 없는 경로 오류 처리 (v0.1.4):** `lancet fd [PATTERN] [PATH...]`는 여러 경로를 받습니다(`lancet fd . a b`는 `a`와 `b` 전체 나열). `fd`, `rg`, `refs`, `ast`에 존재하지 않는 `PATH`를 주면 조용히 `empty`(종료 1)를 내는 대신 stderr에 `PATH: No such file or directory`를 출력하고 종료 2를 반환합니다. `fd`에서 글롭 문자 없이 `/`가 들어간 단독 인자(`lancet fd docs/imgaes`)도 경로로 취급해 오타를 바로 오류로 보고합니다.
