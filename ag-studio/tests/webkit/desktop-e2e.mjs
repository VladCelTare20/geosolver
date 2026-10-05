import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { humanAbsent, humanProofFlow, humanRomanian } from './human-checks.mjs';

const pw = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const PHOTO = process.env.PHOTO || path.resolve(path.dirname(new URL(import.meta.url).pathname), '../../../docs/example-report.png');
const OUT = process.env.SHOTS || fs.mkdtempSync(path.join(os.tmpdir(), 'desktop-e2e-'));
const GUEST = process.env.GUEST_BASE || 'http://127.0.0.1:8787';
const ACCT = process.env.ACCOUNTS_BASE || '';
const PW = process.env.BASIC_PASSWORD || '';
const ENGINES = (process.env.ENGINES || 'chromium,firefox,webkit').split(',');
fs.mkdirSync(OUT, { recursive: true });

const results = [];
let fails = 0;
const log = (...a) => console.log(...a);
const check = (ok, what) => { results.push({ ok: !!ok, what }); log(`${ok ? 'PASS' : 'FAIL'} ${what}`); if (!ok) fails++; };
const skip = (what) => { results.push({ ok: true, skip: true, what }); log(`SKIP ${what}`); };

const PROFILES = [
  { name: 'desktop-1366', viewport: { width: 1366, height: 768 }, dpr: 1, engines: ['chromium', 'firefox', 'webkit'] },
  { name: 'desktop-1920-dark-ro', viewport: { width: 1920, height: 1080 }, dpr: 1.25, scheme: 'dark', locale: 'ro-RO', engines: ['chromium', 'firefox', 'webkit'] },
  { name: 'ipad-air-touch', viewport: { width: 820, height: 1180 }, dpr: 2, touch: true, engines: ['chromium', 'webkit'] },
  { name: 'ipad-pro-land-trackpad', viewport: { width: 1366, height: 1024 }, dpr: 2, engines: ['chromium', 'webkit'] },
  { name: 'ipad-mini-land-trackpad', viewport: { width: 1024, height: 768 }, dpr: 2, engines: ['chromium', 'firefox', 'webkit'] },
];

const vb = (page) => page.evaluate(() => document.querySelector('#fig-viewport > svg').getAttribute('viewBox').split(/[ ,]+/).map(Number));
const fits = (page) => page.evaluate(() => ({ sw: document.documentElement.scrollWidth, vw: document.documentElement.clientWidth }));
const visibleText = (page, sel) => page.evaluate((s) => { const e = document.querySelector(s); return e && !e.hidden ? e.innerText.trim() : ''; }, sel);

async function settle(page) {
  await page.waitForSelector('#state-solving', { state: 'hidden', timeout: 120000 });
  await page.waitForSelector('#verdict:not([hidden]), #state-error:not([hidden])', { timeout: 10000 });
  await page.waitForTimeout(400);
}

