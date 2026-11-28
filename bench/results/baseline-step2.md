# Baseline: step2

End of restructuring step 2 (on top of `456a337`), Apple M4 Pro, median of 5 runs after a warm-up, no index.

| Command | Corpus | ms | output lines |
|---|---|---:|---:|
| rg literal | `rust` | 768.5 | 5 |
| rg regex | `linux` | 1261.6 | 4083 |
| refs, Rust | `rust/compiler` | 579.9 | 587 |
| refs, Python | `django` | 91.3 | 22 |
| refs, Go | `kubernetes/pkg` | 117.1 | 37 |
| refs --rank | `django/django/db` | 32.4 | 21 |
| outline, a directory | `django/django/db/models` | 21.2 | 2087 |
| ast, literal pattern | `rust/compiler/rustc_middle` | 28.7 | 223 |
| ast, no literal | `django/django/db` | 32.4 | 2139 |
| read --symbol | `rust/compiler/rustc_middle/src/ty/context.rs` | 10.7 | 24 |

Every row is faster than step 1, including `rg`, which step 2 didn't touch (941 -> 769 ms):
the machine was less busy, so compare runs made at the same time. Run interleaved
with the step-0 build, parsing each file once instead of four times gives:

| `refs --no-index` | step 0 | step 2a |
|---|---:|---:|
| `span_bug` in `rust/compiler` | 734 ms | 639 ms |
| `NewController` in `kubernetes/pkg` | 201 ms | 115 ms |
| `get_object_or_404` in `django` | 97 ms | 98 ms |

`outline`, `refs --rank` and `ast` print the same as the step-0 build on seven
languages (22 comparisons) after moving per-language data into the LangSpec table.
