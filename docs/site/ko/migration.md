---
nav_title: "rg·fd·sg에서 전환"
title: "기존 도구(rg, fd, ast-grep, sed, patch, rga)에서 전환하기"
description: "1:1 명령 대조표, 의도적 차이, git·LSP를 그대로 유지해야 하는 경계 안내."
lede: "ripgrep·fd·ast-grep의 익숙한 사용감을 유지하면서 줄 밀림과 덮어쓰기 사고를 없앱니다."
keywords: "전환 마이그레이션 ripgrep rg fd ast-grep sg sed patch rga pcre2 lsp 차이"
source: "https://github.com/CREE1116/lancet/blob/main/docs/rg-parity.md"
---
기존 작업 습관을 한 번에 바꿀 필요는 없습니다. `lancet rg`와 `lancet patch`로 익숙한 플래그와 diff 파이프를 그대로 쓰면서, 도구 사이의 이음새에서 오류가 생기는 지점부터 LQL·AST·`--rev` CAS를 도입할 수 있습니다.

## ripgrep(rg)에서 오셨다면 {#from-rg}

`lancet rg`는 ripgrep의 플래그, Rust `regex` 엔진, 무시 파일 순회, 종료 코드(`0` 매치, `1` 없음, `2` 오류), 출력 형식을 그대로 구현했으며 68개 엣지 케이스 픽스처와 Linux 커널 전체 쿼리를 `ripgrep 15.2`와 바이트 단위로 대조 검증했습니다.

| ripgrep 명령 | lancet 대안 | 정확한 동작 차이 |
| --- | --- | --- |
| `rg foo src` | `lancet rg foo src` | 파이프 시에도 `-n`이 기본 ON; 끄려면 `-N` |
| `rg -i -w -t py tok` | `lancet rg -i -w -t py tok` | `-i`/`-s`/`-S` 마지막 플래그 우선; AST 기반 `-S`(`\W` 무시 유지) |
| `rg -g '*.toml' --passthru` | `lancet rg -g '*.toml' --passthru` | `--passthru`/`-A`/`-B`/`-C` 마지막 우선; 미일치 파일도 출력 |
| `rg --type-add 'web:*.tsx'` | `lancet rg --type-add 'web:*.tsx' -t web` | `name:glob`, `name:include:a,b` 모두 지원 |
| `rg -P '(?<=a)b'` | `lancet 'before("a", "b", 1)'` 또는 Rust regex | `-P`/PCRE2는 선형 시간·건전한 트라이그램 컴파일을 위해 미지원 |
| `rg --pre pdftotext pat` | `lancet rg --docs pat` | 외부 프로세스 없이 인프로세스로 PDF·오피스·HWP·압축 검색 |

> [!NOTE] lancet rg의 의도적 차이 3가지
> - **항상 경로 요소순 정렬:** `a/b.txt`가 `a.txt`보다 앞(`rg --sort path`와 동일).
> - **줄 번호(`-n`) 기본 ON:** 파이프·리다이렉션 때도 켜져 있으며 `-N`으로 끕니다.
> - **파일명 헤더 없음:** 항상 `path:line:text` 한 줄 형식이며 `--color auto`는 TTY에서만 색을 입힙니다.

## fd / find에서 오셨다면 {#from-fd}

| fd / find 명령 | lancet fd 대안 | 비고 및 추가 기능 |
| --- | --- | --- |
| `fd -e rs -d 2 src` | `lancet fd -e rs -d 2 src docs` | `.lancet` 인덱스 활용; `[PATTERN] [PATH...]` 다중 경로 지원 |
| `fd -t d -d 2` | `lancet fd --tree --dirs -d 2 .` | 파일 없이 폴더 트리만 간결하게 출력 |
| `fd '*_test.go'` | `lancet fd '*_test.go'` | `*`, `?`, `[` 포함 시 자동으로 글롭(`-g`) 매치 |
| `find . -mtime -7 -type f` | `lancet fd -t f --changed-within 7d` | 단위: `s`, `m`, `h`, `d`, `w` |
| `find . -name '*.pdf'` | `lancet fd --type-class document` | 확장자 나열 없이 9개 의미 클래스로 필터링 |
| `fd config src` (패턴이 폴더명과 같을 때) | `lancet fd -p config src` | `-p`/`--pattern`으로 패턴과 경로를 명확히 구분 |

## ast-grep(sg)에서 오셨다면 {#from-ast}

| ast-grep | lancet ast | 중요한 차이점 |
| --- | --- | --- |
| `sg -p '$X.unwrap()' -l rs` | `lancet ast '$X.unwrap()' -l rs` | 패턴에 리터럴이 있으면 `.lancet` 트라이그램 프리필터로 가속 |
| `sg -p 'f($$$A)' -r 'g($$$A)' -U` | `lancet ast 'f($$$A)' -r 'g($$$A)' --apply` | `--apply` 전엔 dry-run diff; `--apply`는 `--limit`과 병행 불가 |
| `sg -p 'if ($C) $S'` | `lancet ast 'if ($C) $S'` | **엄격한 AST 모양 매치:** `else` 없는 `if`만 매치(`if-else`는 별개 노드) |
| `sg -l cpp ...` | `lancet ast ... -l cpp` | Lancet `cpp`는 `.cpp`/`.cc`뿐 아니라 `.h` 헤더도 함께 검색 |

## cat / sed / patch / rga에서 오셨다면 {#from-edit}

| 기존 도구 사슬 | Lancet 대체 | 해결된 사고 |
| --- | --- | --- |
| `sed -n '10,50p' f.rs` | `lancet read f.rs --lines 10:50` | XXH3-64 `rev` 반환 + `--budget-tokens` 상한 |
| `rg -n` 1회 후 `sed -i` 2회 | `lancet edit f.rs --edits - --rev REV --apply` | 모든 구간이 원본 줄 번호 기준; 심볼릭/하드 링크·권한 보존 |
| `patch -p1 < diff` | `lancet patch diff --apply` | 모든 파일 사전 검증, 정확 위치 재배치, CAS 경합 롤백 |
| `rga` + `pdftotext`/`pandoc` | `lancet 'pat'` + `lancet read f.pdf -p 2` | 같은 추출 파이프라인으로 검색 줄 == 읽기 줄 |

## 전문 도구를 그대로 써야 하는 경우 {#when-not}

> [!WARNING] 의도적인 범위 경계
> - **버전 관리:** `git`은 그대로 쓰세요. Lancet은 `.gitignore`를 준수하며 `git diff`를 `lancet patch`로 적용합니다.
> - **여러 파일에 걸친 타입 기반 리팩터링:** 공개 API·필드·메서드·Python 키워드 매개변수의 크로스 파일 이름 변경은 LSP(`rust-analyzer`, `tsserver`, `gopls`)를 쓰세요. Lancet의 `rename`은 단일 파일 로컬 바인딩만 증명해 변경합니다.
