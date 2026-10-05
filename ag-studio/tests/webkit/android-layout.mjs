import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';

const { chromium, firefox, devices } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const OUT = process.env.SHOTS || fs.mkdtempSync(path.join(os.tmpdir(), 'android-layout-'));
const BASE = process.env.BASE || 'http://127.0.0.1:8787';
const ACCT = process.env.ACCOUNTS_BASE || '';
const PW = process.env.BASIC_PASSWORD || '';
const ENGINES = (process.env.ENGINES || 'chromium,firefox').split(',');
const ONLY = (process.env.DEVS || '').split(',').filter(Boolean);
const SCALES = (process.env.FONT_SCALES || '100').split(',').map(Number);
fs.mkdirSync(OUT, { recursive: true });

const UA = devices['Pixel 7'].userAgent;
const FF_UA = 'Mozilla/5.0 (Android 14; Mobile; rv:143.0) Gecko/143.0 Firefox/143.0';
const DEVS = [
  { name: 'Pixel 7', w: 412, h: 839, dsf: 2.625 },
  { name: 'Pixel 5', w: 393, h: 727, dsf: 2.75 },
  { name: 'Galaxy S9+', w: 320, h: 658, dsf: 4.5 },
  { name: 'Galaxy S23', w: 360, h: 780, dsf: 3 },
  { name: 'Moto G4', w: 360, h: 640, dsf: 3 },
  { name: 'Fold cover', w: 280, h: 653, dsf: 3 },
  { name: 'Fold open', w: 717, h: 512, dsf: 2 },
  { name: 'Fold4 open', w: 673, h: 841, dsf: 2.5 },
  { name: 'Small 320', w: 320, h: 568, dsf: 2 },
  { name: 'Zoom125 360', w: 288, h: 624, dsf: 3.75 },
  { name: 'Zoom150 360', w: 240, h: 520, dsf: 4.5 },
  { name: 'Zoom130 Fold cover', w: 215, h: 502, dsf: 3.9 },
  { name: 'Zoom200 360', w: 180, h: 390, dsf: 6 },
].filter((d) => !ONLY.length || ONLY.includes(d.name));

let fails = 0;
const findings = [];
const log = (...a) => console.log(...a);
const check = (ok, what, detail) => {
  if (!ok) { fails++; findings.push({ what, detail }); }
  log(`${ok ? 'PASS' : 'FAIL'} ${what}${!ok && detail ? ' :: ' + JSON.stringify(detail).slice(0, 900) : ''}`);
};

