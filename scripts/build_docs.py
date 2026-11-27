#!/usr/bin/env python3
"""Build Lancet's bilingual (EN + KO) official documentation site into site/."""

from __future__ import annotations

import json
import os
import posixpath
import re
import shutil
import sys
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SITE_DIR = ROOT / "site"
ASSETS_DIR = SITE_DIR / "assets"
DIAGRAMS_DST = ASSETS_DIR / "diagrams"
DIAGRAMS_SRC = ROOT / "docs" / "assets"

BASE_URL = "https://cree1116.github.io/lancet"

DIAGRAM_FILES = [
    "pipeline.svg",
    "pipeline.ko.svg",
    "coordinates.svg",
    "coordinates.ko.svg",
    "concurrent-edit.svg",
    "concurrent-edit.ko.svg",
    "index-lifecycle.svg",
    "index-lifecycle.ko.svg",
    "failure-modes.svg",
    "failure-modes.ko.svg",
    "bench-index-footprint.svg",
    "bench-index-footprint.ko.svg",
    "bench-workload-profile.svg",
    "bench-workload-profile.ko.svg",
]

GROUPS = {
    "en": [
        ("start", "Getting Started"),
        ("search", "Search & Discovery"),
        ("edit", "Editing & Agents"),
        ("internals", "Architecture & Internals"),
        ("reference", "Reference"),
    ],
    "ko": [
        ("start", "시작하기"),
        ("search", "탐색과 검색"),
        ("edit", "편집과 에이전트"),
        ("internals", "아키텍처와 내부 구조"),
        ("reference", "레퍼런스"),
    ],
}

NAV_ORDER = [
    ("start", "index.html"),
    ("start", "migration.html"),
    ("start", "filtering.html"),
    ("search", "guides/explore.html"),
    ("search", "guides/search.html"),
    ("search", "guides/ast.html"),
    ("search", "guides/read-and-documents.html"),
    ("edit", "guides/edit.html"),
    ("edit", "guides/patch.html"),
    ("edit", "guides/rename.html"),
    ("edit", "guides/agents-mcp.html"),
    ("internals", "concepts/index-and-freshness.html"),
    ("internals", "concepts/write-safety.html"),
    ("internals", "concepts/benchmarks.html"),
    ("reference", "reference/cli.html"),
    ("reference", "reference/rg-parity.html"),
    ("reference", "reference/lql-and-ast.html"),
    ("reference", "reference/json-and-mcp.html"),
    ("reference", "reference/troubleshooting.html"),
]


class ArticleExtractor(HTMLParser):
    """Extract headings and plain searchable text from rendered article HTML."""

    def __init__(self) -> None:
        super().__init__()
        self.headings: list[dict[str, str]] = []
        self.text_parts: list[str] = []
        self._cur_h: tuple[str, str] | None = None
        self._cur_h_text: list[str] = []
        self._skip_depth = 0
        self.ids: set[str] = set()

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        amap = dict(attrs)
        if "id" in amap and amap["id"]:
            self.ids.add(amap["id"])
        if tag == "a" and amap.get("class") == "heading-anchor":
            self._skip_depth += 1
            return
        if self._skip_depth > 0:
            self._skip_depth += 1
            return
        if tag in ("h2", "h3") and amap.get("id"):
            self._cur_h = (tag, amap["id"])
            self._cur_h_text = []

    def handle_endtag(self, tag: str) -> None:
        if self._skip_depth > 0:
            self._skip_depth -= 1
            return
        if self._cur_h and tag == self._cur_h[0]:
            text = re.sub(r"\s+", " ", "".join(self._cur_h_text)).strip()
            if text:
                self.headings.append({"id": self._cur_h[1], "text": text})
            self._cur_h = None
            self._cur_h_text = []

    def handle_data(self, data: str) -> None:
        if self._skip_depth > 0:
            return
        if self._cur_h is not None:
            self._cur_h_text.append(data)
        self.text_parts.append(data)


class LinkValidator(HTMLParser):
    """Collect all id attributes and local href/src references in a page."""

    def __init__(self) -> None:
        super().__init__()
        self.ids: set[str] = set()
        self.refs: list[tuple[str, str]] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        amap = dict(attrs)
        if amap.get("id"):
            self.ids.add(amap["id"])
        for attr in ("href", "src"):
            val = amap.get(attr)
            if val:
                self.refs.append((attr, val))


def copy_diagrams() -> None:
    DIAGRAMS_DST.mkdir(parents=True, exist_ok=True)
    for name in DIAGRAM_FILES:
        src = DIAGRAMS_SRC / name
        if not src.exists():
            raise FileNotFoundError(f"Missing source diagram: {src}")
        shutil.copy2(src, DIAGRAMS_DST / name)


