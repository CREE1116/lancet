# Baseline: step1

End of restructuring step 1 (on top of `78e7d63`), Apple M4 Pro, median of 5 runs after a warm-up, no index.

| Command | Corpus | ms | output lines |
|---|---|---:|---:|
| rg literal | `rust` | 941.5 | 5 |
| rg regex | `linux` | 1524.2 | 4083 |
| refs, Rust (4 parses/file today) | `rust/compiler` | 1146.4 | 587 |
| refs, Python | `django` | 108.1 | 22 |
| refs, Go | `kubernetes/pkg` | 272.9 | 37 |
| refs --rank | `django/django/db` | 41.9 | 21 |
| outline, a directory | `django/django/db/models` | 29.1 | 2087 |
| ast, literal pattern | `rust/compiler/rustc_middle` | 34.8 | 223 |
| ast, no literal | `django/django/db` | 42.5 | 2139 |
| read --symbol | `rust/compiler/rustc_middle/src/ty/context.rs` | 16.0 | 24 |

Against step 0: same output everywhere; `refs` (Rust) 1071 -> 1146 ms here, but run
interleaved with the step-0 build it is 1096 vs 1118 ms (+2%, noise), and Go is faster.

Overlapping the walk with the search (pattern 6 in docs/ARCHITECTURE.md) was measured
on the crate-source tree (29,792 files), interleaved with the step-0 build and
ripgrep 16.0.0, median of 7:

| `lancet rg --no-index ...` | ripgrep | walk, then search (kept) | search on walker threads | walker -> rayon `par_bridge` |
|---|---:|---:|---:|---:|
| `-l -F tokio_unstable` | 461–483 ms | 503–510 ms | 536 ms | 568 ms |
| `-c 'pub unsafe fn \w+\('` | 484–499 ms | 492–509 ms | 519 ms | 557 ms |

Both overlapping variants were slower, so neither went in. mimalloc instead of the
macOS system allocator made no difference either.

Without the index, `lancet rg` is within -1%..+5% of ripgrep when the two run
interleaved (the "12% slower" noted at step 0 came from runs at different times).
`--files` stays ~35 ms slower than ripgrep's unsorted listing (faster than
`rg --sort path`): lancet sorts every listing.
