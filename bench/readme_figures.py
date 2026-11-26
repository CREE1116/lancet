#!/usr/bin/env python3
"""Generate bilingual (English + Korean) README & docs figures in docs/assets/.

All quantitative figures read directly from:
  - bench/results/suite-final.json
  - bench/results/failure_modes.json

Usage:
    python3 bench/readme_figures.py

Every SVG embeds both light and GitHub-dark palettes via @media (prefers-color-scheme: dark).
"""

from __future__ import annotations

import json
import math
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT / "bench" / "results"
OUT = ROOT / "docs" / "assets"

SUITE = json.loads((RESULTS / "suite-final.json").read_text(encoding="utf-8"))
FAILURES = json.loads((RESULTS / "failure_modes.json").read_text(encoding="utf-8"))

CORPUS_ORDER = [
    "llvm",
    "linux",
    "rust",
    "kubernetes",
    "spring-framework",
    "vscode",
    "django",
    "cpython",
]

CORPUS_LABELS = {
    "llvm": ("LLVM 19.1", "C++"),
    "linux": ("Linux v6.12", "C"),
    "rust": ("rust 1.83", "Rust"),
    "kubernetes": ("kubernetes 1.31", "Go"),
    "spring-framework": ("Spring 6.1", "Java"),
    "vscode": ("VS Code 1.95", "TS"),
    "django": ("Django 5.1", "Python"),
    "cpython": ("CPython 3.13", "C/Py"),
}


# ---------------------------------------------------------------------------
# Styling & SVG primitives
# ---------------------------------------------------------------------------


def make_style(lang: str) -> str:
    sans = (
        '"Apple SD Gothic Neo","Pretendard","Noto Sans KR","Malgun Gothic",-apple-system,BlinkMacSystemFont,sans-serif'
        if lang == "ko"
        else '-apple-system,BlinkMacSystemFont,"Segoe UI",Helvetica,Arial,sans-serif'
    )
    return f"""<style>
text{{font-family:{sans}}}
.mono{{font-family:ui-monospace,SFMono-Regular,"SF Mono",Menlo,Consolas,monospace}}
.num{{font-variant-numeric:tabular-nums}}
.panel{{fill:#fbfbfa;stroke:rgba(27,31,36,.12)}}
.card{{fill:#ffffff;stroke:#d0d7de}}
.pill{{fill:#f6f8fa;stroke:#d0d7de}}
.hl{{fill:#eff6ff;stroke:#2563eb}}
.hlwarn{{fill:#fffbeb;stroke:#d97706}}
.ink{{fill:#1f2328}}.ink2{{fill:#59636e}}.mut{{fill:#7d8590}}
.grid{{stroke:#eaeef2}}.axis{{stroke:#afb8c1}}.div{{stroke:#d8dee4}}
.flow{{stroke:#8c959f;fill:none}}.flowacc{{stroke:#2563eb;fill:none}}
.head{{fill:#8c959f}}.headacc{{fill:#2563eb}}
.base{{fill:#94a3b8}}.auto{{fill:#2563eb}}.trust{{fill:#0d9488}}
.zoekt{{fill:#d97706;stroke:#ffffff;stroke-width:1.4}}
.warn{{fill:#d97706}}.bad{{fill:#cf222e}}.good{{fill:#1a7f37}}
.badsoft{{fill:#ffebe9;stroke:#ff8182}}.goodsoft{{fill:#dafbe1;stroke:#4ac26b}}
.track{{fill:#eaeef2}}
.f0{{fill:#dbeafe}}.f1{{fill:#60a5fa}}.f2{{fill:#1d4ed8}}
.cblue{{fill:#2563eb}}.cteal{{fill:#0d9488}}.cwarn{{fill:#d97706}}.cbad{{fill:#cf222e}}.cgood{{fill:#1a7f37}}
@media (prefers-color-scheme: dark){{
.panel{{fill:#0d1117;stroke:rgba(240,246,252,.12)}}
.card{{fill:#161b22;stroke:#30363d}}
.pill{{fill:#21262d;stroke:#30363d}}
.hl{{fill:#13233a;stroke:#58a6ff}}
.hlwarn{{fill:#271f12;stroke:#f59e0b}}
.ink{{fill:#f0f6fc}}.ink2{{fill:#9da7b3}}.mut{{fill:#7d8590}}
.grid{{stroke:#21262d}}.axis{{stroke:#484f58}}.div{{stroke:#30363d}}
.flow{{stroke:#6e7681}}.flowacc{{stroke:#58a6ff}}
.head{{fill:#6e7681}}.headacc{{fill:#58a6ff}}
.base{{fill:#6e7681}}.auto{{fill:#58a6ff}}.trust{{fill:#2dd4bf}}
.zoekt{{fill:#f59e0b;stroke:#161b22;stroke-width:1.4}}
.warn{{fill:#f59e0b}}.bad{{fill:#ff7b72}}.good{{fill:#3fb950}}
.badsoft{{fill:#311618;stroke:#f85149}}.goodsoft{{fill:#12261e;stroke:#2ea043}}
.track{{fill:#21262d}}
.f0{{fill:#172b44}}.f1{{fill:#2f81f7}}.f2{{fill:#79c0ff}}
.cblue{{fill:#58a6ff}}.cteal{{fill:#2dd4bf}}.cwarn{{fill:#f59e0b}}.cbad{{fill:#ff7b72}}.cgood{{fill:#3fb950}}
}}
</style>"""


DEFS = """<defs>
<marker id="ah" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6.5" markerHeight="6.5" orient="auto-start-reverse"><path class="head" d="M0,1 L10,5 L0,9 z"/></marker>
<marker id="aha" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6.5" markerHeight="6.5" orient="auto-start-reverse"><path class="headacc" d="M0,1 L10,5 L0,9 z"/></marker>
</defs>"""


def esc(s: str) -> str:
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def est_px(s: str, size: float = 12.0) -> float:
    """CJK-aware approximate width in pixels for dynamic pill/legend spacing."""
    w = 0.0
    for ch in s:
        o = ord(ch)
        if 0x1100 <= o <= 0x11FF or 0x3000 <= o <= 0x9FFF or 0xAC00 <= o <= 0xD7AF:
            w += 1.02 * size
        elif ch in "mwMW@":
            w += 0.82 * size
        elif ch in "ijlI1.,:|!· ":
            w += 0.34 * size
        else:
            w += 0.57 * size
    return w


def t(x, y, s, cls="ink", size=13, anchor="start", weight=None):
    w = f' font-weight="{weight}"' if weight else ""
    return f'<text x="{x:.1f}" y="{y:.1f}" class="{cls}" font-size="{size}" text-anchor="{anchor}"{w}>{esc(s)}</text>'


def card(x, y, w, h, cls="card", rx=9):
    return f'<rect class="{cls}" x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="{rx}"/>'


def arrow(x1, y1, x2, y2, acc=False):
    c, m = ("flowacc", "aha") if acc else ("flow", "ah")
    return f'<path class="{c}" stroke-width="1.6" d="M{x1:.1f},{y1:.1f} L{x2:.1f},{y2:.1f}" marker-end="url(#{m})"/>'


def diamond(cx, cy, r=5.0, cls="zoekt"):
    return f'<polygon class="{cls}" points="{cx:.1f},{cy-r:.1f} {cx+r:.1f},{cy:.1f} {cx:.1f},{cy+r:.1f} {cx-r:.1f},{cy:.1f}"/>'


def pill(x, y, text_str, size=11.5, cls="pill", tcls="ink2", weight=600, h=24, pad_x=11):
    pw = est_px(text_str, size) + pad_x * 2
    out = [
        f'<rect class="{cls}" x="{x:.1f}" y="{y:.1f}" width="{pw:.1f}" height="{h}" rx="{h/2:.1f}"/>',
        t(x + pad_x, y + h * 0.68, text_str, tcls, size, weight=weight),
    ]
    return "\n".join(out), pw


def svg(w, h, lang, title, desc, body):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" '
        f'role="img" aria-labelledby="title desc">\n<title id="title">{esc(title)}</title>\n'
        f'<desc id="desc">{esc(desc)}</desc>\n{make_style(lang)}\n{DEFS}\n'
        f'<rect class="panel" x="0.5" y="0.5" width="{w-1}" height="{h-1}" rx="12"/>\n'
        + "\n".join(body)
        + "\n</svg>\n"
    )


def fmt_ms(v: float) -> str:
    if v >= 1000:
        return f"{v/1000:.2f} s"
    if v < 10:
        return f"{v:.1f} ms"
    return f"{v:.0f} ms"


def fmt_bytes(b: float) -> str:
    if b >= 1000:
        return f"{b/1000:.1f} KB"
    return f"{b:.0f} B"


def fmt_mib(m: float) -> str:
    if m >= 1024:
        return f"{m/1024:.2f} GiB"
    if m >= 100:
        return f"{m:.0f} MiB"
    return f"{m:.1f} MiB"


def fmt_speedup(sp: float, lang: str) -> str:
    unit = "배" if lang == "ko" else "×"
    if sp >= 10:
        return f"{sp:.0f}{unit}"
    return f"{sp:.1f}{unit}"


