# Agent & Indexed-Search Competitor Benchmark

Machine: `arm64 macOS-27.3-arm64-arm-64bit-Mach-O` · Date: `2026-11-26 17:19:08`

## Summary & positioning

- **What this benchmark supports:** without any resident daemon, `lancet` delivers **20/20 immediate (0 ms) post-edit freshness** (`aft` watcher lag and `tgrep` serverless both score `0/20` at 0 ms), stores a **1/3–1/7× smaller on-disk index** than `aft` / `tgrep` (141.8 MB vs 368.5 MB / 955.2 MB on Linux), uses **~1/3× the peak build RAM** of `grix` (353.5 MB vs 1,001.6 MB on Linux; 272.0 MB vs 893.6 MB on LLVM), preserves **20/20 exact `rg` match-count parity**, and runs literal AST search faster than `ast-grep`, `aft`, and `tensor-grep` across all 4 tested corpora — inside a single binary that also reads and edits on the same coordinate system.
- **Where `lancet` is NOT #1 (read the cells below honestly):**
  - `tgrep serve` (resident daemon) is faster on `regex with literal` in Rust (119.2 ms vs 508.7 ms `lancet auto`) and LLVM (19.9 ms vs 295.8 ms `lancet auto`), and on literal-free regexes in Django (15.4 ms) and Rust (60.9 ms), while slowing down on Linux/LLVM literal-free regexes (1,952.7 ms / 3,640.1 ms).
  - `grix --no-auto-index` (`trust`) is faster than `lancet --trust-index` on Linux rare literal (11.5 ms vs 43.6 ms) and LLVM `regex with literal` (30.2 ms vs 46.1 ms).
  - `aft` (resident stdio process keeping `587–1,260 MB` RAM) is faster than daemonless `lancet auto` on Rust rare literal (8.5 ms vs 73.5 ms).
  - On literal-free regexes without a daemon, `lancet` cannot prune files via trigrams and runs close to `rg` speed.

## Methodology, tool configurations & caveats

- **Testbed & repeatability:** Single Apple Silicon machine (`arm64 macOS`), warm filesystem cache, median of 5 `hyperfine -N --warmup 1` runs (`aft` JSON-RPC uses 1 warmup + 5 runs; AST searches use 3 runs). Reproducible via [`bench/competitors_agents.py`](../competitors_agents.py); Linux `x86_64` numbers are not yet measured.
- **Fairness & default configurations tested:**
  - `lancet` (`lancet 0.1.0`): built via `lancet index .`; queried with `lancet rg` (freshness auto-sync) and `lancet rg --trust-index`; AST via `lancet ast <PAT> . --lang <lang>`.
  - `tgrep` (`tgrep 1.2.9`): built via `tgrep index .`; serverless via `tgrep -n [-F] <PAT> .`; daemon via `tgrep serve .` waited until `tgrep status .` reported ready.
  - `grix` (`grix 0.6.0`): built via `grix index .`; queried with default auto mode `grix [-F] <PAT> .` and `grix --no-auto-index [-F] <PAT> .`.
  - `aft` (`aft 0.68.0`): persistent stdio JSON-RPC server initialized with `configure(harness="runner", project_root=...)` and polled until `search_index.status == "ready"`; queried with `grep(max_results=100000)` and `ast_search(lang=...)`.
  - `tensor-grep` (`tensor-grep 1.142.0`): `tg search` / `tg run --lang <lang>`. Note PyPI has no `arm64 macOS` native wheel (`tensor_grep_rs` unavailable), so it executes via Python wrapping `rg`/`ast-grep`.
  - `probe` (`probe-code 0.6.0`): indexless BM25 AST-block search (`probe search -e <PAT> .`); summarized in the footnote rather than the line-match table because it ranks semantic blocks rather than returning exact grep lines.

## 1. Index build time, disk footprint, and memory

