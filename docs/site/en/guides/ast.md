---
nav_title: "AST Search & Rewrite"
title: "Structural AST Search & Safe Rewrites (24 Languages)"
description: "Pattern matching with $VAR, $_, $$$ARGS, C-like statements, structural equality, dry-run diffs, and overlap skipping."
lede: "Match and transform syntax trees across 24 Tree-sitter languages with trigram prefiltering and dry-run diffs."
keywords: "ast tree-sitter metavariables rewrite apply limit skipped_overlaps if else cpp header"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/rg-and-ast.md"
---
`lancet ast` parses syntax trees with 24 compiled-in Tree-sitter grammars. Unlike plain regexes, patterns match true syntactic structure — ignoring whitespace differences, formatting, comments, and string contents.

## Metavariables & Equality Constraints {#metavariables}

- **`$VAR` (named single node):** captures one named AST node. Repeating the same metavariable (e.g., `$X == $X` or `$X = $X`) enforces **structural equality** across the two subtrees.
- **`$_` (anonymous wildcard):** matches any single named AST node without allocating or storing a named capture.
- **`$$$ARGS` (variadic sibling sequence):** matches 0 or more sibling AST nodes (function arguments, struct fields, statement blocks).
- **C-like statement metavariables:** in C, C++, Rust, Go, JS/TS/TSX, Java, C#, etc., patterns like `if ($C) $S` and `{ $$$B }` bind statement and block subtrees.

## Cross-Language AST Recipes {#recipes}

```bash title="Terminal"
# Rust: preview replacing .unwrap() with ?
lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'

# Rust: find variadic panic! invocations
lancet ast 'panic!($$$ARGS)' src --lang rs

# Python: catch redundant self-comparisons (structural equality on $X)
lancet ast '$X == $X' . --lang py

# Go: rewrite fmt.Errorf wrapping patterns across non-test files
lancet ast 'fmt.Errorf($FMT, $ERR)' pkg/ --lang go --path '!*_test.go' --rewrite 'errors.Wrap($ERR, $FMT)'

# TypeScript / TSX: spot console.log calls
lancet ast 'console.log($$$ARGS)' web/ --lang tsx
```

## Safe Rewrites & Atomic --apply {#rewrites}

1. **Preview first:** passing `--rewrite TEMPLATE` prints unified diffs (`mode: "dry-run"`) without modifying disk.
2. **Inspect `skipped_overlaps`:** when one match is nested inside another match in the same file (for example nested `$X.unwrap().foo.unwrap()`), Lancet keeps the earliest/outermost edit and skips overlapping inner edits cleanly rather than corrupting offsets; check `skipped_overlaps` and re-run once for inner leftovers.
3. **Apply all or nothing:** `--apply` writes every rewritten file or none. A rewrite that adds Tree-sitter syntax errors to any file is refused (`--allow-syntax-errors` overrides), and a file changed since it was read makes the whole rewrite fail with the files already written put back.

> [!WARNING] Important constraints & ast-grep differences
> - **`--apply` refuses `--limit`:** Lancet refuses capped partial rewrites so you don't mutate the first N matches accidentally; narrow with `--path` globs instead.
> - **Strict `if` shape:** `if ($C) $S` matches plain `if` without an `else` branch (unlike `ast-grep`, which matches `if ... else` too).
> - **`--lang cpp` includes `.h`:** C++ AST searches inspect `.h` headers in addition to `.cpp`/`.cc`/`.cxx`/`.hpp`.
> - **Literal-free patterns:** patterns without any 3+ byte literal cannot prune via `.lancet` and must parse all language files (~0.6–1.0× `ast-grep` speed); worst-case `$$$` backtracking is capped by memoization and a step budget.
