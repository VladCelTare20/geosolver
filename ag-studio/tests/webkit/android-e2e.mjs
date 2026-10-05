import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { humanAbsent, humanProofFlow, humanRomanian } from './human-checks.mjs';

const { chromium, firefox, devices } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const PHOTO = process.env.PHOTO || path.resolve(path.dirname(new URL(import.meta.url).pathname), '../../../docs/example-report.png');
const STUB_LOG = process.env.FAKE_CLAUDE_LOG || `${process.env.TMPDIR || '/tmp'}/agstudio-fake-claude`;
const OUT = process.env.SHOTS || fs.mkdtempSync(path.join(os.tmpdir(), 'android-e2e-'));
const GUEST = process.env.GUEST_BASE || 'http://127.0.0.1:8787';
const ACCT = process.env.ACCOUNTS_BASE || '';
const PW = process.env.BASIC_PASSWORD || '';
const only = (process.env.ONLY || '').split(',').filter(Boolean);
fs.mkdirSync(OUT, { recursive: true });

const UA = devices['Pixel 7'].userAgent;
const FF_UA = 'Mozilla/5.0 (Android 14; Mobile; rv:143.0) Gecko/143.0 Firefox/143.0';
const DEVS = {
  'Pixel 7': { w: 412, h: 839, dsf: 2.625 },
  'Galaxy S23': { w: 360, h: 780, dsf: 3 },
  'Fold cover': { w: 280, h: 653, dsf: 3 },
  'Fold open': { w: 717, h: 512, dsf: 2 },
};
const results = [];
let fails = 0;
const log = (...a) => console.log(...a);
const check = (ok, what) => { results.push({ ok: !!ok, what }); log(`${ok ? 'PASS' : 'FAIL'} ${what}`); if (!ok) fails++; };

const SHARE_STUB = () => {
  window.__shared = [];
  navigator.canShare = (d) => !!(d && ((Array.isArray(d.files) && d.files.length && d.files.every((f) => f instanceof File)) || typeof d.text === 'string'));
  navigator.share = (d) => {
    const active = navigator.userActivation ? navigator.userActivation.isActive : true;
    if (!active) return Promise.reject(new DOMException('share needs a user gesture', 'NotAllowedError'));
    const rec = { active, keys: Object.keys(d), files: [] };
    window.__shared.push(rec);
    return Promise.all((d.files || []).map((f) => f.arrayBuffer().then((b) => {
      const u = new Uint8Array(b.slice(0, 8));
      rec.files.push({ name: f.name, type: f.type, size: f.size, head: Array.from(u).map((x) => x.toString(16).padStart(2, '0')).join('') });
    }))).then(() => undefined);
  };
};

function ctxOpts(engine, d, land, scheme, locale) {
  const w = land ? d.h : d.w, h = land ? d.w : d.h;
  const o = { viewport: { width: w, height: h }, colorScheme: scheme, locale: locale || 'en-US', acceptDownloads: true, hasTouch: true };
  if (engine === 'firefox') o.userAgent = FF_UA;
  if (engine === 'chromium') Object.assign(o, { userAgent: UA, deviceScaleFactor: d.dsf, isMobile: true, permissions: ['clipboard-read', 'clipboard-write'] });
  return o;
}

