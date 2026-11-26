#!/usr/bin/env python3
"""Generate the README figures (docs/assets/*.svg).

Numbers come from `bench/scale_bench.py` on Linux v6.12; update BENCH and rerun:
    python3 bench/readme_figures.py
Every SVG carries its own light/dark palette via prefers-color-scheme.
"""

from __future__ import annotations

import math
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "docs" / "assets"

STYLE = """<style>
text{font-family:system-ui,-apple-system,"Segoe UI",Helvetica,Arial,sans-serif}
.panel{fill:#fcfcfb;stroke:rgba(11,11,11,.10)}
.card{fill:#ffffff;stroke:#c3c2b7}
.hl{fill:#eef5fd;stroke:#2a78d6}
.ink{fill:#0b0b0b}.ink2{fill:#52514e}.mut{fill:#898781}
.mono{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}
.grid{stroke:#e1e0d9}.axis{stroke:#c3c2b7}
.flow{stroke:#898781;fill:none}.flowacc{stroke:#2a78d6;fill:none}
.head{fill:#898781}.headacc{fill:#2a78d6}
.base{fill:#a3a19a}.auto{fill:#2a78d6}.trust{fill:#104281}
.f0{fill:#cde2fb}.f1{fill:#6da7ec}.f2{fill:#1c5cab}
.accent{fill:#2a78d6}.good{fill:#006300}
@media (prefers-color-scheme: dark){
.panel{fill:#1a1a19;stroke:rgba(255,255,255,.10)}
.card{fill:#222220;stroke:#383835}
.hl{fill:#16263a;stroke:#3987e5}
.ink{fill:#f0efec}.ink2{fill:#c3c2b7}.mut{fill:#898781}
.grid{stroke:#2c2c2a}.axis{stroke:#383835}
.flow{stroke:#898781}.flowacc{stroke:#3987e5}
.head{fill:#898781}.headacc{fill:#3987e5}
.base{fill:#6b6a65}.auto{fill:#3987e5}.trust{fill:#9ec5f4}
.f0{fill:#1c3350}.f1{fill:#256abf}.f2{fill:#9ec5f4}
.accent{fill:#3987e5}.good{fill:#0ca30c}
}
</style>"""

DEFS = """<defs>
<marker id="ah" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path class="head" d="M0,1 L10,5 L0,9 z"/></marker>
<marker id="aha" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path class="headacc" d="M0,1 L10,5 L0,9 z"/></marker>
</defs>"""


def esc(s: str) -> str:
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def t(x, y, s, cls="ink", size=13, anchor="start", weight=None):
    w = f' font-weight="{weight}"' if weight else ""
    return f'<text x="{x}" y="{y}" class="{cls}" font-size="{size}" text-anchor="{anchor}"{w}>{esc(s)}</text>'


def svg(w, h, title, desc, body):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" '
        f'role="img" aria-labelledby="title desc">\n<title id="title">{esc(title)}</title>\n'
        f'<desc id="desc">{esc(desc)}</desc>\n{STYLE}\n{DEFS}\n'
        f'<rect class="panel" x="0.5" y="0.5" width="{w-1}" height="{h-1}" rx="12"/>\n'
        + "\n".join(body)
        + "\n</svg>\n"
    )


def card(x, y, w, h, cls="card"):
    return f'<rect class="{cls}" x="{x}" y="{y}" width="{w}" height="{h}" rx="8"/>'


def arrow(x1, y1, x2, y2, acc=False):
    c, m = ("flowacc", "aha") if acc else ("flow", "ah")
    return f'<path class="{c}" stroke-width="1.6" d="M{x1},{y1} L{x2},{y2}" marker-end="url(#{m})"/>'


# ---------------------------------------------------------------- pipeline