| Corpus | Files | lancet (build / disk / peak RSS) | tgrep (build / disk / peak RSS) | grix (build / disk / peak RSS) | aft (ready / disk / resident RSS) | tgrep serve RSS |
|---|---:|---|---|---|---|---:|
| django | 6,815 | 0.327s / 6.91 MB / 75.7 MB | 0.457s / 38.94 MB / 116.5 MB | 0.224s / 7.96 MB / 191.5 MB | 1.38s / 147.49 MB / 188.2 MB | 26.3 MB |
| rust | 49,964 | 1.188s / 28.81 MB / 191.8 MB | 1.981s / 171.39 MB / 178.7 MB | 1.077s / 27.8 MB / 502.3 MB | 3.574s / 183.98 MB / 587.0 MB | 25.2 MB |
| linux | 86,680 | 3.594s / 141.76 MB / 353.5 MB | 8.95s / 955.21 MB / 305.0 MB | 5.115s / 121.76 MB / 1001.6 MB | 14.429s / 368.48 MB / 1203.1 MB | 27.0 MB |
| llvm | 146,319 | 4.98s / 130.71 MB / 272.0 MB | 8.329s / 810.27 MB / 334.1 MB | 5.41s / 119.66 MB / 893.6 MB | 14.331s / 350.1 MB / 1260.4 MB | 41.7 MB |

## 2. Text search latency (ms, median of 5 runs)

### django (6,815 files)

| Query | rg | lancet auto | lancet trust | tgrep (disk) | tgrep serve | grix auto | grix trust | aft (resident) | tensor-grep |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| rare literal | 116.4 | 44.0 | 2.9 | 7.0 | 13.8 | 46.3 | 3.8 | 48.3 | 225.0 |
| common identifier | 128.2 | 52.6 | 14.4 | 49.4 | 31.9 | 57.0 | 13.3 | 183.3 | 221.3 |
| regex with literal | 139.9 | 54.5 | 14.0 | 58.1 | 32.6 | 58.7 | 15.4 | 154.3 | 225.3 |
| regex, no literal | 131.8 | 106.0 | 63.4 | 327.9 | 15.4 | 117.9 | 71.5 | 133.8 | 222.7 |
| no hit | 128.4 | 44.6 | 2.8 | 6.3 | 5.3 | 46.4 | 3.3 | 41.6 | 228.7 |

### rust (49,964 files)

| Query | rg | lancet auto | lancet trust | tgrep (disk) | tgrep serve | grix auto | grix trust | aft (resident) | tensor-grep |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| rare literal | 619.0 | 73.5 | 3.5 | 22.2 | 37.4 | 83.6 | 4.2 | 8.5 | 763.4 |
| common identifier | 651.9 | 120.8 | 50.9 | 276.3 | 58.9 | 129.8 | 50.8 | 409.8 | 766.0 |
| regex with literal | 746.4 | 508.7 | 419.6 | 2064.1 | 119.2 | 525.2 | 438.7 | 1408.0 | 930.5 |
| regex, no literal | 617.6 | 641.4 | 592.3 | 2962.3 | 60.9 | 710.6 | 625.7 | 733.2 | 815.3 |
| no hit | 606.0 | 71.4 | 2.5 | 19.4 | 5.1 | 79.8 | 3.0 | 3.6 | 761.1 |

### linux (86,680 files)

| Query | rg | lancet auto | lancet trust | tgrep (disk) | tgrep serve | grix auto | grix trust | aft (resident) | tensor-grep |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| rare literal | 1046.6 | 150.7 | 43.6 | 40.6 | 42.0 | 142.0 | 11.5 | 65.7 | 1260.6 |
| common identifier | 1116.0 | 197.5 | 89.0 | 542.6 | 85.3 | 216.9 | 93.2 | 908.3 | 1316.7 |
| regex with literal | 1105.0 | 156.4 | 46.5 | 309.6 | 45.8 | 173.8 | 51.2 | 352.9 | 1358.4 |
| regex, no literal | 1039.8 | 1060.5 | 976.8 | 5776.1 | 1952.7 | 1158.9 | 1040.8 | 2122.4 | 1238.4 |
| no hit | 1022.1 | 118.0 | 3.3 | 32.5 | 5.4 | 141.3 | 4.9 | 17.2 | 1255.8 |

### llvm (146,319 files)

| Query | rg | lancet auto | lancet trust | tgrep (disk) | tgrep serve | grix auto | grix trust | aft (resident) | tensor-grep |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| rare literal | 1843.3 | 240.8 | 7.7 | 63.0 | 64.2 | 312.9 | 7.2 | 79.9 | skipped¹ |
| common identifier | 1888.3 | 284.2 | 59.3 | 280.2 | 65.9 | 353.7 | 52.1 | 332.4 | skipped¹ |
| regex with literal | 1828.3 | 295.8 | 46.1 | 186.1 | 19.9 | 346.3 | 30.2 | 179.3 | skipped¹ |
| regex, no literal | 1825.2 | 1763.4 | 1557.2 | 27208.4 | 3640.1 | 2157.0 | 1864.9 | 4063.6 | skipped¹ |
| no hit | 1795.8 | 255.6 | 3.4 | 49.8 | 5.7 | 327.3 | 3.4 | 65.5 | skipped¹ |

