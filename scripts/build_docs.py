#!/usr/bin/env python3
"""Zero-dependency Markdown documentation compiler for Lancet (`docs/site/` -> `site/`)."""

from __future__ import annotations

import html
import json
import os
import posixpath
import re
import shutil
import sys
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MD_ROOT = ROOT / "docs" / "site"
NAV_FILE = MD_ROOT / "nav.json"
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

CALLOUT_KINDS = {
    "TIP": "tip",
    "NOTE": "note",
    "WARNING": "warn",
    "CAUTION": "danger",
}


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
        if amap.get("id"):
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


def esc(s: str) -> str:
    return html.escape(s, quote=True)


def md_href_to_html(href: str) -> str:
    if href.startswith(("http://", "https://", "mailto:", "#")):
        return href
    return re.sub(r"\.md(#|$)", r".html\1", href)


def render_inline(text: str) -> str:
    tokens: dict[str, str] = {}

    def put_token(val: str) -> str:
        key = f"\x00T{len(tokens)}\x00"
        tokens[key] = val
        return key

    # 1. Protect inline code spans
    s = re.sub(r"`([^`]+)`", lambda m: put_token(f"<code>{esc(m.group(1))}</code>"), text)

    # 2. Protect safe inline spans used in reference tables
    s = re.sub(
        r"(<span class=['\"]tag-(?:yes|no|ext)['\"]>.*?</span>)",
        lambda m: put_token(m.group(1)),
        s,
    )

    # 3. Escape remaining HTML characters
    s = html.escape(s, quote=False)

    # 4. Markdown links `[label](href)`
    def repl_link(m: re.Match[str]) -> str:
        label = m.group(1)
        href = md_href_to_html(html.unescape(m.group(2)).strip())
        return put_token(f'<a href="{esc(href)}">{label}</a>')

    s = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", repl_link, s)

    # 5. Bold and italic
    s = re.sub(r"\*\*([^\n*]+?)\*\*", r"<strong>\1</strong>", s)
    s = re.sub(r"(?<![\w*])\*([^\n*]+?)\*(?![\w*])", r"<em>\1</em>", s)

    # Restore tokens cleanly (links may wrap code spans, so restore until fixed point)
    for _ in range(3):
        if "\x00T" not in s:
            break
        s = re.sub(r"\x00T\d+\x00", lambda m: tokens[m.group(0)], s)
    return s


def split_table_row(line: str) -> list[str]:
    stripped = line.strip()
    if stripped.startswith("|"):
        stripped = stripped[1:]
    if stripped.endswith("|") and not stripped.endswith("\\|"):
        stripped = stripped[:-1]
    cells: list[str] = []
    cur: list[str] = []
    in_code = False
    i = 0
    while i < len(stripped):
        ch = stripped[i]
        if ch == "\\" and i + 1 < len(stripped) and stripped[i + 1] == "|":
            cur.append("|")
            i += 2
            continue
        if ch == "`":
            in_code = not in_code
            cur.append(ch)
        elif ch == "|" and not in_code:
            cells.append("".join(cur).strip())
            cur = []
        else:
            cur.append(ch)
        i += 1
    cells.append("".join(cur).strip())
    return cells


def slugify(text: str) -> str:
    slug = re.sub(r"[^a-z0-9가-힣]+", "-", text.lower()).strip("-")
    return slug or "section"


def parse_frontmatter(raw: str) -> tuple[dict[str, str], str]:
    if not raw.startswith("---\n"):
        return {}, raw
    end = raw.find("\n---\n", 4)
    if end == -1:
        return {}, raw
    meta_raw = raw[4:end]
    body = raw[end + 5 :]
    meta: dict[str, str] = {}
    for line in meta_raw.splitlines():
        if not line.strip() or line.strip().startswith("#"):
            continue
        k, _, v = line.partition(":")
        val = v.strip()
        if len(val) >= 2 and val[0] == '"' and val[-1] == '"':
            val = json.loads(val)
        meta[k.strip()] = val
    return meta, body