def rel_url(from_file: Path, to_file: Path) -> str:
    rel = os.path.relpath(to_file, from_file.parent)
    return rel.replace(os.sep, "/")


def render_page(
    lang: str,
    page: dict,
    all_pages: dict[str, dict],
) -> tuple[Path, str, dict]:
    rel_path = page["path"]
    docs_base = SITE_DIR / ("docs" if lang == "en" else "ko/docs")
    out_file = docs_base / rel_path
    out_file.parent.mkdir(parents=True, exist_ok=True)

    other_lang = "ko" if lang == "en" else "en"
    other_base = SITE_DIR / ("ko/docs" if lang == "en" else "docs")
    other_file = other_base / rel_path

    home_file = SITE_DIR / ("index.html" if lang == "en" else "ko/index.html")
    css_file = ASSETS_DIR / "docs.css"
    js_file = ASSETS_DIR / "docs.js"
    search_file = ASSETS_DIR / "docs-search.json"
    search_js_file = ASSETS_DIR / "docs-search.js"

    rel_home = rel_url(out_file, home_file)
    rel_docs_root = rel_url(out_file, docs_base) + "/"
    if rel_docs_root == "./":
        rel_docs_root = ""
    rel_css = rel_url(out_file, css_file)
    rel_js = rel_url(out_file, js_file)
    rel_search = rel_url(out_file, search_file)
    rel_search_js = rel_url(out_file, search_js_file)
    rel_switch = rel_url(out_file, other_file)
    rel_diag = rel_url(out_file, DIAGRAMS_DST)

    en_url = f"{BASE_URL}/docs/{'' if rel_path == 'index.html' else rel_path}"
    ko_url = f"{BASE_URL}/ko/docs/{'' if rel_path == 'index.html' else rel_path}"
    canon_url = en_url if lang == "en" else ko_url
    og_img = f"{BASE_URL}/assets/{'og-image.png' if lang == 'en' else 'og-image-ko.png'}"

    body_html = page["render"](rel_diag)
    ext = ArticleExtractor()
    ext.feed(body_html)
    plain_text = re.sub(r"\s+", " ", " ".join(ext.text_parts)).strip()

    # Sidebar HTML
    group_map = dict(GROUPS[lang])
    sidebar_parts: list[str] = []
    for g_key, g_label in GROUPS[lang]:
        items = [p for gk, p in NAV_ORDER if gk == g_key]
        lis: list[str] = []
        for p_path in items:
            target_file = docs_base / p_path
            href = rel_url(out_file, target_file)
            cls = ' class="active"' if p_path == rel_path else ""
            short_title = all_pages[p_path]["nav_title"]
            lis.append(f'<li><a href="{href}"{cls}>{short_title}</a></li>')
        sidebar_parts.append(
            f'<div class="nav-group"><div class="nav-group-title">{g_label}</div>'
            f'<ul class="nav-list">{"".join(lis)}</ul></div>'
        )
    sidebar_html = "".join(sidebar_parts)

    # Prev / Next pager
    order_paths = [p for _, p in NAV_ORDER]
    idx = order_paths.index(rel_path)
    prev_html = "<div></div>"
    next_html = "<div></div>"
    if idx > 0:
        pp = order_paths[idx - 1]
        href = rel_url(out_file, docs_base / pp)
        lbl = "이전" if lang == "ko" else "Previous"
        prev_html = (
            f'<a class="pager-card prev" href="{href}">'
            f"<small>← {lbl}</small><b>{all_pages[pp]['nav_title']}</b></a>"
        )
    if idx + 1 < len(order_paths):
        np = order_paths[idx + 1]
        href = rel_url(out_file, docs_base / np)
        lbl = "다음" if lang == "ko" else "Next"
        next_html = (
            f'<a class="pager-card next" href="{href}">'
            f"<small>{lbl} →</small><b>{all_pages[np]['nav_title']}</b></a>"
        )

    switch_label = "한국어" if lang == "en" else "English"
    home_label = "홈" if lang == "ko" else "Home"
    docs_label = "문서" if lang == "ko" else "Docs"
    search_ph = "문서 검색…" if lang == "ko" else "Search docs…"
    toc_label = "이 페이지 목차" if lang == "ko" else "On this page"
    menu_aria = "문서 메뉴 열기" if lang == "ko" else "Open documentation menu"
    source_note = (
        "MIT OR Apache-2.0 · Rust 단일 바이너리"
        if lang == "ko"
        else "MIT OR Apache-2.0 · Single Rust binary"
    )
    edit_note = (
        "GitHub에서 소스 보기" if lang == "ko" else "View source on GitHub"
    )
    gh_source = page.get("source", "https://github.com/CREE1116/lancet/tree/main/docs")

    html_doc = f"""<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{page['title']} — lancet {docs_label}</title>
<meta name="description" content="{page['description']}">
<meta name="robots" content="index, follow">
<meta name="theme-color" content="#0b0d10" media="(prefers-color-scheme: dark)">
<meta name="theme-color" content="#fbfbfa" media="(prefers-color-scheme: light)">
<link rel="canonical" href="{canon_url}">
<link rel="alternate" hreflang="en" href="{en_url}">
<link rel="alternate" hreflang="ko" href="{ko_url}">
<link rel="alternate" hreflang="x-default" href="{en_url}">
<meta property="og:type" content="article">
<meta property="og:site_name" content="lancet">
<meta property="og:locale" content="{'ko_KR' if lang == 'ko' else 'en_US'}">
<meta property="og:url" content="{canon_url}">
<meta property="og:title" content="{page['title']} — lancet {docs_label}">
<meta property="og:description" content="{page['description']}">
<meta property="og:image" content="{og_img}">
<meta name="twitter:card" content="summary_large_image">
<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Cpath d='M5 27 L24 6 l3 3 L8 28 Z' fill='%2336c2a0'/%3E%3C/svg%3E">
<link rel="stylesheet" href="{rel_css}?v=1">
</head>
<body data-docs-root="{rel_docs_root}" data-search-url="{rel_search}">

<header class="doc-header">
  <div class="doc-header-in">
    <button class="menu-btn" id="menu-btn" type="button" aria-label="{menu_aria}" aria-expanded="false">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M1 2.75A.75.75 0 0 1 1.75 2h12.5a.75.75 0 0 1 0 1.5H1.75A.75.75 0 0 1 1 2.75Zm0 5A.75.75 0 0 1 1.75 7h12.5a.75.75 0 0 1 0 1.5H1.75A.75.75 0 0 1 1 7.75ZM1.75 12h12.5a.75.75 0 0 1 0 1.5H1.75a.75.75 0 0 1 0-1.5Z"/></svg>
    </button>
    <a class="doc-brand" href="{rel_home}">
      <svg viewBox="0 0 32 32" aria-hidden="true"><path d="M5 27 L24 6 l3 3 L8 28 Z" fill="currentColor" style="color:var(--accent)"/></svg>
      lancet
    </a>
    <a class="doc-badge" href="{rel_url(out_file, docs_base / 'index.html')}">{docs_label}</a>
    <button class="search-trigger" id="search-trigger" type="button">
      <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M10.68 11.74a6 6 0 0 1-7.922-8.982 6 6 0 0 1 8.982 7.922l3.04 3.04a.749.749 0 0 1-.326 1.275.749.749 0 0 1-.734-.215ZM11.5 7a4.499 4.499 0 1 0-8.997 0A4.499 4.499 0 0 0 11.5 7Z"/></svg>
      <span>{search_ph}</span>
      <kbd>/</kbd>
    </button>
    <nav class="header-links" aria-label="Primary">
      <a href="{rel_home}" class="hide-sm">{home_label}</a>
      <a href="{rel_switch}" hreflang="{other_lang}" lang="{other_lang}">{switch_label}</a>
      <a class="header-gh" href="https://github.com/CREE1116/lancet" target="_blank" rel="noopener">
        <svg width="15" height="15" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
        <span class="hide-sm">GitHub</span>
      </a>
    </nav>
  </div>
</header>

<div class="drawer-backdrop" id="drawer-backdrop"></div>

<div class="doc-shell">
  <aside class="doc-sidebar" aria-label="Documentation navigation">
    {sidebar_html}
  </aside>

  <main class="doc-main">
    <article class="doc-article">
      <div class="crumb">{group_map[page['group']]}</div>
      <h1>{page['title']}</h1>
      <p class="lede">{page['lede']}</p>
      {body_html}
      <div class="pager">{prev_html}{next_html}</div>
      <div class="meta-foot">
        <span>{source_note}</span>
        <a href="{gh_source}" target="_blank" rel="noopener">{edit_note} →</a>
      </div>
    </article>
  </main>

  <aside class="doc-toc" aria-label="{toc_label}">
    <div class="toc-title">{toc_label}</div>
    <ul class="toc-list" id="doc-toc"></ul>
  </aside>
</div>

<div class="search-modal" id="search-modal" role="dialog" aria-modal="true" aria-label="{search_ph}" hidden>
  <div class="search-panel">
    <div class="search-input-row">
      <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M10.68 11.74a6 6 0 0 1-7.922-8.982 6 6 0 0 1 8.982 7.922l3.04 3.04a.749.749 0 0 1-.326 1.275.749.749 0 0 1-.734-.215ZM11.5 7a4.499 4.499 0 1 0-8.997 0A4.499 4.499 0 0 0 11.5 7Z"/></svg>
      <input id="search-input" type="search" placeholder="{search_ph}" autocomplete="off" spellcheck="false">
      <button class="search-esc" id="search-esc" type="button">Esc</button>
    </div>
    <div class="search-results" id="search-results"></div>
  </div>
</div>

<script src="{rel_search_js}?v=1"></script>
<script src="{rel_js}?v=1"></script>
</body>
</html>
"""
    search_entry = {
        "path": rel_path,
        "group": group_map[page["group"]],
        "title": page["title"],
        "description": page["description"],
        "keywords": page.get("keywords", ""),
        "headings": ext.headings,
        "text": plain_text[:6000],
    }
    return out_file, html_doc, search_entry


