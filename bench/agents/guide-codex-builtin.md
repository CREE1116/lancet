# Working with files (shell + ripgrep + apply_patch)

Use the shell for reading and searching, and `apply_patch` for every change. Line numbers from `rg -n` are the same line numbers `sed -n` and `nl` show, so a hit at `src/x.rs:42` can be read with `sed -n '30,60p' src/x.rs` directly.

| To… | Use |
|---|---|
| **Read** a file or a slice | `sed -n '1,200p' src/x.rs` · `nl -ba src/x.rs \| sed -n '120,180p'` (line numbers) |
| **Read** a PDF | `pdftotext -layout docs/x.pdf -` |
| **Search** | `rg -n -w spin_lock src` · `rg -n -t py -C2 'def \w+_test'` · `rg -l -w NAME` (files only) |
| **Find files** | `rg --files \| rg '\.ts$'` · `rg --files src \| head -100` |
| **Change** text | `apply_patch` with the exact lines you read (enough context that each hunk matches once) |
| **Create** a file | `apply_patch` with `*** Add File: path` |
| See the layout | `rg --files \| head -100` · `ls src` |
| Definitions + uses | `rg -n -w NAME src` (then look at each hit: comments, strings and other things with the same name also match) |

## Editing

- **Patch straight from what you saw.** An `rg -n` hit or a `sed -n` slice already shows the text: copy those lines into the patch as context. Each hunk must match the file exactly once, so it can't hit the wrong place.
- **The patch result is the check.** A patch that applies replaced exactly what it said; you don't need to re-read unless you changed many places.
- **Several files:** one `apply_patch` call can update several files (`*** Update File:` per file). Find them first with `rg -l -w NAME`.
- **Renaming a function or variable:** `rg -n -w OLD` lists every use. Check each hit: a local variable, a field of another struct, a method of another type or a word in a comment can share the name. Rename only the ones that refer to the thing you are renaming.
- **Rename a local variable** inside one function: read the function, then patch only the lines inside it.
- Avoid `sed -i` / `perl -pi` for renames: they change every match, including other things with the same name, and show you nothing.
- After a code change, build or type-check if the project has a quick command for it (`cargo check`, `tsc --noEmit`, `go build ./...`).

## When something fails

- **Patch doesn't apply**: re-read the lines with `sed -n`, copy them exactly (indentation included), add context.
- **File too big**: read a range with `sed -n 'A,Bp'`, or `rg -n` for the part you need first.
- **Too many search results**: narrow with a path, `-t`, `-g`, `-w`, or a more specific pattern.

## In this workspace

The `lancet` command is not available. Use the shell, `rg` and `apply_patch`.
