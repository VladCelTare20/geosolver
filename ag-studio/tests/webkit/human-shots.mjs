import fs from 'node:fs';
import path from 'node:path';

const pw = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const BASE = process.env.BASE || 'http://127.0.0.1:8787';
const PW = process.env.BASIC_PASSWORD || '';
const OUT = process.env.SHOTS || '/tmp/human-shots';
const FIX = process.env.FIXTURES || path.resolve(path.dirname(new URL(import.meta.url).pathname), '../fixtures/human');
const NAMES = (process.env.PROBLEMS || 'imo-2004-p1,imo-2023-p2').split(',');
const ONLY = (process.env.DEVS || '').split(',').filter(Boolean);
fs.mkdirSync(OUT, { recursive: true });

const DEVICES = [
  { name: 'iphone15', engine: 'webkit', device: 'iPhone 15' },
  { name: 'fold-cover-280', engine: 'chromium', viewport: { width: 280, height: 653 }, dpr: 3, touch: true, mobile: true },
  { name: 'ipad-air', engine: 'webkit', viewport: { width: 820, height: 1180 }, dpr: 2, touch: true, mobile: true },
  { name: 'desktop-1920', engine: 'chromium', viewport: { width: 1920, height: 1080 }, dpr: 1 },
];

let fails = 0;
const check = (ok, what) => { console.log(`${ok ? 'PASS' : 'FAIL'} ${what}`); if (!ok) fails++; };
const programs = NAMES.map((n) => ({ name: n, geo: JSON.parse(fs.readFileSync(path.join(FIX, `${n}.json`), 'utf8')).programs[0].geo }));
const browsers = {};

async function context(dev, scheme, lang) {
  const b = browsers[dev.engine] || (browsers[dev.engine] = await pw[dev.engine].launch());
  const base = dev.device ? pw.devices[dev.device] : { viewport: dev.viewport, deviceScaleFactor: dev.dpr, hasTouch: !!dev.touch, isMobile: !!dev.mobile };
  const ctx = await b.newContext({ ...base, colorScheme: scheme, locale: lang === 'ro' ? 'ro-RO' : 'en-US', acceptDownloads: true });
  await ctx.addInitScript((l) => { try { localStorage.setItem('lang', l); } catch (e) {} }, lang);
  const page = await ctx.newPage();
  const errs = [];
  page.on('pageerror', (e) => errs.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error' && !/401|Failed to load resource/.test(m.text())) errs.push(m.text()); });
  await page.goto(`${BASE}/`);
  if (new URL(page.url()).pathname === '/gate') {
    await page.fill('#pw', PW);
    await Promise.all([page.waitForURL((u) => new URL(u).pathname !== '/gate'), page.click('.gate-submit')]);
  }
  await page.waitForFunction(() => window.GS && window.GS.booted);
  return { ctx, page, errs };
}

async function solve(page, geo) {
  await page.click('#tab-geo');
  await page.fill('#geo-input', geo);
  await page.click('#solve');
  await page.waitForSelector('#state-solving', { state: 'hidden', timeout: 120000 });
  await page.waitForSelector('#verdict:not([hidden])', { timeout: 20000 });
  await page.waitForTimeout(500);
}

for (const dev of DEVICES.filter((d) => !ONLY.length || ONLY.includes(d.name))) {
  for (const scheme of ['light', 'dark']) {
    for (const lang of ['en', 'ro']) {
      const { ctx, page, errs } = await context(dev, scheme, lang);
      for (const p of programs) {
        const tag = `${p.name}-${dev.name}-${scheme}-${lang}`;
        try {
          await solve(page, p.geo);
          const st = await page.evaluate(() => ({
            human: !document.getElementById('proof-human-panel').hidden,
            blocks: document.querySelectorAll('#human .hp-block').length,
            tabs: [...document.querySelectorAll('#proof-tabs [role=tab]')].filter((b) => !b.hidden).map((b) => b.textContent.trim()),
            sw: document.documentElement.scrollWidth, vw: document.documentElement.clientWidth,
          }));
          check(st.human && st.blocks > 0, `${tag}: Proof tab shown first (${st.blocks} blocks; tabs ${st.tabs.join(' | ')})`);
          check(st.sw <= st.vw, `${tag}: no sideways scroll (${st.sw}/${st.vw})`);
          const card = page.locator('.proof.card');
          await page.mouse.move(1, 1).catch(() => {});
          await page.evaluate(() => { if (document.activeElement) document.activeElement.blur(); document.documentElement.classList.add('shot-static'); });
          await page.addStyleTag({ content: 'html.shot-static .site-header { position: static !important; } html.shot-static .fig-peek { display: none !important; }' });
          await page.evaluate(() => document.querySelectorAll('#human .hp-block').forEach((li) => li.dispatchEvent(new MouseEvent('mouseleave'))));
          await card.scrollIntoViewIfNeeded();
          await card.screenshot({ path: `${OUT}/${tag}-proof.png`, animations: 'disabled' });
          await page.evaluate(() => document.documentElement.classList.remove('shot-static'));
          if (scheme === 'light' && lang === 'en') {
            const claim = page.locator('#human .hp-block.is-claim').first();
            await claim.scrollIntoViewIfNeeded();
            if (dev.touch) await claim.tap({ position: { x: 30, y: 12 } });
            else await claim.hover({ position: { x: 30, y: 12 } });
            await page.waitForTimeout(450);
            const lit = await page.evaluate(() => document.querySelectorAll('#fig-viewport .hl').length);
            check(lit > 0, `${tag}: a claim lights the figure (${lit} lit)`);
            await page.screenshot({ path: `${OUT}/${tag}-claim-lit.png`, animations: 'disabled' });
            await page.evaluate(() => {
              if (document.activeElement) document.activeElement.blur();
              document.querySelectorAll('#human .hp-block').forEach((li) => li.dispatchEvent(new MouseEvent('mouseleave')));
            });
            await page.waitForTimeout(300);
          }
        } catch (e) {
          check(false, `${tag}: ${e.message.split('\n')[0]}`);
          await page.screenshot({ path: `${OUT}/${tag}-ABORT.png` }).catch(() => {});
        }
      }
      if (dev.name === 'desktop-1920' && scheme === 'light') {
        for (const p of programs) {
          await solve(page, p.geo);
          for (const derivation of [false, true]) {
            const r = await page.evaluate(async (d) => {
              const res = await fetch('/api/export', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ input: document.getElementById('geo-input').value, format: 'pdf', derivation: d }) });
              const b = new Uint8Array(await res.arrayBuffer());
              let s = '';
              for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode.apply(null, b.subarray(i, i + 0x8000));
              return { status: res.status, b64: btoa(s) };
            }, derivation);
            const file = `${OUT}/${p.name}-report-${lang}${derivation ? '-derivation' : ''}.pdf`;
            fs.writeFileSync(file, Buffer.from(r.b64, 'base64'));
            check(r.status === 200 && fs.statSync(file).size > 1000, `${p.name}: PDF report ${lang}${derivation ? ' + derivation' : ''} (${fs.statSync(file).size} B)`);
          }
        }
      }
      check(!errs.length, `${dev.name}-${scheme}-${lang}: no page errors ${errs.slice(0, 2).join(' | ')}`);
      await ctx.close();
    }
  }
}
for (const b of Object.values(browsers)) await b.close();
console.log(fails ? `${fails} FAILURES` : 'ALL PASS');
process.exit(fails ? 1 : 0);
