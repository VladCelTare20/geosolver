import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawn } from 'node:child_process';

const pw = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { chromium, firefox, webkit } = pw;
const GUEST = process.env.GUEST_BASE || 'http://127.0.0.1:8787';
const ACCT = process.env.ACCOUNTS_BASE || '';
const PW = process.env.BASIC_PASSWORD || '';
const BIN = process.env.AGSTUDIO_BIN || '';
const OUT = process.env.SHOTS || fs.mkdtempSync(path.join(os.tmpdir(), 'compat-'));
const ONLY = (process.env.ONLY || '').split(',').filter(Boolean);
fs.mkdirSync(OUT, { recursive: true });

let fails = 0;
const check = (ok, what) => { console.log(`${ok ? 'PASS' : 'FAIL'} ${what}`); if (!ok) fails++; };
const want = (name) => !ONLY.length || ONLY.includes(name);
const shot = (page, name, full = false) => page.screenshot({ path: `${OUT}/${name}.png`, fullPage: full, animations: 'disabled' });
const visible = (page, sel) => page.evaluate((s) => { const e = document.querySelector(s); if (!e) return false; const r = e.getBoundingClientRect(); return getComputedStyle(e).display !== 'none' && r.width > 0 && r.height > 0; }, sel);
const shownText = (page, sel) => page.evaluate((s) => document.querySelector(s).innerText.trim(), sel);

async function passGate(page, base, target = '/') {
  await page.goto(`${base}${target}`);
  if (new URL(page.url()).pathname !== '/gate') return;
  await page.fill('#pw', PW);
  await Promise.all([page.waitForURL((u) => new URL(u).pathname !== '/gate'), page.click('.gate-submit')]);
}

function watchErrors(page) {
  const errs = [];
  page.on('pageerror', (e) => errs.push(`pageerror: ${e.message}`));
  return errs;
}

async function solveOrtho(page) {
  await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
}

async function waitVerdict(page, timeout = 60000) {
  await page.waitForSelector('#verdict:not([hidden])', { timeout });
}

const chrome = await chromium.launch();