def pipeline() -> str:
    W, H = 880, 380
    b = [
        t(28, 40, "How an indexed query skips 91% of the work and stays exact", size=17, weight=600),
        t(28, 62, 'Linux v6.12 · 86,218 files · query "spin_lock" (auto mode)', "ink2", 13),
    ]
    steps = [
        ("1  Compile", ['"spin_lock"  →  required', "trigrams: spi ∧ pin ∧ …", "∧ ock  (sound lower bound)"]),
        ("2  Intersect", ["mmap'd .lancet segments", "posting lists, rarest first", "→ candidate files"]),
        ("3  Freshness", ["disk (mtime, size) ≠ index?", "→ scan that file live,", "no daemon, no reindex"]),
        ("4  Verify", ["same exact evaluator", "as a full scan runs on", "every surviving file"]),
    ]
    cw, ch, gap, y0 = 190, 106, 22, 86
    for i, (head, lines) in enumerate(steps):
        x = 28 + i * (cw + gap)
        b.append(card(x, y0, cw, ch, "hl" if i == 3 else "card"))
        b.append(t(x + 14, y0 + 26, head, "ink", 14, weight=600))
        for j, ln in enumerate(lines):
            b.append(t(x + 14, y0 + 50 + j * 18, ln, "ink2", 12))
        if i < 3:
            b.append(arrow(x + cw + 3, y0 + ch / 2, x + cw + gap - 3, y0 + ch / 2))

    # funnel: linear widths so the 9% really looks like 9%
    fy, full = 232, W - 56 - 200
    rows = [
        (86218, "files in the repository", "f0"),
        (7814, "opened and verified (9.1%)", "f1"),
        (7073, "matching files", "f2"),
    ]
    for i, (n, label, cls) in enumerate(rows):
        y = fy + i * 38
        w = max(full * n / rows[0][0], 6)
        b.append(f'<rect class="{cls}" x="28" y="{y}" width="{w:.1f}" height="26" rx="4"/>')
        lx = 28 + w + 12
        b.append(t(lx, y + 18, f"{n:,}", "ink", 14, weight=600))
        b.append(t(lx + 62, y + 18, label, "ink2", 13))
    b.append(
        t(28, H - 18, "Result is byte-for-byte identical to a full scan — the index only decides what not to open.", "mut", 12)
    )
    return svg(W, H, "Indexed query pipeline",
               "Query compiled to required trigrams, intersected over the index, stale files rescanned, "
               "all candidates verified by the full-scan evaluator: 86,218 files, 7,814 opened, 7,073 matches.", b)


# ---------------------------------------------------------------- coordinates


def coordinates() -> str:
    W, H = 880, 350
    b = [
        t(28, 40, "One coordinate system: find → read → change → find", size=17, weight=600),
        t(28, 62, "Every command speaks the same paths, lines and AST spans, so results chain without re-locating anything.", "ink2", 13),
    ]
    cards = [
        ("Find", "lancet 'execute_one'", ["hit at", "src/search.rs:271"]),
        ("Read", "lancet read src/search.rs", ["--lines 260:300", "or --symbol execute_one"]),
        ("Change", "lancet ast '$X.unwrap()'", ["--rewrite '$X?'", "dry-run diff → --apply"]),
        ("Find again", "lancet 'execute_one'", ["changed file is", "rescanned (mtime)"]),
    ]
    cw, ch, gap, y0 = 190, 112, 22, 86
    for i, (head, cmd, lines) in enumerate(cards):
        x = 28 + i * (cw + gap)
        b.append(card(x, y0, cw, ch, "hl" if i == 2 else "card"))
        b.append(t(x + 14, y0 + 26, head, "ink", 14, weight=600))
        b.append(t(x + 14, y0 + 50, cmd, "ink mono", 11.5))
        for j, ln in enumerate(lines):
            b.append(t(x + 14, y0 + 72 + j * 18, ln, "ink2 mono" if j == 1 or i in (1, 2) else "ink2", 11.5))
        if i < 3:
            b.append(arrow(x + cw + 3, y0 + ch / 2, x + cw + gap - 3, y0 + ch / 2, acc=True))
    # loop back
    lx1, lx2, ly = 28 + 3 * (cw + gap) + cw / 2, 28 + cw / 2, y0 + ch
    b.append(
        f'<path class="flow" stroke-width="1.4" stroke-dasharray="4 4" fill="none" '
        f'd="M{lx1},{ly + 4} C{lx1},{ly + 30} {lx2},{ly + 30} {lx2},{ly + 8}" marker-end="url(#ah)"/>'
    )
    b.append(t(W / 2, ly + 46, "no reindex step in the loop", "mut", 12, anchor="middle"))

    by = 268
    b.append(card(28, by, W - 56, 52))
    b.append(t(44, by + 22, "Same in-process extraction everywhere", "ink", 13, weight=600))
    b.append(t(44, by + 41, "line 42 in a search hit is line 42 in read — for .rs .py .ts …  .pdf .docx .pptx .xlsx .hwp .hwpx  bundle.zip!node.log", "ink2", 12))
    return svg(W, H, "One coordinate system",
               "Search hits, read ranges, symbols and AST rewrites share coordinates; edited files are rescanned "
               "automatically on the next query.", b)


