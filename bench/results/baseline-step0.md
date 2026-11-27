# Baseline: step0

End of restructuring step 0 (on top of `95f90de`), Apple M4 Pro, median of 5 runs after a warm-up, no index.

| Command | Corpus | ms | output lines |
|---|---|---:|---:|
| rg literal | `rust` | 965.3 | 5 |
| rg regex | `linux` | 1567.1 | 4083 |
| refs, Rust (4 parses/file today) | `rust/compiler` | 1071.1 | 587 |
| refs, Python | `django` | 109.1 | 22 |
| refs, Go | `kubernetes/pkg` | 289.2 | 37 |
| refs --rank | `django/django/db` | 40.8 | 21 |
| outline, a directory | `django/django/db/models` | 30.0 | 2087 |
| ast, literal pattern | `rust/compiler/rustc_middle` | 36.5 | 223 |
| ast, no literal | `django/django/db` | 44.4 | 2139 |
| read --symbol | `rust/compiler/rustc_middle/src/ty/context.rs` | 15.6 | 24 |

Known gaps against ripgrep 16.0.0 at this point (crate-source tree, 29,792 files):

- Walking alone (`--files`): 0.10 s vs 0.06 s. Files are collected and sorted before searching starts (step 1, pipeline).
- Case-insensitive literals without the index (`-i -l -F`): ~490 ms vs ~435 ms. Case-sensitive literals run at ripgrep's speed (0.52 s vs 0.51 s).
- `refs` parses every candidate file four times (symbols, function spans, identifiers, scopes); step 2 makes it one.