# ---------------------------------------------------------------------------
# 1. Hero chart: 8-codebase rare-literal search (bench-suite / bench-suite.ko)
# ---------------------------------------------------------------------------


def make_bench_suite(lang: str) -> str:
    ko = lang == "ko"
    W = 920
    total_files = sum(SUITE["corpora"][c]["files"] for c in CORPUS_ORDER)
    left, right = 226, 152
    pw = W - left - right
    row_h, bar_h, gap, top = 62, 12, 3, 150
    H = top + row_h * len(CORPUS_ORDER) + 52
    lo, hi = 1.0, 10_000.0

    def x_pos(v: float) -> float:
        clamped = max(lo, min(hi, v))
        return left + pw * (math.log10(clamped) - math.log10(lo)) / (math.log10(hi) - math.log10(lo))

    title = (
        "8개 코드베이스 · 7개 언어: 리터럴 검색 속도 비교"
        if ko
        else "8 codebases, 7 languages: rare-literal text search"
    )
    subtitle = (
        "Apple M4 Pro · 웜 캐시 5회 중앙값 · 로그 스케일(눈금마다 10배, 짧을수록 빠름)"
        if ko
        else "Apple M4 Pro · warm cache, median of 5 runs · log scale (each gridline is 10× — shorter is faster)"
    )
    b = [
        t(28, 36, title, "ink", 17.5, weight=700),
        t(28, 58, subtitle, "ink2", 12.5),
    ]

    # Header KPI pills
    pills = (
        [
            (f"8개 코드베이스 · 7개 언어 · 파일 {total_files:,}개", "pill", "ink"),
            ("ripgrep과 바이트 단위 일치 (불일치 0건)", "goodsoft", "cgood"),
            ("최대 8.0배 (auto) · 786배 (--trust-index)", "hl", "cblue"),
        ]
        if ko
        else [
            (f"8 codebases · 7 languages · {total_files:,} files", "pill", "ink"),
            ("0 byte diffs vs ripgrep", "goodsoft", "cgood"),
            ("Up to 8.0× auto · 786× --trust-index", "hl", "cblue"),
        ]
    )
    px = 28.0
    for label, pcls, tcls in pills:
        markup, w_pill = pill(px, 70, label, size=11.5, cls=pcls, tcls=tcls, weight=600, h=24, pad_x=10)
        b.append(markup)
        px += w_pill + 10

    # Legend
    lx = 28.0
    ly = 112.0
    legend_items = (
        [
            ("rect", "base", "ripgrep 15.2"),
            ("rect", "auto", "lancet rg (auto · 항상 최신)"),
            ("rect", "trust", "lancet rg --trust-index"),
            ("diamond", "zoekt", "zoekt (재인덱싱 전까지 낡은 결과)"),
        ]
        if ko
        else [
            ("rect", "base", "ripgrep 15.2"),
            ("rect", "auto", "lancet rg (auto · always fresh)"),
            ("rect", "trust", "lancet rg --trust-index"),
            ("diamond", "zoekt", "zoekt (stale until reindex)"),
        ]
    )
    for kind, cls, label in legend_items:
        if kind == "rect":
            b.append(f'<rect class="{cls}" x="{lx:.1f}" y="{ly}" width="11" height="11" rx="2.5"/>')
        else:
            b.append(diamond(lx + 5.5, ly + 5.5, 5.2, cls))
        b.append(t(lx + 17, ly + 9.5, label, "ink2", 12))
        lx += 24 + est_px(label, 12) + 14

    # Log gridlines
    grid_bottom = top + row_h * len(CORPUS_ORDER) - 8
    for v, lab in ((1, "1 ms"), (10, "10 ms"), (100, "100 ms"), (1000, "1 s"), (10000, "10 s")):
        gx = x_pos(v)
        b.append(f'<line class="grid" x1="{gx:.1f}" y1="{top - 6}" x2="{gx:.1f}" y2="{grid_bottom}" stroke-width="1"/>')
        b.append(t(gx, grid_bottom + 16, lab, "mut num", 11, anchor="middle"))
    b.append(f'<line class="axis" x1="{left}" y1="{top - 6}" x2="{left}" y2="{grid_bottom}" stroke-width="1"/>')

    for i, key in enumerate(CORPUS_ORDER):
        c = SUITE["corpora"][key]
        rare = next(q for q in c["text"] if q["query"] == "rare literal")
        rg_ms = rare["ms"]["rg"]["median"]
        auto_ms = rare["ms"]["lancet rg"]["median"]
        trust_ms = rare["ms"]["lancet rg --trust-index"]["median"]
        zoekt_ms = rare["ms"]["zoekt"]["median"]
        cname, clang = CORPUS_LABELS[key]
        token = rare["args"][-1]
        if len(token) > 17:
            token = token[:16] + "…"
        y = top + i * row_h

        if i > 0:
            b.append(f'<line class="grid" x1="28" y1="{y - 4}" x2="{W - 28}" y2="{y - 4}" stroke-width="0.7"/>')

        b.append(t(28, y + 21, f"{cname} ({clang})", "ink", 13, weight=650))
        files_txt = f"{c['files']:,}개 · {token}" if ko else f"{c['files']:,} files · {token}"
        b.append(t(28, y + 39, files_txt, "mut", 11))

        # Bars: rg, auto, trust (+ zoekt diamond on trust row)
        y_rg = y + 7
        y_auto = y_rg + bar_h + gap
        y_trust = y_auto + bar_h + gap

        bw_rg = max(x_pos(rg_ms) - left, 4)
        bw_auto = max(x_pos(auto_ms) - left, 4)
        bw_trust = max(x_pos(trust_ms) - left, 4)
        zx = x_pos(zoekt_ms)

        b.append(f'<rect class="base" x="{left}" y="{y_rg}" width="{bw_rg:.1f}" height="{bar_h}" rx="2.5"/>')
        b.append(t(left + bw_rg + 6, y_rg + 10, fmt_ms(rg_ms), "ink2 num", 11))

        b.append(f'<rect class="auto" x="{left}" y="{y_auto}" width="{bw_auto:.1f}" height="{bar_h}" rx="2.5"/>')
        b.append(t(left + bw_auto + 6, y_auto + 10, fmt_ms(auto_ms), "cblue num", 11, weight=600))

        b.append(f'<rect class="trust" x="{left}" y="{y_trust}" width="{bw_trust:.1f}" height="{bar_h}" rx="2.5"/>')
        b.append(diamond(zx, y_trust + bar_h / 2, 5.0, "zoekt"))
        trust_label_x = max(left + bw_trust, zx + 4) + 7
        b.append(t(trust_label_x, y_trust + 10, f"{fmt_ms(trust_ms)}  (◆ {fmt_ms(zoekt_ms)})", "cteal num", 11, weight=600))

        sp_auto = rg_ms / auto_ms
        sp_trust = rg_ms / trust_ms
        b.append(t(W - 28, y + 23, f"{fmt_speedup(sp_auto, lang)} auto", "cblue num", 12.5, anchor="end", weight=700))
        b.append(t(W - 28, y + 41, f"{fmt_speedup(sp_trust, lang)} trust", "cteal num", 12, anchor="end", weight=650))

    footer = (
        "전체 8개 코퍼스 × 8개 텍스트 쿼리 모두 ripgrep과 바이트 단위 일치 · 데이터: bench/results/suite-final.json"
        if ko
        else "All 8 corpora × 8 text queries verified byte-identical to ripgrep · Data: bench/results/suite-final.json"
    )
    b.append(t(28, H - 14, footer, "mut", 11.5))

    return svg(
        W,
        H,
        lang,
        title,
        "Rare-literal search benchmark across 8 codebases comparing ripgrep, lancet auto, lancet --trust-index, and zoekt.",
        b,
    )