const MEASURE = () => {
  const vw = document.documentElement.clientWidth, vh = window.innerHeight;
  const vis = (el) => {
    if (el.closest('[hidden], [inert]')) return false;
    const cs = getComputedStyle(el);
    if (cs.visibility === 'hidden' || cs.display === 'none' || Number(cs.opacity) === 0) return false;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  };
  const clipped = (el) => {
    for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
      const cs = getComputedStyle(p);
      if (/(auto|scroll|hidden|clip)/.test(cs.overflowX)) return true;
      if (cs.position === 'fixed') break;
    }
    return false;
  };
  const name = (el) => {
    let s = el.tagName.toLowerCase();
    if (el.id) s += '#' + el.id;
    else if (el.classList.length) s += '.' + [...el.classList].slice(0, 2).join('.');
    const txt = (el.getAttribute('aria-label') || el.innerText || '').trim().replace(/\s+/g, ' ').slice(0, 24);
    return txt ? `${s}"${txt}"` : s;
  };
  const off = [];
  for (const el of document.body.querySelectorAll('*')) {
    if (el.closest('.sr-only, svg, .skip-link') && !el.matches('svg')) continue;
    if (el.closest('.sr-only, .skip-link')) continue;
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    if (r.right <= vw + 0.5 && r.left >= -0.5) continue;
    if (!vis(el) || clipped(el)) continue;
    off.push(`${name(el)} [${Math.round(r.left)},${Math.round(r.right)}]`);
  }
  const textClip = [];
  for (const el of document.body.querySelectorAll('button, a, label, h1, h2, h3, p, span, li, .chip, .status-pill')) {
    if (!vis(el) || el.closest('.sr-only, .editor, pre, .fig-viewport, .hist-meta time')) continue;
    const cs = getComputedStyle(el);
    if (cs.textOverflow === 'ellipsis' || el.children.length > 3) continue;
    if (cs.clip !== 'auto' || [...el.querySelectorAll('*')].some((c) => getComputedStyle(c).clip !== 'auto')) continue;
    if (/(hidden|clip)/.test(cs.overflowX) && el.scrollWidth > el.clientWidth + 1) textClip.push(`${name(el)} ${el.scrollWidth}>${el.clientWidth}`);
    else if (/^(BUTTON|A)$/.test(el.tagName) && cs.display !== 'inline') {
      const rg = document.createRange(); rg.selectNodeContents(el);
      const tr = rg.getBoundingClientRect(), er = el.getBoundingClientRect();
      if (tr.width && (tr.right > er.right + 1 || tr.left < er.left - 1 || tr.bottom > er.bottom + 1)) textClip.push(`${name(el)} text ${Math.round(tr.left)}..${Math.round(tr.right)} in ${Math.round(er.left)}..${Math.round(er.right)}`);
    }
  }
  const SEL = 'a[href], button, input:not([type=hidden]), textarea, select, summary, [role=tab], [role=menuitem], [role=radio], label.dropzone';
  const targets = [...document.querySelectorAll(SEL)].filter((el) => vis(el) && !el.closest('.sr-only') && !(el.matches('input[type=file]')) && !el.closest('p, li.step .step-stmt, .verdict-text, .gate-note, .hint, .muted') || (vis(el) && el.matches('.forget-device button, .err-where .link-btn, .refine .link-btn, .guest-note a')));
  const hit = (el) => {
    const r = el.getBoundingClientRect();
    let L = r.left, T = r.top, R = r.right, B = r.bottom;
    for (const ps of ['::after', '::before']) {
      const cs = getComputedStyle(el, ps);
      if (cs.content === 'none' || cs.position !== 'absolute') continue;
      const px = (v) => (v.endsWith('px') ? parseFloat(v) : null);
      const t = px(cs.top), l = px(cs.left), rr = px(cs.right), b = px(cs.bottom);
      if (t !== null && b !== null && l !== null && rr !== null) {
        L = Math.min(L, r.left + l); T = Math.min(T, r.top + t); R = Math.max(R, r.right - rr); B = Math.max(B, r.bottom - b);
      }
    }
    return { L, T, R, B, w: R - L, h: B - T };
  };
  const onTop = (el) => {
    const r = el.getBoundingClientRect();
    const x = Math.min(Math.max(r.left + r.width / 2, 1), vw - 1), y = r.top + r.height / 2;
    if (y < 0 || y > vh) return false;
    const at = document.elementFromPoint(x, y);
    return !!at && (el.contains(at) || at.contains(el) || (el.id && at.closest(`label[for="${el.id}"]`)));
  };
  const boxes = targets.filter(onTop).map((el) => ({ el, b: hit(el) }));
  const small = [];
  for (const x of boxes) {
    const { b } = x;
    if (b.w >= 48 && b.h >= 48) continue;
    if (vw < 360 && x.el.closest('.site-header') && b.w >= 43.5 && b.h >= 43.5) continue;
    const cx = (b.L + b.R) / 2, cy = (b.T + b.B) / 2;
    const fL = cx - 24, fR = cx + 24, fT = cy - 24, fB = cy + 24;
    const near = boxes.filter((o) => o !== x && !o.el.contains(x.el) && !x.el.contains(o.el) && o.b.L < fR && o.b.R > fL && o.b.T < fB && o.b.B > fT);
    const hard = b.w < 43.5 || b.h < 43.5;
    if (hard || near.length) small.push(`${name(x.el)} ${Math.round(b.w)}x${Math.round(b.h)}${near.length ? ' near ' + near.slice(0, 2).map((o) => name(o.el)).join(',') : ''}${hard ? ' <44' : ''}`);
  }
  const hdr = document.querySelector('.site-header .inner');
  let hdrOverlap = [];
  if (hdr) {
    const items = [...hdr.querySelectorAll('.wordmark, .rail-toggle, .lang-seg, [data-theme-toggle], #signin, #account, .header-cta .btn, .header-cta a, .status-pill')].filter(vis);
    for (let i = 0; i < items.length; i++) for (let j = i + 1; j < items.length; j++) {
      if (items[i].contains(items[j]) || items[j].contains(items[i])) continue;
      const a = items[i].getBoundingClientRect(), c = items[j].getBoundingClientRect();
      if (a.left < c.right - 0.5 && a.right > c.left + 0.5 && a.top < c.bottom - 0.5 && a.bottom > c.top + 0.5) hdrOverlap.push(`${name(items[i])} x ${name(items[j])}`);
    }
  }
  return {
    vw, vh, sw: document.documentElement.scrollWidth, bodySw: document.body.scrollWidth,
    off: off.slice(0, 12), textClip: textClip.slice(0, 12), small: small.slice(0, 40), hdrOverlap,
    coarse: matchMedia('(pointer: coarse)').matches, hoverNone: matchMedia('(hover: none)').matches,
    hdrH: hdr ? Math.round(hdr.getBoundingClientRect().height) : 0,
    rootFs: getComputedStyle(document.documentElement).fontSize,
  };
};

