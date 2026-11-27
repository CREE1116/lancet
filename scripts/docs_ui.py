"""Shared HTML component helpers for Lancet's static documentation site."""

from __future__ import annotations

import html
from typing import Iterable, Sequence


def esc(s: str) -> str:
    return html.escape(s, quote=True)


def h2(hid: str, title: str) -> str:
    t = esc(title)
    return (
        f'<h2 id="{esc(hid)}">{t}'
        f'<a class="heading-anchor" href="#{esc(hid)}" aria-label="Link to section">#</a>'
        f"</h2>"
    )


def h3(hid: str, title: str) -> str:
    t = esc(title)
    return (
        f'<h3 id="{esc(hid)}">{t}'
        f'<a class="heading-anchor" href="#{esc(hid)}" aria-label="Link to section">#</a>'
        f"</h3>"
    )


def code(label: str, body: str, lang: str = "en") -> str:
    btn_label = "복사" if lang == "ko" else "Copy"
    return (
        '<div class="doc-code">'
        f'<div class="doc-code-head"><span>{esc(label)}</span>'
        f'<button class="copy-btn" type="button">{btn_label}</button></div>'
        f"<pre><code>{esc(body.strip())}</code></pre>"
        "</div>"
    )


def callout(kind: str, title: str, body_html: str) -> str:
    assert kind in ("tip", "note", "warn", "danger")
    return (
        f'<div class="callout {kind}">'
        f'<div class="callout-title">{esc(title)}</div>'
        f"{body_html}"
        "</div>"
    )


def table(
    headers: Sequence[str],
    rows: Iterable[Sequence[str]],
    num_cols: Sequence[int] = (),
) -> str:
    ths = "".join(f"<th>{h}</th>" for h in headers)
    trs: list[str] = []
    num_set = set(num_cols)
    for r in rows:
        tds = "".join(
            f'<td class="num">{cell}</td>' if i in num_set else f"<td>{cell}</td>"
            for i, cell in enumerate(r)
        )
        trs.append(f"<tr>{tds}</tr>")
    return (
        '<div class="table-wrap"><table>'
        f"<thead><tr>{ths}</tr></thead>"
        f"<tbody>{''.join(trs)}</tbody>"
        "</table></div>"
    )


def diagram(rel_src: str, alt: str, caption: str) -> str:
    return (
        '<figure class="diagram-fig">'
        f'<img src="{esc(rel_src)}" alt="{esc(alt)}" loading="lazy">'
        f"<figcaption>{caption}</figcaption>"
        "</figure>"
    )


def cards(items: Sequence[tuple[str, str, str, str]]) -> str:
    """items: list of (eyebrow, title, desc_html, href)."""
    parts = ['<div class="doc-cards">']
    for eye, title, desc, href in items:
        parts.append(
            f'<a class="doc-card" href="{esc(href)}">'
            f"<small>{esc(eye)}</small>"
            f"<b>{esc(title)}</b>"
            f"<span>{desc}</span>"
            "</a>"
        )
    parts.append("</div>")
    return "".join(parts)