async function newPage(browser, eng, prof) {
  const ctx = await browser.newContext({
    viewport: prof.viewport, deviceScaleFactor: prof.dpr, colorScheme: prof.scheme || 'light', locale: prof.locale || 'en-US',
    hasTouch: !!prof.touch, isMobile: !!prof.touch && eng !== 'firefox', acceptDownloads: true,
  });
  if (eng === 'chromium') await ctx.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: new URL(GUEST).origin }).catch(() => {});
  const page = await ctx.newPage();
  const errs = [];
  page.on('pageerror', (e) => errs.push(`pageerror: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error' && !/401|503|Failed to load resource/.test(m.text())) errs.push(`console: ${m.text()}`); });
  page.on('dialog', (d) => { errs.push(`dialog: ${d.message()}`); d.dismiss(); });
  return { ctx, page, errs };
}

async function guestRun(browser, eng, prof) {
  const tag = `${eng}-${prof.name}`;
  let n = 0;
  const shot = (name, full = false) => page.screenshot({ path: `${OUT}/${tag}-${String(++n).padStart(2, '0')}-${name}.png`, fullPage: full, animations: 'disabled' });
  const { ctx, page, errs } = await newPage(browser, eng, prof);
  const mod = 'Control';
  try {
    await page.goto(`${GUEST}/`);
    check(new URL(page.url()).pathname === '/gate', `${tag}: / -> password page`);
    await shot('gate');
    await page.focus('#pw');
    await page.keyboard.type(PW);
    await Promise.all([page.waitForURL((u) => new URL(u).pathname === '/'), page.keyboard.press('Enter')]);
    await page.waitForFunction(() => window.GS && !document.getElementById('ai-pill').classList.contains('checking'), null, { timeout: 20000 });
    let f = await fits(page);
    check(f.sw <= f.vw, `${tag}: app after gate (Enter submits), fits ${f.sw}/${f.vw}`);
    await shot('app');

    await page.click('#tab-geo');
    await page.focus('#examples-btn');
    await page.keyboard.press('Enter');
    await page.waitForSelector('#examples-menu:not([hidden])');
    const first = await page.evaluate(() => document.activeElement && document.activeElement.getAttribute('role'));
    await page.keyboard.press('Escape');
    await page.waitForTimeout(150);
    const menu = await page.evaluate(() => ({ hidden: document.getElementById('examples-menu').hidden, focus: document.activeElement.id }));
    check(first === 'menuitem' && menu.hidden && menu.focus === 'examples-btn', `${tag}: Examples menu by keyboard: focus ${first}, Escape closes, focus back on ${menu.focus}`);

    await page.focus('#geo-input');
    await page.keyboard.press(`${mod}+Enter`);
    await settle(page);
    const head = await visibleText(page, '#verdict');
    check(/Proved|Demonstrat/.test(head), `${tag}: Ctrl+Enter in the editor solves -> "${head.split('\n')[0]}"`);
    f = await fits(page);
    check(f.sw <= f.vw, `${tag}: result fits ${f.sw}/${f.vw}`);
    await page.evaluate(() => window.scrollTo(0, 0));
    await shot('result');
    const pshot = (p, name) => shot(name);
    if (await page.evaluate(() => document.documentElement.lang === 'ro')) await humanRomanian(page, tag, check, pshot);
    await humanProofFlow(page, tag, check, pshot, { touch: !!prof.touch && eng !== 'firefox', keyboard: !prof.touch });

    await page.focus('#steps li.step[tabindex="0"]');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowDown');
    await page.waitForTimeout(250);
    const lit = await page.evaluate(() => ({ active: (document.activeElement.className || '').toString(), hl: document.querySelectorAll('#fig-viewport .hl').length }));
    check(/step/.test(lit.active) && lit.hl > 0, `${tag}: arrow keys move between steps and light the figure (${lit.hl} lit)`);
    await shot('step-keyboard');

    await page.focus('#fig-viewport');
    const w0 = (await vb(page))[2];
    await page.keyboard.press('+');
    const w1 = (await vb(page))[2];
    await page.keyboard.press('0');
    const w2 = (await vb(page))[2];
    check(w1 < w0 && Math.abs(w2 - w0) < 0.5, `${tag}: figure keys + and 0 zoom and reset (${w0.toFixed(0)} -> ${w1.toFixed(0)} -> ${w2.toFixed(0)})`);
    await page.keyboard.press('f');
    await page.waitForTimeout(250);
    const full = await page.evaluate(() => document.getElementById('fig-frame').classList.contains('is-full'));
    await shot('figure-full');
    await page.keyboard.press('Escape');
    await page.waitForTimeout(250);
    const after = await page.evaluate(() => ({ full: document.getElementById('fig-frame').classList.contains('is-full'), focus: document.activeElement.id }));
    check(full && !after.full && after.focus === 'z-full', `${tag}: F opens full screen, Escape closes it, focus on ${after.focus}`);

    await page.locator('#fig-viewport').scrollIntoViewIfNeeded();
    await page.waitForTimeout(150);
    const pinned = await page.evaluate(() => getComputedStyle(document.getElementById('figure-panel')).position === 'sticky');
    if (prof.touch && eng === 'webkit') skip(`${tag}: wheel checks (Playwright's mobile WebKit has no wheel)`);
    else {
      const r = await page.locator('#fig-viewport').boundingBox();
      await page.mouse.move(r.x + r.width / 2, r.y + r.height / 2);
      const y0 = await page.evaluate(() => scrollY);
      const v0 = await vb(page);
      await page.mouse.wheel(0, pinned ? -200 : 120);
      await page.waitForTimeout(300);
      const y1 = await page.evaluate(() => scrollY);
      const v1 = await vb(page);
      if (pinned) check(v1[2] < v0[2] * 0.9, `${tag}: wheel over the side figure zooms it (${v0[2].toFixed(0)} -> ${v1[2].toFixed(0)})`);
      else check(y1 > y0 && Math.abs(v1[2] - v0[2]) < 0.5, `${tag}: wheel over the in-page figure scrolls the page (${y0} -> ${y1}), no zoom`);
      await page.click('#z-fit');
      const rr = await page.locator('#fig-viewport').boundingBox();
      await page.mouse.move(rr.x + rr.width / 2, rr.y + rr.height / 2);
      await page.keyboard.down(mod);
      await page.mouse.wheel(0, -100);
      await page.keyboard.up(mod);
      await page.waitForTimeout(250);
      const v2 = await vb(page);
      check(v2[2] < v0[2] * 0.95, `${tag}: Ctrl + wheel (trackpad pinch) zooms the figure (${v0[2].toFixed(0)} -> ${v2[2].toFixed(0)})`);
      if (!pinned) {
        const ya = await page.evaluate(() => scrollY);
        const va = await vb(page);
        await page.mouse.wheel(0, 60);
        await page.waitForTimeout(250);
        const yb = await page.evaluate(() => scrollY);
        const vbb = await vb(page);
        check(Math.abs(vbb[1] - va[1]) > 0.1 && yb === ya, `${tag}: wheel over a zoomed in-page figure pans it first (viewBox y ${va[1].toFixed(1)} -> ${vbb[1].toFixed(1)}, page ${ya} -> ${yb})`);
      }
    }
    await page.click('#z-fit');
    await page.locator('#z-in').click();
    const g = await page.evaluate(() => {
      const vp = document.getElementById('fig-viewport');
      const r = vp.getBoundingClientRect();
      const w0 = Number(vp.querySelector(':scope > svg').getAttribute('viewBox').split(/[ ,]+/)[2]);
      const ev = (type, scale) => { const e = new Event(type, { bubbles: true, cancelable: true }); Object.defineProperties(e, { scale: { value: scale }, clientX: { value: r.left + r.width / 2 }, clientY: { value: r.top + r.height / 2 } }); vp.dispatchEvent(e); return e.defaultPrevented; };
      const p = ev('gesturestart', 1); ev('gesturechange', 1.4); ev('gesturechange', 2); ev('gestureend', 2);
      return { p, w0, w1: Number(vp.querySelector(':scope > svg').getAttribute('viewBox').split(/[ ,]+/)[2]) };
    });
    check(g.p && g.w1 < g.w0 * 0.6, `${tag}: Safari trackpad pinch (gesture events) zooms the figure, page zoom prevented (${g.w0.toFixed(0)} -> ${g.w1.toFixed(0)})`);
    const pen = await page.evaluate(() => {
      const vp = document.getElementById('fig-viewport');
      const r = vp.getBoundingClientRect();
      const x0 = Number(vp.querySelector(':scope > svg').getAttribute('viewBox').split(/[ ,]+/)[0]);
      const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
      const ev = (type, x) => vp.dispatchEvent(new PointerEvent(type, { pointerId: 31, pointerType: 'pen', isPrimary: true, clientX: x, clientY: cy, bubbles: true, cancelable: true, button: 0, buttons: type === 'pointerup' ? 0 : 1 }));
      ev('pointerdown', cx); for (let i = 1; i <= 5; i++) ev('pointermove', cx + i * 10); ev('pointerup', cx + 50);
      return { x0, x1: Number(vp.querySelector(':scope > svg').getAttribute('viewBox').split(/[ ,]+/)[0]) };
    });
    check(pen.x1 !== pen.x0, `${tag}: pen drag pans the zoomed figure (x ${pen.x0.toFixed(1)} -> ${pen.x1.toFixed(1)})`);
    await page.click('#z-fit');

    await page.locator('#export-btn').scrollIntoViewIfNeeded();
    for (const fmt of ['pdf', 'png']) {
      const dl = page.waitForEvent('download', { timeout: 60000 });
      await page.click('#export-btn');
      await page.click(`[data-export="${fmt}"]`);
      const d = await dl;
      const p = await d.path();
      const head = p ? fs.readFileSync(p).subarray(0, 4).toString('hex') : '';
      check(head === (fmt === 'pdf' ? '25504446' : '89504e47'), `${tag}: Export ${fmt.toUpperCase()} downloads (${d.suggestedFilename()}, ${p ? fs.statSync(p).size : 0} B)`);
    }
    const sdl = page.waitForEvent('download', { timeout: 20000 });
    await page.click('#z-svg');
    const sd = await sdl;
    const sp = await sd.path();
    check(sp && /<svg/.test(fs.readFileSync(sp, 'utf8').slice(0, 400)), `${tag}: figure SVG downloads (${sd.suggestedFilename()})`);

    await page.click('#copy-proof');
    await page.waitForFunction(() => /cop/i.test(document.getElementById('toasts').innerText), null, { timeout: 5000 }).catch(() => {});
    const toast = await page.evaluate(() => { const ts = document.querySelectorAll('#toasts .toast'); return ts.length ? ts[ts.length - 1].innerText.trim() : ''; });
    let clip = '';
    if (eng === 'chromium') clip = await page.evaluate(() => navigator.clipboard.readText()).catch(() => '');
    check(/cop/i.test(toast) && (eng !== 'chromium' || /concyclic|conciclic/.test(clip)), `${tag}: Copy proof toasts "${toast.split('\n')[0]}"${eng === 'chromium' ? ` and the clipboard holds the proof (${clip.length} chars)` : ''}`);

    await page.locator('#ptab-ai').scrollIntoViewIfNeeded();
    await page.click('#ptab-ai');
    await page.waitForFunction(() => /stub|Deoarece/.test(document.getElementById('ai-text').innerText), null, { timeout: 30000 });
    check(true, `${tag}: AI explanation arrives`);
    await shot('ai');
    await page.click('#ptab-steps');

    await page.emulateMedia({ media: 'print' });
    const pr = await page.evaluate(() => {
      const shown = (s) => { const e = document.querySelector(s); return !!e && getComputedStyle(e).display !== 'none'; };
      return { header: shown('.site-header'), composer: shown('#composer'), tools: shown('.fig-tools'), verdict: shown('#verdict'), steps: shown('#steps'), fig: shown('#fig-viewport'), bg: getComputedStyle(document.body).backgroundColor, ink: getComputedStyle(document.documentElement).getPropertyValue('--fig-ink').trim() };
    });
    check(!pr.header && !pr.composer && !pr.tools && pr.verdict && pr.steps && pr.fig && pr.bg === 'rgb(255, 255, 255)' && pr.ink === '#1b1f24', `${tag}: print shows verdict, figure and steps on white with light figure ink, no header/composer/tools (${JSON.stringify(pr)})`);
    await shot('print', true);
    await page.emulateMedia({ media: 'screen' });

    await page.evaluate(() => window.scrollTo(0, 0));
    await page.click('#tab-describe');
    await page.fill('#describe-input', 'Triangle ABC with AB = AC. Prove the base angles are equal.');
    await page.click('#solve');
    await settle(page);
    check(/Proved|Demonstrat/.test(await visibleText(page, '#verdict')), `${tag}: Describe -> AI -> proof`);
    await humanAbsent(page, tag, check);

    await page.click('#tab-geo');
    const buf = fs.readFileSync(PHOTO);
    const pasted = await page.evaluate(async (b64) => {
      const bin = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
      const file = new File([bin], 'screenshot.png', { type: 'image/png' });
      let ev;
      try { const dt = new DataTransfer(); dt.items.add(file); ev = new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }); } catch (e) { return 'unsupported'; }
      if (!ev.clipboardData || !ev.clipboardData.files.length) return 'unsupported';
      document.body.dispatchEvent(ev);
      return 'ok';
    }, buf.toString('base64'));
    if (pasted === 'unsupported') skip(`${tag}: paste a screenshot (this engine cannot build a synthetic ClipboardEvent with files)`);
    else {
      await page.waitForSelector('#dz-full:not([hidden])', { timeout: 30000 });
      check(await page.evaluate(() => document.getElementById('tab-photo').getAttribute('aria-selected') === 'true'), `${tag}: pasting an image switches to Photo and prepares it (${await page.textContent('#photo-meta')})`);
      await page.click('#photo-remove');
    }
    await page.click('#tab-geo');
    const dropped = await page.evaluate(async (b64) => {
      const bin = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
      const file = new File([bin], 'problem.png', { type: 'image/png' });
      let dt;
      try { dt = new DataTransfer(); dt.items.add(file); } catch (e) { return 'unsupported'; }
      const target = document.querySelector('.site-footer');
      const over = new DragEvent('dragover', { dataTransfer: dt, bubbles: true, cancelable: true });
      if (!over.dataTransfer) return 'unsupported';
      target.dispatchEvent(over);
      const drop = new DragEvent('drop', { dataTransfer: dt, bubbles: true, cancelable: true });
      target.dispatchEvent(drop);
      return over.defaultPrevented && drop.defaultPrevented ? 'ok' : 'not-prevented';
    }, buf.toString('base64'));
    if (dropped === 'unsupported') skip(`${tag}: drop outside the drop zone (no synthetic DragEvent with files here)`);
    else {
      await page.waitForSelector('#dz-full:not([hidden])', { timeout: 30000 }).catch(() => {});
      check(dropped === 'ok' && await page.evaluate(() => !document.getElementById('dz-full').hidden), `${tag}: an image dropped outside the drop zone is taken, not opened by the browser (${dropped})`);
    }
    if (await page.evaluate(() => document.getElementById('tab-photo').getAttribute('aria-selected') !== 'true')) {
      await page.click('#tab-photo');
      await page.setInputFiles('#photo-input', PHOTO);
      await page.waitForSelector('#dz-full:not([hidden])', { timeout: 30000 });
    }
    await page.locator('#dropzone').scrollIntoViewIfNeeded();
    await shot('photo');
    await page.click('#solve');
    await settle(page);
    check(/Proved|Demonstrat/.test(await visibleText(page, '#verdict')), `${tag}: Photo -> AI -> proof`);

    await page.click('#tab-geo');
    await page.fill('#geo-input', 'A B C = triangle\nprove perp(A, B, Q)');
    await page.click('#solve');
    await settle(page);
    const err = await visibleText(page, '#state-error');
    check(err.length > 0, `${tag}: a bad program shows the error card ("${err.split('\n')[0]}")`);
    await page.evaluate(() => window.scrollTo(0, 0));
    await shot('error');

    await page.route('**/api/solve', (route) => route.fulfill({ status: 503, headers: { 'retry-after': '30', 'content-type': 'application/json' }, body: JSON.stringify({ error: 'The server is busy.', code: 'busy' }) }));
    await page.fill('#geo-input', 'A B C = triangle\nH = orthocenter(A, B, C)\nprove perp(A, H, B, C)');
    await page.click('#solve');
    await page.waitForSelector('#state-error:not([hidden])', { timeout: 15000 });
    const busy = await visibleText(page, '#state-error');
    check(busy.length > 0, `${tag}: a busy server shows its card ("${busy.split('\n')[0]}")`);
    await shot('busy');
    await page.unroute('**/api/solve');

    await page.click('[data-theme-toggle]');
    const themeMenu = await page.waitForSelector('.menu:not([hidden]) [role="menuitemradio"]', { timeout: 3000 }).then(() => true).catch(() => false);
    if (themeMenu) { await page.keyboard.press('Escape'); }
    const other = (prof.locale || '').startsWith('ro') ? 'en' : 'ro';
    await page.click(`[data-lang-set="${other}"]`);
    await page.waitForTimeout(300);
    f = await fits(page);
    check(await page.evaluate(() => document.documentElement.lang) === other && f.sw <= f.vw, `${tag}: switch to ${other.toUpperCase()}, fits ${f.sw}/${f.vw}`);
    await shot(`lang-${other}`);
    await page.click(`[data-lang-set="${other === 'ro' ? 'en' : 'ro'}"]`);
    await page.evaluate(() => { localStorage.removeItem('lang'); document.cookie = 'lang=; Max-Age=0; path=/'; });
  } catch (e) {
    check(false, `${tag}: flow aborted: ${e.message.split('\n')[0]}`);
    await shot('ABORT').catch(() => {});
  }
  check(!errs.length, `${tag}: no page/console errors ${errs.slice(0, 3).join(' | ')}`);
  await ctx.close();
}