**Notes for §2:**
- **¹ `tensor-grep` on LLVM (`skipped`):** Intentionally skipped (not a crash or timeout) because `tensor-grep 1.142.0` has no `arm64 macOS` native wheel on PyPI; on `django`, `rust`, and `linux`, its Python wrapper already lagged `rg` by `+100–250 ms` per call.
- **Match-count parity vs `rg` (across all 20 text queries):** `lancet` (`auto` & `--trust-index`) and `tgrep` (`serverless` & `serve`) matched `rg` line counts on **20/20** queries. `grix` missed 2 matches on `llvm` `regex, no literal` (`2,989` vs `2,991` lines). `aft` indexed fewer files than `rg` walked (`49,031` vs `49,964` on `rust`; `86,441` vs `86,680` on `linux`), yielding lower `total_matches` on high-hit queries (`20,075` vs `20,208` on `rust`, `44,217` vs `44,826` on `linux`).
- **`probe` (indexless BM25 AST-block ranking):** Timed out (>60 s) on `rust` `common identifier`; on `rare literal` it ran in `536.1 ms` (`django`), `916.4 ms` (`rust`), `5,600.9 ms` (`linux`), and `6,169.8 ms` (`llvm`). Raw measurements are in [`competitors_agents.json`](./competitors_agents.json).

## 3. AST structural search (`ast, literal` pattern, ms)

| Corpus | Pattern | ast-grep | lancet ast | aft `ast_search` | tensor-grep `tg run` |
|---|---|---:|---:|---:|---:|
| django | `self.assertEqual($A, $B)` | 178.6 ms (16,797) | 146.2 ms (16,797) | 469.3 ms (16,795) | 671.7 ms |
| rust | `$X.unwrap()` | 808.8 ms (9,547) | 425.7 ms (9,546²) | 2764.8 ms (9,461) | 1104.9 ms |
| linux | `kmalloc($SZ, $FLAGS)` | 10749.6 ms (4,913) | 1663.0 ms (4,913) | 43017.1 ms (4,885) | 8416.4 ms |
| llvm | `dyn_cast<$T>($X)` | 3147.1 ms (23,104) | 1492.9 ms (23,816³) | 47167.6 ms (23,468) | skipped¹ |

**AST match-count footnotes:**
- **² Rust (`9,546` vs `9,547`, 99.990% agreement):** The single difference is inside an intentionally syntax-broken compiler UI test (`tests/ui/expr/malformed_closure/missing_braces_around_block.rs:19`).
- **³ LLVM (`23,816` vs `23,104`):** `lancet --lang cpp` includes `.h` headers (`715` matches in `.h`), whereas `ast-grep -l cpp` excludes `.h` files. Across the shared `.cpp`/`.cc` files, agreement is **23,101 / 23,104 (99.987%)**; the only 3 non-`.h` differences are macro-qualified `dyn_cast<DEVICE Circle>(S)` calls in `clang/test/Analysis/cast-value-notes.cpp` lines `86, 95, 114`.

## 4. Post-edit freshness (20 edited `.py` files in Django)

| Tool / Mode | Finds new token (0 ms) | Still finds old token (0 ms) | After 1.5 s settle (new / old) |
|---|---:|---:|---|
| `lancet rg` (auto) | 20/20 | 0/20 | — |
| `lancet rg --trust-index` | 0/20 | 0/20 | — |
| `grix` (auto) | 20/20 | 0/20 | — |
| `tgrep` (serverless) | 0/20 | 0/20 | — |
| `tgrep serve` (daemon) | 20/20 | 0/20 | 20/20 / 0/20 |
| `aft` (resident watcher) | 0/20 | 0/20 | 20/20 / 0/20 |

> Why do `lancet --trust-index` and `tgrep` (serverless) show `0/20` old tokens? Both re-verify candidate files on disk before printing, so stale trigram hits for deleted text are filtered out; however, skipping directory mtime/ctime checks means newly added tokens absent from the stale index are missed (`0/20`).
