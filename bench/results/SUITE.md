# Benchmark suite — 20261126 (llvm text/AST re-measured after the rg --docs fix)

Machine: arm64 macOS-27.3-arm64-arm-64bit-Mach-O. Warm cache, median of 5 runs (hyperfine -N).

Tools: lancet `lancet 0.1.0`, ripgrep `ripgrep 15.2.0`, ugrep `ugrep 7.8.5 aarch64-apple-darwin27.0.0 +neon/AArch64; -P:pcre2jit; -z:zlib,bzip2,lzma,lz4,zstd,brotli,7z,tar/pax/cpio/zip`, git `git version 2.53.0`, fd `fd 10.5.2`, ast-grep `ast-grep 0.51.3`, zoekt `latest (go install)`, hyperfine `hyperfine 2.0.0`

## linux @ v6.12 — 86,680 files, 1,408.3 MiB

Index: lancet 3.33 s, 358 MiB peak, 133.6 MiB on disk, no-op sync 124.61 ms · zoekt 82.27 s, 1187 MiB peak, 2907.9 MiB

| query | lines | rg | lancet rg | lancet rg --trust-index | lancet rg --no-index | ugrep | git grep | zoekt | = rg |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| rare literal | 38 | 1086 | 148 | 40 | 1113 | 816 | 1136 | 44 | ✓ |
| common identifier | 44,826 | 1123 | 194 | 86 | 1190 | 783 | 1254 | 81 | ✓ (other tools: git grep 44,828 lines) |
| ignore case | 26,188 | 1216 | 173 | 69 | 1160 | 776 | — | — | ✓ |
| word | 11,360 | 1131 | 187 | 85 | 1116 | 896 | — | — | ✓ |
| regex with literal | 18,377 | 1070 | 156 | 46 | 1090 | 815 | — | — | ✓ |
| regex, no literal | 9,487 | 1103 | 1070 | 999 | 1064 | 7568 | — | — | ✓ |
| no hit | 0 | 1133 | 151 | 3 | 1256 | 872 | 1008 | 47 | ✓ |
| count -c | 7,021 | 1149 | 189 | 80 | 1077 | — | — | — | ✓ |

| AST | lancet / ast-grep matches | lancet ast | --trust-index | --no-index | ast-grep |
|---|---|---:|---:|---:|---:|
| `kmalloc($SZ, $FLAGS)` | 4913 / 4913 | 2320 | 2209 | 2625 | 9178 |
| `if ($C) { $$$B }` | 473060 / 592005 | 10782 | 10478 | 10475 | 10213 |

File discovery `-e c`: lancet fd 78 ms, lancet fd --trust-index 15 ms, fd 72 ms

## llvm @ llvmorg-19.1.0 — 146,319 files, 1,575.7 MiB

Index: lancet 4.68 s, 280 MiB peak, 122.0 MiB on disk, no-op sync 251.06 ms · zoekt 53.21 s, 1252 MiB peak, 3704.9 MiB

| query | lines | rg | lancet rg | lancet rg --trust-index | lancet rg --no-index | ugrep | git grep | zoekt | = rg |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| rare literal | 348 | 1875 | 244 | 6 | 1996 | 1556 | 1854 | 45 | ✓ (other tools: zoekt 341 lines) |
| common identifier | 12,703 | 1950 | 310 | 55 | 1911 | 1630 | 1835 | 64 | ✓ (other tools: zoekt 12,056 lines) |
| ignore case | 29,547 | 1852 | 281 | 36 | 1987 | 1593 | — | — | ✓ |
| word | 12,473 | 1931 | 301 | 57 | 1914 | 1574 | — | — | ✓ |
| regex with literal | 923 | 1781 | 291 | 47 | 1973 | 1417 | — | — | ✓ |
| regex, no literal | 2,991 | 1815 | 1931 | 1651 | 1949 | 5450 | — | — | ✓ (other tools: ugrep 2,989 lines) |
| no hit | 0 | 1901 | 239 | 2 | 1946 | 2010 | 1777 | 50 | ✓ |
| count -c | 1,903 | 2035 | 291 | 60 | 1941 | — | — | — | ✓ |

| AST | lancet / ast-grep matches | lancet ast | --trust-index | --no-index | ast-grep |
|---|---|---:|---:|---:|---:|
| `dyn_cast<$T>($X)` | 23816 / 23104 | 1700 | 1517 | 1702 | 3957 |
| `if ($C) { $$$B }` | 148502 / 185918 | 5402 | 4943 | 5268 | 4219 |

File discovery `-e cpp`: lancet fd 180 ms, lancet fd --trust-index 23 ms, fd 176 ms

## rust @ 1.83.0 — 49,964 files, 127.4 MiB

Index: lancet 1.16 s, 184 MiB peak, 28.8 MiB on disk, no-op sync 81.9 ms · zoekt 4.85 s, 739 MiB peak, 326.8 MiB