async function acctRun(browser, eng) {
  const tag = `acct-${eng}`;
  let n = 0;
  const shot = (name, full = false) => page.screenshot({ path: `${OUT}/${tag}-${String(++n).padStart(2, '0')}-${name}.png`, fullPage: full, animations: 'disabled' });
  const { ctx, page, errs } = await newPage(browser, eng, { viewport: { width: 1440, height: 900 }, dpr: 1 });
  try {
    await page.goto(`${ACCT}/`);
    await page.waitForLoadState('networkidle');
    let f = await fits(page);
    check(/account/i.test(await page.textContent('body')) && f.sw <= f.vw, `${tag}: landing, fits ${f.sw}/${f.vw}`);
    await shot('landing', true);
    await page.goto(`${ACCT}/auth?mode=register`);
    await page.waitForLoadState('networkidle');
    const user = `k${eng.slice(0, 2)}${process.pid % 1000}${Math.floor(Math.random() * 1e5)}`;
    await page.focus('#username');
    await page.keyboard.type(user);
    const pwf = page.locator('input[type=password]');
    for (let i = 0; i < await pwf.count(); i++) await pwf.nth(i).fill('correct horse battery');
    await Promise.all([page.waitForURL((u) => new URL(u).pathname === '/app', { timeout: 15000 }), page.locator('input[type=password]').last().press('Enter')]);
    await page.waitForFunction(() => window.GS && !document.getElementById('ai-pill').classList.contains('checking'));
    await page.click('#tab-geo');
    await page.focus('#geo-input');
    await page.keyboard.press('Control+Enter');
    await settle(page);
    await page.waitForSelector('#hist-list .hist-open', { timeout: 10000 });
    const rail = await page.evaluate(() => getComputedStyle(document.getElementById('rail')).position);
    check(rail === 'sticky', `${tag}: history rail is beside the composer at 1440 (${rail})`);
    await page.fill('#geo-input', 'A B C = triangle\nM = midpoint(A, B)\nN = midpoint(A, C)\nprove para(M, N, B, C)');
    await page.click('#solve');
    await settle(page);
    await page.focus('#hist-list .hist-item:nth-child(2) .hist-open');
    await page.keyboard.press('Enter');
    await page.waitForTimeout(1200);
    const ed = await page.inputValue('#geo-input');
    check(/orthocenter|reflect/.test(ed), `${tag}: Enter on a history row reopens it`);
    await shot('history');
    await page.setViewportSize({ width: 1024, height: 768 });
    await page.waitForTimeout(300);
    await page.focus('#rail-toggle');
    await page.keyboard.press('Enter');
    await page.waitForTimeout(400);
    const dr = await page.evaluate(() => ({ open: document.body.classList.contains('drawer-open'), focus: document.activeElement.id }));
    await shot('drawer-1024');
    await page.keyboard.press('Escape');
    await page.waitForTimeout(300);
    const dc = await page.evaluate(() => ({ open: document.body.classList.contains('drawer-open'), focus: document.activeElement.id }));
    check(dr.open && dr.focus === 'hist-search' && !dc.open && dc.focus === 'rail-toggle', `${tag}: at 1024 the history drawer opens by keyboard (focus ${dr.focus}) and Escape closes it (focus ${dc.focus})`);
    f = await fits(page);
    check(f.sw <= f.vw, `${tag}: signed-in app at 1024 fits ${f.sw}/${f.vw}`);
  } catch (e) {
    check(false, `${tag}: flow aborted: ${e.message.split('\n')[0]}`);
    await shot('ABORT').catch(() => {});
  }
  check(!errs.length, `${tag}: no page/console errors ${errs.slice(0, 3).join(' | ')}`);
  await ctx.close();
}

for (const eng of ENGINES) {
  const browser = await pw[eng].launch();
  for (const prof of PROFILES) if (prof.engines.includes(eng) && (!process.env.PROFILE || process.env.PROFILE === prof.name)) await guestRun(browser, eng, prof);
  if (ACCT) await acctRun(browser, eng);
  await browser.close();
}
fs.writeFileSync(`${OUT}/results-desktop.json`, JSON.stringify(results, null, 1));
log(`screenshots: ${OUT}`);
log(fails ? `${fails} FAILURES of ${results.length}` : `ALL ${results.length} PASS (${results.filter((r) => r.skip).length} skipped)`);
process.exit(fails ? 1 : 0);
