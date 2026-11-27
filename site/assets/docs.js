(() => {
  const body = document.body;
  const lang = document.documentElement.lang === "ko" ? "ko" : "en";
  const docsRoot = body.dataset.docsRoot || "./";
  const searchUrl = body.dataset.searchUrl || "";

  // 1. Copy buttons on code blocks
  document.querySelectorAll(".doc-code").forEach((box) => {
    const btn = box.querySelector(".copy-btn");
    const code = box.querySelector("pre code, pre");
    if (!btn || !code) return;
    const orig = btn.textContent;
    btn.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(code.textContent);
        btn.textContent = lang === "ko" ? "복사됨" : "Copied";
        setTimeout(() => { btn.textContent = orig; }, 1400);
      } catch (_) {}
    });
  });

  // 2. Mobile drawer
  const menuBtn = document.getElementById("menu-btn");
  const backdrop = document.getElementById("drawer-backdrop");
  const closeDrawer = () => {
    body.classList.remove("drawer-open");
    if (menuBtn) menuBtn.setAttribute("aria-expanded", "false");
  };
  if (menuBtn) {
    menuBtn.addEventListener("click", () => {
      const open = body.classList.toggle("drawer-open");
      menuBtn.setAttribute("aria-expanded", String(open));
    });
  }
  if (backdrop) backdrop.addEventListener("click", closeDrawer);
  document.querySelectorAll(".doc-sidebar a").forEach((a) => a.addEventListener("click", closeDrawer));

  // 3. Auto right-hand TOC + ScrollSpy
  const tocList = document.getElementById("doc-toc");
  const headings = Array.from(document.querySelectorAll(".doc-article h2[id], .doc-article h3[id]"));
  if (tocList && headings.length) {
    const links = new Map();
    headings.forEach((h) => {
      const li = document.createElement("li");
      if (h.tagName === "H3") li.className = "is-h3";
      const a = document.createElement("a");
      a.href = "#" + h.id;
      a.textContent = h.dataset.toc || h.textContent.replace(/#$/, "").trim();
      li.appendChild(a);
      tocList.appendChild(li);
      links.set(h.id, a);
    });
    const setActive = (id) => {
      links.forEach((a, k) => a.classList.toggle("active", k === id));
    };
    const obs = new IntersectionObserver(
      (entries) => {
        const vis = entries.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        if (vis.length) setActive(vis[0].target.id);
      },
      { rootMargin: "-76px 0px -70% 0px", threshold: 0 }
    );
    headings.forEach((h) => obs.observe(h));
    setActive(headings[0].id);
  }

  // 4. Instant documentation search
  const modal = document.getElementById("search-modal");
  const trigger = document.getElementById("search-trigger");
  const input = document.getElementById("search-input");
  const resultsEl = document.getElementById("search-results");
  const escBtn = document.getElementById("search-esc");
  let indexData = window.__LANCET_DOCS_SEARCH__ || null;
  let selectedIdx = 0;

  async function ensureIndex() {
    if (indexData || !searchUrl) return;
    try {
      const res = await fetch(searchUrl);
      if (res.ok) indexData = await res.json();
    } catch (_) {}
  }

  function escapeHtml(s) {
    return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  }

  function renderResults(q) {
    if (!resultsEl) return;
    if (!indexData) {
      resultsEl.innerHTML = `<div class="search-empty">${lang === "ko" ? "검색 인덱스를 불러오는 중…" : "Loading search index…"}</div>`;
      return;
    }
    const items = indexData[lang] || [];
    const terms = q.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const scored = [];
    for (const p of items) {
      const titleL = p.title.toLowerCase();
      const bodyL = p.text.toLowerCase();
      const kwL = (p.keywords || "").toLowerCase();
      if (!terms.length) {
        scored.push({ score: 1, item: p, hash: "", sub: p.description });
        continue;
      }
      const hay = titleL + "\n" + kwL + "\n" + p.headings.map((h) => h.text.toLowerCase()).join("\n") + "\n" + bodyL;
      if (!terms.every((t) => hay.includes(t))) continue;
      let score = 0;
      let bestHash = "";
      let sub = p.description;
      for (const t of terms) {
        if (titleL.includes(t)) score += 20;
        if (kwL.includes(t)) score += 10;
        for (const h of p.headings) {
          if (h.text.toLowerCase().includes(t)) {
            score += 12;
            if (!bestHash) { bestHash = "#" + h.id; sub = h.text; }
          }
        }
        if (bodyL.includes(t)) score += 3;
      }
      if (!bestHash) {
        const idx = bodyL.indexOf(terms[0]);
        if (idx >= 0) {
          const start = Math.max(0, idx - 36);
          const end = Math.min(p.text.length, idx + 96);
          sub = (start > 0 ? "…" : "") + p.text.slice(start, end).trim() + (end < p.text.length ? "…" : "");
        }
      }
      scored.push({ score, item: p, hash: bestHash, sub });
    }
    scored.sort((a, b) => b.score - a.score);
    const top = scored.slice(0, 12);
    selectedIdx = 0;
    if (!top.length) {
      resultsEl.innerHTML = `<div class="search-empty">${lang === "ko" ? "일치하는 문서가 없습니다." : "No matching documentation pages."}</div>`;
      return;
    }
    resultsEl.innerHTML = top.map((r, i) => `
      <a class="search-item${i === 0 ? " selected" : ""}" href="${escapeHtml(docsRoot + r.item.path + r.hash)}" data-idx="${i}">
        <small>${escapeHtml(r.item.group)}</small>
        <b>${escapeHtml(r.item.title)}</b>
        <span>${escapeHtml(r.sub)}</span>
      </a>
    `).join("");
  }

  async function openSearch() {
    if (!modal || !input) return;
    modal.hidden = false;
    input.value = "";
    renderResults("");
    requestAnimationFrame(() => input.focus());
    await ensureIndex();
    renderResults(input.value);
  }

  function closeSearch() {
    if (modal) modal.hidden = true;
  }

  if (trigger) trigger.addEventListener("click", openSearch);
  if (escBtn) escBtn.addEventListener("click", closeSearch);
  if (modal) modal.addEventListener("click", (e) => { if (e.target === modal) closeSearch(); });
  if (input) {
    input.addEventListener("input", () => renderResults(input.value));
    input.addEventListener("keydown", (e) => {
      const links = Array.from(resultsEl ? resultsEl.querySelectorAll(".search-item") : []);
      if (e.key === "Escape") { closeSearch(); return; }
      if (!links.length) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        selectedIdx = (selectedIdx + 1) % links.length;
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        selectedIdx = (selectedIdx - 1 + links.length) % links.length;
      } else if (e.key === "Enter") {
        e.preventDefault();
        links[selectedIdx]?.click();
        return;
      } else {
        return;
      }
      links.forEach((el, i) => el.classList.toggle("selected", i === selectedIdx));
      links[selectedIdx]?.scrollIntoView({ block: "nearest" });
    });
  }

  document.addEventListener("keydown", (e) => {
    const tag = (e.target && e.target.tagName) || "";
    const inInput = tag === "INPUT" || tag === "TEXTAREA" || (e.target && e.target.isContentEditable);
    if (e.key === "Escape") {
      closeDrawer();
      closeSearch();
      return;
    }
    if ((e.key === "/" && !inInput && !e.metaKey && !e.ctrlKey) || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k")) {
      e.preventDefault();
      openSearch();
    }
  });
})();
