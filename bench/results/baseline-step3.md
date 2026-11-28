# Baseline: step3

Commit `4326798`, median of 5 runs after a warm-up, no index.

| Command | Corpus | ms | output lines |
|---|---|---:|---:|
| rg literal | `rust` | 905.5 | 5 |
| rg regex | `linux` | 1478.8 | 4083 |
| refs, Rust | `rust/compiler` | 975.7 | 587 |
| refs, Python | `django` | 101.9 | 22 |
| refs, Go | `kubernetes/pkg` | 164.6 | 37 |
| refs --rank | `django/django/db` | 42.1 | 21 |
| outline, a directory | `django/django/db/models` | 29.0 | 2087 |
| ast, literal pattern | `rust/compiler/rustc_middle` | 38.5 | 223 |
| ast, no literal | `django/django/db` | 39.6 | 2139 |
| read --symbol | `rust/compiler/rustc_middle/src/ty/context.rs` | 17.3 | 24 |

The table above ran on a busier machine (`rg literal`, untouched in this step, was 18% slower than in step 2). Run interleaved against the step-2 build (`dc9fe0a`), median of 7–9:

| Command | step 2 ms | step 3 ms | |
|---|---:|---:|---|
| rg literal (`rust`) | 975.0 | 982.7 | +0.8%, same output |
| refs, Rust | 1021.9 | 971.4 | -4.9%, same output |
| refs, Go | 161.4 | 167.5 | +3.7%, same output |
| ast, literal pattern | 39.3 | 40.7 | +3.5%, same output |
| ast `--rewrite` preview, 42 Rust files change | 35.3 | 69.0 | the new syntax check parses each rewritten file once more (the original's tree is reused) |
| ast `--rewrite` preview, Python, newline in template | 44.7 | 77.6 | step 2 printed (and with `--apply` would have written) 70 files that no longer parse; step 3 refuses |