def validate_site(generated_files: list[Path]) -> list[str]:
    errors: list[str] = []
    parsed_ids: dict[Path, set[str]] = {}
    parsed_refs: dict[Path, list[tuple[str, str]]] = {}

    for fp in generated_files:
        v = LinkValidator()
        v.feed(fp.read_text(encoding="utf-8"))
        parsed_ids[fp] = v.ids
        parsed_refs[fp] = v.refs

    for fp, refs in parsed_refs.items():
        for attr, raw in refs:
            if raw.startswith(("http://", "https://", "data:", "mailto:")):
                continue
            path_part, _, frag = raw.partition("#")
            path_part = path_part.split("?", 1)[0]
            if not path_part:
                if frag and frag not in parsed_ids[fp]:
                    errors.append(f"{fp.relative_to(ROOT)}: broken self-anchor #{frag}")
                continue
            target_norm = posixpath.normpath(
                posixpath.join(str(fp.parent.relative_to(ROOT)).replace(os.sep, "/"), path_part)
            )
            target = ROOT / target_norm
            if not target.exists():
                errors.append(f"{fp.relative_to(ROOT)}: missing {attr} target -> {raw} ({target_norm})")
                continue
            if frag and target in parsed_ids and frag not in parsed_ids[target]:
                errors.append(f"{fp.relative_to(ROOT)}: missing fragment #{frag} in {target_norm}")

    return errors


