const MOD = process.env.PLAYWRIGHT_MODULE || 'playwright';
const { webkit, devices } = await import(MOD);

const BASE = process.env.BASE || 'http://127.0.0.1:8787';
const PASSWORD = process.env.BASIC_PASSWORD || '';
const SHOTS = process.env.SHOTS || '';
const DEVICES = ['iPhone SE', 'iPhone 15', 'iPhone 15 Pro Max'];
const NOTCH = { 'iPhone SE': 0, 'iPhone 15': 59, 'iPhone 15 Pro Max': 59 };

const failures = [];
function check(ok, what) {
  console.log((ok ? 'ok   ' : 'FAIL ') + what);
  if (!ok) failures.push(what);
}

const intoStrips = (inset) => {
  const W = innerWidth, hits = [];
  document.querySelectorAll('a, button, input, textarea, summary, h1, h2, h3, p, label, .wordmark svg').forEach((el) => {
    if (el.closest('[hidden], [inert], .sr-only')) return;
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height || r.bottom <= 0 || getComputedStyle(el).visibility === 'hidden') return;
    if (r.left < inset - 0.5 || r.right > W - inset + 0.5) hits.push((el.id || el.className || el.tagName) + ' [' + Math.round(r.left) + '..' + Math.round(r.right) + ']');
  });
  return hits;
};

const browser = await webkit.launch();
try {
  for (const name of DEVICES) {
    for (const land of [false, true]) {
      const tag = name + (land ? ' landscape' : '');
      const inset = land ? NOTCH[name] : 0;
      const ctx = await browser.newContext({ ...devices[tag], httpCredentials: PASSWORD ? { username: '', password: PASSWORD } : undefined });
      await ctx.addInitScript((inset) => {
        document.addEventListener('DOMContentLoaded', () => {
          const s = document.createElement('style');
          s.textContent = `:root{--sai-l:${inset}px !important;--sai-r:${inset}px !important}`;
          document.head.appendChild(s);
        });
      }, inset);
      const page = await ctx.newPage();
      const errors = [];
      page.on('pageerror', (e) => errors.push(String(e)));
      await page.goto(BASE + '/');
      if (new URL(page.url()).pathname === '/gate') {
        await page.fill('#pw', PASSWORD);
        await Promise.all([page.waitForURL((u) => new URL(u).pathname !== '/gate'), page.click('.gate-submit')]);
      }
      await page.waitForFunction(() => window.GS && !document.documentElement.classList.contains('i18n-pending'));
      await page.waitForTimeout(300);
      const vw = () => page.evaluate(() => [document.documentElement.scrollWidth, innerWidth]);
      let [sw, iw] = await vw();
      check(sw <= iw, `${tag}: page fits (${sw} <= ${iw})`);
      const first = await page.evaluate(() => document.querySelector('#tabs [aria-selected=true]').id);
      const ai = await page.evaluate(() => document.getElementById('ai-pill').classList.contains('on'));
      check(first === (ai ? 'tab-describe' : 'tab-geo'), `${tag}: opens on ${first} (AI ${ai ? 'on' : 'off'})`);
      await page.tap('#tab-geo');
      await page.tap('#syntax-btn');
      await page.waitForTimeout(200);
      [sw, iw] = await vw();
      check(sw <= iw, `${tag}: page fits with Syntax open (${sw} <= ${iw})`);
      if (SHOTS) await page.screenshot({ path: `${SHOTS}/${tag.replace(/ /g, '_')}-syntax.png` });
      await page.tap('#syntax-btn');
      const fs = await page.evaluate(() => ['#geo-input', '#hl', '#gutter'].map((s) => getComputedStyle(document.querySelector(s)).fontSize));
      check(fs.every((v) => v === '16px'), `${tag}: editor layers are 16px (${fs})`);
      if (inset) {
        const hits = await page.evaluate(intoStrips, inset);
        check(!hits.length, `${tag}: nothing under the ${inset}px notch strips ${hits.slice(0, 3).join(' ')}`);
      }
      await page.evaluate(() => document.querySelector('[data-example-solve="ortho"]').click());
      await page.waitForSelector('#verdict:not([hidden])', { timeout: 30000 });
      await page.waitForTimeout(600);
      const step = page.locator('#steps li.step:not(.is-group):not([hidden])').nth(1);
      await step.scrollIntoViewIfNeeded();
      const sb = await step.boundingBox();
      await page.touchscreen.tap(sb.x + 40, sb.y + sb.height / 2);
      await page.waitForTimeout(300);
      const peek = await page.evaluate(() => { const p = document.getElementById('fig-peek'); return p && !p.hidden ? p.querySelectorAll('.hl').length : -1; });
      check(peek > 0, `${tag}: step tap shows the figure peek with ${peek} lit objects`);
      if (SHOTS) await page.screenshot({ path: `${SHOTS}/${tag.replace(/ /g, '_')}-peek.png` });
      await page.evaluate(() => { document.activeElement.blur(); document.getElementById('fig-viewport').scrollIntoView({ block: 'center' }); });
      await page.waitForTimeout(250);
      const vb = () => page.evaluate(() => document.querySelector('#fig-viewport > svg').getAttribute('viewBox'));
      const b = await page.locator('#fig-viewport').boundingBox();
      const v0 = await vb();
      await page.touchscreen.tap(b.x + b.width * 0.3, b.y + b.height * 0.4);
      await page.waitForTimeout(120);
      await page.touchscreen.tap(b.x + b.width * 0.7, b.y + b.height * 0.6);
      await page.waitForTimeout(350);
      check((await vb()) === v0, `${tag}: two taps on different points do not zoom`);
      await page.touchscreen.tap(b.x + b.width / 2, b.y + b.height / 2);
      await page.waitForTimeout(110);
      await page.touchscreen.tap(b.x + b.width / 2 + 3, b.y + b.height / 2 + 3);
      await page.waitForTimeout(80);
      await page.evaluate(([x, y]) => document.getElementById('fig-viewport').dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, clientX: x, clientY: y })), [b.x + b.width / 2, b.y + b.height / 2]);
      await page.waitForTimeout(120);
      check((await vb()) !== v0, `${tag}: a double tap zooms and the trailing dblclick does not undo it`);
      check(!errors.length, `${tag}: no page errors ${errors.join(' | ')}`);
      await ctx.close();
    }
  }
} finally {
  await browser.close();
}
console.log(failures.length ? `${failures.length} FAILED` : 'all passed');
process.exit(failures.length ? 1 : 0);
