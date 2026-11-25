# Lancet Query Language (LQL v0.1)

`lancet` evaluates queries over a deterministic query IR (`src/query/ast.rs`). Every indexed candidate is verified by the exact same evaluator as a full scan, guaranteeing **`Scan == Auto`** by construction.

## 1. Grammar

```text
query   := ["find"] expr ("in" scope)* ("|" stage)*
expr    := or_expr
or_expr := and_expr ("OR" and_expr)*
and_expr:= unary (("AND")? unary)*          # adjacent terms form an implicit AND
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

- **Precedence**: `NOT` > `AND` (explicit or implicit) > `OR`.
- **Strict errors**: unknown operators, filters, types, stages, or trailing tokens return `status: "invalid_query"` with a byte offset; nothing is silently ignored.
- **Canonical form**: `lancet parse <QUERY>` prints the normalized canonical query (`OR`/`AND` flattened, sorted, deduplicated; `NOT NOT x` collapsed) and its JSON IR.

## 2. Semantics

| Feature | Behavior |
|---|---|
| `"text"` (literal) | Case-insensitive (Unicode simple case folding) substring match. |
| `regex("pat")` | Rust `regex` in multi-line mode (`^`/`$` match line boundaries). Case-sensitive unless `(?i)` is specified. |
| `fuzzy("word", k)` | Word tokens (`[\p{Alphabetic}\p{N}_]+`) within char-level Levenshtein distance `k` (case-insensitive, `0 <= k <= 3`). Accelerated on indexed searches via a sound q-gram threshold (`AtLeast`). |
| `symbol("name")` / `symbol:name` | Exact (case-insensitive) AST symbol definition match via tree-sitter (`.rs`, `.py`, `.js`, `.ts`, `.tsx`, `.go`). |
| `near(a, b, d)` / `before(a, b, d)` | Both match inside the unit with `\|tok(b) - tok(a)\| <= d` (`before` additionally requires `b` strictly after `a`). |
| `in file` (default) | Boolean operators and `near`/`before` evaluate across the entire file. |
| `in block` | Maximal run of non-blank lines. |
| `in line` | Single line. A matcher satisfies a unit only if its entire match `[start, end)` lies within that unit. |
| `in function` | Tree-sitter function/method AST nodes (`.rs`, `.py`, `.js`, `.ts`, `.tsx`, `.go`). Sets `symbol` on each hit. |
| `type:` / `ext:` / `path:` | Values within a category are OR-ed; categories are AND-ed. `type:` supports `code`, `document`, `spreadsheet`, `config`, `log`, `data`, `archive`, `image`, `other`. `path:"dir"` matches both `dir` and `dir/**`. |
| Universal file adapters | Enabled by the default `adapters` feature: extracts searchable text in-process from `.pdf`, `.docx`, `.pptx`, `.odt`, `.hwp` (5.0 OLE), `.hwpx`, `.epub`, `.xlsx`/`.xls`/`.xlsm`/`.xlsb`/`.ods`, `.zip`, `.tar`, `.gz`, `.tgz`/`.tar.gz`. Pass `--ocr` to enable external `tesseract` / `pdftoppm` fallback for images and scanned PDFs. Build with `--no-default-features` for a pure-text/code binary. |
| `\| files` | Emits one compact path-only item per matching file. |
| `\| sort count [asc\|desc]` | Sorts results deterministically by hit count (tie-broken by `(path, start_line)`). |
| `\| terms [top:N]` | Extracts top co-occurring non-stopword tokens across matched units (for deterministic query refinement). |
| `\| budget [tokens:]N` | Caps returned results to an estimated token budget and populates `budget` (`estimated: true`). |

## 3. CLI Surface

```text
lancet <QUERY> [PATH] [--json] [--no-index | --trust-index] [--index-dir DIR] [--ocr]
                      [--hidden] [--no-ignore] [-j THREADS] [--max-lines N] [--max-cols N] [--stats]
lancet fd [PATTERN] [PATH] [-t f|d] [-e EXT] [--type-class CLASS] [-g | -r] [-d DEPTH]
                           [--changed-within DUR] [--min-size S] [--max-size S] [--tree]
                           [--no-index | --trust-index] [--json] [--stats]
lancet ast <PATTERN> [PATH] [-l LANG] [-r TEMPLATE [--apply]] [--path GLOB]
                            [--no-index | --trust-index] [--json] [--stats]
lancet read <TARGET> [-l START:END] [-s SYMBOL] [-p PAGE] [--sheet S] [--rows R] [--cols C]
                     [-e ENTRY] [--budget-tokens N] [--raw] [--ocr] [--json]
lancet outline [PATH] [-d DEPTH] [-e EXT] [--limit N] [--ocr] [--json]
lancet refs <SYMBOL> [PATH] [-l LANG] [-e EXT] [--path GLOB] [--all-files]
                            [--no-index | --trust-index] [--json] [--stats]
lancet index [PATH] [--rebuild] [--index-dir DIR] [--ocr] [--hidden] [--no-ignore] [-j THREADS]
lancet status [PATH] [--index-dir DIR]
lancet parse <QUERY>
lancet explain <QUERY> [PATH] [--index-dir DIR] [--no-index | --trust-index]
lancet batch [PATH] (-q <QUERY>... [--name <NAME>...] | --queries-json <JSON>)
```

Exit codes: `0` (`complete`), `1` (`empty` or missing index in `status`), `2` (`invalid_query`, `unsupported`, or I/O error).
