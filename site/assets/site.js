(() => {
  const KO = document.documentElement.lang === 'ko';
  const T = (en, ko) => (KO ? ko : en);
  const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const sleep = ms => new Promise(r => setTimeout(r, ms));

  /* analytics (community referrer + conversion events; works with GoatCounter / Plausible / Umami) */
  const getSource = () => {
    try {
      const saved = sessionStorage.getItem('lancet_src');
      if (saved) return saved;
      const params = new URLSearchParams(location.search);
      const utm = params.get('utm_source') || params.get('ref') || params.get('src');
      let src = utm ? utm.toLowerCase() : '';
      if (!src && document.referrer) {
        const host = new URL(document.referrer).hostname.replace(/^www\./, '');
        if (host && host !== location.hostname) src = host;
      }
      src = src || 'direct';
      sessionStorage.setItem('lancet_src', src);
      return src;
    } catch { return 'direct'; }
  };
  const trafficSource = getSource();
  const track = ev => {
    try {
      if (window.goatcounter && typeof window.goatcounter.count === 'function') {
        window.goatcounter.count({ path: `ev/${ev}`, title: `${ev} (${trafficSource})`, event: true });
      }
      if (typeof window.plausible === 'function') {
        window.plausible(ev, { props: { source: trafficSource } });
      }
      if (window.umami && typeof window.umami.track === 'function') {
        window.umami.track(ev, { source: trafficSource });
      }
    } catch {}
  };

  /* install tabs */
  const cmd = $('#cmd');
  let activeMethod = 'macos';
  $$('.tabs button').forEach(b => b.addEventListener('click', () => {
    $$('.tabs button').forEach(x => x.setAttribute('aria-selected', x === b));
    cmd.textContent = b.dataset.cmd;
    activeMethod = b.dataset.method || b.textContent.trim().toLowerCase();
    track(`install_tab:${activeMethod}`);
  }));
  $('#copy').addEventListener('click', async e => {
    try {
      await navigator.clipboard.writeText(cmd.textContent);
      e.target.textContent = T('Copied', '복사됨');
      track(`install_copy:${activeMethod}`);
    } catch { e.target.textContent = T('Select & copy', '직접 선택해 복사'); }
    setTimeout(() => e.target.textContent = T('Copy', '복사'), 1400);
  });

  /* one-click agent instructions copy */
  const AGENT_RULES = `# lancet — Code Search, Read & Edit Rules
Use \`lancet\` as your unified CLI replacement for \`rg\`, \`fd\`, \`ast-grep\`, \`cat\`, and \`sed\`.

1. Orient:  lancet fd --tree --max-depth 2 . | lancet outline FILE
2. Search:  lancet rg -w SYMBOL src  |  lancet 'query in ext:rs'
3. Read:    lancet read FILE --symbol NAME --json   # note "rev"
4. Refs:    lancet refs SYMBOL src --lang rs
5. AST:     lancet ast '$X.unwrap()' src --rewrite '$X?' [--apply]
6. Edit:    lancet edit FILE --find '...' --with '...' --rev REV --apply
   (Batch multiple hunks in one call with --edits -; line numbers match read 1:1)
7. Patch:   git diff | lancet patch --apply
8. On conflict (exit 2): re-run lancet read, re-apply on new content, retry with new rev.`;

  const copyAgent = async (btn, resetLabel) => {
    try {
      await navigator.clipboard.writeText(AGENT_RULES);
      const label = btn.querySelector('span') || btn;
      label.textContent = T('Copied to clipboard', '클립보드에 복사됨');
      track('agent_copy');
      setTimeout(() => label.textContent = resetLabel, 1600);
    } catch {
      const label = btn.querySelector('span') || btn;
      label.textContent = T('Copy failed', '복사 실패');
      setTimeout(() => label.textContent = resetLabel, 1600);
    }
  };
  const heroAgentBtn = $('#copy-agent-hero');
  if (heroAgentBtn) {
    const lbl = heroAgentBtn.querySelector('span').textContent;
    heroAgentBtn.addEventListener('click', () => copyAgent(heroAgentBtn, lbl));
  }
  const startAgentBtn = $('#copy-agent-start');
  if (startAgentBtn) {
    const lbl = startAgentBtn.textContent;
    startAgentBtn.addEventListener('click', () => copyAgent(startAgentBtn, lbl));
  }

  /* GitHub Star count + click tracking */
  $$('.gh, .btn-star').forEach(el => el.addEventListener('click', () => track('github_star_click')));
  const fmtStars = n => (n >= 1000 ? (n / 1000).toFixed(1).replace(/\.0$/, '') + 'k' : String(n));
  const applyStars = count => {
    if (!count || count <= 0) return;
    const text = fmtStars(count);
    $$('[data-gh-stars]').forEach(el => { el.textContent = text; el.hidden = false; });
  };
  try {
    const cached = sessionStorage.getItem('lancet_stars');
    if (cached) {
      const { n, ts } = JSON.parse(cached);
      if (Date.now() - ts < 600000) applyStars(n);
    }
    if (!cached || Date.now() - JSON.parse(cached).ts >= 600000) {
      fetch('https://api.github.com/repos/CREE1116/lancet')
        .then(r => (r.ok ? r.json() : null))
        .then(d => {
          if (d && typeof d.stargazers_count === 'number') {
            sessionStorage.setItem('lancet_stars', JSON.stringify({ n: d.stargazers_count, ts: Date.now() }));
            applyStars(d.stargazers_count);
          }
        })
        .catch(() => {});
    }
  } catch {}

  /* hero terminal: real output, recorded from the lancet repository */
  const session = [
    ['cmd', 'lancet rg -w write_if_rev src'],
    ['out', '<span class="t-path">src/ast_grep.rs</span>:1211:                    match edit::<span class="t-hl">write_if_rev</span>(\n<span class="t-path">src/edit.rs</span>:348:pub fn <span class="t-hl">write_if_rev</span>(path: &amp;Path, expect_rev: &amp;str, data: &amp;[u8]) -> ...\n<span class="t-path">src/edit_cmd.rs</span>:327:        match edit::<span class="t-hl">write_if_rev</span>(&amp;o.path, &amp;rev, new_src.as_bytes()) {'],
    ['cmd', 'lancet refs write_if_rev src'],
    ['out', 'Definitions (1):\n  <span class="t-d">[def:function]</span> <span class="t-path">edit.rs</span>:348:8: pub fn write_if_rev(...)\nReferences (4 shown of 4):\n  <span class="t-d">[ref]</span> <span class="t-path">edit_cmd.rs</span>:327:21 (in edit_file)\n  <span class="t-d">[ref]</span> <span class="t-path">patch.rs</span>:722:25 (in write_content)'],
    ['cmd', 'lancet read src/edit.rs --symbol sync_dir'],
    ['out', '<span class="t-d"># src/edit.rs [function sync_dir]</span>\n<span class="t-d"> 284 |</span> fn sync_dir(dir: &amp;Path) {\n<span class="t-d"> 285 |</span>     #[cfg(unix)]\n<span class="t-d"> 286 |</span>     if let Ok(d) = File::open(dir) {\n<span class="t-d"> 287 |</span>         let _ = d.sync_all();'],
    ['cmd', "lancet edit src/net.rs --find 'let retries = 5;' \\\n    --with 'let retries = 8;' --rev e70d5a898f122a62 --apply"],
    ['out', '<span class="t-err">error: file changed since rev e70d5a898f122a62 (now 1486bdffbc06c68f); read it again before editing</span>'],
  ];
  const term = $('#term');
  async function runTerm() {
    if (reduce) {
      term.innerHTML = session.map(([k, t]) => k === 'cmd' ? `<span class="t-p">$</span> <span class="t-c">${t}</span>` : t).join('\n') + '\n';
      return;
    }
    for (;;) {
      term.innerHTML = '';
      for (const [kind, text] of session) {
        if (kind === 'cmd') {
          const line = document.createElement('span');
          line.innerHTML = '<span class="t-p">$</span> <span class="t-c"></span><span class="cursor"></span>';
          term.appendChild(line);
          const c = line.querySelector('.t-c');
          for (const ch of text) { c.textContent += ch; await sleep(ch === ' ' ? 18 : 26); }
          await sleep(380);
          line.querySelector('.cursor').remove();
          term.insertAdjacentText('beforeend', '\n');
        } else {
          const o = document.createElement('span');
          o.innerHTML = text + '\n\n';
          term.appendChild(o);
          term.scrollTop = term.scrollHeight;
          await sleep(1300);
        }
      }
      const end = document.createElement('span');
      end.innerHTML = '<span class="t-p">$</span> <span class="cursor"></span>';
      term.appendChild(end);
      await sleep(4200);
    }
  }
  runTerm();

  /* explainer */
  const files = $('#files');
  const N = 32 * 14;
  let seed = 7;
  const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const cells = Array.from({ length: N }, () => {
    const i = document.createElement('i');
    files.appendChild(i);
    const r = rnd();
    return { el: i, cand: r < 0.09, hit: r < 0.08, live: r > 0.985 };
  });
  const grams = ['spi', 'pin', 'in_', 'n_l', '_lo', 'loc', 'ock'];
  const gramBox = $('#grams');
  const caption = $('#caption');
  const fmt = n => n.toLocaleString('en-US');
  function countTo(el, target, ms) {
    if (reduce) { el.textContent = fmt(target); return; }
    const t0 = performance.now();
    const tick = t => {
      const p = Math.max(0, Math.min(1, (t - t0) / ms)); // rAF time can precede t0
      el.textContent = fmt(Math.round(target * (1 - Math.pow(1 - p, 3))));
      if (p < 1 && running) requestAnimationFrame(tick);
      else el.textContent = fmt(target);
    };
    requestAnimationFrame(tick);
  }

  let running = true, current = 0, token = 0;
  const DUR = [11000, 9000, 12000];
  const scenes = $$('.scene');
  const tabs = $$('.scene-tab');

  function resetScene(i) {
    if (i === 0) {
      gramBox.innerHTML = '';
      cells.forEach(c => c.el.className = '');
      ['#c-cand', '#c-hit'].forEach(s => $(s).textContent = '0');
    }
    $$('.fade', scenes[i]).forEach(f => f.classList.remove('reveal'));
    if (i === 2) { $('#laneA').innerHTML = ''; $('#laneB').innerHTML = ''; $('#filerev').textContent = 'rev e70d…'; }
  }
  function step(lane, left, cls, html) {
    const s = document.createElement('div');
    s.className = 'step fade ' + (cls || '');
    s.style.left = left;
    s.innerHTML = html;
    $(lane).appendChild(s);
    requestAnimationFrame(() => requestAnimationFrame(() => s.classList.add('reveal')));
  }
  function setRev(t) { const r = $('#filerev'); r.textContent = 'rev ' + t; r.classList.add('flash'); setTimeout(() => r.classList.remove('flash'), 500); }

  const timelines = [
    [
      [300, () => { caption.innerHTML = T('A query becomes the <b>trigrams</b> every match must contain.', '쿼리를 "결과라면 꼭 들어 있어야 하는" <b>트라이그램</b>으로 쪼갭니다.'); }],
      ...grams.map((g, k) => [500 + k * 140, () => { const s = document.createElement('span'); s.className = 'gram fade'; s.textContent = g; gramBox.appendChild(s); requestAnimationFrame(() => requestAnimationFrame(() => s.classList.add('reveal'))); }]),
      [2000, () => { caption.innerHTML = T('The index intersects their posting lists: <b>91% of files are never opened</b>.', '인덱스에서 교집합을 구하면 <b>파일 91%는 아예 안 엽니다</b>.'); cells.forEach(c => c.el.classList.add(c.cand ? 'cand' : 'dim')); countTo($('#c-cand'), 7814, 1200); }],
      [4300, () => { caption.innerHTML = T('Files edited after <code>lancet index</code> are spotted by mtime, size, ctime and inode, and <b>scanned live</b>.', '<code>lancet index</code> 뒤에 바뀐 파일은 mtime·size·ctime·inode로 잡아내서 <b>바로 스캔</b>합니다.'); cells.forEach(c => { if (c.live) c.el.className = 'live'; }); }],
      [6600, () => { caption.innerHTML = T('Every candidate goes through the same check a full scan uses. <b>Same answer as ripgrep</b>, a fraction of the I/O.', '남은 후보는 전체 스캔과 같은 기준으로 확인합니다. 결과는 <b>ripgrep과 같고</b>, 읽는 양은 훨씬 적습니다.'); cells.forEach(c => { if (c.hit || c.live) c.el.className = c.hit ? 'hit' : 'live'; }); countTo($('#c-hit'), 7073, 1000); }],
    ],
    [
      [300, () => { caption.innerHTML = T('A search hit is a <b>path and a line</b>…', '검색 결과는 <b>경로와 줄 번호</b>. 그 줄이…'); $('[data-scene="1"] [data-at="1"]').classList.add('reveal'); }],
      [2600, () => { caption.innerHTML = T('…that is the <b>same line</b> in <code>read</code>, which also returns the file\'s <b>rev</b>…', '…<code>read</code>에서도 <b>같은 줄</b>입니다. 파일 <b>rev</b>도 같이 받고요.'); $('[data-scene="1"] [data-at="2"]').classList.add('reveal'); }],
      [5200, () => { caption.innerHTML = T('…and <code>edit</code> changes exactly that, only if the file is <b>still at that rev</b>.', '<code>edit</code>는 파일이 <b>그 rev 그대로일 때만</b> 그 부분을 고칩니다.'); $('[data-scene="1"] [data-at="3"]').classList.add('reveal'); }],
    ],
    [
      [300, () => { caption.innerHTML = T('Two agents read the same file at the same <b>rev</b>.', '에이전트 둘이 같은 <b>rev</b>에서 파일을 읽습니다.'); step('#laneA', '1%', '', 'read · rev e70d'); step('#laneB', '1%', '', 'read · rev e70d'); }],
      [2200, () => { caption.innerHTML = T('Agent A writes first: the rev still matches, so the write lands.', 'A가 먼저 씁니다. rev가 그대로니까 반영.'); step('#laneA', '27%', 'ok', '✓ edit --rev e70d'); setRev('1486…'); }],
      [4300, () => { caption.innerHTML = T('Agent B\'s rev is now stale: <b>conflict, 0 bytes written</b>. Nothing is overwritten.', 'B의 rev는 이제 옛날 것. <b>conflict가 나고 한 바이트도 안 씁니다</b>.'); step('#laneB', '27%', 'no', '✗ conflict · 0 bytes'); }],
      [6400, () => { caption.innerHTML = T('B reads again, redoes its change on A\'s version, and lands it.', 'B는 다시 읽고, A가 고친 위에 자기 수정을 다시 얹어서 반영합니다.'); step('#laneB', '55%', '', 'read · rev 1486'); }],
      [7800, () => { step('#laneB', '77%', 'ok', '✓ edit --rev 1486'); setRev('9c02…'); $('[data-scene="2"] [data-at="5"]').classList.add('reveal'); caption.innerHTML = T('Measured with 4 agents and 800 updates each way:', '에이전트 4개로 업데이트 800번씩 돌려 본 결과:'); }],
    ],
  ];

  async function play(i) {
    const my = ++token;
    current = i;
    scenes.forEach((s, k) => s.classList.toggle('on', k === i));
    tabs.forEach((t, k) => { t.setAttribute('aria-selected', k === i); t.querySelector('.prog').style.transition = 'none'; t.querySelector('.prog').style.width = k < i ? '100%' : '0'; });
    resetScene(i);
    if (reduce || !running) { timelines[i].forEach(([, f]) => f()); tabs[i].querySelector('.prog').style.width = '100%'; return; }
    const prog = tabs[i].querySelector('.prog');
    requestAnimationFrame(() => { prog.style.transition = `width ${DUR[i]}ms linear`; prog.style.width = '100%'; });
    let t = 0;
    for (const [at, f] of timelines[i]) {
      await sleep(at - t); t = at;
      if (my !== token) return;
      f();
    }
    await sleep(DUR[i] - t);
    if (my === token && running) play((i + 1) % scenes.length);
  }
  tabs.forEach(t => t.addEventListener('click', () => play(+t.dataset.i)));
  const btn = $('#play');
  function setRunning(v) {
    running = v;
    btn.textContent = v ? T('❚❚ pause', '❚❚ 일시정지') : T('▶ play', '▶ 재생');
    btn.setAttribute('aria-label', v ? T('Pause animation', '애니메이션 일시정지') : T('Play animation', '애니메이션 재생'));
    play(current);
  }
  btn.addEventListener('click', () => setRunning(!running));
  if (reduce) { running = false; btn.textContent = T('▶ play', '▶ 재생'); }
  // Start the explainer when it scrolls into view.
  let started = false;
  new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting && !started) { started = true; play(0); } }), { threshold: 0.35 }).observe($('#explainer'));

  /* benchmark chart: numbers from README / bench/results/SUITE.md */
  const data = [
    ['Linux v6.12', 'C', 86680, '3.3 s · 134 MB', [5.8, 7.5], [13, 403], '4.0×'],
    ['LLVM 19.1', 'C++', 146319, '4.7 s · 122 MB', [6.1, 8.0], [34, 786], '2.3×'],
    ['rust 1.83', 'Rust', 49964, '1.2 s · 29 MB', [1.5, 9.6], [2, 273], '1.8×'],
    ['kubernetes 1.31', 'Go', 26232, '1.0 s · 30 MB', [3.5, 5.8], [9, 210], '1.9×'],
    ['VS Code 1.95', 'TypeScript', 7708, '0.4 s · 12 MB', [2.1, 3.9], [4, 63], '2.7×'],
    ['Django 5.1', 'Python', 6815, '0.2 s · 7 MB', [2.4, 2.9], [10, 60], '1.4×'],
    ['CPython 3.13', 'C / Python', 4882, '0.3 s · 10 MB', [3.0, 5.4], [5, 30], '2.6×'],
    ['Spring 6.1', 'Java', 10163, '0.3 s · 12 MB', [2.9, 5.3], [6, 95], '3.1×'],
  ];
  $('#bench-rows').innerHTML = data.map(d => `<tr><td>${d[0]} <span style="color:var(--text-3)">(${d[1]})</span></td><td class="num">${fmt(d[2])}</td><td class="num">${d[3]}</td><td class="num">${d[4][0]}–${d[4][1]}×</td><td class="num">${d[5][0]}–${d[5][1]}×</td><td class="num">${d[6]}</td></tr>`).join('');

  const tip = $('#tip');
  function drawChart() {
    const host = $('#bench-chart');
    const W = Math.max(320, host.clientWidth), narrow = W < 620;
    const L = narrow ? 112 : 170, R = 56, rowH = 46, top = 26;
    const H = top + data.length * rowH + 10;
    const lo = Math.log10(1), hi = Math.log10(1000);
    const x = v => L + (Math.log10(v) - lo) / (hi - lo) * (W - L - R);
    const ticks = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000].filter(t => !narrow || [1, 10, 100, 1000].includes(t));
    let s = `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}"><g class="grid">`;
    for (const t of ticks) s += `<line x1="${x(t)}" x2="${x(t)}" y1="${top - 8}" y2="${H - 6}"/><text x="${x(t)}" y="${top - 13}" text-anchor="middle">${t}×</text>`;
    s += `</g>`;
    data.forEach((d, i) => {
      const y = top + i * rowH;
      s += `<rect class="hitrow" data-i="${i}" x="0" y="${y}" width="${W}" height="${rowH}"/>`;
      s += `<text class="corpus" x="0" y="${y + 20}">${d[0]}</text><text class="sub" x="0" y="${y + 35}">${d[1]} · ${fmt(d[2])} ${T('files', '파일')}</text>`;
      const ya = y + 16, yt = y + 30;
      s += `<line class="bar" x1="${x(d[4][0])}" x2="${x(d[4][1])}" y1="${ya}" y2="${ya}" stroke="var(--series-1)"/>`;
      s += `<line class="bar" x1="${x(d[5][0])}" x2="${x(d[5][1])}" y1="${yt}" y2="${yt}" stroke="var(--series-2)"/>`;
      s += `<text class="val" x="${x(d[4][1]) + 8}" y="${ya + 4}">${d[4][1]}×</text>`;
      s += `<text class="val" x="${x(d[5][1]) + 8}" y="${yt + 4}">${d[5][1]}×</text>`;
    });
    s += `</svg>`;
    host.innerHTML = s;
    $$('.hitrow', host).forEach(r => {
      r.addEventListener('mousemove', e => {
        const d = data[+r.dataset.i];
        tip.innerHTML = `<b>${d[0]} <span style="color:var(--text-3);font-weight:400">${d[1]}</span></b>
          <div><span>${T('auto vs rg', 'auto 대 rg')}</span><span>${d[4][0]}–${d[4][1]}×</span></div>
          <div><span>${T('trust vs rg', 'trust 대 rg')}</span><span>${d[5][0]}–${d[5][1]}×</span></div>
          <div><span>${T('files', '파일 수')}</span><span>${fmt(d[2])}</span></div>
          <div><span>${T('index build · size', '인덱스 빌드 · 크기')}</span><span>${d[3]}</span></div>
          <div><span>${T('AST vs ast-grep', 'AST 대 ast-grep')}</span><span>${d[6]}</span></div>`;
        const box = $('.chart').getBoundingClientRect();
        let tx = e.clientX - box.left + 16, ty = e.clientY - box.top + 12;
        if (tx + 250 > box.width) tx = e.clientX - box.left - 256;
        tip.style.left = tx + 'px'; tip.style.top = ty + 'px'; tip.style.opacity = 1;
      });
      r.addEventListener('mouseleave', () => tip.style.opacity = 0);
    });
  }
  drawChart();
  let rz; addEventListener('resize', () => { clearTimeout(rz); rz = setTimeout(drawChart, 120); });
})();
