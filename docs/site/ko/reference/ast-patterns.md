---
nav_title: "AST 패턴 & 24개 언어"
title: "AST 패턴 문법과 24개 Tree-sitter 언어"
description: "메타변수, 구조적 동일성, 리라이트 템플릿과 규칙, lancet ast와 lancet refs가 함께 쓰는 --lang 별칭 전체."
lede: "패턴이 무엇에 맞는지, 리라이트가 무엇을 쓰는지, 어떤 언어와 별칭이 들어 있는지."
keywords: "ast 패턴 메타변수 $VAR $_ $$$ARGS 리라이트 템플릿 lang 별칭 tree-sitter"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/rg-and-ast.md"
---
`lancet ast PATTERN [PATH...]`는 후보 파일을 Tree-sitter로 파싱하고 구문 트리에서 패턴을 찾습니다. 텍스트 검색은 `lancet rg`([안내](../guides/search.md))이며, 따로 쿼리 언어는 없습니다.

## 패턴 문법 {#syntax}

| 패턴 요소 | 맞는 것 |
| --- | --- |
| 대상 언어의 코드 | 같은 구문 트리(공백, 서식, 주석 무시) |
| `$NAME` | 이름 있는 노드 정확히 하나, `NAME`으로 캡처 |
| 같은 `$NAME` 두 번 | 구조가 같은 두 서브트리(`$X == $X`) |
| `$_` | 아무 이름 있는 노드 하나, 캡처 안 함 |
| `$$$NAME` | 형제 노드 0개 이상(인자, 필드, 문장), 캡처 |
| `if ($C) $S`, `{ $$$B }` | C 계열 언어(C, C++, Rust, Go, JS/TS/TSX, Java, C# 등)의 문장과 블록 |

패턴은 대상 언어로 파싱되어야 합니다(필요하면 lancet이 식·문장·항목으로 감쌉니다). 맞는 것이 없으면 종료 코드 `1`, 파싱되지 않으면 이유와 함께 `2`입니다.

## 리라이트 {#rewrite}

| 규칙 | 동작 |
| --- | --- |
| `--rewrite TEMPLATE` | 맞은 곳마다 캡처를 넣은 `TEMPLATE`으로 바꿉니다. unified diff만 출력하고 아무것도 쓰지 않습니다 |
| `--apply` | 바뀌는 파일을 전부 쓰거나 하나도 안 씁니다. 읽은 뒤 바뀐 파일이 있으면 전체가 실패하고, 이미 쓴 파일은 되돌립니다 |
| 구문 검사 | 어느 파일이든 Tree-sitter 구문 오류를 늘리는 리라이트는 거부합니다(`--allow-syntax-errors`로 무시) |
| 겹친 매치 | 한 파일에서 매치가 겹치면 바깥 것을 바꾸고 나머지는 `skipped_overlaps`로 셉니다. 다시 실행하면 나머지도 바뀝니다 |
| `--limit` | `--apply`와 함께 쓰면 거부합니다. 개수를 자른 검색이 부분 리라이트가 되지 않게 하기 위해서입니다 |

## 속도 {#speed}

패턴 속 리터럴(3바이트 이상의 식별자, 키워드)로 인덱스를 찾고 바이트 검색으로 확인한 뒤에만 파싱하므로, 맞을 수 있는 파일만 파싱합니다. 리터럴이 없는 패턴(`$X == $X`)은 그 언어의 모든 파일을 파싱해야 합니다. `$$$`의 백트래킹은 메모이제이션과 노드당 시도 예산으로 묶여 있어서, 예산을 넘는 패턴은 끝없이 돌지 않고 경고와 함께 끝납니다.

## 24개 Tree-sitter 언어 (--lang) {#langs-24}

| 언어 | `--lang` 별칭 | 언어 | `--lang` 별칭 |
| --- | --- | --- | --- |
| Rust | `rs`, `rust` | Scala | `scala`, `sc` |
| Python | `py`, `python`, `pyi` | Swift | `swift` |
| JavaScript | `js`, `javascript`, `mjs`, `cjs`, `jsx` | Ruby | `rb`, `ruby` |
| TypeScript | `ts`, `typescript`, `mts`, `cts` | PHP | `php` |
| TSX | `tsx` | Bash | `sh`, `bash`, `zsh`, `shell` |
| Go | `go`, `golang` | Lua | `lua` |
| C | `c` | HCL / Terraform | `hcl`, `tf`, `tfvars`, `terraform` |
| C++ | `cpp`, `c++`, `cxx`, `cc`, `hpp`, `hxx`, `hh` | HTML | `html`, `htm` |
| Zig | `zig` | CSS | `css` |
| Java | `java` | JSON | `json`, `jsonc` |
| C# | `cs`, `csharp`, `c#` | YAML | `yaml`, `yml` |
| Kotlin | `kt`, `kts`, `kotlin` | TOML | `toml` |

`--lang cpp`는 `.h` 헤더도 찾습니다. `lancet refs`와 `lancet outline`도 같은 문법을 쓰고, `lancet rename`은 Rust, Python, JavaScript, TypeScript/TSX, Go의 스코프를 해석합니다.
