import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';

const { webkit, devices } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const PHOTO = process.env.PHOTO || path.resolve(path.dirname(new URL(import.meta.url).pathname), '../../../docs/example-report.png');
const STUB_LOG = process.env.FAKE_CLAUDE_LOG || `${process.env.TMPDIR || '/tmp'}/agstudio-fake-claude`;
const OUT = process.env.SHOTS || fs.mkdtempSync(path.join(os.tmpdir(), 'iphone-e2e-'));
const GUEST = process.env.GUEST_BASE || 'http://127.0.0.1:8787';
const ACCT = process.env.ACCOUNTS_BASE || '';
const PW = process.env.BASIC_PASSWORD || '';
fs.mkdirSync(OUT, { recursive: true });

const only = (process.env.ONLY || '').split(',').filter(Boolean);
const DEVS = ['iPhone SE', 'iPhone 15', 'iPhone 15 Pro Max'];
const results = [];
let fails = 0;
const log = (...a) => console.log(...a);
const check = (ok, what) => { results.push({ ok: !!ok, what }); log(`${ok ? 'PASS' : 'FAIL'} ${what}`); if (!ok) fails++; };

const SHARE_STUB = () => {
  window.__shared = [];
  navigator.canShare = (d) => !!(d && Array.isArray(d.files) && d.files.length && d.files.every((f) => f instanceof File));
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

const browser = await webkit.launch();

function fits(page) {
  return page.evaluate(() => ({ sw: document.documentElement.scrollWidth, vw: document.documentElement.clientWidth }));
}

async function newCtx(devName, scheme, opts = {}) {
  const ctx = await browser.newContext({ ...devices[devName], colorScheme: scheme, locale: opts.locale || 'en-US', acceptDownloads: true });
  if (opts.share) await ctx.addInitScript(SHARE_STUB);
  const page = await ctx.newPage();
  const errs = [];
  page.on('pageerror', (e) => errs.push(`pageerror: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error' && !/401|Failed to load resource/.test(m.text())) errs.push(`console: ${m.text()}`); });
  page.on('dialog', (d) => { errs.push(`dialog ${d.type()}: ${d.message()}`); d.dismiss(); });
  return { ctx, page, errs };
}

function mkShot(tag) {
  let n = 0;
  return async (page, name, full = false) => {
    const p = `${OUT}/${tag}-${String(++n).padStart(2, '0')}-${name}.png`;
    await page.screenshot({ path: p, fullPage: full, animations: 'disabled' });
    return p;
  };
}

async function passGate(page, base, path = '/') {
  await page.goto(`${base}${path}`);
  check(new URL(page.url()).pathname === '/gate', `gate shown for ${path} (${page.url()})`);
  await page.fill('#pw', PW);
  await Promise.all([page.waitForURL((u) => new URL(u).pathname !== '/gate'), page.click('.gate-submit')]);
}

async function pinchAndPan(page) {
  await page.evaluate(() => document.getElementById('fig-viewport').scrollIntoView({ block: 'center' }));
  await page.waitForTimeout(250);
  const vb = () => page.evaluate(() => document.querySelector('#fig-viewport > svg').getAttribute('viewBox'));
  const v0 = await vb();
  const pinched = await page.evaluate(() => {
    const vp = document.getElementById('fig-viewport');
    const r = vp.getBoundingClientRect();
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    const ev = (type, id, x, y) => vp.dispatchEvent(new PointerEvent(type, { pointerId: id, pointerType: 'touch', isPrimary: id === 11, clientX: x, clientY: y, bubbles: true, cancelable: true, button: 0, buttons: type === 'pointerup' ? 0 : 1 }));
    ev('pointerdown', 11, cx - 30, cy); ev('pointerdown', 12, cx + 30, cy);
    for (let i = 1; i <= 6; i++) { ev('pointermove', 11, cx - 30 - i * 12, cy); ev('pointermove', 12, cx + 30 + i * 12, cy); }
    ev('pointerup', 12, cx + 102, cy); ev('pointerup', 11, cx - 102, cy);
    return document.querySelector('#fig-viewport > svg').getAttribute('viewBox');
  });
  const w = (s) => Number(String(s).split(/[ ,]+/)[2]);
  check(w(pinched) < w(v0) * 0.8, `pinch zooms the figure (viewBox w ${w(v0).toFixed(1)} -> ${w(pinched).toFixed(1)})`);
  const panned = await page.evaluate(() => {
    const vp = document.getElementById('fig-viewport');
    const r = vp.getBoundingClientRect();
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    const ev = (type, x, y) => vp.dispatchEvent(new PointerEvent(type, { pointerId: 21, pointerType: 'touch', isPrimary: true, clientX: x, clientY: y, bubbles: true, cancelable: true, button: 0, buttons: type === 'pointerup' ? 0 : 1 }));
    ev('pointerdown', cx, cy);
    for (let i = 1; i <= 5; i++) ev('pointermove', cx + i * 8, cy);
    ev('pointerup', cx + 40, cy);
    return document.querySelector('#fig-viewport > svg').getAttribute('viewBox');
  });
  const x = (s) => Number(String(s).split(/[ ,]+/)[0]);
  check(x(panned) !== x(pinched), `one-finger drag pans the zoomed figure (viewBox x ${x(pinched).toFixed(1)} -> ${x(panned).toFixed(1)})`);
}

async function settle(page) {
  await page.waitForSelector('#state-solving:not([hidden])', { timeout: 10000 });
  await page.waitForSelector('#state-solving', { state: 'hidden', timeout: 120000 });
  await page.waitForSelector('#verdict:not([hidden]), #state-error:not([hidden])', { timeout: 10000 });
  await page.waitForTimeout(400);
}

async function solveExample(page) {
  const chip = page.locator('#example-chips [data-example-solve="ortho"]');
  if (await chip.count()) await chip.click();
  else await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
  await page.waitForSelector('#verdict:not([hidden])', { timeout: 60000 });
  await page.waitForTimeout(600);
}

async function exportFlow(page, tag, shot, share) {
  for (const fmt of ['pdf', 'png']) {
    await page.locator('#export-btn').scrollIntoViewIfNeeded();
    if (share) {
      await page.click('#export-btn');
      await page.click(`[data-export="${fmt}"]`);
      const tbtn = page.locator('#toasts .toast button').filter({ hasText: /Share|Partajeaz|Salveaz/ });
      await tbtn.first().waitFor({ timeout: 60000 });
      await page.waitForTimeout(400);
      await shot(page, `export-${fmt}-ready`);
      await tbtn.first().click();
      await page.waitForTimeout(500);
      const sh = await page.evaluate(() => window.__shared);
      const f = sh[sh.length - 1] && sh[sh.length - 1].files[0];
      const magic = fmt === 'pdf' ? '25504446' : '89504e47';
      check(f && f.head.startsWith(magic) && f.size > 1000, `${tag}: ${fmt.toUpperCase()} handed to the share sheet from a fresh tap (${f && f.name}, ${f && f.size} B)`);
    } else {
      const dl = page.waitForEvent('download', { timeout: 60000 });
      await page.click('#export-btn');
      await page.click(`[data-export="${fmt}"]`);
      const d = await dl;
      const p = await d.path();
      const size = p ? fs.statSync(p).size : 0;
      const head = p ? fs.readFileSync(p).subarray(0, 4).toString('hex') : '';
      check(size > 1000 && head === (fmt === 'pdf' ? '25504446' : '89504e47'), `${tag}: ${fmt.toUpperCase()} downloads (${d.suggestedFilename()}, ${size} B)`);
      await page.waitForTimeout(300);
      await shot(page, `export-${fmt}-downloaded`);
    }
  }
}

async function guestRun(devBase, land, scheme) {
  const devName = devBase + (land ? ' landscape' : '');
  const tag = `${devBase.replace(/ /g, '')}-${land ? 'land' : 'port'}-${scheme}`;
  const shot = mkShot(tag);
  const share = !land;
  const { ctx, page, errs } = await newCtx(devName, scheme, { share });
  try {
    await page.goto(`${GUEST}/`);
    await page.waitForLoadState('networkidle');
    let f = await fits(page);
    check(new URL(page.url()).pathname === '/gate' && f.sw <= f.vw, `${tag}: / -> gate page, fits (${f.sw}/${f.vw})`);
    await shot(page, 'gate');
    await page.fill('#pw', PW);
    await Promise.all([page.waitForURL((u) => new URL(u).pathname === '/'), page.click('.gate-submit')]);
    await page.waitForFunction(() => window.GS && !document.getElementById('ai-pill').classList.contains('checking'));
    await page.waitForTimeout(300);
    const st = await page.evaluate(() => {
      const vis = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden'; };
      const pill = ['ai-pill', 'ai-pill-m'].map((id) => document.getElementById(id)).find(vis);
      return {
        pill: pill ? `${pill.id}: ${pill.innerText.trim()} (${pill.className})` : 'none visible',
        dots: [...document.querySelectorAll('.tab-ai-dot')].filter(vis).length,
        sw: document.documentElement.scrollWidth, vw: document.documentElement.clientWidth,
        headerRight: Math.max(...[...document.querySelectorAll('.site-header .inner *')].filter((e) => e.offsetParent).map((e) => e.getBoundingClientRect().right)),
      };
    });
    check(/ on\)|on$/.test(st.pill) || /\bon\b/.test(st.pill), `${tag}: past the gate, app loads; AI pill "${st.pill}"`);
    check(st.dots === 0, `${tag}: no "AI unavailable" dots on Describe/Photo (${st.dots})`);
    check(st.sw <= st.vw && st.headerRight <= st.vw + 0.5, `${tag}: app fits (scroll ${st.sw}/${st.vw}, header right ${Math.round(st.headerRight)})`);
    await shot(page, 'app');

    await page.click('#tab-geo');
    await solveExample(page);
    const head = await page.textContent('#verdict');
    check(/Proved/i.test(head), `${tag}: example solves -> "${head.trim().slice(0, 40)}"`);
    f = await fits(page);
    check(f.sw <= f.vw, `${tag}: result fits (${f.sw}/${f.vw})`);
    await page.locator('#verdict').scrollIntoViewIfNeeded();
    await shot(page, 'result');

    const step = page.locator('#steps li.step:not(.is-group):not([hidden])').nth(1);
    await step.scrollIntoViewIfNeeded();
    const sb = await step.boundingBox();
    await page.touchscreen.tap(sb.x + 40, sb.y + Math.min(sb.height / 2, 20));
    await page.waitForTimeout(400);
    const lit = await page.evaluate(() => {
      const p = document.getElementById('fig-peek');
      const fig = document.querySelector('#fig-viewport .hl');
      return { peek: p && !p.hidden ? p.querySelectorAll('.hl').length : 0, lit: document.querySelectorAll('#fig-viewport .hl').length, sel: !!document.querySelector('#steps .step.is-active, #steps .step[aria-current], #steps .step.active, #steps .step.sel') };
    });
    check(lit.lit > 0, `${tag}: tapping a step lights the figure (${lit.lit} lit, peek ${lit.peek})`);
    await shot(page, 'step-tap');

    await pinchAndPan(page);
    await page.waitForFunction(() => { const p = document.getElementById('fig-peek'); return !p || p.hidden; }, null, { timeout: 3000 }).catch(() => {});
    check(await page.evaluate(() => { const p = document.getElementById('fig-peek'); return !p || p.hidden; }), `${tag}: the floating thumbnail hides once the figure is on screen`);
    await shot(page, 'figure-zoomed');
    await page.click('#z-fit').catch(() => {});

    await exportFlow(page, tag, shot, share);

    await page.locator('#ptab-ai').scrollIntoViewIfNeeded();
    await page.click('#ptab-ai');
    await page.waitForFunction(() => /stub/.test(document.getElementById('ai-text').innerText), null, { timeout: 30000 });
    const ai = await page.textContent('#ai-text');
    check(/stub/.test(ai), `${tag}: AI explanation arrives (${ai.trim().slice(0, 60)}…)`);
    await page.locator('#proof-tabs').scrollIntoViewIfNeeded();
    await shot(page, 'ai-explanation');

    await page.evaluate(() => window.scrollTo(0, 0));
    await page.click('#tab-describe');
    await page.fill('#describe-input', 'Triangle ABC with AB = AC. Prove the base angles are equal.');
    await page.click('#solve');
    await settle(page);
    const dv = await page.evaluate(() => ({ v: document.getElementById('verdict').hidden ? '' : document.getElementById('verdict').innerText, e: document.getElementById('state-error').hidden ? '' : document.getElementById('state-error').innerText }));
    check(/Proved/i.test(dv.v), `${tag}: guest Describe -> stub -> "${(dv.v || dv.e).trim().split('\n')[0]}"`);
    await page.evaluate(() => window.scrollTo(0, 0));
    await shot(page, 'describe-solved');

    await page.click('#tab-photo');
    await page.setInputFiles('#photo-input', PHOTO);
    await page.waitForSelector('#dz-full:not([hidden])', { timeout: 30000 });
    const meta = await page.textContent('#photo-meta');
    await page.locator('#dropzone').scrollIntoViewIfNeeded();
    await shot(page, 'photo-prepared');
    await page.click('#solve');
    await settle(page);
    const up = fs.existsSync(`${STUB_LOG}/problem.jpg`) ? fs.statSync(`${STUB_LOG}/problem.jpg`) : null;
    check(up && Date.now() - up.mtimeMs < 20000 && up.size < 2e6, `${tag}: the model received a fresh problem.jpg (${up && up.size} B)`);
    const pv = await page.evaluate(() => ({ v: document.getElementById('verdict').hidden ? '' : document.getElementById('verdict').innerText, e: document.getElementById('state-error').hidden ? '' : document.getElementById('state-error').innerText }));
    check(/Proved/i.test(pv.v), `${tag}: guest Photo (${meta.trim()}) -> "${(pv.v || pv.e).trim().split('\n')[0]}"`);
    await shot(page, 'photo-solved');

    await page.evaluate(() => window.scrollTo(0, 0));
    await page.click('[data-lang-set="ro"]');
    await page.waitForTimeout(400);
    const ro = await page.evaluate(() => ({ lang: document.documentElement.lang, solve: document.getElementById('solve').innerText.trim(), sw: document.documentElement.scrollWidth, vw: document.documentElement.clientWidth }));
    check(ro.lang === 'ro' && ro.sw <= ro.vw, `${tag}: RO switch (lang=${ro.lang}, solve "${ro.solve}", fits ${ro.sw}/${ro.vw})`);
    await shot(page, 'ro-app');
    await page.locator('#ptab-ai').scrollIntoViewIfNeeded();
    await page.click('#ptab-ai');
    await page.waitForFunction(() => /Deoarece|stub/.test(document.getElementById('ai-text').innerText), null, { timeout: 30000 }).catch(() => {});
    const aiRo = await page.textContent('#ai-text');
    check(/Deoarece/.test(aiRo), `${tag}: RO AI explanation is Romanian (${aiRo.trim().slice(0, 40)}…)`);
    await page.locator('#proof-tabs').scrollIntoViewIfNeeded();
    await shot(page, 'ro-ai');
    await page.evaluate(() => { localStorage.removeItem('lang'); document.cookie = 'lang=; Max-Age=0; path=/'; });
  } catch (e) {
    check(false, `${tag}: flow aborted: ${e.message.split('\n')[0]}`);
    await shot(page, 'ABORT').catch(() => {});
  }
  check(!errs.length, `${tag}: no page/console errors ${errs.slice(0, 3).join(' | ')}`);
  await ctx.close();
}

async function acctRun(devBase, land, scheme) {
  const devName = devBase + (land ? ' landscape' : '');
  const tag = `acct-${devBase.replace(/ /g, '')}-${land ? 'land' : 'port'}-${scheme}`;
  const shot = mkShot(tag);
  const { ctx, page, errs } = await newCtx(devName, scheme);
  try {
    await passGate(page, ACCT, '/');
    await page.waitForLoadState('networkidle');
    let f = await fits(page);
    check(/Create a free account|account/i.test(await page.textContent('body')) && f.sw <= f.vw, `${tag}: landing after gate, fits (${f.sw}/${f.vw})`);
    await shot(page, 'landing');
    await shot(page, 'landing-full', true);
    await page.goto(`${ACCT}/auth?mode=register`);
    await page.waitForLoadState('networkidle');
    f = await fits(page);
    check(f.sw <= f.vw, `${tag}: register page fits (${f.sw}/${f.vw})`);
    await shot(page, 'register');
    const user = `u${process.pid % 10000}${tag.length}${Math.floor(Math.random() * 1e6)}`.slice(0, 20);
    const inputs = await page.evaluate(() => [...document.querySelectorAll('input')].map((i) => `${i.id}:${i.type}:${getComputedStyle(i).fontSize}`));
    check(inputs.every((s) => s.endsWith(':16px') || /hidden|checkbox/.test(s)), `${tag}: auth inputs are 16px (${inputs.join(' ')})`);
    await page.fill('#username', user);
    const pwFields = page.locator('input[type=password]');
    for (let i = 0; i < await pwFields.count(); i++) await pwFields.nth(i).fill('correct horse battery');
    await Promise.all([page.waitForURL((u) => new URL(u).pathname === '/app', { timeout: 15000 }), page.click('form button[type=submit]')]);
    await page.waitForFunction(() => window.GS && !document.getElementById('ai-pill').classList.contains('checking'));
    await page.waitForTimeout(300);
    await shot(page, 'app-signed-in');
    await solveExample(page);
    await page.waitForTimeout(500);
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.click('#rail-toggle');
    await page.waitForTimeout(500);
    const hist = await page.evaluate(() => ({ items: document.querySelectorAll('#hist-list li, #hist-list .hist-item').length, open: document.body.classList.contains('drawer-open'), focus: document.activeElement && (document.activeElement.id || document.activeElement.className) }));
    check(hist.open && hist.items >= 1, `${tag}: history drawer opens with ${hist.items} item(s), focus on ${hist.focus}`);
    await shot(page, 'history');
    await page.goto(`${ACCT}/auth`);
    await page.waitForLoadState('networkidle');
    await shot(page, 'auth-signin-redirect');
  } catch (e) {
    check(false, `${tag}: flow aborted: ${e.message.split('\n')[0]}`);
    await shot(page, 'ABORT').catch(() => {});
  }
  check(!errs.length, `${tag}: no page/console errors ${errs.slice(0, 3).join(' | ')}`);
  await ctx.close();
}

const runs = [];
for (const d of DEVS) for (const land of [false, true]) for (const s of ['light', 'dark']) runs.push([d, land, s]);
const want = (k) => !only.length || only.includes(k);
const pick = (r) => !process.env.DEV || r[0] === process.env.DEV;
if (want('guest')) {
  for (let i = 0; i < runs.length; i += 3) await Promise.all(runs.slice(i, i + 3).filter(pick).map((r) => guestRun(...r)));
}
if (want('acct') && ACCT) {
  for (let i = 0; i < runs.length; i += 4) await Promise.all(runs.slice(i, i + 4).filter(pick).map((r) => acctRun(...r)));
}
await browser.close();
fs.writeFileSync(`${OUT}/results-${only.join('_') || 'all'}.json`, JSON.stringify(results, null, 1));
log(`screenshots: ${OUT}`);
log(fails ? `${fails} FAILURES of ${results.length}` : `ALL ${results.length} PASS`);
process.exit(fails ? 1 : 0);
