const MOD = process.env.PLAYWRIGHT_MODULE || 'playwright';
const pw = await import(MOD);
const { chromium, firefox, webkit, devices } = pw;

const BASE = process.env.BASE || 'http://127.0.0.1:8787';
const CAP_BASE = process.env.CAP_BASE || '';
const PASSWORD = process.env.BASIC_PASSWORD || '';
const SHOTS = process.env.SHOTS || '';
const ONLY = process.env.ONLY || '';
const ENGINES = (process.env.ENGINES || 'chromium,firefox,webkit').split(',');

const failures = [];
function check(ok, what) {
  console.log((ok ? 'ok   ' : 'FAIL ') + what);
  if (!ok) failures.push(what);
}

const PROFILES = {
  chromium: { name: 'Pixel 7', launch: chromium, options: devices['Pixel 7'] },
  webkit: { name: 'iPhone SE', launch: webkit, options: devices['iPhone SE'] },
  firefox: { name: 'Firefox 320', launch: firefox, options: { viewport: { width: 320, height: 568 }, hasTouch: false } },
};

async function enter(page, base) {
  await page.goto(base + '/');
  if (new URL(page.url()).pathname === '/gate') {
    await page.fill('#pw', PASSWORD);
    await Promise.all([page.waitForURL((u) => new URL(u).pathname !== '/gate'), page.click('.gate-submit')]);
  }
  await page.waitForFunction(() => window.GS && !document.documentElement.classList.contains('i18n-pending'));
}

async function solveGeo(page, program) {
  await page.click('#tab-geo');
  await page.fill('#geo-input', program);
  await page.click('#solve');
}

async function fits(page) {
  return page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.clientWidth, innerWidth]);
}