| query | lines | rg | lancet rg | lancet rg --trust-index | lancet rg --no-index | ugrep | git grep | zoekt | = rg |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| rare literal | 187 | 675 | 70 | 3 | 666 | 426 | 685 | 36 | ✓ |
| common identifier | 20,208 | 722 | 123 | 52 | 684 | 452 | 695 | 61 | ✓ (other tools: git grep 20,141, zoekt 20,185 lines) |
| ignore case | 4,314 | 689 | 80 | 11 | 691 | 438 | — | — | ✓ |
| word | 13,955 | 703 | 121 | 50 | 675 | 461 | — | — | ✓ |
| regex with literal | 74,623 | 794 | 520 | 452 | 699 | 524 | — | — | ✓ |
| regex, no literal | 339 | 654 | 667 | 600 | 663 | 717 | — | — | ✓ |
| no hit | 0 | 675 | 72 | 2 | 647 | 442 | 698 | 40 | ✓ |
| count -c | 3,560 | 729 | 118 | 47 | 700 | — | — | — | ✓ |

| AST | lancet / ast-grep matches | lancet ast | --trust-index | --no-index | ast-grep |
|---|---|---:|---:|---:|---:|
| `$X.unwrap()` | 9546 / 9547 | 478 | 495 | 568 | 848 |
| `if $C { $$$B }` | 31680 / 54363 | 1163 | 874 | 1055 | 914 |

File discovery `-e rs`: lancet fd 54 ms, lancet fd --trust-index 11 ms, fd 53 ms

## kubernetes @ v1.31.0 — 26,232 files, 220.0 MiB

Index: lancet 0.97 s, 224 MiB peak, 30.1 MiB on disk, no-op sync 80.48 ms · zoekt 4.07 s, 1005 MiB peak, 537.0 MiB

| query | lines | rg | lancet rg | lancet rg --trust-index | lancet rg --no-index | ugrep | git grep | zoekt | = rg |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| rare literal | 17 | 424 | 82 | 3 | 452 | 275 | 434 | 43 | ✓ |
| common identifier | 23,778 | 444 | 126 | 52 | 437 | 291 | 409 | 78 | ✓ (other tools: ugrep 23,769 lines) |
| ignore case | 4,562 | 432 | 84 | 13 | 429 | 307 | — | — | ✓ |
| word | 23,776 | 475 | 128 | 55 | 448 | 293 | — | — | ✓ (other tools: ugrep 23,767 lines) |
| regex with literal | 5,140 | 451 | 104 | 31 | 450 | 328 | — | — | ✓ |
| regex, no literal | 226 | 475 | 445 | 373 | 438 | 674 | — | — | ✓ |
| no hit | 0 | 481 | 84 | 2 | 484 | 284 | 459 | 36 | ✓ |
| count -c | 3,233 | 465 | 130 | 53 | 440 | — | — | — | ✓ |

| AST | lancet / ast-grep matches | lancet ast | --trust-index | --no-index | ast-grep |
|---|---|---:|---:|---:|---:|
| `fmt.Errorf($A, $B)` | 11936 / 11936 | 910 | 1049 | 828 | 1737 |
| `if $C { $$$B }` | 220028 / 232579 | 1972 | 1854 | 1920 | 2001 |

File discovery `-e go`: lancet fd 72 ms, lancet fd --trust-index 7 ms, fd 76 ms

## vscode @ 1.95.0 — 7,708 files, 113.9 MiB

Index: lancet 0.38 s, 114 MiB peak, 11.9 MiB on disk, no-op sync 33.99 ms · zoekt 2.35 s, 573 MiB peak, 244.8 MiB

| query | lines | rg | lancet rg | lancet rg --trust-index | lancet rg --no-index | ugrep | git grep | zoekt | = rg |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| rare literal | 93 | 139 | 38 | 4 | 142 | 95 | 129 | 37 | ✓ |
| common identifier | 7,543 | 148 | 55 | 22 | 137 | 95 | 116 | 89 | ✓ (other tools: git grep 7,544, zoekt 7,544 lines) |
| ignore case | 21,208 | 169 | 67 | 36 | 135 | 97 | — | — | ✓ |
| word | 7,425 | 148 | 71 | 37 | 132 | 92 | — | — | ✓ |
| regex with literal | 39 | 147 | 49 | 14 | 143 | 115 | — | — | ✓ |
| regex, no literal | 106 | 140 | 129 | 90 | 124 | 277 | — | — | ✓ |
| no hit | 0 | 144 | 37 | 2 | 142 | 93 | 114 | 37 | ✓ |
| count -c | 1,195 | 136 | 54 | 22 | 132 | — | — | — | ✓ |

| AST | lancet / ast-grep matches | lancet ast | --trust-index | --no-index | ast-grep |
|---|---|---:|---:|---:|---:|
| `this._register($X)` | 7366 / 7366 | 239 | 183 | 716 | 643 |
| `if ($C) { $$$B }` | 51456 / 63042 | 1140 | 708 | 724 | 717 |

File discovery `-e ts`: lancet fd 32 ms, lancet fd --trust-index 4 ms, fd 34 ms

## django @ 5.1 — 6,815 files, 41.9 MiB

