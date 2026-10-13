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

for (const engine of ENGINES) {
  const profile = PROFILES[engine];
  if (!profile) continue;
  await layoutChecks(engine, profile);
  await copyChecks(engine, profile);
  await gateTheme(engine, profile);
  await appWritesThemeCookie(engine, profile);
  await dailyCap(engine, profile);
}

console.log(failures.length ? `\n${failures.length} FAILED` : '\nall ok');
process.exit(failures.length ? 1 : 0);