def write_sitemap() -> int:
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"',
        '        xmlns:xhtml="http://www.w3.org/1999/xhtml">',
    ]
    pairs = [("", "ko/")]
    for _, p_path in NAV_ORDER:
        suffix = "" if p_path == "index.html" else p_path
        pairs.append((f"docs/{suffix}", f"ko/docs/{suffix}"))

    for en_rel, ko_rel in pairs:
        en_url = f"{BASE_URL}/{en_rel}"
        ko_url = f"{BASE_URL}/{ko_rel}"
        for loc in (en_url, ko_url):
            lines.extend([
                "  <url>",
                f"    <loc>{loc}</loc>",
                f'    <xhtml:link rel="alternate" hreflang="en" href="{en_url}"/>',
                f'    <xhtml:link rel="alternate" hreflang="ko" href="{ko_url}"/>',
                f'    <xhtml:link rel="alternate" hreflang="x-default" href="{en_url}"/>',
                "  </url>",
            ])
    lines.append("</urlset>")
    (SITE_DIR / "sitemap.xml").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return len(pairs) * 2


def clean_docs_dirs() -> None:
    for d in (SITE_DIR / "docs", SITE_DIR / "ko" / "docs"):
        if d.exists():
            shutil.rmtree(d)


def main() -> int:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from docs_en import PAGES_EN  # noqa: PLC0415
    from docs_ko import PAGES_KO  # noqa: PLC0415

    clean_docs_dirs()
    copy_diagrams()

    search_index = {"en": [], "ko": []}
    generated: list[Path] = []

    for lang, pages in (("en", PAGES_EN), ("ko", PAGES_KO)):
        by_path = {p["path"]: p for p in pages}
        for _, p_path in NAV_ORDER:
            out_file, content, entry = render_page(lang, by_path[p_path], by_path)
            out_file.write_text(content, encoding="utf-8")
            generated.append(out_file)
            search_index[lang].append(entry)

    search_json = json.dumps(search_index, ensure_ascii=False, indent=2) + "\n"
    (ASSETS_DIR / "docs-search.json").write_text(search_json, encoding="utf-8")
    (ASSETS_DIR / "docs-search.js").write_text(
        f"window.__LANCET_DOCS_SEARCH__ = {json.dumps(search_index, ensure_ascii=False)};\n",
        encoding="utf-8",
    )
    sitemap_count = write_sitemap()

    errors = validate_site(generated)
    if errors:
        for e in errors:
            print(f"ERROR: {e}", file=sys.stderr)
        return 1

    print(
        f"Built {len(generated)} documentation pages, copied {len(DIAGRAM_FILES)} diagrams, "
        f"wrote docs-search.json/js and {sitemap_count} sitemap URLs, 0 broken links."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
