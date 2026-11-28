---
nav_title: "lancet fd 호환성 표"
title: "lancet fd — fd 기능 호환성 매트릭스"
description: "fd 10.5의 모든 플래그: 지원, 받기만 함, 거부와 그 이유. 출력·순서·종료 코드 비교."
lede: "fd의 명령줄과 출력을 lancet의 탐색기 위에서. CI에서 fd 10.5.2와 133개 케이스로 대조합니다."
keywords: "fd 호환 글롭 정규식 smart case type extension size changed-within owner exclude prune format exec"
source: "https://github.com/CREE1116/lancet/blob/main/docs/fd-parity.md"
---
`lancet fd`는 fd의 명령줄을 받아 fd의 출력을 냅니다. 패턴은 항목 이름에 맞춰 보는 정규식이고(smart case), `-g`면 글롭이며, 경로는 패턴 뒤에 오고, 디렉터리는 끝에 `/`를 붙여 출력합니다. 종료 코드도 fd와 같습니다(찾은 것이 있든 없든 `0`, 없는 검색 경로 같은 fd 자체 오류는 `1`, 잘못된 인자는 `2`). 밑에서는 lancet의 탐색기를, `--trust-index`면 인덱스를 씁니다.

## 검증 {#verification}

- **픽스처 대조(`bench/fd_parity.py`, CI에서 실행):** 숨김·무시 파일, `.gitignore`·`.ignore`·`.fdignore`·하위 무시 파일, 파일과 디렉터리를 가리키는 심볼릭 링크와 깨진 링크, 실행 파일, 빈 파일과 빈 디렉터리, FIFO, 여러 점 확장자, 대문자·유니코드·공백이 든 이름, `-`로 시작하는 이름, 정확한 크기, 오래된 수정 시각이 든 픽스처에서 133개 케이스를 돌려 fd 10.5.2와 종료 코드와 출력 줄 집합을 비교합니다. fd의 출력 순서는 스레드에 따라 달라서 정렬해 비교하고, `--max-results`·`-1`은 fd가 어느 결과를 고를지 정해져 있지 않아 개수만 비교합니다.

## 차이 {#differences}

1. **순서.** 결과를 경로순으로 정렬하므로 실행할 때마다 같습니다. fd는 스레드가 찾은 순서대로 출력합니다.
2. **`--max-results`·`-1`**은 fd가 먼저 찾은 것이 아니라 경로순으로 앞의 결과를 돌려줍니다.
3. **인덱스 디렉터리**(`.lancet`)는 나열하지 않습니다(모든 lancet 명령이 건너뜀). `.rgignore`도 따라서 `fd`와 `rg`가 같은 파일을 봅니다.
4. **`-l`, `-x`, `-X`, `--batch-size`**는 거부합니다. lancet은 결과마다 프로세스를 띄우지 않습니다(`rg --pre`를 거부하는 것과 같은 이유). `lancet fd -0 … | xargs -0 CMD`를 쓰세요.
5. fd의 **전역 무시 파일**(`~/.config/fd/ignore`)은 읽지 않습니다.

## 패턴 {#patterns}

| 플래그 | 상태 | 비고 |
|---|---|---|
| `[pattern]` | 지원 | 이름에 맞춰 보는 정규식, `''`나 `.`은 모두 일치 |
| `-g`, `--glob` / `--regex` | 지원 | 마지막 것이 이김 |
| `-F`, `--fixed-strings` | 지원 | 이름의 부분 문자열 |
| `--exact` | 지원 | 이름 전체(`-p`면 경로 전체) |
| `--and PATTERN` | 지원 | 여러 번 가능, 모두 맞아야 함 |
| `-s` / `-i` | 지원 | 기본은 smart case: 패턴에 대문자가 있을 때만 구분(`\S` 같은 이스케이프는 세지 않음) |
| `-p`, `--full-path` | 지원 | 절대 경로에 맞춰 봄 |
| `-p` 없이 `/`가 든 패턴 | 지원 | fd처럼 오류. Glob 도구식 패턴이면 fd 표기를 알려 줌(`src/**/*.ts` → `lancet fd -g '*.ts' src`) |

## 필터 {#filters}