def make_bench_linux_compat() -> str:
    """Keep docs/assets/bench-linux.svg updated from suite-final.json Linux v6.12 data."""
    W = 920
    c = SUITE["corpora"]["linux"]
    qmap = {q["query"]: q for q in c["text"]}
    rows = [
        ("Rare literal", qmap["rare literal"]["args"][-1], "rg", qmap["rare literal"]),
        ("Common identifier", qmap["common identifier"]["args"][-1], "rg", qmap["common identifier"]),
        ("Regex w/ literal", qmap["regex with literal"]["args"][-1], "rg", qmap["regex with literal"]),
        ("No match", "zero-hit token", "rg", qmap["no hit"]),
        (
            "AST w/ literal",
            c["ast"][0]["pattern"],
            "ast-grep",
            {
                "ms": {
                    "rg": c["ast"][0]["ms"]["ast-grep"],
                    "lancet rg": c["ast"][0]["ms"]["lancet ast"],
                    "lancet rg --trust-index": c["ast"][0]["ms"]["lancet ast --trust-index"],
                }
            },
        ),
        (
            "File discovery",
            c["fd"]["query"],
            "fd",
            {
                "ms": {
                    "rg": c["fd"]["ms"]["fd"],
                    "lancet rg": c["fd"]["ms"]["lancet fd"],
                    "lancet rg --trust-index": c["fd"]["ms"]["lancet fd --trust-index"],
                }
            },
        ),
    ]
    left, right = 236, 150
    pw = W - left - right
    row_h, bar_h, top = 66, 14, 112
    H = top + row_h * len(rows) + 56
    lo, hi = 1.0, 10_000.0

    def x_pos(v):
        return left + pw * (math.log10(max(lo, v)) - math.log10(lo)) / (math.log10(hi) - math.log10(lo))

    b = [
        t(28, 38, f"Linux kernel {c['tag']} — {c['files']:,} files, {c['mib']:,.0f} MiB", "ink", 17, weight=700),
        t(28, 60, "Median wall time, Apple M4 Pro. Log scale: every gridline is 10× — shorter is faster.", "ink2", 12.5),
    ]
    lx = 28.0
    for cls, name in (("base", "rg / ast-grep / fd"), ("auto", "lancet (auto: always fresh)"), ("trust", "lancet --trust-index")):
        b.append(f'<rect class="{cls}" x="{lx:.1f}" y="78" width="12" height="12" rx="3"/>')
        b.append(t(lx + 18, 88.5, name, "ink2", 12))
        lx += 24 + est_px(name, 12) + 16
    for v, lab in ((1, "1 ms"), (10, "10 ms"), (100, "100 ms"), (1000, "1 s"), (10000, "10 s")):
        gx = x_pos(v)
        b.append(f'<line class="grid" x1="{gx:.1f}" y1="{top - 8}" x2="{gx:.1f}" y2="{top + row_h * len(rows) - 6}" stroke-width="1"/>')
        b.append(t(gx, top + row_h * len(rows) + 12, lab, "mut num", 11, anchor="middle"))
    b.append(f'<line class="axis" x1="{left}" y1="{top - 8}" x2="{left}" y2="{top + row_h * len(rows) - 6}" stroke-width="1"/>')

    for i, (label, detail, tool, item) in enumerate(rows):
        y = top + i * row_h
        base = item["ms"]["rg"]["median"]
        auto = item["ms"]["lancet rg"]["median"]
        trust = item["ms"]["lancet rg --trust-index"]["median"]
        b.append(t(28, y + 18, label, "ink", 13.5, weight=650))
        b.append(t(28, y + 36, detail, "ink2 mono", 11))
        for j, (cls, v, name) in enumerate((("base", base, tool), ("auto", auto, "auto"), ("trust", trust, "trust"))):
            by = y + j * (bar_h + 2.5)
            bw = max(x_pos(v) - left, 4)
            b.append(f'<rect class="{cls}" x="{left}" y="{by:.1f}" width="{bw:.1f}" height="{bar_h}" rx="3"/>')
            txt = f"{fmt_ms(v)}  {name}" if j == 0 else fmt_ms(v)
            b.append(t(left + bw + 6, by + 11, txt, "ink2 num", 11))
        sp = base / trust
        b.append(t(W - 28, y + 30, f"{sp:.0f}× faster" if sp >= 10 else f"{sp:.1f}× faster", "cgood num", 13, anchor="end", weight=700))
    b.append(t(28, H - 16, "Data: bench/results/suite-final.json · Reproduce: python3 bench/suite.py", "mut", 11.5))
    return svg(W, H, "en", "Linux v6.12 benchmark", "Benchmark on Linux v6.12 from suite-final.json.", b)


# ---------------------------------------------------------------------------
# 2. Pipeline (pipeline / pipeline.ko)
# ---------------------------------------------------------------------------


def make_pipeline(lang: str) -> str:
    ko = lang == "ko"
    W, H = 920, 392
    title = (
        "인덱스 쿼리가 작업의 91%를 건너뛰고도 정확한 이유"
        if ko
        else "How an indexed query skips 91% of the work and stays exact"
    )
    subtitle = (
        'Linux v6.12 · 파일 86,218개 · "spin_lock" 쿼리 (auto 모드)'
        if ko
        else 'Linux v6.12 · 86,218 files · query "spin_lock" (auto mode)'
    )
    b = [
        t(28, 36, title, "ink", 17.5, weight=700),
        t(28, 58, subtitle, "ink2", 12.5),
    ]
    steps = (
        [
            ("1  컴파일", ['"spin_lock" → 필수', "트라이그램: spi ∧ pin", "∧ … ∧ ock (보수적 하한)"]),
            ("2  교집합", ["mmap .lancet 세그먼트", "가장 드문 포스팅부터", "교집합 → 후보 파일"]),
            ("3  신선도 확인", ["mtime·size·ctime·inode", "달라진 파일만 즉시 스캔", "데몬·재인덱싱 없음"]),
            ("4  정확한 검증", ["풀스캔과 같은 평가기로", "남은 모든 후보 파일을", "바이트 단위로 검증"]),
        ]
        if ko
        else [
            ("1  Compile", ['"spin_lock" → required', "trigrams: spi ∧ pin ∧ …", "∧ ock (sound lower bound)"]),
            ("2  Intersect", ["mmap'd .lancet segments", "posting lists, rarest first", "→ candidate files"]),
            ("3  Freshness", ["mtime·size·ctime·inode", "changed? scan live,", "no daemon, no reindex"]),
            ("4  Verify", ["same exact evaluator", "as a full scan runs on", "every surviving file"]),
        ]
    )
    cw, ch, gap, y0 = 201, 112, 20, 82
    for i, (head, lines) in enumerate(steps):
        x = 28 + i * (cw + gap)
        b.append(card(x, y0, cw, ch, "hl" if i == 3 else "card"))
        b.append(t(x + 14, y0 + 27, head, "cblue" if i == 3 else "ink", 14, weight=700))
        for j, ln in enumerate(lines):
            b.append(t(x + 14, y0 + 52 + j * 19, ln, "ink2", 12))
        if i < 3:
            b.append(arrow(x + cw + 3, y0 + ch / 2, x + cw + gap - 3, y0 + ch / 2))

    fy, full = 228, 530.0
    rows = (
        [
            (86218, "저장소 전체 파일", "f0"),
            (7814, "열어서 검증한 파일 (9.1%)", "f1"),
            (7073, "실제 매치된 파일 (8.2%)", "f2"),
        ]
        if ko
        else [
            (86218, "files in the repository", "f0"),
            (7814, "opened and verified (9.1%)", "f1"),
            (7073, "matching files (8.2%)", "f2"),
        ]
    )
    for i, (n, label, cls) in enumerate(rows):
        y = fy + i * 38
        w = max(full * n / rows[0][0], 8)
        b.append(f'<rect class="{cls}" x="28" y="{y}" width="{w:.1f}" height="26" rx="4"/>')
        lx = 28 + w + 12
        b.append(t(lx, y + 18, f"{n:,}", "ink num", 14, weight=700))
        b.append(t(lx + 68, y + 18, label, "ink2", 12.5))

    footer = (
        "출력은 풀스캔(ripgrep)과 바이트 단위로 같습니다 — 인덱스는 '열지 않을 파일'만 결정합니다."
        if ko
        else "Result is byte-for-byte identical to a full scan — the index only decides what not to open."
    )
    b.append(t(28, H - 16, footer, "mut", 12))
    return svg(W, H, lang, title, subtitle, b)


# ---------------------------------------------------------------------------
# 3. Coordinates (coordinates / coordinates.ko)
# ---------------------------------------------------------------------------


def make_coordinates(lang: str) -> str:
    ko = lang == "ko"
    W, H = 920, 360
    title = (
        "하나의 좌표계: 찾기 → 읽기 → 고치기 → 다시 찾기"
        if ko
        else "One coordinate system: find → read → change → find"
    )
    subtitle = (
        "모든 명령이 같은 경로·줄·AST 구간과 내용 리비전(rev)을 공유해, 단계 사이에서 좌표를 잃지 않습니다."
        if ko
        else "Every command shares paths, lines, AST spans and rev hashes, so steps chain without re-locating anything."
    )
    b = [
        t(28, 36, title, "ink", 17.5, weight=700),
        t(28, 58, subtitle, "ink2", 12.5),
    ]
    cards = (
        [
            ("1  찾기 (Find)", "lancet rg / query", ["src/search.rs:271", "정확한 줄·열 반환"]),
            ("2  읽기 (Read)", "lancet read --symbol", ["--lines 260:300", "정확한 줄 + rev 반환"]),
            ("3  고치기 (Change)", "edit --rev / patch / ast", ["dry-run diff 확인 →", "--apply 원자적 반영"]),
            ("4  다시 찾기", "lancet 'execute_one'", ["바뀐 파일만 즉시 스캔", "별도 재인덱싱 없음"]),
        ]
        if ko
        else [
            ("1  Find", "lancet rg / query", ["hit at", "src/search.rs:271"]),
            ("2  Read", "lancet read --symbol", ["--lines 260:300", "exact lines + rev hash"]),
            ("3  Change", "edit --rev / patch / ast", ["dry-run diff preview →", "--apply atomically"]),
            ("4  Find again", "lancet 'execute_one'", ["changed file auto-", "rescanned on next query"]),
        ]
    )
    cw, ch, gap, y0 = 201, 114, 20, 82
    for i, (head, cmd, lines) in enumerate(cards):
        x = 28 + i * (cw + gap)
        b.append(card(x, y0, cw, ch, "hl" if i == 2 else "card"))
        b.append(t(x + 14, y0 + 26, head, "cblue" if i == 2 else "ink", 13.5, weight=700))
        b.append(t(x + 14, y0 + 50, cmd, "ink mono", 11.5))
        for j, ln in enumerate(lines):
            b.append(t(x + 14, y0 + 73 + j * 18, ln, "ink2", 11.5))
        if i < 3:
            b.append(arrow(x + cw + 3, y0 + ch / 2, x + cw + gap - 3, y0 + ch / 2, acc=True))

    lx1 = 28 + 3 * (cw + gap) + cw / 2
    lx2 = 28 + cw / 2
    ly = y0 + ch
    b.append(
        f'<path class="flow" stroke-width="1.4" stroke-dasharray="4 4" fill="none" '
        f'd="M{lx1:.1f},{ly + 4} C{lx1:.1f},{ly + 32} {lx2:.1f},{ly + 32} {lx2:.1f},{ly + 8}" marker-end="url(#ah)"/>'
    )
    loop_txt = "루프 중간에 재인덱싱이나 데몬 대기 없음" if ko else "no reindex step or background daemon in the loop"
    b.append(t(W / 2, ly + 47, loop_txt, "mut", 12, anchor="middle"))

    by = 272
    b.append(card(28, by, W - 56, 58))
    bhead = "모든 형식에서 동일한 인프로세스 추출" if ko else "Same in-process extraction everywhere"
    bline = (
        "검색 결과 42번째 줄 = read의 42번째 줄 — .rs .py .ts …  .pdf .docx .pptx .xlsx .hwp .hwpx  bundle.zip!node.log"
        if ko
        else "line 42 in search is line 42 in read — for .rs .py .ts …  .pdf .docx .pptx .xlsx .hwp .hwpx  bundle.zip!node.log"
    )
    b.append(t(44, by + 24, bhead, "ink", 13.5, weight=650))
    b.append(t(44, by + 44, bline, "ink2", 12))
    return svg(W, H, lang, title, subtitle, b)


