---
nav_title: "LQL & 24 AST Languages"
title: "LQL v0.1 Formal Grammar & 24-Language AST Spec"
description: "EBNF grammar, precedence, matcher/scope/pipe semantics, 9 type classes, and all 24 --lang aliases."
lede: "Formal EBNF grammar, exact query semantics, and the complete 24-language Tree-sitter alias table."
keywords: "lql bnf grammar precedence near before fuzzy regex symbol expand terms budget lang aliases"
source: "https://github.com/CREE1116/lancet/blob/main/docs/grammar.md"
---
Complete formal specification of **LQL v0.1** and the **24 Tree-sitter AST languages** shared by `lancet ast` and `lancet refs`.

## LQL v0.1 EBNF Grammar & Precedence {#bnf}

```ebnf title="EBNF (docs/grammar.md)"
query   := ["find"] expr ("in" scope)* ("|" stage)*
expr    := or_expr
or_expr := and_expr ("OR" and_expr)*
and_expr:= unary (("AND")? unary)*          # adjacent terms form implicit AND
unary   := "NOT" unary | primary
primary := string
         | word                            # unquoted single token -> literal
         | "(" expr ")"
         | "regex(" string ")"
         | "fuzzy(" string ["," uint] ")"  # k in 0..=3, default 1
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
         | "terms" ["top:" uint]           # 1..=100, default 15
         | "budget" ["tokens:"] uint       # uint >= 1
```

**Precedence:** `NOT` > `AND` (explicit or implicit) > `OR`. Unknown keywords or trailing tokens return `status: "invalid_query"` with a 0-based byte `pos`.

## Matchers, Scopes & Stages Semantics {#semantics}

| Construct | Exact Semantics |
| --- | --- |
| `"text"` / bare token | Case-insensitive (Unicode simple case folding) substring match. |
| `regex("pat")` | Rust `regex` multi-line mode (`^`/`$` per line); case-sensitive unless `(?i)`. |
| `fuzzy("word", k)` | Word tokens (`[\p{Alphabetic}\p{N}_]+`) with char Levenshtein distance `0 <= k <= 3` (default 1); index uses sound q-gram `AtLeast` threshold. |
| `symbol("name")` / `symbol:name` | Exact case-insensitive AST definition (`.rs`, `.py`, `.js`, `.ts`, `.tsx`, `.go`). |
| `near(a, b, d)` / `before(a, b, d)` | Both match inside unit with `\|tok(b) - tok(a)\| <= d` (`before` requires `b` strictly after `a`). |
| `in file` / `in block` / `in line` | Whole file (default), maximal non-blank line run (`block`), or single line (match `[start, end)` inside unit). |
| `in function` | Tree-sitter function/method AST nodes (`.rs`, `.py`, `.js`, `.ts`, `.tsx`, `.go`); sets `symbol` on hits. |
| `type:` / `ext:` / `path:` | Values OR within a filter category, AND across categories. 9 `type:` classes: `code`, `document`, `spreadsheet`, `config`, `log`, `data`, `archive`, `image`, `other`. |
| `\| expand` / `sort` / `terms` / `budget` | `expand parent\|unit\|context:N`, deterministic `sort count`/`path`, top `1..=100` non-stopword terms, estimated token cap. |

## 24 Tree-sitter AST Languages (--lang) {#langs-24}

| Language | `--lang` Aliases | Language | `--lang` Aliases |
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