| 플래그 | 상태 | 비고 |
|---|---|---|
| `-t`, `--type` | 지원 | `f`, `d`, `l`, `s`, `p`, `b`, `c`, `x`(파일 포함), `e`(빈 파일·디렉터리), 여러 번 가능 |
| `-e`, `--extension` | 지원 | 대소문자 무시, 앞 점 생략 가능, 여러 점(`tar.gz`), fd처럼 디렉터리에도 적용 |
| `-S`, `--size` | 지원 | `[+-]NUM`과 `b`, `k`, `m`, `g`, `t`(1000 단위) 또는 `ki`, `mi`, `gi`, `ti`(1024 단위), 파일만 통과 |
| `--changed-within` / `--changed-before`와 별칭 | 지원 | humantime 형식 기간(`10h`, `2weeks`, `1h 30min`), `@UNIX초`, 로컬 날짜 `YYYY-MM-DD[ HH:MM:SS]`(날짜는 Unix에서만) |
| `-o`, `--owner` | 지원(Unix) | `[!]user[:[!]group]`, 이름 또는 id |
| `-d`, `--max-depth` / `--min-depth` / `--exact-depth` | 지원 | |
| `-E`, `--exclude` | 지원 | 글롭, 제외한 디렉터리는 들어가지 않음 |
| `--prune` | 지원 | |
| `--ignore-contain NAME` | 지원 | |

## 탐색과 무시 파일 {#traversal}

| 플래그 | 상태 |
|---|---|
| `-H`, `--hidden` / `--no-hidden` | 지원 |
| `-I`, `--no-ignore` / `--ignore` | 지원 |
| `--no-ignore-vcs` / `--ignore-vcs` | 지원 |
| `--no-require-git` / `--require-git` | 지원 |
| `--no-ignore-parent` / `--ignore-parent` | 지원 |
| `-u`, `--unrestricted` | 지원 |
| `--ignore-file PATH` | 지원 |
| `-L`, `--follow` / `--no-follow` | 지원 |
| `--one-file-system`(`--mount`, `--xdev`) | 지원 |
| `-j`, `--threads` | 지원 |

## 경로와 출력 {#output}

| 플래그 | 상태 | 비고 |
|---|---|---|
| `[path]...`, `--search-path` | 지원 | 여러 경로, 준 경로에 이어 붙여 출력(`src/`, `./src`, `.` 모두 fd와 같음) |
| `-C`, `--base-directory` | 지원 | |
| `-a`, `--absolute-path` / `--relative-path` | 지원 | |
| `-0`, `--print0` | 지원 | |
| `--strip-cwd-prefix[=WHEN]` | 지원 | fd 규칙: `-0`일 때와 `-`로 시작하는 이름에는 `./`를 남김 |
| `--path-separator` | 지원 | |
| `--format` | 지원 | `{}`, `{/}`, `{//}`, `{.}`, `{/.}`, `{{`, `}}` |
| `--max-results`, `-1` | 지원 | 경로순으로 앞의 결과 |
| `-q`, `--quiet`(`--has-results`) | 지원 | 하나라도 맞으면 종료 0, 아니면 1 |
| `-c`, `--color`, `--hyperlink`, `--show-errors` | 받기만 함 | 효과 없음 |
| `-l`, `--list-details` | **미지원** | `ls`를 실행함. `lancet fd -0 … \| xargs -0 ls -l` |
| `-x`, `-X`, `--batch-size` | **미지원** | 결과마다 프로세스를 띄우지 않음. `xargs -0` 사용 |

## lancet이 더한 것 {#additions}

| 플래그 | 하는 일 |
|---|---|
| `--tree` | 결과를 검색 경로 아래 트리로 그림 |
| `--dirs` | 디렉터리만(`-t d`), `--tree`와 함께 쓰기 좋음 |
| `--type-class CLASS,...` | 분류별 파일: `code`, `document`, `spreadsheet`, `config`, `log`, `data`, `archive`, `image`, `other` |
| `--json`(`--meta`) | 모든 항목(크기·수정 시각 포함)을 JSON 하나로 |
| `--trust-index` | 디스크를 걷지 않고 인덱스에서 파일 목록을 읽음. 빈 디렉터리는 나오지 않음 |