async function shot(page, file, full = false) {
  await page.screenshot({ path: path.join(OUT, file), fullPage: full, animations: 'disabled' }).catch((e) => log(`shot ${file} failed: ${e.message}`));
}

async function gate(page, base) {
  await page.goto(`${base}/`);
  if (new URL(page.url()).pathname === '/gate') {
    await page.fill('#pw', PW);
    await Promise.all([page.waitForURL((u) => new URL(u).pathname !== '/gate'), page.click('.gate-submit')]);
  }
}

function ctxOpts(engine, d, land, scheme) {
  const w = land ? d.h : d.w, h = land ? d.w : d.h;
  const o = { viewport: { width: w, height: h }, colorScheme: scheme, locale: 'en-US', acceptDownloads: true, hasTouch: true };
  if (engine === 'firefox') o.userAgent = FF_UA;
  if (engine === 'chromium') Object.assign(o, { userAgent: UA, deviceScaleFactor: d.dsf, isMobile: true });
  return o;
}

async function measure(page, tag, state, opts = {}) {
  const m = await page.evaluate(MEASURE);
  const t = `${tag} ${state}`;
  check(m.sw <= m.vw && m.bodySw <= m.vw, `${t}: no sideways scroll (${m.sw}/${m.vw})`, m.off);
  check(!m.off.length, `${t}: nothing past the screen edge`, m.off);
  check(!m.textClip.length, `${t}: no clipped text`, m.textClip);
  check(!m.hdrOverlap.length, `${t}: header controls do not overlap`, m.hdrOverlap);
  if (opts.targets) check(!m.small.length, `${t}: touch targets >= 48 px or spaced (coarse=${m.coarse})`, m.small);
  return m;
}

async function solve(page) {
  await page.evaluate(() => window.scrollTo(0, 0));
  const chip = page.locator('#example-chips [data-example-solve="ortho"]');
  await chip.scrollIntoViewIfNeeded();
  await chip.click();
  await page.waitForSelector('#verdict:not([hidden])', { timeout: 60000 });
  await page.waitForTimeout(500);
}

async function setLang(page, lang) {
  await page.evaluate((l) => { const b = document.querySelector(`[data-lang-set="${l}"]`); if (b) b.click(); }, lang);
  await page.waitForTimeout(250);
}

async function deviceRun(engine, d, browser, scale) {
  const dn = d.name.replace(/[^A-Za-z0-9]+/g, '');
  for (const land of [false, true]) {
    if (land && d.name.startsWith('Zoom200')) continue;
    for (const scheme of ['light', 'dark']) {
      const lang = scheme === 'light' ? 'en' : 'ro';
      const tag = `${engine}-${dn}-${land ? 'land' : 'port'}-${scheme}-${lang}${scale !== 100 ? '-fs' + scale : ''}`;
      const ctx = await browser.newContext(ctxOpts(engine, d, land, scheme));
      if (scale !== 100) await ctx.addInitScript((s) => { document.addEventListener('DOMContentLoaded', () => { const st = document.createElement('style'); st.textContent = `html{font-size:${s}% !important}`; document.head.appendChild(st); }); }, scale);
      const page = await ctx.newPage();
      const errs = [];
      page.on('pageerror', (e) => errs.push(e.message));
      try {
        await page.goto(`${BASE}/`);
        if (lang === 'ro') { await page.goto(`${BASE}/gate?lang=ro`); }
        await page.waitForLoadState('networkidle');
        await measure(page, tag, 'gate', { targets: engine === 'chromium' && !d.name.startsWith('Zoom') });
        await shot(page, `${tag}-01-gate.png`);
        await page.fill('#pw', PW);
        await Promise.all([page.waitForURL((u) => new URL(u).pathname !== '/gate'), page.click('.gate-submit')]);
        await page.waitForFunction(() => window.GS && !document.getElementById('ai-pill').classList.contains('checking'));
        await page.waitForTimeout(300);
        await measure(page, tag, 'app', { targets: engine === 'chromium' && !d.name.startsWith('Zoom') });
        await shot(page, `${tag}-02-app.png`);
        await page.click('#tab-geo');
        await page.click('#syntax-btn');
        await page.waitForTimeout(150);
        await measure(page, tag, 'syntax');
        await page.click('#syntax-btn');
        await page.click('#examples-btn');
        await page.waitForTimeout(200);
        const mm = await page.evaluate(() => { const r = document.getElementById('examples-menu').getBoundingClientRect(); return { l: r.left, r: r.right, vw: document.documentElement.clientWidth, b: r.bottom, vh: innerHeight }; });
        check(mm.l >= 0 && mm.r <= mm.vw, `${tag} examples menu inside the screen (${Math.round(mm.l)}..${Math.round(mm.r)} of ${mm.vw})`);
        await shot(page, `${tag}-03-examples.png`);
        await page.keyboard.press('Escape');
        await solve(page);
        await page.locator('#verdict').scrollIntoViewIfNeeded();
        await measure(page, tag, 'result', { targets: engine === 'chromium' && !d.name.startsWith('Zoom') });
        await shot(page, `${tag}-04-result.png`);
        if (!land) await shot(page, `${tag}-05-result-full.png`, true);
        await page.locator('#export-btn').scrollIntoViewIfNeeded();
        await page.click('#export-btn');
        await page.waitForTimeout(200);
        const em = await page.evaluate(() => { const r = document.getElementById('export-menu').getBoundingClientRect(); return { l: r.left, r: r.right, vw: document.documentElement.clientWidth }; });
        check(em.l >= 0 && em.r <= em.vw, `${tag} export menu inside the screen (${Math.round(em.l)}..${Math.round(em.r)} of ${em.vw})`);
        await shot(page, `${tag}-06-export-menu.png`);
        await page.keyboard.press('Escape');
        await page.locator('#z-full').scrollIntoViewIfNeeded();
        await page.click('#z-full');
        await page.waitForTimeout(300);
        const fm = await page.evaluate(() => { const f = document.getElementById('fig-frame').getBoundingClientRect(); const v = document.getElementById('fig-viewport').getBoundingClientRect(); const t = [...document.querySelectorAll('.fig-tools .icon-btn')].map((b) => b.getBoundingClientRect()); return { fw: f.width, fh: f.height, vh: v.height, vw: innerWidth, ih: innerHeight, toolsR: Math.max(...t.map((r) => r.right)) }; });
        check(Math.abs(fm.fw - fm.vw) < 1 && Math.abs(fm.fh - fm.ih) < 1 && fm.vh > 80 && fm.toolsR <= fm.vw, `${tag} full-screen figure fills the screen (${Math.round(fm.fw)}x${Math.round(fm.fh)}, figure ${Math.round(fm.vh)} high)`, fm);
        await shot(page, `${tag}-07-fullscreen.png`);
        await page.click('#z-full');
      } catch (e) {
        check(false, `${tag}: flow aborted: ${e.message.split('\n')[0]}`);
        await shot(page, `${tag}-ABORT.png`);
      }
      check(!errs.length, `${tag}: no page errors`, errs);
      await ctx.close();
    }
  }
}

