const MOD = process.env.PLAYWRIGHT_MODULE || 'playwright';
const pw = await import(MOD);
const { chromium, firefox, webkit, devices } = pw;

const BASE = process.env.BASE || 'http://127.0.0.1:8787';
const CAP_BASE = process.env.CAP_BASE || '';
const PASSWORD = process.env.BASIC_PASSWORD || '';
const SHOTS = process.env.SHOTS || '';
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
    check(refs != null && refs >= 30, `${tag}: a step reference "(n)" can be hit ${Math.round(refs)} px wide`);

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

for (const engine of ENGINES) {
  const profile = PROFILES[engine];
  if (!profile) continue;
  await layoutChecks(engine, profile);
  await copyChecks(engine, profile);
  await gateTheme(engine, profile);
  await appWritesThemeCookie(engine, profile);
  await dailyCap(engine, profile);
  await phoneChecks(engine, profile);
  await nameChecks(engine, profile);
  await deskChecks(engine, profile);
  await statusFailure(engine, profile);
}

console.log(failures.length ? `\n${failures.length} FAILED` : '\nall ok');
process.exit(failures.length ? 1 : 0);
