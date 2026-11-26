# Agent & Indexed-Search Competitor Benchmark

Machine: `arm64 macOS-27.3-arm64-arm-64bit-Mach-O` · Date: `2026-11-26 17:19:08`

Tools:
- `lancet`: `lancet 0.1.0` (daemonless trigram index + exact evaluator)
- `tgrep`: `tgrep 1.2.9` (Microsoft; serverless on-disk index + `tgrep serve` daemon)
- `grix`: `grix 0.6.0` (auto-refreshing trigram index + `--no-auto-index`)
- `aft`: `aft 0.68.0` (CortexKit; persistent stdio JSON-RPC process + watcher)
- `tensor-grep`: `tensor-grep 1.142.0` (Python CLI wrapping `rg`/`ast-grep`; no arm64 macOS native binary on PyPI)
- `probe`: `probe-code 0.6.0` (indexless AST-block BM25 search)

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
| rare literal | 1843.3 | 240.8 | 7.7 | 63.0 | 64.2 | 312.9 | 7.2 | 79.9 | — |
| common identifier | 1888.3 | 284.2 | 59.3 | 280.2 | 65.9 | 353.7 | 52.1 | 332.4 | — |
| regex with literal | 1828.3 | 295.8 | 46.1 | 186.1 | 19.9 | 346.3 | 30.2 | 179.3 | — |
| regex, no literal | 1825.2 | 1763.4 | 1557.2 | 27208.4 | 3640.1 | 2157.0 | 1864.9 | 4063.6 | — |
| no hit | 1795.8 | 255.6 | 3.4 | 49.8 | 5.7 | 327.3 | 3.4 | 65.5 | — |

## 3. AST structural search (`ast, literal` pattern, ms)

| Corpus | Pattern | ast-grep | lancet ast | aft `ast_search` | tensor-grep `tg run` |
|---|---|---:|---:|---:|---:|
| django | `self.assertEqual($A, $B)` | 178.6 ms (16,797) | 146.2 ms (16,797) | 469.3 ms (16,795) | 671.7 ms |
| rust | `$X.unwrap()` | 808.8 ms (9,547) | 425.7 ms (9,546) | 2764.8 ms (9,461) | 1104.9 ms |
| linux | `kmalloc($SZ, $FLAGS)` | 10749.6 ms (4,913) | 1663.0 ms (4,913) | 43017.1 ms (4,885) | 8416.4 ms |
| llvm | `dyn_cast<$T>($X)` | 3147.1 ms (23,104) | 1492.9 ms (23,816) | 47167.6 ms (23,468) | — |

## 4. Post-edit freshness (20 edited `.py` files in Django)

| Tool / Mode | Finds new token (0 ms) | Still finds old token (0 ms) | After 1.5 s settle (new / old) |
|---|---:|---:|---|
| `lancet rg` (auto) | 20/20 | 0/20 | — |
| `lancet rg --trust-index` | 0/20 | 0/20 | — |
| `grix` (auto) | 20/20 | 0/20 | — |
| `tgrep` (serverless) | 0/20 | 0/20 | — |
| `tgrep serve` (daemon) | 20/20 | 0/20 | 20/20 / 0/20 |
| `aft` (resident watcher) | 0/20 | 0/20 | 20/20 / 0/20 |