def render_blocks(lines: list[str], rel_diag: str, lang: str) -> str:
    out: list[str] = []
    i = 0
    n = len(lines)
    copy_lbl = "복사" if lang == "ko" else "Copy"

    while i < n:
        line = lines[i]
        stripped = line.strip()
        if not stripped:
            i += 1
            continue

        # 1. Fenced code block: ```lang title="Label"
        m_fence = re.match(r'^```([^\s`]*)(?:\s+title="([^"]+)")?\s*$', stripped)
        if m_fence:
            info = m_fence.group(1) or "text"
            label = m_fence.group(2) or info
            code_lines: list[str] = []
            i += 1
            while i < n and lines[i].strip() != "```":
                code_lines.append(lines[i])
                i += 1
            i += 1
            body = esc("\n".join(code_lines).strip("\n"))
            out.append(
                f'<div class="doc-code">'
                f'<div class="doc-code-head"><span>{esc(label)}</span>'
                f'<button class="copy-btn" type="button">{copy_lbl}</button></div>'
                f"<pre><code>{body}</code></pre></div>"
            )
            continue

        # 2. Cards block :::cards ... :::
        if stripped == ":::cards":
            i += 1
            card_parts = ['<div class="doc-cards">']
            while i < n and lines[i].strip() != ":::":
                row = lines[i].strip()
                if row.startswith("- "):
                    parts = [p.strip() for p in row[2:].split("|")]
                    if len(parts) == 4:
                        eye, title, desc, href = parts
                        href_html = md_href_to_html(href)
                        card_parts.append(
                            f'<a class="doc-card" href="{esc(href_html)}">'
                            f"<small>{esc(eye)}</small>"
                            f"<b>{esc(title)}</b>"
                            f"<span>{render_inline(desc)}</span>"
                            f"</a>"
                        )
                i += 1
            i += 1
            card_parts.append("</div>")
            out.append("".join(card_parts))
            continue

        # 3. Headings `## Title {#id}` / `### Title {#id}`
        m_head = re.match(r"^(#{2,3})\s+(.+?)(?:\s+\{#([A-Za-z0-9_-]+)\})?\s*$", stripped)
        if m_head:
            lvl = len(m_head.group(1))
            title = m_head.group(2).strip()
            hid = m_head.group(3) or slugify(title)
            out.append(
                f'<h{lvl} id="{esc(hid)}">{render_inline(title)}'
                f'<a class="heading-anchor" href="#{esc(hid)}" aria-label="Link to section">#</a>'
                f"</h{lvl}>"
            )
            i += 1
            continue

        # 4. Diagrams `![alt](diagram:file.svg "caption")`
        m_diag = re.match(r'^!\[(.*?)\]\(diagram:([^)\s]+)\s+"(.*)"\)\s*$', stripped)
        if m_diag:
            alt, fname, caption = m_diag.groups()
            out.append(
                f'<figure class="diagram-fig">'
                f'<img src="{esc(rel_diag)}/{esc(fname)}" alt="{esc(alt)}" loading="lazy">'
                f"<figcaption>{render_inline(caption)}</figcaption>"
                f"</figure>"
            )
            i += 1
            continue

        # 5. Callouts / Blockquotes `> [!TIP] Title`
        if stripped.startswith(">"):
            qlines: list[str] = []
            while i < n and lines[i].strip().startswith(">"):
                q = lines[i].lstrip()
                q = q[1:]
                if q.startswith(" "):
                    q = q[1:]
                qlines.append(q)
                i += 1
            first = qlines[0].strip() if qlines else ""
            m_call = re.match(r"^\[!(TIP|NOTE|WARNING|CAUTION)\]\s*(.*)$", first)
            if m_call:
                kind = CALLOUT_KINDS[m_call.group(1)]
                title = m_call.group(2).strip() or m_call.group(1).title()
                inner_html = render_blocks(qlines[1:], rel_diag, lang)
                out.append(
                    f'<div class="callout {kind}">'
                    f'<div class="callout-title">{esc(title)}</div>'
                    f"{inner_html}</div>"
                )
            else:
                out.append(f"<blockquote>{render_blocks(qlines, rel_diag, lang)}</blockquote>")
            continue

        # 6. GFM Pipe Tables
        if stripped.startswith("|") and i + 1 < n and re.match(r"^\|[\s:|-]+\|\s*$", lines[i + 1].strip()):
            headers = split_table_row(lines[i])
            aligns_raw = split_table_row(lines[i + 1])
            num_cols = {idx for idx, a in enumerate(aligns_raw) if a.endswith(":")}
            i += 2
            rows_html: list[str] = []
            while i < n and lines[i].strip().startswith("|"):
                cells = split_table_row(lines[i])
                tds = "".join(
                    f'<td class="num">{render_inline(c)}</td>' if ci in num_cols else f"<td>{render_inline(c)}</td>"
                    for ci, c in enumerate(cells)
                )
                rows_html.append(f"<tr>{tds}</tr>")
                i += 1
            ths = "".join(f"<th>{render_inline(h)}</th>" for h in headers)
            out.append(
                f'<div class="table-wrap"><table><thead><tr>{ths}</tr></thead>'
                f"<tbody>{''.join(rows_html)}</tbody></table></div>"
            )
            continue

        # 7. Unordered list `- `
        if stripped.startswith("- "):
            items: list[str] = []
            while i < n and lines[i].strip().startswith("- "):
                item_lines = [lines[i].strip()[2:]]
                i += 1
                while i < n and lines[i].strip() and not lines[i].strip().startswith(("- ", "#", ">", "```", "|", ":::")):
                    if re.match(r"^\d+\.\s+", lines[i].strip()):
                        break
                    item_lines.append(lines[i].strip())
                    i += 1
                items.append(f"<li>{render_inline(' '.join(item_lines))}</li>")
            out.append(f"<ul>{''.join(items)}</ul>")
            continue

        # 8. Ordered list `1. `
        if re.match(r"^\d+\.\s+", stripped):
            items = []
            while i < n and re.match(r"^\d+\.\s+", lines[i].strip()):
                text_part = re.sub(r"^\d+\.\s+", "", lines[i].strip(), count=1)
                item_lines = [text_part]
                i += 1
                while i < n and lines[i].strip() and not lines[i].strip().startswith(("- ", "#", ">", "```", "|", ":::")):
                    if re.match(r"^\d+\.\s+", lines[i].strip()):
                        break
                    item_lines.append(lines[i].strip())
                    i += 1
                items.append(f"<li>{render_inline(' '.join(item_lines))}</li>")
            out.append(f"<ol>{''.join(items)}</ol>")
            continue

        # 9. Paragraph
        para: list[str] = []
        while i < n:
            cur = lines[i].strip()
            if (
                not cur
                or cur.startswith(("#", ">", "```", "- ", "|", ":::", "!["))
                or re.match(r"^\d+\.\s+", cur)
            ):
                break
            para.append(cur)
            i += 1
        if para:
            out.append(f"<p>{render_inline(' '.join(para))}</p>")

    return "\n".join(out)