# ---------------------------------------------------------------------------
# 4. Failure modes 2x2 dashboard (failure-modes / failure-modes.ko)
# ---------------------------------------------------------------------------


def make_failure_modes(lang: str) -> str:
    ko = lang == "ko"
    W, H = 920, 608
    title = (
        "도구를 이어 붙이면 실제로 생기는 사고들 (재현 결과)"
        if ko
        else "What actually breaks when tools are chained (reproduced)"
    )
    subtitle = (
        "실제 도구 실행 측정치 (ripgrep 15.2 · BSD sed/patch · pandoc 3 · rga 0.10 · zoekt) · bench/results/failure_modes.json"
        if ko
        else "Measured with real tools (ripgrep 15.2 · BSD sed/patch · pandoc 3 · rga 0.10 · zoekt) · bench/results/failure_modes.json"
    )
    b = [
        t(28, 36, title, "ink", 17.5, weight=700),
        t(28, 58, subtitle, "ink2", 12),
    ]

    col_w = 424.0
    xL, xR = 28.0, 28.0 + col_w + 16.0
    yT, hT = 76.0, 210.0
    yB, hB = 302.0, 270.0

    # --- Top-Left: Scenario B (Lost updates)
    b.append(card(xL, yT, col_w, hT))
    b.append(t(xL + 16, yT + 26, "B · 동시 편집 시 수정 유실" if ko else "B · Concurrent multi-agent lost updates", "ink", 14, weight=700))
    b.append(t(xL + 16, yT + 44, "에이전트 4개가 같은 파일에 동시 추가" if ko else "4 agents appending concurrently to one file", "ink2", 11.5))

    b_data = FAILURES["B_lost_updates"]
    ch_tot = b_data["chain"]["updates"]
    ch_lost = b_data["chain"]["lost"]
    ch_kept = ch_tot - ch_lost
    la_tot = b_data["lancet"]["updates"]
    la_lost = b_data["lancet"]["lost"]
    la_retried = b_data["lancet"]["conflicts_retried"]

    bw = col_w - 32
    # Chain bar
    y1 = yT + 66
    b.append(t(xL + 16, y1, "일반 도구: 읽기 → 고치기 → 통째로 쓰기" if ko else "Tool chain: read → edit → write back", "ink", 12, weight=600))
    kept_w = bw * (ch_kept / ch_tot)
    b.append(f'<rect class="track" x="{xL + 16}" y="{y1 + 8}" width="{bw:.1f}" height="18" rx="4"/>')
    b.append(f'<rect class="base" x="{xL + 16}" y="{y1 + 8}" width="{kept_w:.1f}" height="18" rx="4"/>')
    b.append(f'<rect class="bad" x="{xL + 16 + kept_w:.1f}" y="{y1 + 8}" width="{bw - kept_w:.1f}" height="18" rx="4"/>')
    lost_pct = 100 * ch_lost / ch_tot
    b.append(
        t(
            xL + 16,
            y1 + 41,
            (f"{ch_tot}개 중 {ch_lost}개 유실 ({lost_pct:.0f}%) · 오류 없이 조용히 덮어씀" if ko else f"{ch_lost} of {ch_tot} lost ({lost_pct:.0f}%) · silent overwrite, exit 0"),
            "cbad num",
            11.5,
            weight=650,
        )
    )

    # Lancet bar
    y2 = yT + 132
    b.append(t(xL + 16, y2, "lancet edit --rev --apply (낙관적 잠금)" if ko else "lancet edit --rev --apply (CAS lock)", "ink", 12, weight=600))
    b.append(f'<rect class="good" x="{xL + 16}" y="{y2 + 8}" width="{bw:.1f}" height="18" rx="4"/>')
    b.append(
        t(
            xL + 16,
            y2 + 41,
            (f"{la_tot}개 중 {la_lost}개 유실 (0%) · 충돌 {la_retried}회 감지 후 재시도" if ko else f"{la_lost} of {la_tot} lost (0%) · {la_retried} conflicts caught & retried"),
            "cgood num",
            11.5,
            weight=650,
        )
    )

    # --- Top-Right: Scenarios E & G (Document coordinates & stale index)
    b.append(card(xR, yT, col_w, hT))
    b.append(t(xR + 16, yT + 26, "E · G · 낡은 좌표와 낡은 인덱스" if ko else "E · G · Broken doc lines & stale index", "ink", 14, weight=700))

    e_data = FAILURES["E_document_coordinates"]
    g_data = FAILURES["G_stale_index"]
    mbw = 148.0
    bx = xR + 114.0

    # E section
    ye = yT + 50
    b.append(t(xR + 16, ye, "E · .docx 40개 검색 후 해당 줄 열기 (적중 수)" if ko else "E · .docx search → read line 40 terms (hit rate)", "ink2", 11.5, weight=600))
    b.append(t(xR + 16, ye + 20, "rga → pandoc", "ink", 11.5))
    b.append(f'<rect class="badsoft" x="{bx}" y="{ye + 9}" width="{mbw:.1f}" height="13" rx="3"/>')
    b.append(t(bx + mbw + 8, ye + 20, f"{e_data['chain_line_contains_term']} / {e_data['terms']} (0%)", "cbad num", 11.5, weight=650))

    b.append(t(xR + 16, ye + 39, "lancet", "ink", 11.5, weight=650))
    b.append(f'<rect class="good" x="{bx}" y="{ye + 28}" width="{mbw:.1f}" height="13" rx="3"/>')
    b.append(t(bx + mbw + 8, ye + 39, f"{e_data['lancet_line_contains_term']} / {e_data['terms']} (100%)", "cgood num", 11.5, weight=650))
    b.append(t(xR + 16, ye + 57, "rga는 .xlsx·.hwpx 0건 · lancet은 모두 검색·열람" if ko else "rga finds 0 in .xlsx / .hwpx · lancet reads all", "mut", 10.5))

    b.append(f'<line class="div" x1="{xR + 16}" y1="{yT + 117}" x2="{xR + col_w - 16}" y2="{yT + 117}" stroke-width="1"/>')

    # G section
    yg = yT + 135
    b.append(t(xR + 16, yg, "G · 파일 20개 수정 직후 검색 (새 단어 / 지운 단어)" if ko else "G · Search right after editing 20 files (new / deleted)", "ink2", 11.5, weight=600))
    z_new = g_data["zoekt"]["finds_new_token_in"]
    z_old = g_data["zoekt"]["still_finds_removed_token_in"]
    l_new = g_data["lancet_auto"]["finds_new_token_in"]
    l_old = g_data["lancet_auto"]["still_finds_removed_token_in"]

    b.append(t(xR + 16, yg + 21, "zoekt (index)", "ink", 11.5))
    b.append(f'<rect class="badsoft" x="{bx}" y="{yg + 10}" width="{mbw:.1f}" height="13" rx="3"/>')
    b.append(t(bx + mbw + 8, yg + 21, f"{z_new}/20 새 · {z_old}/20 낡음" if ko else f"{z_new}/20 new · {z_old}/20 stale", "cbad num", 11.5, weight=650))

    b.append(t(xR + 16, yg + 42, "lancet auto", "ink", 11.5, weight=650))
    b.append(f'<rect class="good" x="{bx}" y="{yg + 31}" width="{mbw:.1f}" height="13" rx="3"/>')
    b.append(t(bx + mbw + 8, yg + 42, f"{l_new}/20 새 · {l_old}/20 낡음" if ko else f"{l_new}/20 new · {l_old}/20 stale", "cgood num", 11.5, weight=650))

    # --- Bottom-Left: Scenario H (Context cost per function)
    b.append(card(xL, yB, col_w, hB))
    b.append(t(xL + 16, yB + 26, "H · 함수 하나 읽을 때 컨텍스트 크기" if ko else "H · Context sent to LLM per function", "ink", 14, weight=700))
    b.append(t(xL + 16, yB + 44, "6개 코드베이스 각 60개 함수 중앙값 (cat vs read --symbol)" if ko else "Median bytes across 60 functions per repo (linear scale)", "ink2", 11.5))

    # Mini legend
    b.append(f'<rect class="base" x="{xL + 16}" y="{yB + 56}" width="10" height="10" rx="2"/>')
    b.append(t(xL + 30, yB + 65, "cat (파일 전체)" if ko else "cat (whole file)", "ink2", 11))
    b.append(f'<rect class="auto" x="{xL + 148}" y="{yB + 56}" width="10" height="10" rx="2"/>')
    b.append(t(xL + 162, yB + 65, "lancet read --symbol", "cblue", 11, weight=600))

    h_rows = sorted(FAILURES["H_context_cost"], key=lambda r: r["median_cat_bytes"], reverse=True)
    max_cat = max(r["median_cat_bytes"] for r in h_rows)
    h_bx = xL + 98.0
    h_bw = 175.0
    for i, r in enumerate(h_rows):
        ry = yB + 82 + i * 29
        name = "spring" if r["corpus"] == "spring-framework" else r["corpus"]
        cat_b = r["median_cat_bytes"]
        sym_b = r["median_symbol_bytes"]
        ratio = cat_b / sym_b
        w_cat = max(h_bw * cat_b / max_cat, 5)
        w_sym = max(h_bw * sym_b / max_cat, 3)

        b.append(t(xL + 16, ry + 13, name, "ink", 11.5, weight=600))
        b.append(f'<rect class="base" x="{h_bx}" y="{ry + 1}" width="{w_cat:.1f}" height="8" rx="2"/>')
        b.append(f'<rect class="auto" x="{h_bx}" y="{ry + 11}" width="{w_sym:.1f}" height="8" rx="2"/>')
        label_txt = f"{fmt_bytes(cat_b)} → {fmt_bytes(sym_b)} ({ratio:.0f}{'배' if ko else '×'})"
        b.append(t(W / 2 - 22, ry + 14, label_txt, "ink2 num", 11, anchor="end"))

    trunc_note = (
        "고정 50줄 창(sed -n)은 함수의 1.7~8.3%를 잘라 먹음"
        if ko
        else "A fixed 50-line window (sed -n) truncates 1.7–8.3% of functions"
    )
    b.append(t(xL + 16, yB + hB - 12, trunc_note, "mut", 10.5))

    # --- Bottom-Right: Scenarios I & D (File identity & sed refactor false hits)
    b.append(card(xR, yB, col_w, hB))
    b.append(t(xR + 16, yB + 26, "I · D · 파일 정체성 보존 & 리팩터링 오탐" if ko else "I · D · File identity & text-rename false hits", "ink", 14, weight=700))

    # Scenario I matrix
    yi = yB + 48
    b.append(t(xR + 16, yi, "I · 심볼릭 링크 · 하드 링크 · 0640 권한 파일 수정" if ko else "I · Editing a symlinked, hard-linked, 0640 file", "ink2", 11.5, weight=600))
    cols_i = (
        ["심볼릭 링크", "하드 링크", "0640 권한"] if ko else ["Symlink", "Hard link", "Mode 0640"]
    )
    cx_list = [xR + 195, xR + 280, xR + 365]
    for cx, cname in zip(cx_list, cols_i):
        b.append(t(cx, yi + 18, cname, "mut", 10.5, anchor="middle"))

    i_rows = [
        ("sed -i", FAILURES["I_file_identity"]["sed -i"]),
        ("perl -pi", FAILURES["I_file_identity"]["perl -pi"]),
        ("tmp+rename" if not ko else "tmp+rename (에디터)", FAILURES["I_file_identity"]["tmp + rename (editor tools)"]),
        ("lancet edit", FAILURES["I_file_identity"]["lancet edit"]),
    ]
    for idx, (rname, vals) in enumerate(i_rows):
        ry = yi + 36 + idx * 20
        is_l = rname == "lancet edit"
        b.append(t(xR + 16, ry, rname, "cblue" if is_l else "ink", 11.5, weight=700 if is_l else 500))
        flags = [vals["symlink_kept_and_target_edited"], vals["hard_link_still_shared"], vals["mode_kept"]]
        for cx, ok in zip(cx_list, flags):
            b.append(t(cx, ry, "✓" if ok else "✗", "cgood" if ok else "cbad", 12, anchor="middle", weight=700))

    b.append(f'<line class="div" x1="{xR + 16}" y1="{yB + 150}" x2="{xR + col_w - 16}" y2="{yB + 150}" stroke-width="1"/>')

    # Scenario D false hits
    yd = yB + 168
    b.append(t(xR + 16, yd, "D · sed 이름 바꾸기 시 주석·문자열 오탐 비율" if ko else "D · Text rename touching comments/strings (vs refs/ast)", "ink2", 11.5, weight=600))
    d_rows = sorted(FAILURES["D_text_refactor_false_hits"], key=lambda r: r["pct"], reverse=True)[:4]
    d_bx = xR + 162.0
    d_bw = 165.0
    for idx, r in enumerate(d_rows):
        ry = yd + 17 + idx * 19
        cname = "spring" if r["corpus"] == "spring-framework" else r["corpus"]
        w_bar = max(d_bw * r["pct"] / 14.0, 4)
        b.append(t(xR + 16, ry + 3, f"{cname} ({r['identifier']})", "ink", 10.5))
        b.append(f'<rect class="track" x="{d_bx}" y="{ry - 6}" width="{d_bw:.1f}" height="10" rx="2"/>')
        b.append(f'<rect class="warn" x="{d_bx}" y="{ry - 6}" width="{w_bar:.1f}" height="10" rx="2"/>')
        b.append(t(d_bx + d_bw + 8, ry + 3, f"{r['pct']:.1f}% ({r['in_comments_or_strings']:,})", "cwarn num", 10.5, weight=650))

    d_note = "lancet refs / ast: 0% (코드만 인식)" if ko else "lancet refs / ast: 0% (code only — comments & strings excluded)"
    b.append(t(xR + 16, yB + hB - 11, d_note, "cgood", 10.5, weight=600))

    footer = (
        "재현: python3 bench/failure_modes.py" if ko else "Reproduce all scenarios: python3 bench/failure_modes.py"
    )
    b.append(t(28, H - 14, footer, "mut", 11.5))
    return svg(W, H, lang, title, subtitle, b)