if (want('guard')) {
  for (const [locale, word] of [['en-US', 'too old'], ['ro-RO', 'prea vechi']]) {
    const ctx = await chrome.newContext({ locale, viewport: { width: 390, height: 844 } });
    await ctx.addInitScript(() => { delete window.ResizeObserver; });
    const page = await ctx.newPage();
    await passGate(page, GUEST);
    await page.waitForLoadState('load');
    const note = await visible(page, '#gs-old');
    const shell = await visible(page, '.site-header');
    const text = note ? await shownText(page, '#gs-old') : '';
    check(note && !shell && text.includes(word), `guard (${locale}): a browser without ResizeObserver gets the update notice and no app ("${text.split('\n')[0]}")`);
    await shot(page, `guard-old-${locale}`);
    await ctx.close();
  }
  const ctx = await chrome.newContext({ viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  await passGate(page, GUEST);
  await page.waitForLoadState('load');
  check(!(await visible(page, '#gs-old')) && !(await visible(page, '#gs-fail')) && await visible(page, '.site-header'), 'guard: a current browser sees no notice');
  await ctx.close();
}

if (want('bootfail')) {
  const ctx = await chrome.newContext({ viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  await passGate(page, GUEST);
  await page.route(/\/assets\/app\.js/, (r) => r.abort());
  await page.goto(`${GUEST}/`);
  await page.waitForLoadState('load');
  await page.waitForTimeout(200);
  const note = await visible(page, '#gs-fail');
  check(note && await visible(page, '.site-header'), 'boot failure: app.js not loading shows the reload banner over the page');
  await shot(page, 'bootfail');
  await ctx.close();
}

if (want('nojs')) {
  const ctx = await chrome.newContext({ javaScriptEnabled: false, viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  await passGate(page, GUEST);
  check(new URL(page.url()).pathname === '/', `no JS: the password page works without JavaScript (${page.url()})`);
  const note = await page.evaluate(() => document.body.innerText);
  const shell = await visible(page, '.site-header');
  check(/needs JavaScript/.test(note) && /nevoie de JavaScript/.test(note) && !shell, 'no JS: the app shows the EN + RO JavaScript note instead of a dead shell');
  await shot(page, 'nojs-app');
  if (ACCT) {
    await passGate(page, ACCT);
    const landing = await page.evaluate(() => document.body.innerText);
    check(/needs JavaScript/.test(landing) && /GeoSolver/.test(landing), 'no JS: the landing page keeps its content under the note');
    await shot(page, 'nojs-landing');
    await page.goto(`${ACCT}/auth`);
    check(/needs JavaScript/.test(await page.evaluate(() => document.body.innerText)) && !(await visible(page, '#form')), 'no JS: the sign-in page explains instead of showing a form that cannot submit');
    await shot(page, 'nojs-auth');
  }
  await ctx.close();
}

if (want('storage')) {
  const ctx = await chrome.newContext({ viewport: { width: 390, height: 844 } });
  await ctx.addInitScript(() => {
    const deny = () => { throw new DOMException('The operation is insecure.', 'SecurityError'); };
    Object.defineProperty(window, 'localStorage', { get: deny, configurable: true });
    Object.defineProperty(window, 'sessionStorage', { get: deny, configurable: true });
    Object.defineProperty(Document.prototype, 'cookie', { get: deny, set: deny, configurable: true });
  });
  const page = await ctx.newPage();
  const errs = watchErrors(page);
  await passGate(page, GUEST);
  await page.waitForFunction(() => window.GS && window.GS.booted);
  await solveOrtho(page);
  await waitVerdict(page);
  await page.click('[data-lang-set="ro"]');
  await page.waitForTimeout(300);
  const ro = await page.evaluate(() => document.documentElement.lang + ' ' + document.querySelector('#solve').innerText);
  check(ro.startsWith('ro'), `storage blocked: switching to Romanian still works in this page (${ro})`);
  await page.click('[data-lang-set="en"]');
  await page.click('[data-theme-toggle]');
  await page.waitForTimeout(200);
  check(errs.length === 0, `storage blocked: boot, solve, language and theme menu raise no errors (${errs.join('; ')})`);
  await shot(page, 'storage-blocked');
  await ctx.close();
}

if (want('cookies')) {
  const fx = await firefox.launch({ firefoxUserPrefs: { 'network.cookie.cookieBehavior': 2 } });
  const ctx = await fx.newContext({ viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  await page.goto(`${GUEST}/`);
  await page.fill('#pw', PW);
  await Promise.all([page.waitForLoadState('load'), page.click('.gate-submit')]);
  await page.waitForTimeout(300);
  const url = new URL(page.url());
  const msg = await page.evaluate(() => (document.getElementById('pw-err') || {}).textContent || '');
  check(url.pathname === '/gate' && url.searchParams.get('cookies') === '0' && /cookie/.test(msg), `cookies blocked (Firefox): the gate says why it came back (${url.pathname}${url.search}: "${msg}")`);
  await shot(page, 'cookies-blocked-gate');
  await ctx.close();
  await fx.close();
}

if (want('offline')) {
  const ctx = await chrome.newContext({ viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  await passGate(page, GUEST);
  await page.waitForFunction(() => window.GS && window.GS.booted);
  let release;
  const held = new Promise((r) => { release = r; });
  await page.route('**/api/solve', async (route) => { await held; try { await route.continue(); } catch (e) { await route.abort('internetdisconnected').catch(() => {}); } }, { times: 1 });
  await solveOrtho(page);
  await page.waitForSelector('#state-solving:not([hidden])');
  await ctx.setOffline(true);
  await page.evaluate(() => window.dispatchEvent(new Event('offline')));
  release();
  await page.waitForSelector('#state-error:not([hidden])', { timeout: 20000 });
  const err = await shownText(page, '#state-error');
  check(/offline/i.test(err) && await visible(page, '#err-retry'), `offline mid-solve: a clear offline error with Retry ("${err.split('\n')[0]}")`);
  await shot(page, 'offline-error');
  await ctx.setOffline(false);
  await waitVerdict(page, 30000);
  check(true, 'offline mid-solve: back online, the solve runs again by itself and shows the verdict');
  await shot(page, 'offline-recovered');
  await ctx.close();
}

if (want('stall')) {
  const ctx = await chrome.newContext({ viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  await page.clock.install();
  await passGate(page, GUEST);
  await page.waitForFunction(() => window.GS && window.GS.booted);
  await page.route('**/api/solve', () => {});
  await solveOrtho(page);
  await page.waitForSelector('#state-solving:not([hidden])');
  await page.clock.fastForward(60_000);
  await page.waitForTimeout(200);
  const stillSolving = await visible(page, '#state-solving');
  await page.clock.fastForward(40_000);
  await page.waitForSelector('#state-error:not([hidden])', { timeout: 10000 });
  const err = await shownText(page, '#state-error');
  check(stillSolving && /No answer from the server/.test(err) && await visible(page, '#err-retry'), `silent connection: after the time limit + grace the solve ends in a retry state ("${err.split('\n')[0]}")`);
  await shot(page, 'stalled');
  await ctx.close();
}

if (want('slow3g')) {
  const ctx = await chrome.newContext({ viewport: { width: 360, height: 640 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true });
  const page = await ctx.newPage();
  await passGate(page, GUEST);
  const cdp = await ctx.newCDPSession(page);
  await cdp.send('Network.enable');
  await cdp.send('Network.clearBrowserCache');
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 });
  await cdp.send('Network.emulateNetworkConditions', { offline: false, latency: 2000, downloadThroughput: 51200, uploadThroughput: 51200 });
  const errs = watchErrors(page);
  const t0 = Date.now();
  await page.goto(`${GUEST}/`, { timeout: 120000 });
  await page.waitForFunction(() => window.GS && window.GS.booted, null, { timeout: 120000 });
  const boot = Date.now() - t0;
  await solveOrtho(page);
  await page.waitForSelector('#state-solving:not([hidden])', { timeout: 5000 });
  await waitVerdict(page, 120000);
  check(errs.length === 0, `slow 3G + 4x CPU: boots in ${boot} ms and solves with no errors`);
  await shot(page, 'slow3g');
  await ctx.close();
}

if (want('inert')) {
  const ctx = await chrome.newContext({ viewport: { width: 1280, height: 900 } });
  await ctx.addInitScript(() => { delete HTMLElement.prototype.inert; });
  const page = await ctx.newPage();
  await passGate(page, GUEST);
  await solveOrtho(page);
  await waitVerdict(page);
  await page.click('#z-full');
  await page.waitForTimeout(300);
  const hidden = await page.evaluate(() => document.querySelector('.site-header').closest('[aria-hidden="true"]') !== null);
  await page.evaluate(() => document.getElementById('solve').focus());
  const inside = await page.evaluate(() => document.getElementById('fig-frame').contains(document.activeElement));
  check(hidden && inside, `no native inert: the full-screen figure hides the page from AT (${hidden}) and keeps focus inside (${inside})`);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
  const restored = await page.evaluate(() => document.querySelectorAll('[data-gs-inert]').length === 0 && !document.querySelector('.site-header').closest('[aria-hidden="true"]'));
  check(restored, 'no native inert: closing restores the page');
  await ctx.close();
}

if (want('skew') && ACCT) {
  const ctx = await chrome.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await ctx.newPage();
  await page.clock.install({ time: Date.now() + 3 * 3600 * 1000 });
  await passGate(page, ACCT);
  await page.goto(`${ACCT}/auth?mode=register`);
  await page.fill('#username', `skew${Math.floor(Math.random() * 1e6)}`);
  await page.fill('#password', 'correct horse battery');
  await Promise.all([page.waitForURL((u) => new URL(u).pathname === '/app', { timeout: 15000 }), page.click('form button[type=submit]')]);
  await page.waitForFunction(() => window.GS && window.GS.booted);
  await solveOrtho(page);
  await waitVerdict(page);
  await page.waitForSelector('#hist-list .hist-item', { timeout: 10000 });
  const when = await page.evaluate(() => document.querySelector('#hist-list .hist-item time').textContent);
  check(/now|sec/.test(when) && !/hr|hour/.test(when), `client clock 3 h fast: the history row still says "${when}"`);
  await shot(page, 'clock-skew');
  await ctx.close();
}

if (want('long')) {
  const ctx = await chrome.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await ctx.newPage();
  await passGate(page, GUEST);
  await page.waitForFunction(() => window.GS && window.GS.booted);
  const cdp = await ctx.newCDPSession(page);
  await cdp.send('Performance.enable');
  const sample = async () => {
    await cdp.send('HeapProfiler.collectGarbage');
    const m = Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map((x) => [x.name, x.value]));
    return { heap: m.JSHeapUsedSize / 1048576, nodes: m.Nodes, listeners: m.JSEventListeners };
  };
  const ids = ['ortho', 'euler', 'nine', 'simson'];
  let first;
  for (let i = 0; i < 40; i++) {
    await page.evaluate((id) => {
      const b = document.querySelector(`[data-example-solve="${id}"]`);
      if (b) b.click(); else { document.getElementById('examples-btn').click(); document.querySelector(`[data-example="${id}"]`).click(); document.getElementById('solve').click(); }
    }, ids[i % ids.length]);
    await page.waitForSelector('#state-solving', { state: 'hidden', timeout: 60000 });
    await page.waitForSelector('#verdict:not([hidden])', { timeout: 60000 });
    if (i === 4) first = await sample();
  }
  const last = await sample();
  check(last.heap - first.heap < 4 && last.nodes - first.nodes < 3000 && last.listeners - first.listeners < 300,
    `40 solves: heap ${first.heap.toFixed(1)} -> ${last.heap.toFixed(1)} MB, DOM nodes ${first.nodes} -> ${last.nodes}, listeners ${first.listeners} -> ${last.listeners}`);
  await ctx.close();
}

if (want('restart') && BIN) {
  const port = Number(process.env.RESTART_PORT || 19249);
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'compat-restart-'));
  const env = { ...process.env, AGSTUDIO_BASIC_AUTH: `:${PW}`, AGSTUDIO_GUEST_MODE: '1', AGSTUDIO_DB: `${dir}/a.db`, AGSTUDIO_RATE_PER_MIN: '0' };
  const start = async () => {
    const child = spawn(BIN, ['serve', '--port', String(port)], { env, stdio: 'ignore' });
    for (let i = 0; i < 100; i++) {
      try { const r = await fetch(`http://127.0.0.1:${port}/healthz`); if (r.ok) return child; } catch (e) {}
      await new Promise((r) => setTimeout(r, 100));
    }
    throw new Error('server did not start');
  };
  const stop = (child) => new Promise((res) => { child.once('exit', res); child.kill('SIGTERM'); });
  let srv = await start();
  const base = `http://127.0.0.1:${port}`;
  const ctx = await chrome.newContext({ viewport: { width: 1280, height: 900 }, acceptDownloads: true });
  const page = await ctx.newPage();
  const errs = watchErrors(page);
  await passGate(page, base);
  await page.waitForFunction(() => window.GS && window.GS.booted);
  await solveOrtho(page);
  await waitVerdict(page);
  await stop(srv);
  srv = await start();
  const dl = page.waitForEvent('download', { timeout: 90000 });
  await page.click('#export-btn');
  await page.click('[data-export="pdf"]');
  const d = await dl;
  const size = fs.statSync(await d.path()).size;
  check(size > 1000, `server restarted mid-session: PDF export still downloads (${d.suggestedFilename()}, ${size} B)`);
  await page.click('#solve');
  await page.waitForSelector('#state-solving', { state: 'hidden', timeout: 60000 });
  check(await visible(page, '#verdict') && errs.length === 0, `server restarted mid-session: the next solve works, no page errors (${errs.join('; ')})`);
  await ctx.close();
  await stop(srv);
}

if (want('engines')) {
  for (const [name, type] of [['firefox', firefox], ['webkit', webkit]]) {
    const b = await type.launch();
    const ctx = await b.newContext({ viewport: { width: 1280, height: 900 } });
    const page = await ctx.newPage();
    const errs = watchErrors(page);
    await passGate(page, GUEST);
    await page.waitForFunction(() => window.GS && window.GS.booted);
    await solveOrtho(page);
    await waitVerdict(page);
    await page.waitForTimeout(500);
    check(errs.length === 0 && !(await visible(page, '#gs-fail')) && !(await visible(page, '#gs-old')), `${name}: boots, solves, no notices, no errors (${errs.join('; ')})`);
    await shot(page, `engine-${name}`);
    await b.close();
  }
}

await chrome.close();
console.log(`\n${fails ? 'FAILED' : 'OK'}: ${fails} failure(s); screenshots in ${OUT}`);
process.exit(fails ? 1 : 0);
