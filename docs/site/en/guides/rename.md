---
nav_title: "Local Scope Rename"
title: "Scope-Proven Local Rename (lancet rename)"
description: "Rename local bindings across Rust, Python, JS, TS, TSX, and Go with two-pass scope and shadowing verification."
lede: "Refuses ambiguous or cross-file bindings rather than guessing, and proves zero shadowing or capture before writing."
keywords: "rename local scope python parameter shadowing capture comment_mentions refs fallback"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md"
---
`lancet rename` performs **tier-1 local-scope symbol renaming** within a single file. Instead of heuristics or plain text substitution, it resolves lexical scopes per language and re-resolves the whole file after substitution to prove zero shadowing or variable capture.

## Target Syntax & Supported Languages (6) {#usage}

Supported languages: **Rust** (`.rs`), **Python** (`.py`), **JavaScript** (`.js`), **TypeScript** (`.ts`), **TSX** (`.tsx`), and **Go** (`.go`).

```bash title="Terminal"
# Dry-run diff (COL is optional when OLD only resolves to one binding on LINE)
lancet rename src/scope.rs:42 old_name new_name

# Disambiguate column (1-based byte offset, matching `lancet rg --column`) + CAS rev
lancet rename src/scope.rs:42:9 old_name new_name --rev <REV> --apply
```

You can point `FILE:LINE[:COL]` at either the **declaration** or **any resolved use** inside the binding's scope. Comments inside the scope mentioning `old` are left intact and reported in `comment_mentions`.

## Two-Pass Scope & Shadowing Verification {#how-proof-works}

1. **Pass 1 (Binding discovery):** builds the scope tree (hoisting, block/function scopes, closure parameters, loop binders) to find every AST identifier bound to that exact local declaration.
2. **Pass 2 (Post-rename re-resolution):** simulates replacing every occurrence with `new` and runs scope resolution again on the new source. If any use now resolves to a different declaration (shadowing) or captures a surrounding binding, Lancet refuses with `unsupported` and writes nothing.

## What lancet rename Intentionally Refuses (and Why) {#refusals}

| Refused Construct | Why Guessing Would Break Code |
| --- | --- |
| Module / top-level / exported symbols | Can be imported across files; requires project-wide compiler/LSP |
| Struct fields & methods | Accessed outside the local scope via receivers across files |
| **Python function parameters** | Callers in other files may pass them as keyword arguments (`fn(old=1)`) |
| Macro bodies & JSX tag names | Opaque token expansion or component resolution outside normal variable bindings |
| Go composite-literal keys (`T{old: v}`) | Field labels, not local variable reads |
| Dynamic Python/JS constructs (`eval`, `with`, `locals()`, `global`, `nonlocal`) | Dynamic lookup makes static scope proof unsound |
| Shadowing / variable capture | `new` collides with an existing binding in scope |

> [!TIP] Fallback for cross-file or refused symbols
> Inspect every definition and usage site with `lancet refs <SYMBOL> src --json`, then apply deterministic edits per file using `lancet edit --edits` or your language's LSP.