# ---------------------------------------------------------------------------
# 5. Index Footprint: Lancet vs Zoekt (bench-index-footprint / .ko)
# ---------------------------------------------------------------------------


def make_index_footprint(lang: str) -> str:
    ko = lang == "ko"
    W = 920
    top = 128
    row_h = 44
    H = top + row_h * len(CORPUS_ORDER) + 42

    title = (
        "인덱스 비용 비교 (8개 코퍼스): lancet 대 zoekt"
        if ko
        else "Index footprint across 8 codebases: lancet vs zoekt"
    )
    subtitle = (
        "모든 코퍼스에서 zoekt보다 4~25배 빠른 빌드, 9~30배 작은 디스크, 3.2~5.0배 적은 피크 메모리"
        if ko
        else "Across all 8 repositories: 4–25× faster build, 9–30× smaller on disk, 3.2–5.0× lower peak RAM"
    )
    b = [
        t(28, 36, title, "ink", 17.5, weight=700),
        t(28, 58, subtitle, "ink2", 12.5),
    ]

    # Legend
    lx = 28.0
    for cls, lab in (("base", "zoekt-index"), ("auto", "lancet index")):
        b.append(f'<rect class="{cls}" x="{lx:.1f}" y="74" width="12" height="12" rx="2.5"/>')
        b.append(t(lx + 18, 84, lab, "ink2", 12, weight=600))
        lx += 24 + est_px(lab, 12) + 18

    cols = (
        [
            ("빌드 시간 (초)", "4~25배 빠름", "build_s", lambda v: f"{v:.2f}s" if v < 10 else f"{v:.1f}s"),
            ("디스크 크기", "9~30배 작음", "size_mib", fmt_mib),
            ("피크 메모리 (RAM)", "3.2~5.0배 적음", "peak_rss_mib", lambda v: f"{v:.0f} MB"),
        ]
        if ko
        else [
            ("Build time", "4–25× faster", "build_s", lambda v: f"{v:.2f} s" if v < 10 else f"{v:.1f} s"),
            ("Disk size", "9–30× smaller", "size_mib", fmt_mib),
            ("Peak RAM", "3.2–5.0× less", "peak_rss_mib", lambda v: f"{v:.0f} MB"),
        ]
    )

    x_label = 28.0
    col_starts = [194.0, 434.0, 674.0]
    col_bar_w = 142.0

    for cx, (chead, csub, _, _) in zip(col_starts, cols):
        b.append(t(cx, 106, chead, "ink", 12.5, weight=700))
        b.append(t(cx, 121, csub, "cblue", 11, weight=600))

    max_vals = {
        k: max(
            max(SUITE["corpora"][c]["lancet_index"][k], SUITE["corpora"][c]["zoekt_index"][k])
            for c in CORPUS_ORDER
        )
        for k in ("build_s", "size_mib", "peak_rss_mib")
    }

    for i, key in enumerate(CORPUS_ORDER):
        c = SUITE["corpora"][key]
        cname, clang = CORPUS_LABELS[key]
        y = top + i * row_h + 6
        if i > 0:
            b.append(f'<line class="grid" x1="28" y1="{y - 5}" x2="{W - 28}" y2="{y - 5}" stroke-width="0.7"/>')
        b.append(t(x_label, y + 14, f"{cname} ({clang})", "ink", 12.5, weight=650))
        b.append(t(x_label, y + 30, f"{c['files']:,} {'파일' if ko else 'files'} · {c['mib']:,.0f} MB", "mut", 10.5))

        for cx, (_, _, metric, fmt_fn) in zip(col_starts, cols):
            zv = c["zoekt_index"][metric]
            lv = c["lancet_index"][metric]
            mv = max_vals[metric]
            zw = max(col_bar_w * zv / mv, 3)
            lw = max(col_bar_w * lv / mv, 3)

            b.append(f'<rect class="base" x="{cx}" y="{y + 2}" width="{zw:.1f}" height="11" rx="2.5"/>')
            b.append(t(cx + zw + 5, y + 11.5, fmt_fn(zv), "ink2 num", 10.5))

            b.append(f'<rect class="auto" x="{cx}" y="{y + 16}" width="{lw:.1f}" height="11" rx="2.5"/>')
            b.append(t(cx + lw + 5, y + 25.5, fmt_fn(lv), "cblue num", 10.5, weight=650))

    footer = (
        "비교 기준: lancet LANCET04 인덱스 (문서 단위) vs zoekt-index (위치 인덱스 · 2 MB 파일 제한) · 데이터: suite-final.json"
        if ko
        else "Comparison: lancet LANCET04 document-level trigram index vs zoekt-index (positional · 2 MB file cap) · Data: suite-final.json"
    )
    b.append(t(28, H - 14, footer, "mut", 11))
    return svg(W, H, lang, title, subtitle, b)