async function newCtx(browser, engine, d, land, scheme, opts = {}) {
  const ctx = await browser.newContext(ctxOpts(engine, d, land, scheme, opts.locale));
  if (opts.share) await ctx.addInitScript(SHARE_STUB);
  const page = await ctx.newPage();
  const errs = [];
  page.on('pageerror', (e) => errs.push(`pageerror: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error' && !/401|Failed to load resource/.test(m.text())) errs.push(`console: ${m.text()}`); });
  page.on('dialog', (dl) => { errs.push(`dialog ${dl.type()}: ${dl.message()}`); dl.dismiss(); });
  return { ctx, page, errs };
}

function mkShot(tag) {
  let n = 0;
  return async (page, name, full = false) => {
    const p = `${OUT}/${tag}-${String(++n).padStart(2, '0')}-${name}.png`;
    await page.screenshot({ path: p, fullPage: full, animations: 'disabled' }).catch(() => {});
    return p;
  };
}

const fits = (page) => page.evaluate(() => ({ sw: document.documentElement.scrollWidth, vw: document.documentElement.clientWidth }));

async function back(page) {
  await page.goBack({ waitUntil: 'commit', timeout: 4000 }).catch(() => {});
  await page.waitForTimeout(400);
}

async function cdpPinch(page, ctx) {
  const cdp = await ctx.newCDPSession(page);
  await page.evaluate(() => document.getElementById('fig-viewport').scrollIntoView({ block: 'center' }));
  await page.waitForTimeout(250);
  const vb = () => page.evaluate(() => document.querySelector('#fig-viewport > svg').getAttribute('viewBox'));
  const w = (s) => Number(String(s).split(/[ ,]+/)[2]);
  const x0 = (s) => Number(String(s).split(/[ ,]+/)[0]);
  const v0 = await vb();
  const r = await page.evaluate(() => { const b = document.getElementById('fig-viewport').getBoundingClientRect(); return { cx: b.left + b.width / 2, cy: b.top + b.height / 2 }; });
  const tp = (a, b) => [{ x: r.cx - a, y: r.cy, id: 1 }, { x: r.cx + b, y: r.cy, id: 2 }];
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: tp(25, 25) });
  for (let i = 1; i <= 8; i++) { await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: tp(25 + i * 9, 25 + i * 9) }); await page.waitForTimeout(16); }
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForTimeout(200);
  const v1 = await vb();
  const pageZoom = await page.evaluate(() => (window.visualViewport ? window.visualViewport.scale : 1));
  check(w(v1) < w(v0) * 0.8 && pageZoom === 1, `real two-finger pinch (CDP touch) zooms the figure, not the page (viewBox w ${w(v0).toFixed(1)} -> ${w(v1).toFixed(1)}, page scale ${pageZoom})`);
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: r.cx, y: r.cy, id: 3 }] });
  for (let i = 1; i <= 6; i++) { await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: r.cx + i * 8, y: r.cy, id: 3 }] }); await page.waitForTimeout(16); }
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForTimeout(200);
  const v2 = await vb();
  check(x0(v2) !== x0(v1), `one-finger drag pans the zoomed figure (viewBox x ${x0(v1).toFixed(1)} -> ${x0(v2).toFixed(1)})`);
  await cdp.detach();
}

async function synthPinch(page) {
  await page.evaluate(() => document.getElementById('fig-viewport').scrollIntoView({ block: 'center' }));
  await page.waitForTimeout(250);
  const res = await page.evaluate(() => {
    const vp = document.getElementById('fig-viewport');
    const svg = () => document.querySelector('#fig-viewport > svg').getAttribute('viewBox');
    const v0 = svg();
    const r = vp.getBoundingClientRect();
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    const ev = (type, id, x, y) => vp.dispatchEvent(new PointerEvent(type, { pointerId: id, pointerType: 'touch', isPrimary: id === 11, clientX: x, clientY: y, bubbles: true, cancelable: true, button: 0, buttons: type === 'pointerup' ? 0 : 1 }));
    ev('pointerdown', 11, cx - 30, cy); ev('pointerdown', 12, cx + 30, cy);
    for (let i = 1; i <= 6; i++) { ev('pointermove', 11, cx - 30 - i * 12, cy); ev('pointermove', 12, cx + 30 + i * 12, cy); }
    ev('pointerup', 12, cx + 102, cy); ev('pointerup', 11, cx - 102, cy);
    return [v0, svg()];
  });
  const w = (s) => Number(String(s).split(/[ ,]+/)[2]);
  check(w(res[1]) < w(res[0]) * 0.8, `pinch (pointer events) zooms the figure (viewBox w ${w(res[0]).toFixed(1)} -> ${w(res[1]).toFixed(1)})`);
}

async function settle(page) {
  await page.waitForSelector('#state-solving:not([hidden])', { timeout: 10000 });
  await page.waitForSelector('#state-solving', { state: 'hidden', timeout: 120000 });
  await page.waitForSelector('#verdict:not([hidden]), #state-error:not([hidden])', { timeout: 10000 });
  await page.waitForTimeout(400);
}

async function solveExample(page) {
  await page.evaluate(() => window.scrollTo(0, 0));
  const chip = page.locator('#example-chips [data-example-solve="ortho"]');
  if (await chip.count() && await chip.isVisible()) await chip.click();
  else await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
  await page.waitForSelector('#verdict:not([hidden])', { timeout: 60000 });
  await page.waitForTimeout(600);
}

async function exportFlow(page, tag, shot, engine) {
  for (const fmt of ['pdf', 'png']) {
    await page.locator('#export-btn').scrollIntoViewIfNeeded();
    const dl = page.waitForEvent('download', { timeout: 60000 }).catch(() => null);
    await page.click('#export-btn');
    await page.click(`[data-export="${fmt}"]`);
    const d = await dl;
    const p = d ? await d.path() : null;
    const size = p ? fs.statSync(p).size : 0;
    const head = p ? fs.readFileSync(p).subarray(0, 4).toString('hex') : '';
    const magic = fmt === 'pdf' ? '25504446' : '89504e47';
    check(size > 1000 && head === magic, `${tag}: ${fmt.toUpperCase()} is saved straight to Downloads (${d && d.suggestedFilename()}, ${size} B)`);
    await page.waitForTimeout(300);
    await shot(page, `export-${fmt}`);
    if (engine === 'chromium') {
      const tbtn = page.locator('#toasts .toast button').filter({ hasText: /^(Share|Partajează)$/ });
      const n = await tbtn.count();
      check(n === 1, `${tag}: the ${fmt.toUpperCase()} toast offers Share (${n})`);
      if (n) {
        await tbtn.first().click();
        await page.waitForTimeout(500);
        const sh = await page.evaluate(() => window.__shared || []);
        const f = sh[sh.length - 1] && sh[sh.length - 1].files[0];
        check(f && f.head.startsWith(magic) && f.size === size, `${tag}: Share hands the same ${fmt.toUpperCase()} to the share sheet from the tap (${f && f.name}, ${f && f.size} B)`);
      }
    }
  }
}

async function guestRun(browser, engine, devName, land, scheme) {
  const d = DEVS[devName];
  const tag = `${engine}-${devName.replace(/ /g, '')}-${land ? 'land' : 'port'}-${scheme}`;
  const shot = mkShot(tag);
  const { ctx, page, errs } = await newCtx(browser, engine, d, land, scheme, { share: true });
  try {
    await page.goto(`${GUEST}/`);
    await page.waitForLoadState('networkidle');
    let f = await fits(page);
    check(new URL(page.url()).pathname === '/gate' && f.sw <= f.vw, `${tag}: / -> gate page, fits (${f.sw}/${f.vw})`);
    const pwAttrs = await page.evaluate(() => { const i = document.getElementById('pw'); return `${i.type}/${i.getAttribute('autocomplete')}/${i.getAttribute('enterkeyhint')}`; });
    check(pwAttrs === 'password/current-password/go', `${tag}: gate field is a password-manager field with a Go key (${pwAttrs})`);
    await shot(page, 'gate');
    await page.fill('#pw', PW);
    await Promise.all([page.waitForURL((u) => new URL(u).pathname === '/'), page.press('#pw', 'Enter')]);
    await page.waitForFunction(() => window.GS && !document.getElementById('ai-pill').classList.contains('checking'));
    await page.waitForTimeout(300);
    const tc = await page.evaluate(() => [...document.querySelectorAll('meta[name="theme-color"]')].filter((m) => !m.media || matchMedia(m.media).matches).map((m) => m.content));
    const bg = await page.evaluate(() => getComputedStyle(document.querySelector('.site-header')).backgroundColor);
    check(tc.length === 1 && tc[0] === (scheme === 'dark' ? '#121417' : '#f7f7f5'), `${tag}: address bar colour (theme-color) matches the ${scheme} header (${tc.join(',')} vs ${bg})`);
    f = await fits(page);
    check(f.sw <= f.vw, `${tag}: app fits (${f.sw}/${f.vw})`);
    await shot(page, 'app');

    await page.click('#tab-geo');
    await solveExample(page);
    const head = await page.textContent('#verdict');
    check(/Proved/i.test(head), `${tag}: example solves -> "${head.trim().slice(0, 40)}"`);
    await page.locator('#verdict').scrollIntoViewIfNeeded();
    await shot(page, 'result');
    await humanProofFlow(page, tag, check, shot, { touch: true });

    const step = page.locator('#steps li.step:not(.is-group):not([hidden])').nth(1);
    await step.scrollIntoViewIfNeeded();
    const sb = await step.boundingBox();
    await page.touchscreen.tap(sb.x + 40, sb.y + Math.min(sb.height / 2, 20));
    await page.waitForTimeout(400);
    const lit = await page.evaluate(() => document.querySelectorAll('#fig-viewport .hl').length);
    check(lit > 0, `${tag}: tapping a step lights the figure (${lit} lit)`);
    await shot(page, 'step-tap');

    if (engine === 'chromium') await cdpPinch(page, ctx); else await synthPinch(page);
    await shot(page, 'figure-zoomed');
    await page.click('#z-fit').catch(() => {});

    await page.locator('#z-full').scrollIntoViewIfNeeded();
    const yBefore = await page.evaluate(() => Math.round(window.scrollY));
    await page.click('#z-full');
    await page.waitForTimeout(300);
    check(await page.evaluate(() => document.getElementById('fig-frame').classList.contains('is-full')), `${tag}: full-screen figure opens`);
    await shot(page, 'fullscreen');
    const urlBefore = page.url();
    await back(page);
    const afterBack = await page.evaluate(() => ({ full: document.getElementById('fig-frame').classList.contains('is-full'), path: location.pathname, locked: document.documentElement.classList.contains('no-scroll'), y: Math.round(window.scrollY) }));
    check(page.url() === urlBefore && !afterBack.full && !afterBack.locked, `${tag}: Android Back closes the full-screen figure and stays in the app (${afterBack.path}, full=${afterBack.full})`);
    check(Math.abs(afterBack.y - yBefore) <= 2, `${tag}: the page is back where it was (scrollY ${yBefore} -> ${afterBack.y})`);
    if (new URL(page.url()).pathname !== '/') { await page.goto(`${GUEST}/`); await page.waitForFunction(() => window.GS); await solveExample(page); }
    await page.locator('#z-full').scrollIntoViewIfNeeded();
    await page.click('#z-full');
    await page.waitForTimeout(200);
    await page.click('#z-full');
    await page.waitForTimeout(300);
    await page.evaluate(() => { window.__sameDoc = 1; });
    await back(page);
    await page.waitForLoadState('load');
    const left = await page.evaluate(() => !window.__sameDoc).catch(() => true);
    check(left, `${tag}: after closing full screen with its button, Back leaves this page as usual (now ${new URL(page.url()).pathname})`);
    if (left) { await page.goto(`${GUEST}/`); await page.waitForFunction(() => window.GS && !document.getElementById('ai-pill').classList.contains('checking')); await solveExample(page); }

    await exportFlow(page, tag, shot, engine);

    if (engine === 'chromium') {
      await page.locator('#copy-proof').scrollIntoViewIfNeeded();
      await page.click('#copy-proof');
      await page.waitForTimeout(300);
      const clip = await page.evaluate(() => navigator.clipboard.readText().catch((e) => `ERR ${e.message}`));
      check(/1\.|concyclic|conciclic/.test(clip) && clip.length > 40, `${tag}: Copy proof puts the proof on the clipboard (${clip.length} chars: ${clip.split('\n')[0].slice(0, 40)})`);
      await shot(page, 'copied');
    }

    await page.locator('#ptab-ai').scrollIntoViewIfNeeded();
    await page.click('#ptab-ai');
    await page.waitForFunction(() => /stub/.test(document.getElementById('ai-text').innerText), null, { timeout: 30000 });
    check(true, `${tag}: AI explanation arrives`);
    await page.locator('#proof-tabs').scrollIntoViewIfNeeded();
    await shot(page, 'ai-explanation');

    await page.evaluate(() => window.scrollTo(0, 0));
    await page.click('#tab-describe');
    await page.fill('#describe-input', 'Triangle ABC with AB = AC. Prove the base angles are equal.');
    await page.click('#solve');
    await settle(page);
    const dv = await page.evaluate(() => (document.getElementById('verdict').hidden ? document.getElementById('state-error').innerText : document.getElementById('verdict').innerText));
    check(/Proved/i.test(dv), `${tag}: Describe -> "${dv.trim().split('\n')[0]}"`);
    await page.evaluate(() => window.scrollTo(0, 0));
    await shot(page, 'describe-solved');
    await humanAbsent(page, tag, check);

    await page.click('#tab-photo');
    const acc = await page.evaluate(() => { const i = document.getElementById('photo-input'); return `${i.accept}|${i.hasAttribute('capture') ? i.getAttribute('capture') : '-'}`; });
    check(acc === 'image/*|-', `${tag}: photo input offers camera and gallery (accept=image/*, no capture) (${acc})`);
    await page.setInputFiles('#photo-input', PHOTO);
    await page.waitForSelector('#dz-full:not([hidden])', { timeout: 30000 });
    const meta = await page.textContent('#photo-meta');
    await page.locator('#dropzone').scrollIntoViewIfNeeded();
    await shot(page, 'photo-prepared');
    await page.click('#solve');
    await settle(page);
    const up = fs.existsSync(`${STUB_LOG}/problem.jpg`) ? fs.statSync(`${STUB_LOG}/problem.jpg`) : null;
    check(up && Date.now() - up.mtimeMs < 20000, `${tag}: the model received a fresh problem.jpg (${up && up.size} B)`);
    const pv = await page.evaluate(() => (document.getElementById('verdict').hidden ? document.getElementById('state-error').innerText : document.getElementById('verdict').innerText));
    check(/Proved/i.test(pv), `${tag}: Photo (${meta.trim()}) -> "${pv.trim().split('\n')[0]}"`);
    await shot(page, 'photo-solved');

    await page.evaluate(() => window.scrollTo(0, 0));
    await page.click('[data-lang-set="ro"]');
    await page.waitForTimeout(400);
    const ro = await page.evaluate(() => ({ lang: document.documentElement.lang, solve: document.getElementById('solve').innerText.trim(), sw: document.documentElement.scrollWidth, vw: document.documentElement.clientWidth }));
    check(ro.lang === 'ro' && ro.sw <= ro.vw, `${tag}: RO switch (lang=${ro.lang}, solve "${ro.solve}", fits ${ro.sw}/${ro.vw})`);
    await shot(page, 'ro-app');
    await humanRomanian(page, tag, check, shot);
    await page.locator('#ptab-ai').scrollIntoViewIfNeeded();
    await page.click('#ptab-ai');
    await page.waitForFunction(() => /Deoarece/.test(document.getElementById('ai-text').innerText), null, { timeout: 30000 }).catch(() => {});
    check(/Deoarece/.test(await page.textContent('#ai-text')), `${tag}: RO AI explanation is Romanian`);
    await page.locator('#proof-tabs').scrollIntoViewIfNeeded();
    await shot(page, 'ro-ai');
    await page.evaluate(() => { localStorage.removeItem('lang'); document.cookie = 'lang=; Max-Age=0; path=/'; });
  } catch (e) {
    check(false, `${tag}: flow aborted: ${e.message.split('\n')[0]}`);
    await shot(page, 'ABORT');
  }
  check(!errs.length, `${tag}: no page/console errors ${errs.slice(0, 3).join(' | ')}`);
  await ctx.close();
}

async function installability(browser) {
  const d = DEVS['Pixel 7'];
  const { ctx, page } = await newCtx(browser, 'chromium', d, false, 'light');
  await page.goto(`${GUEST}/gate`);
  await page.fill('#pw', PW);
  await Promise.all([page.waitForURL((u) => new URL(u).pathname === '/'), page.click('.gate-submit')]);
  await page.waitForLoadState('networkidle');
  const cdp = await ctx.newCDPSession(page);
  const man = await cdp.send('Page.getAppManifest', {}).catch((e) => ({ errors: [{ message: e.message }] }));
  check(man && man.url && !(man.errors || []).length, `manifest parses (${man.url}; errors ${JSON.stringify(man.errors)})`);
  const inst = await cdp.send('Page.getInstallabilityErrors').catch((e) => ({ installabilityErrors: [{ errorId: e.message }] }));
  check(!inst.installabilityErrors.length, `Chrome installability: ${inst.installabilityErrors.length ? JSON.stringify(inst.installabilityErrors) : 'installable'}`);
  const m = JSON.parse(man.data || '{}');
  const icons = m.icons || [];
  check(icons.some((i) => /maskable/.test(i.purpose || '') && i.sizes === '512x512') && icons.some((i) => i.sizes === '192x192'), `manifest has 192 + 512 icons and a 512 maskable one`);
  check(m.id && m.start_url && m.display === 'standalone' && m.scope, `manifest id=${m.id} start_url=${m.start_url} display=${m.display} scope=${m.scope}`);
  for (const i of icons.filter((x) => x.type === 'image/png')) {
    const r = await page.request.get(new URL(i.src, GUEST).toString(), { headers: { cookie: '' } });
    check(r.ok() && r.headers()['content-type'] === 'image/png', `icon ${i.src} served without the password (${r.status()} ${r.headers()['content-type']})`);
  }
  await cdp.detach();
  await ctx.close();
}

async function acctRun(browser, engine, devName, land, scheme) {
  const d = DEVS[devName];
  const tag = `acct-${engine}-${devName.replace(/ /g, '')}-${land ? 'land' : 'port'}-${scheme}`;
  const shot = mkShot(tag);
  const { ctx, page, errs } = await newCtx(browser, engine, d, land, scheme);
  try {
    await page.goto(`${ACCT}/`);
    await page.waitForLoadState('networkidle');
    let f = await fits(page);
    check(f.sw <= f.vw, `${tag}: landing fits (${f.sw}/${f.vw})`);
    await shot(page, 'landing');
    await page.goto(`${ACCT}/auth?mode=register`);
    await page.waitForLoadState('networkidle');
    f = await fits(page);
    check(f.sw <= f.vw, `${tag}: register page fits (${f.sw}/${f.vw})`);
    const ac = await page.evaluate(() => [...document.querySelectorAll('input')].map((i) => `${i.id}:${i.getAttribute('autocomplete')}`).join(' '));
    check(/new-password/.test(ac) && /username/.test(ac), `${tag}: register fields carry autofill hints (${ac})`);
    await shot(page, 'register');
    const user = `a${process.pid % 10000}${Math.floor(Math.random() * 1e6)}`.slice(0, 20);
    await page.fill('#username', user);
    const pwf = page.locator('input[type=password]');
    for (let i = 0; i < await pwf.count(); i++) await pwf.nth(i).fill('correct horse battery');
    await Promise.all([page.waitForURL((u) => new URL(u).pathname === '/app', { timeout: 15000 }), page.click('form button[type=submit]')]);
    await page.waitForFunction(() => window.GS && !document.getElementById('ai-pill').classList.contains('checking'));
    await solveExample(page);
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.waitForTimeout(300);
    const toggle = await page.evaluate(() => { const b = document.getElementById('rail-toggle'); const r = b.getBoundingClientRect(); return { vis: r.width > 0, w: r.width }; });
    if (toggle.vis) {
      await page.click('#rail-toggle');
      await page.waitForTimeout(450);
      const h = await page.evaluate(() => ({ items: document.querySelectorAll('#hist-list .hist-item').length, open: document.body.classList.contains('drawer-open') }));
      check(h.open && h.items >= 1, `${tag}: history drawer opens with ${h.items} item(s)`);
      await shot(page, 'history');
      const urlBefore = page.url();
      await back(page);
      const st = await page.evaluate(() => ({ open: document.body.classList.contains('drawer-open'), locked: document.documentElement.classList.contains('no-scroll'), inert: [...document.body.children].filter((c) => c.inert).length }));
      check(page.url() === urlBefore && !st.open && !st.locked && !st.inert, `${tag}: Android Back closes the history drawer and stays in the app (${new URL(page.url()).pathname}, open=${st.open}, inert=${st.inert})`);
      await shot(page, 'history-closed-by-back');
      if (new URL(page.url()).pathname === '/app') {
        await page.click('#rail-toggle');
        await page.waitForTimeout(400);
        await page.locator('#hist-list .hist-open').first().click();
        await page.waitForTimeout(800);
        const st2 = await page.evaluate(() => ({ open: document.body.classList.contains('drawer-open'), v: !document.getElementById('verdict').hidden }));
        check(!st2.open && st2.v, `${tag}: opening a history row closes the drawer and shows the result`);
        await page.evaluate(() => { window.__sameDoc = 1; });
        await back(page);
        await page.waitForLoadState('load');
        const left = await page.evaluate(() => !window.__sameDoc).catch(() => true);
        check(left, `${tag}: after the drawer closed itself, Back leaves this page as usual (now ${new URL(page.url()).pathname})`);
      }
    } else {
      check(true, `${tag}: history is the side rail at this width (no drawer)`);
    }
    await page.goto(`${ACCT}/auth`);
    await page.waitForLoadState('networkidle');
    check(new URL(page.url()).pathname === '/app', `${tag}: /auth while signed in goes to /app`);
  } catch (e) {
    check(false, `${tag}: flow aborted: ${e.message.split('\n')[0]}`);
    await shot(page, 'ABORT');
  }
  check(!errs.length, `${tag}: no page/console errors ${errs.slice(0, 3).join(' | ')}`);
  await ctx.close();
}

const want = (k) => !only.length || only.includes(k);
const cr = await chromium.launch();
if (want('install')) await installability(cr);
const runs = [];
for (const dn of ['Pixel 7', 'Galaxy S23', 'Fold cover', 'Fold open']) for (const land of [false, true]) runs.push([dn, land, land === (dn === 'Fold open') ? 'light' : 'dark']);
if (want('guest')) for (let i = 0; i < runs.length; i += 3) await Promise.all(runs.slice(i, i + 3).map((r) => guestRun(cr, 'chromium', ...r)));
if (want('acct') && ACCT) for (let i = 0; i < runs.length; i += 4) await Promise.all(runs.slice(i, i + 4).map((r) => acctRun(cr, 'chromium', ...r)));
await cr.close();
if (want('firefox')) {
  const ff = await firefox.launch();
  await Promise.all([guestRun(ff, 'firefox', 'Galaxy S23', false, 'light'), guestRun(ff, 'firefox', 'Fold cover', false, 'dark')]);
  if (ACCT) await acctRun(ff, 'firefox', 'Galaxy S23', false, 'light');
  await ff.close();
}
fs.writeFileSync(`${OUT}/results-${only.join('_') || 'all'}.json`, JSON.stringify(results, null, 1));
log(`screenshots: ${OUT}`);
log(fails ? `${fails} FAILURES of ${results.length}` : `ALL ${results.length} PASS`);
process.exit(fails ? 1 : 0);
