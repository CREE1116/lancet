---
nav_title: "LQL 문법 · AST 24개 언어"
title: "LQL v0.1 정식 문법 & 24개 언어 AST 명세"
description: "EBNF 문법, 연산자 우선순위, 매처/스코프/파이프 의미론, 9종 type: 클래스, 24개 --lang 별칭."
lede: "EBNF 정식 문법, 정확한 쿼리 의미론, Tree-sitter 24개 언어 별칭 표."
keywords: "lql bnf 문법 우선순위 near before fuzzy regex symbol expand terms budget lang 별칭"
source: "https://github.com/CREE1116/lancet/blob/main/docs/grammar.md"
---
**LQL v0.1** 정식 문법과 `lancet ast`·`lancet refs`가 공유하는 **Tree-sitter 24개 언어** 명세입니다.

## LQL v0.1 EBNF 문법 & 우선순위 {#bnf}

```ebnf title="EBNF (docs/grammar.md)"
query   := ["find"] expr ("in" scope)* ("|" stage)*
expr    := or_expr
or_expr := and_expr ("OR" and_expr)*
and_expr:= unary (("AND")? unary)*          # 공백 인접 항은 암묵적 AND
unary   := "NOT" unary | primary
primary := string
         | word                            # 따옴표 없는 단일 토큰 -> 리터럴
         | "(" expr ")"
         | "regex(" string ")"
         | "fuzzy(" string ["," uint] ")"  # k는 0..=3, 기본 1
         | "symbol(" string ")" | "symbol:" (string | word)
         | "near(" matcher "," matcher "," uint ")"
         | "before(" matcher "," matcher "," uint ")"

matcher := string | word | "regex(" string ")" | "fuzzy(" string ["," uint] ")"
         | "symbol(" string ")" | "symbol:" (string | word)

scope   := ("line" | "block" | "file" | "function"
         |  "type:" values | "ext:" values | "path:" values)+
values  := value | "(" value ("," value)* ")"

stage   := "limit" uint                    # uint >= 1
         | "count"
         | "files"
         | "group" "file"
         | "expand" ("parent" | "unit" | "context:" uint)
         | "sort" ("count" ["asc" | "desc"] | "path")
         | "terms" ["top:" uint]           # 1..=100, 기본 15
         | "budget" ["tokens:"] uint       # uint >= 1
```

**우선순위:** `NOT` > `AND`(명시/암묵) > `OR`. 알 수 없는 키워드나 여분 토큰은 0부터 시작하는 바이트 `pos`와 함께 `status: "invalid_query"`를 반환합니다.

## 매처, 스코프 & 파이프 의미론 {#semantics}

| 구문 | 정확한 의미 |
| --- | --- |
| `"text"` / 일반 토큰 | 대소문자 무시(Unicode simple case folding) 부분 문자열 매치. |
| `regex("pat")` | 멀티라인 Rust `regex`(줄 단위 `^`/`$`); 기본 대소문자 구분(`(?i)` 지정 가능). |
| `fuzzy("word", k)` | 단어 토큰(`[\p{Alphabetic}\p{N}_]+`) 중 문자 레벤슈타인 거리 `0 <= k <= 3`(기본 1); 인덱스는 건전한 q-gram `AtLeast` 사용. |
| `symbol("name")` / `symbol:name` | 대소문자 무시 AST 정의 정확 매치(`.rs`, `.py`, `.js`, `.ts`, `.tsx`, `.go`). |
| `near(a, b, d)` / `before(a, b, d)` | 단위 내 토큰 거리 `\|tok(b) - tok(a)\| <= d`(`before`는 `b`가 `a`보다 엄격히 뒤). |
| `in file` / `in block` / `in line` | 파일 전체(기본), 빈 줄 없는 연속 문단(`block`), 단일 줄(`line`; 매치 전체 `[start, end)`가 단위 안). |
| `in function` | Tree-sitter 함수/메서드 AST 노드(`.rs`, `.py`, `.js`, `.ts`, `.tsx`, `.go`); 결과 `symbol` 설정. |
| `type:` / `ext:` / `path:` | 카테고리 내부는 OR, 서로 다른 카테고리는 AND. 9종 `type:`: `code`, `document`, `spreadsheet`, `config`, `log`, `data`, `archive`, `image`, `other`. |
| `\| expand` / `sort` / `terms` / `budget` | `expand parent\|unit\|context:N`, 결정적 `sort count`/`path`, 불용어 제외 상위 `1..=100` 어휘, 추정 토큰 상한. |

## Tree-sitter 24개 지원 언어 (--lang) {#langs-24}

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