# ---------------------------------------------------------------------------
# 6. Full Workload Profile: Text (LLVM) + AST across 8 corpora
# ---------------------------------------------------------------------------


def make_workload_profile(lang: str) -> str:
    ko = lang == "ko"
    W, H = 920, 742
    title = (
        "전체 워크로드 프로필: 이기는 구간과 한계 구간"
        if ko
        else "Full workload profile: where the index wins and where it ties"
    )
    subtitle = (
        "리터럴이 있으면 수십~수백 배 빠르고, 리터럴 없는 정규식·AST는 풀스캔/전체 파싱이 지배합니다."
        if ko
        else "Literal anchors deliver 2–786× speedups; literal-free regex & AST fall back to full scan / full parse."
    )
    b = [
        t(28, 36, title, "ink", 17.5, weight=700),
        t(28, 58, subtitle, "ink2", 12.5),
    ]

    # --- Top Card: LLVM 19.1 all 8 text queries vs ripgrep (log speedup around 1x)
    c_top_y, c_top_h = 76.0, 320.0
    b.append(card(28, c_top_y, W - 56, c_top_h))
    t_head = (
        "LLVM 19.1 (파일 146,319개) · 8개 텍스트 쿼리 유형별 ripgrep 대비 속도 배수"
        if ko
        else "LLVM 19.1 (146,319 files) · Speedup vs ripgrep across all 8 text query types"
    )
    b.append(t(44, c_top_y + 25, t_head, "ink", 13.5, weight=700))

    # Legend in top card
    lx = 44.0
    t_leg = (
        [("auto", "lancet rg (auto · 항상 최신)"), ("trust", "lancet rg --trust-index"), ("warn", "리터럴 없음 (인덱스 미사용)")]
        if ko
        else [("auto", "lancet rg (auto · always fresh)"), ("trust", "lancet rg --trust-index"), ("warn", "literal-free (index cannot narrow)")]
    )
    for cls, lab in t_leg:
        b.append(f'<rect class="{cls}" x="{lx:.1f}" y="{c_top_y + 36}" width="11" height="11" rx="2.5"/>')
        b.append(t(lx + 16, c_top_y + 45.5, lab, "ink2", 11.5))
        lx += 22 + est_px(lab, 11.5) + 14

    llvm_q = {q["query"]: q for q in SUITE["corpora"]["llvm"]["text"]}
    q_order = [
        ("no hit", "없는 단어 (no hit)" if ko else "No hit (zero matches)"),
        ("rare literal", "희귀 리터럴" if ko else "Rare literal"),
        ("regex with literal", "리터럴 포함 정규식" if ko else "Regex with literal"),
        ("ignore case", "대소문자 무시 (-i)" if ko else "Ignore case (-i)"),
        ("word", "단어 경계 (-w)" if ko else "Word boundary (-w)"),
        ("count -c", "매치 수 집계 (-c)" if ko else "Line count (-c)"),
        ("common identifier", "흔한 식별자" if ko else "Common identifier"),
        ("regex, no literal", "리터럴 없는 정규식" if ko else "Regex, no literal"),
    ]
    t_left, t_right = 216.0, 212.0
    t_pw = W - t_left - t_right
    t_lo, t_hi = 0.5, 1000.0

    def tx(sp: float) -> float:
        cl = max(t_lo, min(t_hi, sp))
        return t_left + t_pw * (math.log10(cl) - math.log10(t_lo)) / (math.log10(t_hi) - math.log10(t_lo))

    t_y0 = c_top_y + 66
    t_rh = 28.0
    t_bot = t_y0 + len(q_order) * t_rh

    for mult, lab in ((0.5, "0.5×"), (1, "1× (rg)"), (10, "10×"), (100, "100×"), (1000, "1000×")):
        gx = tx(mult)
        if mult == 1:
            b.append(f'<line class="axis" x1="{gx:.1f}" y1="{t_y0 - 4}" x2="{gx:.1f}" y2="{t_bot}" stroke-width="1.2" stroke-dasharray="4 3"/>')
        else:
            b.append(f'<line class="grid" x1="{gx:.1f}" y1="{t_y0 - 4}" x2="{gx:.1f}" y2="{t_bot}" stroke-width="1"/>')
        b.append(t(gx, t_bot + 14, lab, "ink2 num" if mult == 1 else "mut num", 10.5, anchor="middle", weight=650 if mult == 1 else None))

    x1 = tx(1.0)
    for idx, (qkey, qlabel) in enumerate(q_order):
        q = llvm_q[qkey]
        rg_ms = q["ms"]["rg"]["median"]
        auto_ms = q["ms"]["lancet rg"]["median"]
        trust_ms = q["ms"]["lancet rg --trust-index"]["median"]
        sp_a = rg_ms / auto_ms
        sp_t = rg_ms / trust_ms
        ry = t_y0 + idx * t_rh
        is_nolit = qkey == "regex, no literal"

        if is_nolit:
            b.append(f'<rect class="hlwarn" x="38" y="{ry}" width="{W - 76}" height="{t_rh - 2}" rx="5"/>')

        b.append(t(44, ry + 17, qlabel, "cwarn" if is_nolit else "ink", 12, weight=700 if is_nolit else 550))

        # Auto bar (from 1x)
        xa = tx(sp_a)
        ba_x, ba_w = min(x1, xa), max(abs(xa - x1), 3)
        b.append(f'<rect class="{"warn" if is_nolit else "auto"}" x="{ba_x:.1f}" y="{ry + 4}" width="{ba_w:.1f}" height="8.5" rx="2"/>')

        # Trust bar (from 1x)
        xt = tx(sp_t)
        bt_x, bt_w = min(x1, xt), max(abs(xt - x1), 3)
        b.append(f'<rect class="{"warn" if is_nolit else "trust"}" x="{bt_x:.1f}" y="{ry + 14.5}" width="{bt_w:.1f}" height="8.5" rx="2" opacity="{"0.65" if is_nolit else "1"}"/>')

        note = (
            f"auto {sp_a:.2f}{'배' if ko else '×'} ({fmt_ms(auto_ms)}) · trust {sp_t:.2f}{'배' if ko else '×'}"
            if is_nolit
            else f"auto {fmt_speedup(sp_a, lang)} · trust {fmt_speedup(sp_t, lang)}"
        )
        b.append(t(W - 44, ry + 17, note, "cwarn num" if is_nolit else "ink2 num", 11.5, anchor="end", weight=650 if is_nolit else 550))

    # --- Bottom Card: Structural AST search across all 8 corpora vs ast-grep
    c_bot_y, c_bot_h = 412.0, 294.0
    b.append(card(28, c_bot_y, W - 56, c_bot_h))
    b_head = (
        "8개 코드베이스 · 구조적 AST 검색 속도 (ast-grep 대비, 로그 스케일)"
        if ko
        else "All 8 codebases · Structural AST search speedup vs ast-grep (log scale)"
    )
    b.append(t(44, c_bot_y + 25, b_head, "ink", 13.5, weight=700))

    lx = 44.0
    b_leg = (
        [("auto", "리터럴 포함 AST (인덱스+memmem 사전 필터)"), ("warn", "리터럴 없는 AST (if ($C) { $$$B } — 전체 파싱)")]
        if ko
        else [("auto", "AST with literal (index + memmem prefilter)"), ("warn", "Literal-free AST (if ($C) { $$$B } — full parse)")]
    )
    for cls, lab in b_leg:
        b.append(f'<rect class="{cls}" x="{lx:.1f}" y="{c_bot_y + 36}" width="11" height="11" rx="2.5"/>')
        b.append(t(lx + 16, c_bot_y + 45.5, lab, "ink2", 11.5))
        lx += 22 + est_px(lab, 11.5) + 16

    ast_rows = sorted(
        CORPUS_ORDER,
        key=lambda k: SUITE["corpora"][k]["ast"][0]["ms"]["ast-grep"]["median"]
        / SUITE["corpora"][k]["ast"][0]["ms"]["lancet ast"]["median"],
        reverse=True,
    )
    a_left, a_right = 276.0, 198.0
    a_pw = W - a_left - a_right
    a_lo, a_hi = 0.4, 5.0

    def ax(sp: float) -> float:
        cl = max(a_lo, min(a_hi, sp))
        return a_left + a_pw * (math.log10(cl) - math.log10(a_lo)) / (math.log10(a_hi) - math.log10(a_lo))

    a_y0 = c_bot_y + 66
    a_rh = 25.0
    a_bot = a_y0 + len(ast_rows) * a_rh

    for mult, lab in ((0.5, "0.5×"), (1, "1× (ast-grep)"), (2, "2×"), (3, "3×"), (4, "4×"), (5, "5×")):
        gx = ax(mult)
        if mult == 1:
            b.append(f'<line class="axis" x1="{gx:.1f}" y1="{a_y0 - 4}" x2="{gx:.1f}" y2="{a_bot}" stroke-width="1.2" stroke-dasharray="4 3"/>')
        else:
            b.append(f'<line class="grid" x1="{gx:.1f}" y1="{a_y0 - 4}" x2="{gx:.1f}" y2="{a_bot}" stroke-width="1"/>')
        b.append(t(gx, a_bot + 14, lab, "ink2 num" if mult == 1 else "mut num", 10.5, anchor="middle", weight=650 if mult == 1 else None))

    ax1 = ax(1.0)
    for idx, key in enumerate(ast_rows):
        c = SUITE["corpora"][key]
        cname, clang = CORPUS_LABELS[key]
        a_lit = c["ast"][0]
        a_nolit = c["ast"][1]
        sp_lit = a_lit["ms"]["ast-grep"]["median"] / a_lit["ms"]["lancet ast"]["median"]
        sp_nolit = a_nolit["ms"]["ast-grep"]["median"] / a_nolit["ms"]["lancet ast"]["median"]
        ry = a_y0 + idx * a_rh

        b.append(t(44, ry + 14, f"{cname}", "ink", 11.5, weight=650))
        b.append(t(140, ry + 14, a_lit["pattern"], "mut mono", 10))

        # Literal AST bar (right from 1x)
        xl = ax(sp_lit)
        b.append(f'<rect class="auto" x="{ax1:.1f}" y="{ry + 3}" width="{max(xl - ax1, 3):.1f}" height="8.5" rx="2"/>')

        # Literal-free AST bar (around 1x)
        xn = ax(sp_nolit)
        nx, nw = min(ax1, xn), max(abs(xn - ax1), 3)
        b.append(f'<rect class="warn" x="{nx:.1f}" y="{ry + 13}" width="{nw:.1f}" height="8.5" rx="2"/>')

        u = "배" if ko else "×"
        b.append(
            t(
                W - 44,
                ry + 15,
                f"{sp_lit:.2f}{u} {'리터럴' if ko else 'lit'} · {sp_nolit:.2f}{u} {'없음' if ko else 'no-lit'}",
                "ink2 num",
                11,
                anchor="end",
            )
        )

    footer = (
        "1× 점선 오른쪽은 lancet이 더 빠름, 왼쪽은 기준 도구가 더 빠름 · 데이터: bench/results/suite-final.json"
        if ko
        else "Right of the 1× dashed line = lancet is faster; left = baseline is faster · Data: bench/results/suite-final.json"
    )
    b.append(t(28, H - 14, footer, "mut", 11.5))
    return svg(W, H, lang, title, subtitle, b)