async function layoutChecks(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ ...profile.options, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    const tag = `${engine}/${profile.name}`;
    await enter(page, BASE);

    const long = 'Z'.repeat(150);
    await solveGeo(page, `A B C = triangle\nprove coll(A, B, ${long})`);
    await page.waitForSelector('#state-error:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(300);
    let [sw, cw, iw] = await fits(page);
    check(sw <= cw && iw <= cw + 1, `${tag}: a 150-character token in a .geo error does not widen the page (${sw} <= ${cw}, inner ${iw})`);
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/final-geoerr-${engine}.png` });

    const names = ['AAAAAAAAAAAAAAAA', 'BBBBBBBBBBBBBBBB', 'CCCCCCCCCCCCCCCC', 'HHHHHHHHHHHHHHHH'];
    await solveGeo(
      page,
      `${names[0]} ${names[1]} ${names[2]} = triangle\n${names[3]} = orthocenter(${names[0]}, ${names[1]}, ${names[2]})\nprove perp(${names[0]}, ${names[3]}, ${names[1]}, ${names[2]})`
    );
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(500);
    [sw, cw, iw] = await fits(page);
    check(sw <= cw, `${tag}: four 16-character point names keep GIVEN/PROVE inside the screen (${sw} <= ${cw})`);
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/final-longnames-${engine}.png` });

    const toLong = await page.evaluate(() => {
      const c = document.getElementById('geo-input');
      return c.value.length;
    });
    check(toLong > 0, `${tag}: the long-name program stayed in the editor`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function copyChecks(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 }, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    const tag = `${engine}`;
    await enter(page, BASE);
    await page.click('#tab-geo');
    await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(600);
    await page.waitForSelector('#human .hp-block, #human p', { timeout: 10000 });
    const selected = await page.evaluate(() => {
      const root = document.getElementById('human');
      const range = document.createRange();
      range.selectNodeContents(root);
      const sel = getSelection();
      sel.removeAllRanges();
      sel.addRange(range);
      return sel.toString();
    });
    const leaked = ['is perpendicular to', 'In the full derivation', 'directed angle', 'is parallel to'].filter((s) => selected.includes(s));
    check(selected.length > 40 && !leaked.length, `${tag}: selecting the proof copies the visible text only (${selected.length} chars, leaked: ${leaked.join('|') || 'none'})`);
    check(/[⟂∠∡]/.test(selected), `${tag}: the glyphs themselves are copied`);

    await page.click('#ptab-steps');
    await page.waitForTimeout(200);
    const printed = await page.evaluate(() => {
      const label = document.querySelector('.st-block > .label');
      return label ? getComputedStyle(label).breakAfter : 'none';
    });
    await page.emulateMedia({ media: 'print' });
    const printBreak = await page.evaluate(() => {
      const label = document.querySelector('.st-block > .label');
      const cs = label ? getComputedStyle(label) : null;
      return cs ? cs.breakAfter || cs.pageBreakAfter : 'none';
    });
    await page.emulateMedia({ media: 'screen' });
    check(['avoid', 'avoid-page'].includes(printBreak) || engine === 'firefox', `${tag}: print keeps the GIVEN/PROVE label with its first fact (screen ${printed}, print ${printBreak})`);

    const dl = page.waitForEvent('download', { timeout: 15000 });
    await page.click('#export-btn');
    await page.click('[data-export="svg"]');
    const download = await dl;
    const path = await download.path();
    const fs = await import('node:fs');
    const text = fs.readFileSync(path, 'utf8');
    const first = /^\s*<svg\b[^>]*>\s*<rect\b[^>]*fill="#ffffff"/.test(text);
    check(first, `${tag}: the exported SVG starts with a white background rect`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function gateTheme(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ viewport: { width: 390, height: 800 }, colorScheme: 'dark' });
    const page = await ctx.newPage();
    await page.goto(BASE + '/gate');
    const before = await page.evaluate(() => [document.documentElement.getAttribute('data-theme'), getComputedStyle(document.body).backgroundColor]);
    check(before[0] === null, `${engine}: with no stored theme the gate follows the OS (dark)`);
    const url = new URL(BASE);
    await ctx.addCookies([{ name: 'theme', value: 'light', domain: url.hostname, path: '/' }]);
    await page.goto(BASE + '/gate');
    const after = await page.evaluate(() => [document.documentElement.getAttribute('data-theme'), getComputedStyle(document.body).backgroundColor]);
    check(after[0] === 'light' && after[1] !== before[1], `${engine}: the theme cookie the app wrote makes the gate light on a dark OS (${before[1]} -> ${after[1]})`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function appWritesThemeCookie(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ viewport: { width: 1000, height: 800 }, colorScheme: 'dark', httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    await enter(page, BASE);
    for (let i = 0; i < 3; i++) {
      const mode = await page.evaluate(() => localStorage.getItem('theme'));
      if (mode === 'light') break;
      await page.click('[data-theme-toggle]');
      await page.waitForTimeout(100);
    }
    const cookies = await ctx.cookies();
    check(cookies.some((c) => c.name === 'theme' && c.value === 'light'), `${engine}: choosing Light in the app writes the theme cookie`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function dailyCap(engine, profile) {
  if (!CAP_BASE) return;
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    await enter(page, CAP_BASE);
    await page.click('#tab-describe');
    const text = 'Triangle ABC with AB = 5, AC = 4, BC = 6 and D on BC with BD = 2. Prove AD² = 14.';
    await page.fill('#describe-input', text);
    await page.click('#solve');
    await page.waitForSelector('#verdict:not([hidden]), #state-error:not([hidden])', { timeout: 30000 });
    await page.click('#tab-describe');
    await page.fill('#describe-input', text + ' ');
    await page.click('#solve');
    await page.waitForSelector('#state-error:not([hidden])', { timeout: 30000 });
    const err = await page.evaluate(() => ({
      title: document.getElementById('err-title').textContent,
      body: document.querySelector('#state-error .err-head p')?.textContent || '',
      retry: !!document.getElementById('err-retry'),
    }));
    check(!err.retry && !/minute/i.test(err.body) && /today|tomorrow/i.test(err.body), `${engine}: the daily AI cap says so, no wait-a-minute advice and no Try again (${err.title} / ${err.body})`);
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/final-cap-${engine}.png` });

    await page.click('#tab-geo');
    await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.click('#ptab-ai');
    await page.waitForFunction(() => document.getElementById('ai-text').textContent.trim().length > 0 && !document.querySelector('#ai-text .ai-loading'), null, { timeout: 30000 });
    const ai = await page.evaluate(() => ({ text: document.getElementById('ai-text').textContent, regen: !document.getElementById('ai-regen').hidden }));
    check(!ai.regen && /today|tomorrow|azi|mâine/i.test(ai.text), `${engine}: the AI tab shows the server's message and hides Regenerate (${ai.text.slice(0, 80)})`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}


function coarse(profile) {
  return !!profile.options.hasTouch;
}

async function phoneChecks(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ ...profile.options, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    const tag = `${engine}/${profile.name}`;
    await enter(page, BASE);
    await page.click('#tab-geo');
    await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForSelector('#human .hp-chain', { timeout: 10000 });
    await page.waitForTimeout(500);

    const chain = await page.evaluate(() => {
      const bad = [];
      let rows = 0;
      document.querySelectorAll('#human .hp-row').forEach((row) => {
        const eq = row.querySelector('.hp-eq'), nb = row.querySelector('.hp-rhs .m-nb');
        if (!eq || !nb || !eq.textContent.trim()) return;
        const tn = document.createTreeWalker(nb, NodeFilter.SHOW_TEXT).nextNode();
        const rg = document.createRange();
        rg.setStart(tn, 0);
        rg.setEnd(tn, 1);
        rows++;
        if (rg.getBoundingClientRect().left < eq.getBoundingClientRect().right - 0.5) bad.push(row.textContent.trim().slice(0, 20));
      });
      return { rows, bad };
    });
    check(chain.rows >= 3 && chain.bad.length === 0, `${tag}: no angle sign in a stacked chain starts left of its "=" (${chain.rows} rows, bad: ${chain.bad.join('|') || 'none'})`);

    const width = (profile.options.viewport || {}).width;
    const tabs = await page.evaluate(() => [...document.querySelectorAll('#proof-tabs > button')].map((b) => Math.round(b.getBoundingClientRect().top)));
    if (width < 400) check(tabs.length === 3 && new Set(tabs).size === 1, `${tag}: the three proof tabs stay on one row (${tabs.join(',')})`);

    const foot = await page.evaluate(() => [...document.querySelectorAll('.site-footer a, .site-footer button')].filter((e) => e.offsetParent).map((e) => Math.round(e.getBoundingClientRect().top)));
    check(foot.length < 2 || new Set(foot).size === 1, `${tag}: the footer links share a row (${foot.join(',')})`);

    await page.evaluate(() => scrollTo(0, 0));
    await page.click('#export-btn');
    await page.waitForTimeout(300);
    const menu = await page.evaluate(() => {
      const el = document.getElementById('export-menu'), m = el.getBoundingClientRect();
      const items = [...el.querySelectorAll('[role^=menuitem]')].map((i) => i.getBoundingClientRect());
      return { top: m.top, bottom: m.bottom, vh: innerHeight, scrollable: el.scrollHeight > el.clientHeight + 1, lastBottom: Math.max(...items.map((b) => b.bottom)) };
    });
    check(menu.top >= -1 && menu.bottom <= menu.vh + 1 && (menu.lastBottom <= menu.vh + 1 || menu.scrollable), `${tag}: the Export menu stays on screen (${Math.round(menu.top)}..${Math.round(menu.bottom)} of ${menu.vh})`);
    await page.keyboard.press('Escape');

    await solveGeo(page, 'A B C = triangle\nO = circumcenter(A, B, C)\nprove cyclic(A, B, C, Q)');
    await page.waitForSelector('#state-error:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(400);
    const mark = await page.evaluate(() => {
      const ta = document.getElementById('geo-input'), m = document.querySelector('#hl-code .tk-err');
      if (!m) return null;
      const t = ta.getBoundingClientRect(), b = m.getBoundingClientRect();
      return b.left >= t.left - 1 && b.right <= t.right + 1;
    });
    check(mark === true, `${tag}: the token a compile error marks is inside the editor`);
    if (coarse(profile)) {
      await page.evaluate(() => document.querySelector('[data-example-solve="imo2023"]') || 0);
      await page.click('#tab-geo');
      await page.click('#examples-btn');
      await page.click('[data-example="imo2023"]');
      await page.click('#solve');
      await page.waitForSelector('#verdict:not([hidden])', { timeout: 120000 });
      await page.waitForSelector('#human .hp-ref', { state: 'attached', timeout: 20000 });
      await page.waitForTimeout(500);
    const refs = await page.evaluate(() => {
      const el = [...document.querySelectorAll('#human .hp-ref')].find((e) => e.getBoundingClientRect().width > 0);
      if (!el) return null;
      el.scrollIntoView({ block: 'center' });
      const b = el.getBoundingClientRect();
      const hit = (x, y) => { const e = document.elementFromPoint(x, y); return e && (e === el || el.contains(e)); };
      const cy = b.top + b.height / 2;
      let lo = 0, hi = 0;
      while (lo < 30 && hit(b.left - lo - 1, cy)) lo++;
      while (hi < 30 && hit(b.right + hi + 1, cy)) hi++;
      return b.width + lo + hi;
    });
    check(refs != null && refs >= 44, `${tag}: a step reference "(n)" can be hit ${Math.round(refs)} px wide`);

    }
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function nameChecks(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ ...profile.options, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    const tag = `${engine}/${profile.name}`;
    await enter(page, BASE);
    await solveGeo(page, 'Alpha Beta Gamma = triangle\nHorth = orthocenter(Alpha, Beta, Gamma)\nprove perp(Alpha, Horth, Beta, Gamma)');
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(600);
    const r = await page.evaluate(() => ({
      labels: [...document.querySelectorAll('#fig-viewport .f-lbl')].map((e) => e.textContent),
      tspans: document.querySelectorAll('#fig-viewport .f-lbl tspan').length,
      goal: document.querySelector('.st-goal .goal').textContent,
      subs: document.querySelectorAll('#statement sub').length,
    }));
    check(r.labels.includes('Alpha') && r.labels.includes('Horth') && r.tspans === 0, `${tag}: long point names are drawn whole, with no subscript (${r.labels.join(',')})`);
    check(r.goal.includes('Alpha Horth') && r.goal.includes('Beta Gamma') && r.subs === 0, `${tag}: the statement keeps the two points of a segment apart (${JSON.stringify(r.goal)})`);

    const names = ['PointAlphaNumber', 'PointBravoNumber', 'PointCharlieNum1', 'PointDeltaNumber'];
    await solveGeo(page, `${names[0]} ${names[1]} ${names[2]} = triangle\n${names[3]} = orthocenter(${names[0]}, ${names[1]}, ${names[2]})\nprove perp(${names[0]}, ${names[3]}, ${names[1]}, ${names[2]})`);
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(600);
    const m = await page.evaluate(() => {
      const vp = document.getElementById('fig-viewport').getBoundingClientRect();
      let min = 1e9;
      document.querySelectorAll('#fig-viewport .f-lbl').forEach((e) => {
        const b = e.getBoundingClientRect();
        min = Math.min(min, b.left - vp.left, vp.right - b.right, b.top - vp.top, vp.bottom - b.bottom);
      });
      return min;
    });
    check(m >= -0.5, `${tag}: every label of four 16-character names lies inside the figure frame (smallest margin ${m.toFixed(1)} px)`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function deskChecks(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const auth = PASSWORD ? { username: '', password: PASSWORD } : undefined;
    let ctx = await browser.newContext({ viewport: { width: 1366, height: 768 }, httpCredentials: auth });
    let page = await ctx.newPage();
    await enter(page, BASE);
    await page.waitForTimeout(500);
    const chips = await page.evaluate(() => document.querySelector('#example-chips button').getBoundingClientRect().bottom);
    check(chips <= 768, `${engine}: at 1366x768 the example chips are on the first screen (bottom ${Math.round(chips)})`);
    await page.click('#tab-geo');
    await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.evaluate(() => scrollTo(0, 0));
    await page.click('#export-btn');
    await page.waitForTimeout(300);
    const menu = await page.evaluate(() => { const r = document.getElementById('export-menu').getBoundingClientRect(); return [r.top, r.bottom, innerHeight]; });
    check(menu[0] >= 0 && menu[1] <= menu[2] + 1, `${engine}: at 1366x768 the Export menu opens inside the screen (${Math.round(menu[0])}..${Math.round(menu[1])} of ${menu[2]})`);
    await page.keyboard.press('Escape');
    await page.click('#tab-geo');
    await page.click('#syntax-btn');
    await page.waitForTimeout(150);
    await page.keyboard.press('Escape');
    const syn = await page.evaluate(() => document.getElementById('syntax-help').hidden);
    check(syn, `${engine}: Escape closes the syntax panel`);
    const fav = await (await ctx.request.get(BASE + '/favicon.svg')).text();
    check(/prefers-color-scheme: dark/.test(fav), `${engine}: the favicon has a dark-tab colour`);
    await ctx.close();

    ctx = await browser.newContext({ viewport: { width: 820, height: 1180 }, httpCredentials: auth });
    page = await ctx.newPage();
    await enter(page, BASE);
    await page.click('#tab-geo');
    await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(500);
    const vp = await page.evaluate(() => document.getElementById('fig-viewport').getBoundingClientRect().height);
    check(vp <= 561, `${engine}: on a 820x1180 tablet the figure is at most 560 px tall (${Math.round(vp)})`);
    await page.click('#examples-btn');
    await page.click('[data-example="numeric"]');
    await page.click('#solve');
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(500);
    const numeric = await page.evaluate(() => [...document.querySelectorAll('#statement .facts li')].map((l) => l.textContent.trim()));
    check(numeric.length > 3 && !numeric.some((t) => /^(BD|CE|AF) = 2$/.test(t)), `${engine}: the one-seventh triangle states no length the program does not (${numeric.length} facts)`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function statusFailure(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ ...profile.options, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    await ctx.addInitScript(() => {
      const st = window.setTimeout;
      window.setTimeout = (f, ms, ...a) => st(f, Math.min(ms || 0, 30), ...a);
    });
    let dead = false;
    await ctx.route('**/api/status', (route) => (dead ? route.abort() : route.continue()));
    const page = await ctx.newPage();
    const tag = `${engine}/${profile.name}`;
    await enter(page, BASE);
    dead = true;
    await page.reload();
    await page.waitForFunction(() => window.GS && !document.documentElement.classList.contains('i18n-pending'));
    await page.waitForFunction(() => /Retry|Reîncearcă/.test(document.getElementById('ai-pill-m').textContent), null, { timeout: 15000 }).catch(() => {});
    const pill = await page.evaluate(() => document.getElementById('ai-pill-m').textContent);
    check(/Retry|Reîncearcă/.test(pill), `${tag}: a status call that never answers ends in "${pill}", not "Checking AI" forever`);
    await page.click('#tab-describe');
    const gate = await page.evaluate(() => { const g = document.querySelector('#panel-describe [data-gate]'); return { hidden: g.hidden, retry: !!g.querySelector('[data-retry-status]') }; });
    check(!gate.hidden && gate.retry, `${tag}: the Describe panel explains it and offers Retry`);
    dead = false;
    await page.click('#panel-describe [data-retry-status]');
    await page.waitForFunction(() => document.getElementById('ai-pill-m').classList.contains('on'), null, { timeout: 15000 }).catch(() => {});
    const ok = await page.evaluate(() => document.getElementById('ai-pill-m').className);
    check(/\bon\b/.test(ok), `${tag}: Retry restores the AI pill (${ok})`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

const TAB_WIDTHS = [280, 300, 320, 340, 360, 375, 390, 400, 412, 430, 480];
const TAB_SCALES = [100, 125, 150, 200];

async function proofTabSweep(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ ...profile.options, viewport: { width: 390, height: 800 }, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    await enter(page, BASE);
    await page.click('#tab-geo');
    await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForSelector('#proof-tabs:not([hidden])', { timeout: 10000 });
    await page.evaluate(() => {
      const st = document.createElement('style');
      st.id = 'sweep-scale';
      document.head.appendChild(st);
    });
    const bad = [];
    const tight = [];
    let combos = 0;
    for (const lang of ['en', 'ro']) {
      await page.evaluate((l) => document.querySelector(`[data-lang-set="${l}"]`).click(), lang);
      await page.waitForTimeout(250);
      for (const scale of TAB_SCALES) {
        await page.evaluate((s) => { document.getElementById('sweep-scale').textContent = `html{font-size:${s}% !important}`; }, scale);
        for (const width of TAB_WIDTHS) {
          await page.setViewportSize({ width, height: 800 });
          await page.waitForTimeout(80);
          for (const which of ['human', 'steps', 'ai']) {
            await page.evaluate((w) => document.getElementById('ptab-' + w).click(), which);
            await page.waitForTimeout(40);
            combos++;
            const m = await page.evaluate((want) => {
              const tabs = document.getElementById('proof-tabs');
              const tr = tabs.getBoundingClientRect();
              const bs = [...tabs.querySelectorAll('button')].filter((b) => !b.hidden);
              const rects = bs.map((b) => b.getBoundingClientRect());
              const tops = new Set(rects.map((r) => Math.round(r.top)));
              const inside = rects.every((r) => r.left >= tr.left - 0.5 && r.right <= tr.right + 0.5);
              const noClip = bs.every((b) => {
                const rg = document.createRange();
                rg.selectNodeContents(b);
                const tx = rg.getBoundingClientRect(), bx = b.getBoundingClientRect();
                return b.scrollWidth <= b.clientWidth + 1 && tx.left >= bx.left - 1 && tx.right <= bx.right + 1;
              });
              const splitWord = bs.some((b) => {
                const walk = (n) => {
                  if (n.nodeType === 1) return !n.classList.contains('sr-only') && [...n.childNodes].some(walk);
                  if (n.nodeType !== 3) return false;
                  const re = /\S+/g;
                  let w;
                  while ((w = re.exec(n.nodeValue))) {
                    const r = document.createRange();
                    r.setStart(n, w.index);
                    r.setEnd(n, w.index + w[0].length);
                    const tops = [...r.getClientRects()].filter((x) => x.width > 0.5).map((x) => Math.round(x.top));
                    if (tops.length && Math.max(...tops) - Math.min(...tops) > 2) return true;
                  }
                  return false;
                };
                return walk(b);
              });
              const vw = Math.min(want, document.documentElement.clientWidth);
              return {
                splitWord,
                page: document.documentElement.scrollWidth <= vw && document.body.scrollWidth <= vw,
                onscreen: rects.every((r) => r.left >= -0.5 && r.right <= vw + 0.5),
                inside, noClip,
                rows: tops.size, n: bs.length,
                h: Math.round(tr.height),
              };
            }, width);
            const tag = `${lang} ${width}px ${scale}% [${which}]`;
            const why = [m.splitWord && 'word-split', !m.page && 'page-scroll', !m.onscreen && 'off-screen', !m.inside && 'outside-track', !m.noClip && 'clipped', m.rows !== 1 && m.rows !== m.n && 'orphan'].filter(Boolean);
            if (why.length) bad.push(`${tag} ${why.join('+')} (rows ${m.rows}/${m.n})`);
            if (width >= 400 && scale === 100 && m.rows !== 1) tight.push(`${tag} rows ${m.rows}`);
          }
        }
        if (SHOTS && (scale === 150 || scale === 200)) {
          await page.setViewportSize({ width: 320, height: 700 });
          await page.waitForTimeout(100);
          await page.locator('#proof-tabs').scrollIntoViewIfNeeded();
          await page.screenshot({ path: `${SHOTS}/tabs-${engine}-${lang}-320-${scale}.png` });
        }
      }
    }
    check(!bad.length, `${engine}: the proof tabs never overflow, clip, orphan or break a word (${combos} combinations of 280..480 px x 100-200% x EN/RO x 3 tabs)${bad.length ? `: ${bad.length} bad, e.g. ` + bad.filter((x, i) => i % 3 === 0).slice(0, 12).join(' ; ') : ''}`);
    check(!tight.length, `${engine}: from 400 px at normal text size the proof tabs share one row${tight.length ? ': ' + tight.slice(0, 6).join(' ; ') : ''}`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

const AS_DRAWN = 'A B C = triangle\nP = point: angle(P, B, A) = angle(P, C, A)\nprove cyclic(A, P, B, C)';

async function lineBoxes(page, selector) {
  return page.evaluate((sel) => {
    const out = [];
    document.querySelectorAll(sel).forEach((nb) => {
      const rs = [];
      const tw = document.createTreeWalker(nb, NodeFilter.SHOW_TEXT);
      let n;
      while ((n = tw.nextNode())) {
        if (n.parentElement.closest('.sr-only')) continue;
        const rg = document.createRange();
        rg.selectNodeContents(n);
        [...rg.getClientRects()].filter((r) => r.width > 0.5).forEach((r) => rs.push(r));
      }
      const lines = rs.some((a) => rs.some((c) => c.top >= a.bottom - 1));
      if (lines) out.push(nb.textContent.trim().slice(0, 30));
    });
    return out;
  }, selector);
}

async function atomChecks(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const auth = PASSWORD ? { username: '', password: PASSWORD } : undefined;
    for (const [w, h] of [[1280, 720], [820, 1180], [640, 900]]) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, httpCredentials: auth });
      const page = await ctx.newPage();
      await enter(page, BASE);
      await page.click('#tab-geo');
      await page.click('#examples-btn');
      await page.click('[data-example="imo2023"]');
      await page.click('#solve');
      await page.waitForSelector('#verdict:not([hidden])', { timeout: 120000 });
      await page.waitForSelector('#human .hp-chain', { timeout: 20000 });
      for (const lang of ['en', 'ro']) {
        await page.evaluate((l) => document.querySelector(`[data-lang-set="${l}"]`).click(), lang);
        await page.waitForTimeout(500);
        const split = await lineBoxes(page, '#human .m-nb');
        check(split.length === 0, `${engine}: ${w}x${h} ${lang}: no angle name in the proof is split across lines (${split.length ? split.slice(0, 4).join(' | ') : 'none'})`);
        const rows = await page.evaluate(() => {
          const lhs = [...document.querySelectorAll('#human .hp-lhs')].filter((e) => e.textContent.trim());
          return lhs.filter((e) => e.getClientRects().length && e.getBoundingClientRect().height > 1.9 * parseFloat(getComputedStyle(e).lineHeight)).map((e) => e.textContent.trim().slice(0, 20));
        });
        check(rows.length === 0, `${engine}: ${w}x${h} ${lang}: no left-hand side of a chain wraps (${rows.join(' | ') || 'none'})`);
      }
      await ctx.close();
    }
  } finally {
    await browser.close();
  }
}

async function figureChecks(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const auth = PASSWORD ? { username: '', password: PASSWORD } : undefined;
    const measure = (page) => page.evaluate(() => {
      const vp = document.getElementById('fig-viewport').getBoundingClientRect();
      const ls = [...document.querySelectorAll('#fig-viewport .f-lbl')].map((e) => ({ t: e.textContent, b: e.getBoundingClientRect() }));
      const over = [];
      for (let i = 0; i < ls.length; i++) for (let j = i + 1; j < ls.length; j++) {
        const a = ls[i].b, c = ls[j].b;
        const ox = Math.min(a.right, c.right) - Math.max(a.left, c.left), oy = Math.min(a.bottom, c.bottom) - Math.max(a.top, c.top);
        if (ox > 0 && oy > 0 && ox * oy > 0.2 * Math.min(a.width * a.height, c.width * c.height)) over.push(ls[i].t + '/' + ls[j].t);
      }
      const margin = ls.reduce((m, x) => Math.min(m, x.b.left - vp.left, vp.right - x.b.right, x.b.top - vp.top, vp.bottom - x.b.bottom), 1e9);
      return { over, margin, n: ls.length };
    });
    let ctx = await browser.newContext({ viewport: { width: 568, height: 320 }, hasTouch: true, httpCredentials: auth });
    let page = await ctx.newPage();
    await enter(page, BASE);
    await page.click('#tab-geo');
    await page.click('#examples-btn');
    await page.click('[data-example="imo2023"]');
    await page.click('#solve');
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 120000 });
    await page.waitForTimeout(900);
    let m = await measure(page);
    check(m.n > 10 && m.over.length === 0, `${engine}: 568x320 landscape, IMO 2023: no two figure labels overlap (${m.over.join(' ') || 'none'})`);
    if (SHOTS) {
      await page.locator('#fig-viewport').scrollIntoViewIfNeeded();
      await page.locator('#fig-viewport').screenshot({ path: `${SHOTS}/final-imo-landscape-${engine}.png` });
    }
    await ctx.close();

    ctx = await browser.newContext({ ...profile.options, httpCredentials: auth });
    page = await ctx.newPage();
    await enter(page, BASE);
    const names = ['PointAlpha01', 'PointBravo02', 'PointCharl03', 'PointDelta04'];
    await solveGeo(page, `${names[0]} ${names[1]} ${names[2]} = triangle\n${names[3]} = orthocenter(${names[0]}, ${names[1]}, ${names[2]})\nprove perp(${names[0]}, ${names[3]}, ${names[1]}, ${names[2]})`);
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(900);
    m = await measure(page);
    check(m.margin >= 4, `${engine}/${profile.name}: 12-character names keep at least 4 px between every label and the figure frame (${m.margin.toFixed(1)} px)`);
    const wide = ['WWWWWWWWWWWW', 'MMMMMMMMMMMM', 'QQQQQQQQQQQQ', 'HHHHHHHHHHHH'];
    await solveGeo(page, `${wide[0]} ${wide[1]} ${wide[2]} = triangle\n${wide[3]} = orthocenter(${wide[0]}, ${wide[1]}, ${wide[2]})\nprove perp(${wide[0]}, ${wide[3]}, ${wide[1]}, ${wide[2]})`);
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(900);
    m = await measure(page);
    check(m.margin >= 4, `${engine}/${profile.name}: the widest 12-character names keep at least 4 px to the frame (${m.margin.toFixed(1)} px)`);
    await ctx.close();

    ctx = await browser.newContext({ viewport: { width: 1280, height: 720 }, httpCredentials: auth });
    page = await ctx.newPage();
    await enter(page, BASE);
    for (const tab of ['describe', 'photo', 'geo']) {
      await page.click(`#tab-${tab}`);
      await page.waitForTimeout(150);
      const bottom = await page.evaluate(() => Math.max(...[...document.querySelectorAll('#example-chips > *')].map((c) => c.getBoundingClientRect().bottom)));
      check(bottom <= 716, `${engine}: at 1280x720 the example chips end inside the screen on the ${tab} tab (bottom ${Math.round(bottom)} of 720)`);
    }
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function verdictChecks(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ ...profile.options, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    const tag = `${engine}/${profile.name}`;
    await enter(page, BASE);
    await solveGeo(page, AS_DRAWN);
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 60000 });
    await page.waitForTimeout(500);
    for (const lang of ['en', 'ro']) {
      await page.evaluate((l) => document.querySelector(`[data-lang-set="${l}"]`).click(), lang);
      await page.waitForTimeout(400);
      const r = await page.evaluate(() => {
        const note = document.querySelector('#verdict .verdict-meta > .meta-note');
        if (!note) return null;
        const q = note.getBoundingClientRect(), v = document.getElementById('verdict').getBoundingClientRect();
        const prev = note.previousElementSibling ? note.previousElementSibling.getBoundingClientRect() : null;
        const lh = parseFloat(getComputedStyle(note).lineHeight) || q.height;
        return { left: q.left - v.left, width: q.width, vwidth: v.width, lines: Math.round(q.height / lh), below: prev ? q.top >= prev.bottom - 1 : true, text: note.textContent };
      });
      const phone = (profile.options.viewport || {}).width < 600;
      check(r != null && r.below && (!phone || (r.left <= 24 && r.width >= r.vwidth - 56)), `${tag} ${lang}: the "as drawn" sentence sits on its own line across the card (${r ? `left ${Math.round(r.left)}, width ${Math.round(r.width)} of ${Math.round(r.vwidth)}, ${r.lines} line(s): ${r.text}` : 'missing'})`);
      check(r != null && r.lines <= 2, `${tag} ${lang}: the "as drawn" sentence takes at most two lines (${r ? r.lines : 'missing'})`);
    }
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function peekChecks(engine, profile) {
  if (!coarse(profile)) return;
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ ...profile.options, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    const tag = `${engine}/${profile.name}`;
    await enter(page, BASE);
    await page.click('#tab-geo');
    await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.click('#ptab-steps');
    await page.waitForTimeout(300);
    const steps = page.locator('#steps .step');
    const last = steps.nth((await steps.count()) - 1);
    await last.scrollIntoViewIfNeeded();
    await page.evaluate(() => scrollBy(0, 40));
    await page.waitForTimeout(300);
    const box = await last.boundingBox();
    await page.touchscreen.tap(box.x + 30, Math.min(box.y + box.height / 2, 600));
    await page.waitForTimeout(500);
    const shown = await page.evaluate(() => { const p = document.getElementById('fig-peek'); return !!p && !p.hidden; });
    check(shown, `${tag}: tapping a step far below the figure shows the step peek`);
    await page.evaluate(() => scrollTo(0, 0));
    await page.waitForTimeout(600);
    const gone = await page.evaluate(() => { const p = document.getElementById('fig-peek'); return !p || p.hidden; });
    const solve = await page.evaluate(() => { const b = document.getElementById('solve').getBoundingClientRect(); return b.top >= 0 && b.bottom <= innerHeight; });
    check(gone, `${tag}: scrolling back up to the Solve button and the verdict dismisses the peek (Solve on screen: ${solve})`);
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/final-peek-${engine}.png` });
    await ctx.close();
  } finally {
    await browser.close();
  }
}


const CHAIN_WIDTHS = [280, 320, 360, 390, 430, 600, 680, 768, 820, 1024, 1280];
const CHAIN_SCALES = [100, 125, 150, 175, 200];

async function chainOverflowChecks(engine, profile) {
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ viewport: { width: 600, height: 800 }, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    await enter(page, BASE);
    await page.click('#tab-geo');
    await page.click('#examples-btn');
    await page.click('[data-example="imo2023"]');
    await page.click('#solve');
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 120000 });
    await page.waitForSelector('#human .hp-chain', { timeout: 20000 });
    await page.evaluate(() => {
      const st = document.createElement('style');
      st.id = 'sweep-scale';
      document.head.appendChild(st);
    });
    const bad = [];
    let combos = 0;
    let escaped = 0;
    for (const lang of ['en', 'ro']) {
      await page.evaluate((l) => document.querySelector(`[data-lang-set="${l}"]`).click(), lang);
      await page.waitForTimeout(250);
      for (const scale of CHAIN_SCALES) {
        await page.evaluate((x) => { document.getElementById('sweep-scale').textContent = `html{font-size:${x}% !important}`; }, scale);
        for (const width of CHAIN_WIDTHS) {
          await page.setViewportSize({ width, height: 800 });
          await page.waitForTimeout(80);
          for (const state of ['proof', 'export', 'full']) {
            if (state === 'proof') await page.evaluate(() => document.getElementById('ptab-human').click());
            if (state === 'export') await page.evaluate(() => document.getElementById('export-btn').click());
            if (state === 'full') await page.evaluate(() => document.getElementById('z-full').click());
            await page.waitForTimeout(state === 'proof' ? 40 : 120);
            const m = await page.evaluate(() => {
              let leak = 0;
              document.querySelectorAll('#human .sr-only').forEach((e) => {
                if (e.closest('[hidden]') || !e.getClientRects().length) return;
                const chain = e.closest('.hp-chain');
                if (chain && getComputedStyle(chain).overflowX !== 'visible' && e.offsetParent !== chain) leak++;
              });
              return { sw: document.documentElement.scrollWidth, bw: document.body.scrollWidth, vw: innerWidth, leak };
            });
            combos++;
            escaped += m.leak;
            if (Math.max(m.sw, m.bw) > m.vw) bad.push(`${lang} ${width}px ${scale}% ${state}: ${Math.max(m.sw, m.bw)} > ${m.vw}`);
            if (state === 'export') await page.keyboard.press('Escape');
            if (state === 'full' && (await page.evaluate(() => document.getElementById('fig-frame').classList.contains('is-full')))) await page.evaluate(() => document.getElementById('z-full').click());
          }
        }
      }
    }
    check(!bad.length, `${engine}: the page never scrolls sideways on the Proof tab, with Export open or with the figure full screen (${combos} combinations of ${CHAIN_WIDTHS[0]}..${CHAIN_WIDTHS[CHAIN_WIDTHS.length - 1]} px x 100-200% x EN/RO)${bad.length ? `: ${bad.length} bad, e.g. ` + bad.slice(0, 8).join(' ; ') : ''}`);
    check(escaped === 0, `${engine}: no screen-reader span inside a scrolling derivation chain has a containing block outside it (${escaped} escaped)`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function refTapChecks(engine, profile) {
  if (!coarse(profile)) return;
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ ...profile.options, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    const tag = `${engine}/${profile.name}`;
    await enter(page, BASE);
    await page.evaluate(() => {
      const st = document.createElement('style');
      st.id = 'sweep-scale';
      document.head.appendChild(st);
    });
    const boxes = () => page.evaluate(() => {
      const out = [];
      document.querySelectorAll('#human .hp-ref').forEach((e, i) => {
        if (!e.getClientRects().length || e.closest('[hidden]')) return;
        const b = e.getBoundingClientRect();
        const cs = getComputedStyle(e, '::after');
        const l = parseFloat(cs.left) || 0, r = parseFloat(cs.right) || 0;
        out.push({ i, k: e.getAttribute('data-ref'), l: b.left + l, r: b.right - r, t: b.top, b: b.bottom, tl: b.left, tr: b.right, pt: parseFloat(getComputedStyle(e).paddingTop) || 0 });
      });
      return out;
    });
    let same = 0, taps = 0, contested = 0;
    const sameBad = [], wrong = [], widths = new Set();
    for (const ex of ['imo2019', 'imo2023']) {
      await page.setViewportSize({ width: 390, height: 800 });
      await page.click('#tab-geo');
      await page.click('#examples-btn');
      await page.click(`[data-example="${ex}"]`);
      await page.click('#solve');
      await page.waitForSelector('#verdict:not([hidden])', { timeout: 120000 });
      await page.waitForSelector('#human .hp-ref', { state: 'attached', timeout: 20000 });
      await page.waitForTimeout(400);
      for (const scale of [100, 125, 150, 200]) {
        await page.evaluate((x) => { document.getElementById('sweep-scale').textContent = `html{font-size:${x}% !important}`; }, scale);
        for (const width of [320, 340, 360, 375, 390, 412, 430]) {
          await page.setViewportSize({ width, height: 800 });
          await page.waitForTimeout(80);
          const bx = await boxes();
          bx.forEach((a) => widths.add(Math.round(a.r - a.l)));
          for (let i = 0; i < bx.length; i++) for (let j = i + 1; j < bx.length; j++) {
            const a = bx[i], c = bx[j];
            const ox = Math.min(a.r, c.r) - Math.max(a.l, c.l);
            if (Math.abs(a.t - c.t) <= 3) {
              same++;
              if (ox > 0.5) sameBad.push(`${ex} ${width}px ${scale}% (${a.k},${c.k}) ${ox.toFixed(1)} px`);
              continue;
            }
            let first = a.t < c.t ? a : c, second = first === a ? c : a;
            if (ox <= 2 || second.t >= first.b - 2) continue;
            await page.evaluate((n) => document.querySelectorAll('#human .hp-ref')[n].scrollIntoView({ block: 'center' }), first.i);
            await page.waitForTimeout(60);
            const again = await boxes();
            first = again.find((q) => q.i === first.i);
            second = again.find((q) => q.i === second.i);
            if (!first || !second) continue;
            const x = Math.min(Math.max((first.tl + first.tr) / 2, Math.max(first.l, second.l) + 1), Math.min(first.r, second.r) - 1);
            const y = (first.b + second.t) / 2 - 1;
            const gap = (q) => {
              const dx = Math.max(q.tl - x, 0, x - q.tr), dy = Math.max(q.t + q.pt - y, 0, y - (q.b - q.pt));
              return dx * dx + dy * dy;
            };
            const d1 = gap(first), d2 = gap(second);
            if (Math.abs(d1 - d2) < 2) continue;
            const near = d1 < d2 ? first : second;
            await page.evaluate(() => { if (document.activeElement && document.activeElement.blur) document.activeElement.blur(); });
            const top = await page.evaluate(([px, py]) => { const e = document.elementFromPoint(px, py); const r = e && e.closest('.hp-ref'); return r ? r.getAttribute('data-ref') : null; }, [x, y]);
            if (top !== null && top !== near.k) contested++;
            await page.mouse.click(x, y);
            taps++;
            const got = await page.evaluate(() => (document.activeElement && document.activeElement.id) || '');
            if (got !== 'hp-step-' + near.k) wrong.push(`${ex} ${width}px ${scale}% tap nearer (${near.k}) gave ${got || 'nothing'}`);
          }
        }
      }
    }
    check(same > 0 && sameBad.length === 0, `${tag}: step references on one line never share tap area (${same} pairs, boxes ${[...widths].sort((p, q) => p - q).join('/')} px${sameBad.length ? '; ' + sameBad.slice(0, 5).join(' ; ') : ''})`);
    check(taps > 0 && wrong.length === 0, `${tag}: a tap where the boxes of two references on adjacent lines meet goes to the nearer one (${taps} taps, ${contested} of them under the other reference's box${wrong.length ? '; ' + wrong.slice(0, 4).join(' ; ') : ''})`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

async function targetChecks(engine, profile) {
  if (!coarse(profile)) return;
  const browser = await profile.launch.launch();
  try {
    const ctx = await browser.newContext({ ...profile.options, httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
    const page = await ctx.newPage();
    const tag = `${engine}/${profile.name}`;
    await enter(page, BASE);
    await page.click('#tab-geo');
    await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
    await page.waitForTimeout(500);
    const small = await page.evaluate(() => {
      const bad = [];
      const nm = (e) => (e.id ? '#' + e.id : e.className ? '.' + String(e.className).split(' ')[0] : e.tagName) + '"' + e.textContent.trim().slice(0, 14) + '"';
      document.querySelectorAll('.site-header .seg > button, .site-header [data-theme-toggle], #proof-tabs > button, .tabs > button, .facts li, .goal').forEach((e) => {
        if (!e.getClientRects().length || e.closest('[hidden]')) return;
        const r = e.getBoundingClientRect();
        if (r.height < 43.5 || r.width < 43.5) bad.push(`${nm(e)} ${Math.round(r.width)}x${Math.round(r.height)}`);
      });
      return bad;
    });
    check(small.length === 0, `${tag}: header buttons, tabs and fact rows are at least 44 px each way (${small.slice(0, 5).join(', ') || 'all'})`);
    await ctx.close();
  } finally {
    await browser.close();
  }
}

for (const engine of ENGINES) {
  const profile = PROFILES[engine];
  if (!profile) continue;
  if (ONLY && ONLY !== 'tabs' && ONLY !== 'fixes' && ONLY !== 'chain') continue;
  if (ONLY === 'chain') { await refTapChecks(engine, profile); await chainOverflowChecks(engine, profile); continue; }
  if (ONLY === 'tabs') { await proofTabSweep(engine, profile); continue; }
  if (ONLY === 'fixes') { await atomChecks(engine, profile); await figureChecks(engine, profile); await verdictChecks(engine, profile); await peekChecks(engine, profile); await targetChecks(engine, profile); await refTapChecks(engine, profile); await chainOverflowChecks(engine, profile); continue; }
  await layoutChecks(engine, profile);
  await copyChecks(engine, profile);
  await gateTheme(engine, profile);
  await appWritesThemeCookie(engine, profile);
  await dailyCap(engine, profile);
  await phoneChecks(engine, profile);
  await nameChecks(engine, profile);
  await deskChecks(engine, profile);
  await statusFailure(engine, profile);
  await proofTabSweep(engine, profile);
  await atomChecks(engine, profile);
  await figureChecks(engine, profile);
  await verdictChecks(engine, profile);
  await peekChecks(engine, profile);
  await targetChecks(engine, profile);
  await refTapChecks(engine, profile);
  await chainOverflowChecks(engine, profile);
}

console.log(failures.length ? `\n${failures.length} FAILED` : '\nall ok');
process.exit(failures.length ? 1 : 0);