async function acctRun(engine, d, browser) {
  const dn = d.name.replace(/[^A-Za-z0-9]+/g, '');
  for (const land of [false, true]) {
    const scheme = land ? 'dark' : 'light';
    const tag = `${engine}-${dn}-${land ? 'land' : 'port'}-${scheme}-acct`;
    const ctx = await browser.newContext(ctxOpts(engine, d, land, scheme));
    const page = await ctx.newPage();
    try {
      await page.goto(`${ACCT}/`);
      await page.waitForLoadState('networkidle');
      await measure(page, tag, 'landing', { targets: engine === 'chromium' && !d.name.startsWith('Zoom') });
      await shot(page, `${tag}-01-landing.png`);
      await page.goto(`${ACCT}/auth`);
      await page.waitForLoadState('networkidle');
      await measure(page, tag, 'signin', { targets: engine === 'chromium' && !d.name.startsWith('Zoom') });
      await shot(page, `${tag}-03-signin.png`);
      await page.goto(`${ACCT}/auth?mode=register`);
      await page.waitForLoadState('networkidle');
      await measure(page, tag, 'register', { targets: engine === 'chromium' && !d.name.startsWith('Zoom') });
      await shot(page, `${tag}-04-register.png`);
      if (!land) { await page.goto(`${ACCT}/`); await page.waitForLoadState('networkidle'); await shot(page, `${tag}-02-landing-full.png`, true); }
    } catch (e) {
      check(false, `${tag}: flow aborted: ${e.message.split('\n')[0]}`);
    }
    await ctx.close();
  }
}

for (const engine of ENGINES) {
  const browser = await (engine === 'firefox' ? firefox : chromium).launch();
  for (const scale of SCALES) {
    for (let i = 0; i < DEVS.length; i += 3) await Promise.all(DEVS.slice(i, i + 3).map((d) => deviceRun(engine, d, browser, scale)));
  }
  if (ACCT && SCALES.includes(100)) for (let i = 0; i < DEVS.length; i += 4) await Promise.all(DEVS.slice(i, i + 4).map((d) => acctRun(engine, d, browser)));
  await browser.close();
}
fs.writeFileSync(path.join(OUT, `layout-findings-${ENGINES.join('_')}-${SCALES.join('_')}.json`), JSON.stringify(findings, null, 1));
log(`screenshots: ${OUT}`);
log(fails ? `${fails} FAILURES` : 'ALL PASS');
process.exit(fails ? 1 : 0);