Index: lancet 0.21 s, 76 MiB peak, 6.9 MiB on disk, no-op sync 44.44 ms · zoekt 1.19 s, 302 MiB peak, 94.6 MiB

| query | lines | rg | lancet rg | lancet rg --trust-index | lancet rg --no-index | ugrep | git grep | zoekt | = rg |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| rare literal | 63 | 127 | 46 | 3 | 125 | 105 | 99 | 35 | ✓ |
| common identifier | 16,981 | 130 | 52 | 11 | 125 | 113 | 94 | 81 | ✓ |
| ignore case | 4,365 | 130 | 49 | 8 | 131 | 108 | — | — | ✓ |
| word | 16,981 | 133 | 55 | 13 | 123 | 107 | — | — | ✓ |
| regex with literal | 13,336 | 136 | 56 | 14 | 126 | 150 | — | — | ✓ |
| regex, no literal | 54 | 127 | 109 | 64 | 119 | 253 | — | — | ✓ |
| no hit | 0 | 135 | 46 | 2 | 126 | 99 | 112 | 34 | ✓ |
| count -c | 685 | 145 | 57 | 11 | 141 | — | — | — | ✓ |

| AST | lancet / ast-grep matches | lancet ast | --trust-index | --no-index | ast-grep |
|---|---|---:|---:|---:|---:|
| `self.assertEqual($A, $B)` | 16797 / 16797 | 172 | 146 | 176 | 244 |
| `if $C:
    $$$B` | 10012 / 10012 | 256 | 203 | 250 | 206 |

File discovery `-e py`: lancet fd 46 ms, lancet fd --trust-index 3 ms, fd 48 ms

## cpython @ v3.13.0 — 4,882 files, 101.2 MiB

Index: lancet 0.26 s, 116 MiB peak, 10.3 MiB on disk, no-op sync 12.64 ms · zoekt 3.65 s, 509 MiB peak, 226.8 MiB

| query | lines | rg | lancet rg | lancet rg --trust-index | lancet rg --no-index | ugrep | git grep | zoekt | = rg |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| rare literal | 9 | 66 | 12 | 4 | 66 | 47 | 66 | 39 | ✓ (other tools: zoekt 7 lines) |
| common identifier | 7,675 | 62 | 19 | 8 | 59 | 47 | 62 | 45 | ✓ (other tools: zoekt 7,670 lines) |
| ignore case | 51,824 | 72 | 24 | 14 | 66 | 51 | — | — | ✓ |
| word | 7,572 | 65 | 20 | 9 | 65 | 47 | — | — | ✓ |
| regex with literal | 2,933 | 67 | 22 | 11 | 65 | 80 | — | — | ✓ |
| regex, no literal | 561 | 66 | 92 | 88 | 91 | 577 | — | — | ✓ |
| no hit | 0 | 64 | 13 | 2 | 67 | 42 | 73 | 34 | ✓ |
| count -c | 332 | 70 | 17 | 7 | 67 | — | — | — | ✓ |

| AST | lancet / ast-grep matches | lancet ast | --trust-index | --no-index | ast-grep |
|---|---|---:|---:|---:|---:|
| `Py_DECREF($X);` | 7220 / 7220 | 148 | 142 | 146 | 378 |
| `if ($C) { $$$B }` | 37344 / 43124 | 497 | 486 | 497 | 388 |

File discovery `-e c`: lancet fd 10 ms, lancet fd --trust-index 3 ms, fd 12 ms

## spring-framework @ v6.1.14 — 10,163 files, 52.9 MiB

Index: lancet 0.32 s, 122 MiB peak, 12.2 MiB on disk, no-op sync 40.67 ms · zoekt 1.85 s, 395 MiB peak, 146.3 MiB

| query | lines | rg | lancet rg | lancet rg --trust-index | lancet rg --no-index | ugrep | git grep | zoekt | = rg |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| rare literal | 54 | 211 | 41 | 3 | 221 | 126 | 218 | 35 | ✓ |
| common identifier | 2,600 | 234 | 61 | 22 | 223 | 131 | 206 | 44 | ✓ |
| ignore case | 7,436 | 219 | 53 | 15 | 220 | 126 | — | — | ✓ |
| word | 2,600 | 206 | 72 | 34 | 206 | 135 | — | — | ✓ |
| regex with literal | 62 | 209 | 51 | 15 | 215 | 135 | — | — | ✓ |
| regex, no literal | 54 | 208 | 207 | 178 | 212 | 293 | — | — | ✓ |
| no hit | 0 | 221 | 42 | 2 | 228 | 130 | 212 | 35 | ✓ |
| count -c | 1,024 | 223 | 57 | 22 | 215 | — | — | — | ✓ |

| AST | lancet / ast-grep matches | lancet ast | --trust-index | --no-index | ast-grep |
|---|---|---:|---:|---:|---:|
| `Assert.notNull($A, $B)` | 2596 / 2596 | 104 | 61 | 149 | 320 |
| `if ($C) { $$$B }` | 18670 / 24480 | 408 | 336 | 396 | 358 |

File discovery `-e java`: lancet fd 41 ms, lancet fd --trust-index 5 ms, fd 41 ms