# ---------------------------------------------------------------------------
# 7. Index lifecycle (index-lifecycle / index-lifecycle.ko)
# ---------------------------------------------------------------------------


def make_lifecycle(lang: str) -> str:
    ko = lang == "ko"
    W, H = 920, 310
    c = SUITE["corpora"]["linux"]
    li = c["lancet_index"]
    pct = 100.0 * li["size_mib"] / c["mib"]

    title = (
        "인덱스 구조: 불변 세그먼트와 저렴한 최신 상태 유지"
        if ko
        else "The index: immutable segments, cheap to keep fresh"
    )
    subtitle = (
        "LSM 방식: 새 파일·바뀐 파일은 새 세그먼트에 쓰고, 이전 버전은 툼스톤 표시, 작은 세그먼트는 계층형 컴팩션으로 병합"
        if ko
        else "LSM-style: new and changed files go to new segments, old versions are tombstoned, small segments are merged."
    )
    b = [
        t(28, 36, title, "ink", 17.5, weight=700),
        t(28, 58, subtitle, "ink2", 12.5),
    ]

    y0 = 86
    # Working tree card
    b.append(card(28, y0, 178, 124))
    b.append(t(42, y0 + 26, "작업 트리" if ko else "Working tree", "ink", 14, weight=700))
    wt_lines = (
        [".gitignore 인식 탐색", "mtime·크기·ctime·inode", "변경 없으면 건너뜀"]
        if ko
        else ["walk (.gitignore-aware)", "mtime·size·ctime·inode", "unchanged → skip"]
    )
    for j, ln in enumerate(wt_lines):
        b.append(t(42, y0 + 52 + j * 19, ln, "ink2", 12))

    b.append(arrow(210, y0 + 62, 248, y0 + 62))

    # Segments card
    sx = 252
    b.append(card(sx, y0, 390, 124))
    b.append(t(sx + 14, y0 + 26, ".lancet/  (mmap 제로카피 LANCET04)" if ko else ".lancet/  (mmap'd, zero-copy LANCET04)", "ink", 14, weight=700))
    for k in range(6):
        cls = "f1" if k < 4 else "f0"
        b.append(f'<rect class="{cls}" x="{sx + 14 + k * 60}" y="{y0 + 42}" width="52" height="34" rx="4"/>')
        b.append(t(sx + 40 + k * 60, y0 + 63, f"seg {k + 1}", "ink", 11, anchor="middle"))
    seg_notes = (
        ["세그먼트당 입력 ≤96 MiB / 문서 ≤65,536개", "교체·삭제된 파일은 툼스톤으로 마킹"]
        if ko
        else ["≤96 MiB input / ≤65,536 docs per segment", "tombstones mark replaced or deleted files"]
    )
    b.append(t(sx + 14, y0 + 96, seg_notes[0], "ink2", 11.5))
    b.append(t(sx + 14, y0 + 113, seg_notes[1], "ink2", 11.5))

    b.append(arrow(sx + 394, y0 + 62, sx + 432, y0 + 62))

    # Compaction card
    cx = sx + 436
    b.append(card(cx, y0, W - 28 - cx, 124, "hl"))
    b.append(t(cx + 14, y0 + 26, "계층형 컴팩션" if ko else "Tiered compaction", "cblue", 14, weight=700))
    cmp_lines = (
        ["세그먼트 >6 또는 25% 삭제", "→ 작은 세그먼트 k-way 병합", "큰 세그먼트는 그대로 유지"]
        if ko
        else [">6 segments or ≥25% dead", "→ k-way merge small ones", "big segments left alone"]
    )
    for j, ln in enumerate(cmp_lines):
        b.append(t(cx + 14, y0 + 52 + j * 19, ln, "ink2", 12))

    stats = (
        [
            (f"{li['build_s']:.2f} s", "콜드 빌드"),
            (f"{li['peak_rss_mib']} MB", "피크 메모리 (버퍼 재사용)"),
            (f"{li['size_mib']:.0f} MB ({pct:.1f}%)", "디스크 인덱스 크기"),
            (f"{li['noop_sync_ms']:.0f} ms", "변경 없는 동기화"),
            (f"{li['segments']}개", f"세그먼트 ({c['files']:,}파일)"),
        ]
        if ko
        else [
            (f"{li['build_s']:.2f} s", "cold build"),
            (f"{li['peak_rss_mib']} MB", "peak RAM (reused buffers)"),
            (f"{li['size_mib']:.0f} MB ({pct:.1f}%)", "index on disk"),
            (f"{li['noop_sync_ms']:.0f} ms", "no-op sync"),
            (f"{li['segments']} segs", f"Linux {c['tag']} ({c['files']:,} files)"),
        ]
    )
    sw = (W - 56) / len(stats)
    for i, (v, lab) in enumerate(stats):
        x0 = 28 + i * sw
        b.append(t(x0 + 4, 256, v, "ink num", 18.5, weight=700))
        b.append(t(x0 + 4, 276, lab, "ink2", 11.5))

    b.append(t(W - 28, H - 14, f"Linux {c['tag']} · {c['files']:,} {'파일' if ko else 'files'} · suite-final.json", "mut", 11, anchor="end"))
    return svg(W, H, lang, title, subtitle, b)