def rel_url(from_file: Path, to_file: Path) -> str:
    return os.path.relpath(to_file, from_file.parent).replace(os.sep, "/")


def copy_diagrams() -> None:
    DIAGRAMS_DST.mkdir(parents=True, exist_ok=True)
    for name in DIAGRAM_FILES:
        src = DIAGRAMS_SRC / name
        if not src.exists():
            raise FileNotFoundError(f"Missing source diagram: {src}")
        shutil.copy2(src, DIAGRAMS_DST / name)


def clean_docs_dirs() -> None:
    for d in (SITE_DIR / "docs", SITE_DIR / "ko" / "docs"):
        if d.exists():
            shutil.rmtree(d)


def load_pages(nav: dict, lang: str) -> list[dict]:
    pages: list[dict] = []
    for entry in nav["pages"]:
        md_rel = entry["path"]
        md_file = MD_ROOT / lang / md_rel
        if not md_file.exists():
            raise FileNotFoundError(f"Missing Markdown doc: {md_file}")
        meta, body = parse_frontmatter(md_file.read_text(encoding="utf-8"))
        html_rel = re.sub(r"\.md$", ".html", md_rel)
        pages.append({
            "path": html_rel,
            "md_rel": md_rel,
            "group": entry["group"],
            "nav_title": meta["nav_title"],
            "title": meta["title"],
            "description": meta["description"],
            "lede": meta["lede"],
            "keywords": meta.get("keywords", ""),
            "source": meta.get("source", f"https://github.com/CREE1116/lancet/blob/main/docs/site/{lang}/{md_rel}"),
            "body_md": body,
        })
    return pages


