---
nav_title: "AST Patterns & 24 Languages"
title: "AST Pattern Syntax & the 24 Tree-sitter Languages"
description: "Metavariables, structural equality, rewrite templates, rewrite rules, and every --lang alias shared by lancet ast and lancet refs."
lede: "What a pattern matches, what a rewrite writes, and which languages and aliases are compiled in."
keywords: "ast pattern metavariable $VAR $_ $$$ARGS rewrite template lang aliases tree-sitter"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/rg-and-ast.md"
---
`lancet ast PATTERN [PATH...]` parses each candidate file with Tree-sitter and matches the pattern against its syntax tree. Text search is `lancet rg` ([guide](../guides/search.md)); there is no separate query language.

## Pattern syntax {#syntax}

| Pattern element | Matches |
| --- | --- |
| Code in the target language | The same syntax tree, ignoring whitespace, formatting and comments |
| `$NAME` | Exactly one named node, captured as `NAME` |
| `$NAME` used twice | Two subtrees that are structurally equal (`$X == $X`) |
| `$_` | Any one named node, not captured |
| `$$$NAME` | Zero or more sibling nodes (arguments, fields, statements), captured |
| `if ($C) $S`, `{ $$$B }` | Statements and blocks in C-like languages (C, C++, Rust, Go, JS/TS/TSX, Java, C#, …) |

A pattern must parse in the target language (lancet wraps it as an expression, statement or item as needed). A pattern that matches nothing gives exit code `1`; one that does not parse gives `2` with the reason.

## Rewrites {#rewrite}

| Rule | Behaviour |
| --- | --- |
| `--rewrite TEMPLATE` | Each match is replaced by `TEMPLATE` with captures substituted; prints a unified diff and writes nothing |
| `--apply` | Writes every rewritten file, or none: a file that changed since it was read makes the whole rewrite fail and the files already written are put back |
| Syntax check | A rewrite that adds Tree-sitter syntax errors to any file is refused (`--allow-syntax-errors` overrides) |
| Nested matches | When matches overlap in one file, the outer one is rewritten and `skipped_overlaps` counts the rest; run again for them |
| `--limit` | Refused together with `--apply`, so a capped search can never become a partial rewrite |

## Speed {#speed}

Literals in the pattern (identifiers, keywords of 3+ bytes) are looked up in the index and checked with a byte search before any file is parsed, so only files that can match are parsed. A pattern with no literal (`$X == $X`) has to parse every file of the language. Backtracking in `$$$` is bounded by memoization and a per-node step budget; a pattern that exceeds it finishes with a warning instead of running forever.

## 24 Tree-sitter languages (--lang) {#langs-24}

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

`--lang cpp` also searches `.h` headers. `lancet refs` and `lancet outline` use the same grammars; `lancet rename` resolves scopes for Rust, Python, JavaScript, TypeScript/TSX and Go.
