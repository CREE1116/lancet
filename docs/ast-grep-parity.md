# `lancet ast` and ast-grep: measured differences

`lancet ast PATTERN [PATH...]` takes ast-grep's pattern language (`$X`, `$_`, `$$$ARGS`, code in the target language) and its `--rewrite` templates, on 24 Tree-sitter grammars. This page says where the two tools find the same matches and where they don't, measured rather than guessed.

## How it was measured

`bench/ast_parity.py` runs 34 patterns over real code in six languages (rustc_middle, Django's `db`, Kubernetes' `pkg/util`, VS Code's `base`, Linux's `kernel/sched`, Spring's `spring-core`) with both tools and compares matches by file and byte range. ast-grep 0.51.3, default settings.

| | Matches |
|---|---:|
| Found by both | 24,099 |
| Only ast-grep | 2,618 |
| Only lancet | 141 |
| Agreement | 89.7% |

27 of the 34 patterns give exactly the same matches. Every difference falls into one of the rows below.

## Where ast-grep matches and lancet doesn't (2,618)

All of these are nodes that have **more than the pattern says**. ast-grep lets a match carry extra trailing parts; lancet requires the whole node to be accounted for.

| Case | Count | Example |
|---|---:|---|
| `if` with an `else` / `elif` for a pattern without one | 2,595 | `if ($C) { $$$B }` vs `if (x) { … } else { … }` |
| `raise X(…) from e` for `raise $E($$$A)` | 13 | Python |
| `async def` for `def $F(self): $$$B` | 7 | Python |
| `new E(…) { …body… }` (anonymous class) for `throw new $E($$$A);` | 2 | Java |
| A statement inside a macro body | 1 | C, `list_for_each_entry(…) x->y = z;` |

**Why lancet keeps this stricter:** a rewrite replaces the whole matched node with the template. If `if ($C) { $$$B }` matched an `if … else`, rewriting it with a template that has no `else` would delete the `else` branch. lancet's rule is that it never rewrites what the pattern didn't describe. To match these with lancet, say so in the pattern: `if ($C) { $$$B } else { $$$E }`, `raise $E($$$A) from $F`, `async def $F(self): $$$B`.

## Where lancet matches and ast-grep doesn't (141)

| Case | Count | Example |
|---|---:|---|
| Call patterns ast-grep parses as something else | 103 | Go `fmt.Errorf($$$A)` (94), Go `errors.New($S)` (6), C `spin_lock($X)` (3): ast-grep finds none |
| A comment inside the matched code | 34 | `if x:  # comment` + `return False`: lancet's patterns skip comments |
| Rust `_` for a metavariable | 3 | `let $X = $Y;` matches `let _ = f();`: ast-grep binds metavariables to named nodes only |
| `$$$R` matching nothing | 1 | Go `if err != nil { return $$$R }` matches a bare `return` |

**Comments and rewrites:** because lancet's patterns skip comments, a match can contain a comment that no capture carries into the template. lancet doesn't rewrite such a match. It prints a warning instead, and with `--apply` the whole rewrite is refused, so a comment is never deleted silently.

## Other differences

| | ast-grep | lancet |
|---|---|---|
| YAML rules, `sg scan`, `sg new`, constraints (`inside`, `has`, `not`), `--strictness` | Yes | No: lancet has the pattern search and rewrite (`sg run`) |
| Rewrites across files | Each file on its own | All files or none; a lost race puts back the files already written |
| A rewrite that adds syntax errors | Written | Refused (`--allow-syntax-errors` overrides) |
| `--apply` with `--limit` | Allowed | Refused (a capped search must not become a partial rewrite) |
| Finding candidate files | Parses every file of the language | Pattern literals filter files through the index and a byte search first |
| `--lang cpp` | `.cpp`, `.cc`, … | Also `.h` |
| `$$$` backtracking | Unbounded | Memoized, with a step budget per node (a pathological pattern finishes with a warning) |

Reproduce with `python3 bench/ast_parity.py` (it needs `bench/.cache/tools/ast-grep` and the corpora from `bench/corpora/`).