# ---------------------------------------------------------------- benchmark

# (label, detail, baseline tool, baseline ms, lancet auto ms, lancet trust ms)
BENCH = [
    ("Rare literal", '"PM_RESUME"', "rg", 1043.5, 146.8, 45.0),
    ("Rare identifier", '"kmalloc_node_track"', "rg", 1037.9, 107.5, 9.7),
    ("Common word", '"spin_lock" · 7,073 files', "rg", 1063.6, 188.3, 90.5),
    ("No match", "zero-hit query", "rg", 1046.8, 104.4, 4.7),
    ("AST pattern", "kmalloc($SZ, $FLAGS)", "ast-grep", 7953.0, 1586.0, 1484.0),
    ("File discovery", "*.c files", "fd", 78.9, 81.8, 19.9),
]


def bench() -> str:
    W = 880
    left, right = 230, 150
    pw = W - left - right
    row_h, bar_h, top = 66, 14, 112
    H = top + row_h * len(BENCH) + 58
    lo, hi = 1, 10_000

    def x(v):
        return left + pw * (math.log10(v) - math.log10(lo)) / (math.log10(hi) - math.log10(lo))

    b = [
        t(28, 40, "Linux kernel v6.12 — 86,218 files, 1.4 GB", size=17, weight=600),
        t(28, 62, "Median wall time, Apple M4 Pro (12 cores). Log scale: every gridline is 10× — shorter is faster.", "ink2", 13),
    ]
    # legend
    lx = 28
    for cls, name in (("base", "rg / ast-grep / fd"), ("auto", "lancet (auto: always fresh)"), ("trust", "lancet --trust-index")):
        b.append(f'<rect class="{cls}" x="{lx}" y="78" width="12" height="12" rx="3"/>')
        b.append(t(lx + 18, 89, name, "ink2", 12.5))
        lx += 34 + 7.2 * len(name)
    # grid
    for v, lab in ((1, "1 ms"), (10, "10 ms"), (100, "100 ms"), (1000, "1 s"), (10000, "10 s")):
        gx = x(v)
        b.append(f'<line class="grid" x1="{gx:.1f}" y1="{top - 8}" x2="{gx:.1f}" y2="{top + row_h * len(BENCH) - 6}" stroke-width="1"/>')
        b.append(t(gx, top + row_h * len(BENCH) + 12, lab, "mut", 11.5, anchor="middle"))
    b.append(f'<line class="axis" x1="{left}" y1="{top - 8}" x2="{left}" y2="{top + row_h * len(BENCH) - 6}" stroke-width="1"/>')

    for i, (label, detail, tool, base, auto, trust) in enumerate(BENCH):
        y = top + i * row_h
        b.append(t(28, y + 18, label, "ink", 13.5, weight=600))
        b.append(t(28, y + 36, detail, "ink2 mono" if "(" in detail or '"' in detail else "ink2", 11.5))
        for j, (cls, v, name) in enumerate(((("base"), base, tool), ("auto", auto, "auto"), ("trust", trust, "trust"))):
            by = y + j * (bar_h + 2)
            bw = max(x(v) - left, 3)
            b.append(f'<rect class="{cls}" x="{left}" y="{by}" width="{bw:.1f}" height="{bar_h}" rx="3"/>')
            val = f"{v/1000:.2f} s" if v >= 1000 else f"{v:.1f} ms" if v < 100 else f"{v:.0f} ms"
            txt = f"{val}  {name}" if j == 0 else val
            b.append(t(left + bw + 6, by + 11, txt, "ink2", 11))
        sp = base / trust
        b.append(t(W - 28, y + 30, f"{sp:.0f}× faster" if sp >= 2 else f"{sp:.1f}×", "good", 13.5, anchor="end", weight=600))
    b.append(t(28, H - 16, "Speedup = baseline ÷ lancet --trust-index.  lancet scan mode (no index) ≈ rg on text queries.  Reproduce: bench/scale_bench.py", "mut", 11.5))
    return svg(W, H, "Benchmark on the Linux kernel",
               "; ".join(f"{l}: {tool} {b_:.0f} ms, lancet auto {a:.0f} ms, trust {tr:.1f} ms" for l, _, tool, b_, a, tr in BENCH), b)