# ---------------------------------------------------------------------------
# 8. Concurrent edit & patch safety (concurrent-edit / concurrent-edit.ko)
# ---------------------------------------------------------------------------


def make_concurrent_edit(lang: str) -> str:
    ko = lang == "ko"
    W, H = 920, 384
    bd = FAILURES["B_lost_updates"]
    ch_tot, ch_lost = bd["chain"]["updates"], bd["chain"]["lost"]
    la_tot, la_lost, la_retried = bd["lancet"]["updates"], bd["lancet"]["lost"], bd["lancet"]["conflicts_retried"]

    title = (
        "동시 수정 및 패치 안전성: rev CAS + 배타적 파일 잠금"
        if ko
        else "Concurrent edits & patch safety: rev CAS + exclusive file lock"
    )
    subtitle = (
        "두 에이전트가 같은 rev를 읽고 동시에 쓰더라도 하나만 커밋되고, 진 쪽은 0바이트 쓰기 후 conflict를 받아 재시도합니다."
        if ko
        else "Two agents read the same rev — only one commits; the loser writes 0 bytes, receives conflict, and retries cleanly."
    )
    b = [
        t(28, 36, title, "ink", 17.5, weight=700),
        t(28, 58, subtitle, "ink2", 12),
    ]

    # Left: two concurrent agents
    lx, lw, lh = 28.0, 224.0, 88.0
    y_top, y_bot = 82.0, 192.0
    left_cards = (
        [
            ("에이전트 A · read", "rev: 9f3a…", "edit --lines 42 --rev 9f3a…", y_top),
            ("에이전트 B · read", "rev: 9f3a…", "edit --lines 88 --rev 9f3a…", y_bot),
        ]
        if ko
        else [
            ("Agent A · read", "rev: 9f3a…", "edit --lines 42 --rev 9f3a…", y_top),
            ("Agent B · read", "rev: 9f3a…", "edit --lines 88 --rev 9f3a…", y_bot),
        ]
    )
    for head, rev_s, cmd_s, cy in left_cards:
        b.append(card(lx, cy, lw, lh))
        b.append(t(lx + 14, cy + 25, head, "ink", 13.5, weight=700))
        b.append(t(lx + 14, cy + 46, rev_s, "cblue mono", 11.5, weight=650))
        b.append(t(lx + 14, cy + 68, cmd_s, "ink2 mono", 11))

    # Arrows left -> center lock
    cx, cw, ch = 290.0, 316.0, 198.0
    b.append(arrow(lx + lw + 3, y_top + lh / 2, cx - 4, y_top + lh / 2, acc=True))
    b.append(arrow(lx + lw + 3, y_bot + lh / 2, cx - 4, y_bot + lh / 2, acc=False))

    # Center: Exclusive OS file lock card
    b.append(card(cx, y_top, cw, ch, "hl"))
    b.append(t(cx + 16, y_top + 26, "배타적 OS 파일 잠금 (직렬화)" if ko else "Exclusive OS file lock (serialized)", "cblue", 13.5, weight=700))

    inner = (
        [
            ("1 · inode + rev CAS 검증", "rename 교체·오래된 rev 즉시 거부"),
            ("2 · Tree-sitter 구문 게이트", "파싱 오류가 순증가하면 쓰기 거부"),
            ("3 · 정체성 보존 원자적 쓰기", "심볼릭·하드 링크·권한·소유자 유지"),
        ]
        if ko
        else [
            ("1 · inode + rev CAS check", "rejects rename-over & stale rev"),
            ("2 · Tree-sitter syntax gate", "refuses net increase in parse errors"),
            ("3 · Identity-safe atomic write", "keeps symlink, hard links, mode, owner"),
        ]
    )
    for i, (ih, isub) in enumerate(inner):
        iy = y_top + 40 + i * 50
        b.append(card(cx + 12, iy, cw - 24, 42, "card", rx=6))
        b.append(t(cx + 22, iy + 18, ih, "ink", 12, weight=650))
        b.append(t(cx + 22, iy + 34, isub, "ink2", 11))

    # Right: winner & loser
    rx, rw = 644.0, 248.0
    b.append(arrow(cx + cw + 3, y_top + lh / 2, rx - 4, y_top + lh / 2, acc=True))
    b.append(arrow(cx + cw + 3, y_bot + lh / 2, rx - 4, y_bot + lh / 2, acc=False))

    # Winner card
    b.append(card(rx, y_top, rw, lh, "goodsoft"))
    b.append(t(rx + 14, y_top + 25, "에이전트 A · 성공 (exit 0)" if ko else "Agent A · Winner (exit 0)", "cgood", 13.5, weight=700))
    b.append(t(rx + 14, y_top + 47, "rev 일치 → 원자적 쓰기 완료" if ko else "rev matched → written atomically", "ink", 11.5))
    b.append(t(rx + 14, y_top + 68, "rev_after: 4c1b… 반환" if ko else "returns rev_after: 4c1b…", "cgood mono", 11.5, weight=650))

    # Loser card
    b.append(card(rx, y_bot, rw, lh, "hlwarn"))
    b.append(t(rx + 14, y_bot + 25, "에이전트 B · 충돌 (exit 2)" if ko else "Agent B · Conflict (exit 2)", "cwarn", 13.5, weight=700))
    b.append(t(rx + 14, y_bot + 47, "9f3a ≠ 4c1b → 0바이트 쓰기" if ko else "9f3a ≠ 4c1b → 0 bytes written", "ink", 11.5))
    b.append(t(rx + 14, y_bot + 68, "다시 읽기(4c1b…) → 재적용·재시도" if ko else "re-read (4c1b…) → re-apply & retry", "ink2", 11.5))

    # Dashed loop-back arrow from Loser bottom back to Agent B bottom
    x_from = rx + rw / 2
    x_to = lx + lw / 2
    y_edge = y_bot + lh
    b.append(
        f'<path class="flow" stroke-width="1.4" stroke-dasharray="4 4" fill="none" '
        f'd="M{x_from:.1f},{y_edge + 3} C{x_from:.1f},{y_edge + 32} {x_to:.1f},{y_edge + 32} {x_to:.1f},{y_edge + 6}" marker-end="url(#ah)"/>'
    )
    patch_note = (
        "patch --apply도 쓰기 전 전체 파일을 검증하며, 도중 경합이 발생하면 이미 쓴 파일까지 롤백합니다"
        if ko
        else "patch --apply verifies every file first and rolls back written files if a concurrent write wins mid-apply"
    )
    b.append(t(W / 2, y_edge + 42, patch_note, "mut", 11.5, anchor="middle"))

    # Evidence strip at bottom
    ev_y = 338.0
    b.append(card(28, ev_y, W - 56, 30, "pill", rx=7))
    ev_txt = (
        f"실측 (에이전트 {bd['agents']}개 × {bd['chain']['trials']}회 = {ch_tot}건): "
        f"기존 읽기-수정-쓰기 {ch_lost}/{ch_tot}건 유실 ({100*ch_lost//ch_tot}%)  ·  "
        f"lancet {la_lost}/{la_tot}건 유실 (충돌 {la_retried:,}회 감지·재시도)"
        if ko
        else f"Measured ({bd['agents']} agents × {bd['chain']['trials']} trials = {ch_tot} updates): "
        f"naive read-modify-write lost {ch_lost}/{ch_tot} ({100*ch_lost//ch_tot}%)  ·  "
        f"lancet lost {la_lost}/{la_tot} ({la_retried:,} conflicts retried)"
    )
    b.append(t(W / 2, ev_y + 19.5, ev_txt, "ink num", 11.5, anchor="middle", weight=600))
    return svg(W, H, lang, title, subtitle, b)


# ---------------------------------------------------------------------------
# Entrypoint
# ---------------------------------------------------------------------------


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    builders = {
        "bench-suite": make_bench_suite,
        "pipeline": make_pipeline,
        "coordinates": make_coordinates,
        "failure-modes": make_failure_modes,
        "bench-index-footprint": make_index_footprint,
        "bench-workload-profile": make_workload_profile,
        "index-lifecycle": make_lifecycle,
        "concurrent-edit": make_concurrent_edit,
    }
    generated: list[Path] = []
    for name, fn in builders.items():
        for lang in ("en", "ko"):
            suffix = ".ko.svg" if lang == "ko" else ".svg"
            path = OUT / f"{name}{suffix}"
            content = fn(lang)
            ET.fromstring(content)
            path.write_text(content, encoding="utf-8")
            generated.append(path)

    compat_path = OUT / "bench-linux.svg"
    compat_content = make_bench_linux_compat()
    ET.fromstring(compat_content)
    compat_path.write_text(compat_content, encoding="utf-8")
    generated.append(compat_path)

    for p in generated:
        print(f"wrote {p.relative_to(ROOT)} ({p.stat().st_size:,} bytes)")


if __name__ == "__main__":
    main()