def render_page(lang: str, page: dict, nav_order: list[str], groups: list[dict], by_path: dict[str, dict]) -> tuple[Path, str, dict]:
    rel_path = page["path"]
    docs_base = SITE_DIR / ("docs" if lang == "en" else "ko/docs")
    out_file = docs_base / rel_path
    out_file.parent.mkdir(parents=True, exist_ok=True)

    other_lang = "ko" if lang == "en" else "en"
    other_base = SITE_DIR / ("ko/docs" if lang == "en" else "docs")
    other_file = other_base / rel_path
    home_file = SITE_DIR / ("index.html" if lang == "en" else "ko/index.html")

    rel_home = rel_url(out_file, home_file)
    rel_docs_root = rel_url(out_file, docs_base) + "/"
    if rel_docs_root == "./":
        rel_docs_root = ""
    rel_css = rel_url(out_file, ASSETS_DIR / "docs.css")
    rel_js = rel_url(out_file, ASSETS_DIR / "docs.js")
    rel_search = rel_url(out_file, ASSETS_DIR / "docs-search.json")
    rel_search_js = rel_url(out_file, ASSETS_DIR / "docs-search.js")
    rel_switch = rel_url(out_file, other_file)
    rel_diag = rel_url(out_file, DIAGRAMS_DST)

    en_url = f"{BASE_URL}/docs/{'' if rel_path == 'index.html' else rel_path}"
    ko_url = f"{BASE_URL}/ko/docs/{'' if rel_path == 'index.html' else rel_path}"
    canon_url = en_url if lang == "en" else ko_url
    og_img = f"{BASE_URL}/assets/{'og-image.png' if lang == 'en' else 'og-image-ko.png'}"

    body_html = render_blocks(page["body_md"].splitlines(), rel_diag, lang)
    ext = ArticleExtractor()
    ext.feed(body_html)
    plain_text = re.sub(r"\s+", " ", " ".join(ext.text_parts)).strip()

    group_map = {g["key"]: g["label"] for g in groups}
    sidebar_parts: list[str] = []
    for g in groups:
        lis: list[str] = []
        for p_path in nav_order:
            p = by_path[p_path]
            if p["group"] != g["key"]:
                continue
            href = rel_url(out_file, docs_base / p_path)
            cls = ' class="active"' if p_path == rel_path else ""
            lis.append(f'<li><a href="{href}"{cls}>{esc(p["nav_title"])}</a></li>')
        sidebar_parts.append(
            f'<div class="nav-group"><div class="nav-group-title">{esc(g["label"])}</div>'
            f'<ul class="nav-list">{"".join(lis)}</ul></div>'
        )
    sidebar_html = "".join(sidebar_parts)

    idx = nav_order.index(rel_path)
    prev_html = "<div></div>"
    next_html = "<div></div>"
    if idx > 0:
        pp = nav_order[idx - 1]
        href = rel_url(out_file, docs_base / pp)
        lbl = "이전" if lang == "ko" else "Previous"
        prev_html = f'<a class="pager-card prev" href="{href}"><small>← {lbl}</small><b>{esc(by_path[pp]["nav_title"])}</b></a>'
    if idx + 1 < len(nav_order):
        np = nav_order[idx + 1]
        href = rel_url(out_file, docs_base / np)
        lbl = "다음" if lang == "ko" else "Next"
        next_html = f'<a class="pager-card next" href="{href}"><small>{lbl} →</small><b>{esc(by_path[np]["nav_title"])}</b></a>'

    switch_label = "한국어" if lang == "en" else "English"
    home_label = "홈" if lang == "ko" else "Home"
    docs_label = "문서" if lang == "ko" else "Docs"
    search_ph = "문서 검색…" if lang == "ko" else "Search docs…"
    toc_label = "이 페이지 목차" if lang == "ko" else "On this page"
    menu_aria = "문서 메뉴 열기" if lang == "ko" else "Open documentation menu"
    source_note = "MIT OR Apache-2.0 · Rust 단일 바이너리" if lang == "ko" else "MIT OR Apache-2.0 · Single Rust binary"
    edit_note = "GitHub에서 Markdown 소스 보기" if lang == "ko" else "View Markdown source on GitHub"
    md_gh_url = f"https://github.com/CREE1116/lancet/blob/main/docs/site/{lang}/{page['md_rel']}"

    html_doc = f"""<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{esc(page['title'])} — lancet {docs_label}</title>
<meta name="description" content="{esc(page['description'])}">
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
<meta property="og:title" content="{esc(page['title'])} — lancet {docs_label}">
<meta property="og:description" content="{esc(page['description'])}">
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
      <div class="crumb">{esc(group_map[page['group']])}</div>
      <h1>{esc(page['title'])}</h1>
      <p class="lede">{render_inline(page['lede'])}</p>
      {body_html}
      <div class="pager">{prev_html}{next_html}</div>
      <div class="meta-foot">
        <span>{source_note}</span>
        <a href="{esc(md_gh_url)}" target="_blank" rel="noopener">{edit_note} →</a>
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
        "keywords": page["keywords"],
        "headings": ext.headings,
        "text": plain_text[:6000],
    }
    return out_file, html_doc, search_entry


def write_sitemap(nav_order: list[str]) -> int:
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"',
        '        xmlns:xhtml="http://www.w3.org/1999/xhtml">',
    ]
    pairs = [("", "ko/")]
    for p_path in nav_order:
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


def main() -> int:
    nav = json.loads(NAV_FILE.read_text(encoding="utf-8"))
    nav_order = [re.sub(r"\.md$", ".html", e["path"]) for e in nav["pages"]]

    clean_docs_dirs()
    copy_diagrams()

    search_index = {"en": [], "ko": []}
    generated: list[Path] = []

    for lang in ("en", "ko"):
        pages = load_pages(nav, lang)
        by_path = {p["path"]: p for p in pages}
        for p_path in nav_order:
            out_file, content, entry = render_page(
                lang, by_path[p_path], nav_order, nav["groups"][lang], by_path
            )
            out_file.write_text(content, encoding="utf-8")
            generated.append(out_file)
            search_index[lang].append(entry)

    search_json = json.dumps(search_index, ensure_ascii=False, indent=2) + "\n"
    (ASSETS_DIR / "docs-search.json").write_text(search_json, encoding="utf-8")
    (ASSETS_DIR / "docs-search.js").write_text(
        f"window.__LANCET_DOCS_SEARCH__ = {json.dumps(search_index, ensure_ascii=False)};\n",
        encoding="utf-8",
    )
    sitemap_count = write_sitemap(nav_order)

    errors = validate_site(generated)
    if errors:
        for e in errors:
            print(f"ERROR: {e}", file=sys.stderr)
        return 1

    print(
        f"Built {len(generated)} Markdown documentation pages, copied {len(DIAGRAM_FILES)} diagrams, "
        f"wrote docs-search.json/js and {sitemap_count} sitemap URLs, 0 broken links."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