# ---------------------------------------------------------------- index lifecycle


def lifecycle() -> str:
    W, H = 880, 300
    b = [
        t(28, 40, "The index: immutable segments, cheap to keep fresh", size=17, weight=600),
        t(28, 62, "LSM-style: new and changed files go to new segments, old versions are tombstoned, small segments are merged.", "ink2", 13),
    ]
    y0 = 92
    # files
    b.append(card(28, y0, 150, 120))
    b.append(t(42, y0 + 26, "Working tree", "ink", 14, weight=600))
    for j, ln in enumerate(["walk (.gitignore-aware)", "compare mtime + size", "unchanged → skip"]):
        b.append(t(42, y0 + 50 + j * 18, ln, "ink2", 12))
    b.append(arrow(182, y0 + 60, 222, y0 + 60))
    # segments
    sx = 226
    b.append(card(sx, y0, 380, 120))
    b.append(t(sx + 14, y0 + 26, ".lancet/  (mmap'd, zero-copy)", "ink", 14, weight=600))
    for k in range(6):
        cls = "f1" if k < 4 else "f0"
        b.append(f'<rect class="{cls}" x="{sx + 14 + k * 58}" y="{y0 + 42}" width="50" height="34" rx="4"/>')
        b.append(t(sx + 39 + k * 58, y0 + 63, f"seg {k + 1}", "ink" if k >= 4 else "ink", 11, anchor="middle"))
    b.append(t(sx + 14, y0 + 96, "≤96 MiB input / ≤65,536 docs per segment", "ink2", 12))
    b.append(t(sx + 14, y0 + 112, "tombstones mark replaced or deleted files", "ink2", 12))
    b.append(arrow(sx + 384, y0 + 60, sx + 424, y0 + 60))
    # compaction
    cx = sx + 428
    b.append(card(cx, y0, W - 28 - cx, 120, "hl"))
    b.append(t(cx + 14, y0 + 26, "Tiered compaction", "ink", 14, weight=600))
    for j, ln in enumerate([">6 segments or ≥25% dead", "→ k-way merge small ones", "big segments left alone"]):
        b.append(t(cx + 14, y0 + 50 + j * 18, ln, "ink2", 12))
    # stats strip
    stats = [("4.2 s", "cold build"), ("~350 MB", "peak memory"), ("140 MB", "index (10%)"), ("110 ms", "no-op sync"), ("134 ms", "sync +200 files")]
    sw = (W - 56) / len(stats)
    for i, (v, lab) in enumerate(stats):
        x0 = 28 + i * sw
        b.append(t(x0 + 4, 252, v, "ink", 20, weight=650))
        b.append(t(x0 + 4, 272, lab, "ink2", 12))
    b.append(t(W - 28, 290, "Linux v6.12, 86,218 files", "mut", 11, anchor="end"))
    return svg(W, H, "Index lifecycle",
               "Working tree compared by mtime and size, new files written to immutable mmap segments with "
               "tombstones, small segments merged by tiered compaction. Linux: 4.2 s cold build, 350 MB peak, "
               "140 MB index, 110 ms no-op sync.", b)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name, fn in (("pipeline", pipeline), ("coordinates", coordinates), ("bench-linux", bench), ("index-lifecycle", lifecycle)):
        (OUT / f"{name}.svg").write_text(fn(), encoding="utf-8")
        print(f"wrote docs/assets/{name}.svg")


if __name__ == "__main__":
    main()
