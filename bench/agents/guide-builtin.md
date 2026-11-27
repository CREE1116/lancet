# Working with files (built-in tools + ripgrep)

Use the built-in Read, Grep, Glob, Edit and Write tools for files, and `rg` in the shell when you need ripgrep flags the Grep tool doesn't take. Line numbers from Grep/`rg -n` are the same line numbers Read shows, so a hit at `src/x.rs:42` can be read with `offset: 42` directly.

| To… | Use |
|---|---|
| **Read** a file or a slice | Read `file_path` (`offset`, `limit` for big files: it shows the first 2000 lines otherwise) |
| **Read** a PDF | Read the `.pdf`, or `pdftotext -layout docs/x.pdf -` in the shell |
| **Search** | Grep (`pattern`, `path`, `glob`, `output_mode`), or `rg -n -w spin_lock src` · `rg -n -t py -C2 'def \w+_test'` |
| **Find files** | Glob `**/*.ts` · `src/**/mod.rs` |
| **Change** text | Edit `file_path`, `old_string`, `new_string` (`replace_all`) |
| **Create** a file | Write `file_path`, `content` |
| See the layout | `rg --files | head -100` · Glob `src/**` |
| Definitions + uses | `rg -n -w NAME src` (then look at each hit: comments, strings and other things with the same name also match) |

## Editing

- **Edit straight from what you saw.** A Grep hit or a Read already shows the text: pass it as `old_string`. It must occur exactly once in the file (add surrounding text if not, or `replace_all`), so it can't hit the wrong place.
- **The Edit result is the check.** A successful Edit means the text was replaced exactly; you don't need to re-read or re-grep unless you changed many places.
- **Several files:** one Edit per file (with `replace_all` when every occurrence in that file should change). Find the files first with `rg -l -w NAME`.
- **Renaming a function or variable:** `rg -n -w OLD` lists every use. Check each hit: a local variable, a field of another struct, a method of another type or a word in a comment can share the name. Rename only the ones that refer to the thing you are renaming; use `replace_all` only in files where every hit does.
- **Rename a local variable** inside one function: Read the function, then edit only the lines inside it (include enough context in `old_string` that each edit is unique).
- Avoid `sed -i` / `perl -pi` for renames: they change every match, including other things with the same name, and show you nothing.
- After a code change, build or type-check if the project has a quick command for it (`cargo check`, `tsc --noEmit`, `go build ./...`).

## When something fails

- **Edit: old_string not found / not unique**: re-read the lines, copy them exactly (indentation included), add context, or use `replace_all`.
- **Read: file too big**: use `offset` and `limit`, or Grep for the part you need first.
- **Grep: too many results**: narrow with `path`, `glob`, `-w`, or a more specific pattern.

## In this workspace

The `lancet` command is not available. Use the built-in tools and `rg`.
