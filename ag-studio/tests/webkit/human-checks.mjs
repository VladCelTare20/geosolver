import fs from 'node:fs';
import path from 'node:path';

const EXPECT = !!process.env.EXPECT_HUMAN;

export async function humanProofFlow(page, tag, check, shot, opts = {}) {
  const st = await page.evaluate(() => {
    const t = document.getElementById('ptab-human');
    return {
      shown: !!t && !t.hidden,
      selected: !!t && t.getAttribute('aria-selected') === 'true',
      blocks: document.querySelectorAll('#human .hp-block').length,
      panel: !document.getElementById('proof-human-panel').hidden,
      stepsHidden: document.getElementById('proof-steps-panel').hidden,
      full: document.getElementById('ptab-steps').textContent.trim(),
    };
  });
  if (!st.shown) {
    check(!EXPECT, `${tag}: human proof ${EXPECT ? 'MISSING (EXPECT_HUMAN set)' : 'not offered by this server; the step list is shown as before'}`);
    return false;
  }
  check(st.selected && st.panel && st.stepsHidden && st.blocks > 0, `${tag}: the Proof tab opens first (${st.blocks} block(s); second tab "${st.full}")`);

  const block = page.locator('#human .hp-block').last();
  await block.scrollIntoViewIfNeeded();
  if (opts.touch) await block.tap({ position: { x: 40, y: 14 } });
  else await block.hover({ position: { x: 40, y: 14 } });
  await page.waitForTimeout(400);
  const lit = await page.evaluate(() => ({ hl: document.querySelectorAll('#fig-viewport .hl').length, active: !!document.querySelector('#human .hp-block.is-active') }));
  check(lit.hl > 0 && lit.active, `${tag}: ${opts.touch ? 'tapping' : 'hovering'} a proof block lights the figure (${lit.hl} lit)`);
  if (shot) await shot(page, 'human-block');

  if (opts.keyboard) {
    await page.focus('#human .hp-block[tabindex="0"]');
    await page.keyboard.press('End');
    await page.waitForTimeout(250);
    const k = await page.evaluate(() => {
      const a = document.activeElement;
      const items = [...document.querySelectorAll('#human .hp-block')];
      return { idx: items.indexOf(a), n: items.length, hl: document.querySelectorAll('#fig-viewport .hl').length };
    });
    check(k.idx === k.n - 1 && k.hl > 0, `${tag}: End moves to the last proof block and lights the figure (block ${k.idx + 1}/${k.n}, ${k.hl} lit)`);
    await page.keyboard.press('Home');
    await page.waitForTimeout(150);
    check(await page.evaluate(() => document.activeElement === document.querySelector('#human .hp-block')), `${tag}: Home moves to the first proof block`);
  }

  const chip = page.locator('#human .hp-cites .cite').last();
  const n = await chip.getAttribute('data-step');
  await chip.scrollIntoViewIfNeeded();
  if (opts.touch) await chip.tap();
  else await chip.click();
  await page.waitForTimeout(700);
  const j = await page.evaluate(() => ({
    tab: document.getElementById('ptab-steps').getAttribute('aria-selected'),
    panel: !document.getElementById('proof-steps-panel').hidden,
    focus: document.activeElement && document.activeElement.id,
  }));
  check(j.tab === 'true' && j.panel && j.focus === `step-${n}`, `${tag}: chip ${n} opens the full derivation at step ${n} (focus ${j.focus})`);
  const pooled = await page.evaluate(() => {
    const box = document.createElement('div');
    box.className = 'hp';
    document.body.appendChild(box);
    const perp = (a, b, pts) => ({ kind: 'perp', args: [a, b], points: pts });
    const h = { version: 1, as_drawn: false, setup: [], blocks: [{ id: 1, kind: 'conclusion', n: null, stmt: { kind: 'eqangle', args: ['∡(AH, BC)', '∡(HK, BC)'], points: ['A', 'B', 'C', 'H', 'K'] },
      body: [{ kind: 'pooled', stmt: { kind: 'eqangle', args: ['∡(AH, BC)', '∡(HK, BC)'], points: ['A', 'B', 'C', 'H', 'K'] },
        reasons: [{ kind: 'hyp', stmt: perp('AH', 'BC', ['A', 'B', 'C', 'H']), step: 1 }, { ...perp('HK', 'BC', ['B', 'C', 'H', 'K']), step: 2, block: null, because: [] }],
        combination: [{ reason: 0, row: 0, coef: '1' }, { reason: 1, row: 0, coef: '-1' }] }],
      engine_steps: [], points: ['A', 'H', 'K', 'B', 'C'], objects: [] }], metrics: {} };
    window.GS.renderHuman(box, h, { points: [], proof: { steps: [] } }, {});
    const btn = box.querySelector('.hp-compute-toggle');
    const before = btn && box.querySelector('.hp-compute').hidden;
    if (btn) btn.click();
    const after = btn && !box.querySelector('.hp-compute').hidden && btn.getAttribute('aria-expanded') === 'true';
    const rows = box.querySelectorAll('.hp-compute .hp-row').length;
    const text = window.GS.humanText(h, { points: [], proof: { steps: [] } }, { lang: 'en' });
    box.remove();
    return { before, after, rows, text };
  });
  check(pooled.before && pooled.after && pooled.rows === 2 && /Combining AH ⟂ BC and HK ⟂ BC, we get/.test(pooled.text),
    `${tag}: "show the computation" expands a pooled sentence into its ${pooled.rows} weighted rows`);
  const f = await page.evaluate(() => ({ sw: document.documentElement.scrollWidth, vw: document.documentElement.clientWidth }));
  check(f.sw <= f.vw, `${tag}: the proof tabs fit (${f.sw}/${f.vw})`);
  if (shot) await shot(page, 'full-derivation-from-chip');
  return true;
}

export async function humanParity(page, tag, check, dir) {
  const names = fs.readdirSync(dir).filter((f) => f.endsWith('.json')).map((f) => f.slice(0, -5)).sort();
  const norm = (x) => x.split(/\s+/).join(' ').trim();
  for (const name of names) {
    const f = JSON.parse(fs.readFileSync(path.join(dir, `${name}.json`), 'utf8'));
    for (const lang of ['en', 'ro']) {
      const want = fs.readFileSync(path.join(dir, `${name}.${lang}.txt`), 'utf8');
      const got = await page.evaluate(([h, v, l]) => window.GS.humanText(h, v, { lang: l, auxText: window.GS.auxText }), [f.human, f.view, lang]);
      const same = norm(got) === norm(want);
      let at = 0;
      if (!same) { const a = norm(got), b = norm(want); while (at < a.length && a[at] === b[at]) at++; }
      check(same, `${tag}: the browser writes ${name} (${lang}) as the server does${same ? '' : ` — differs at ${at}: …${norm(got).slice(Math.max(0, at - 40), at + 60)}… vs …${norm(want).slice(Math.max(0, at - 40), at + 60)}…`}`);
    }
  }
}

export async function humanRomanian(page, tag, check, shot) {
  const shown = await page.evaluate(() => { const t = document.getElementById('ptab-human'); return !!t && !t.hidden; });
  if (!shown) return;
  await page.locator('#ptab-human').scrollIntoViewIfNeeded();
  await page.click('#ptab-human');
  await page.waitForTimeout(250);
  const ro = await page.evaluate(() => ({ text: document.getElementById('human').innerText, tab: document.getElementById('ptab-human').textContent.trim(), full: document.getElementById('ptab-steps').textContent.trim() }));
  const english = [' denotes ', ' are concyclic', ' are collinear', 'Claim ', 'Proof.', ' lies on ', 'Combining', 'We show', 'as required'].filter((w) => ro.text.includes(w));
  check(ro.tab === 'Demonstrație' && ro.full === 'Derivarea completă' && /trebuie rotită|citite din figură/.test(ro.text) && !english.length,
    `${tag}: RO human proof (tabs "${ro.tab}" | "${ro.full}"; English left: ${english.join(', ') || 'none'})`);
  if (shot) await shot(page, 'ro-human');
}

export async function humanAbsent(page, tag, check) {
  const st = await page.evaluate(() => ({
    human: !document.getElementById('ptab-human').hidden,
    steps: !document.getElementById('proof-steps-panel').hidden && document.querySelectorAll('#steps li.step').length,
    label: document.getElementById('ptab-steps').textContent.trim(),
    hint: !document.getElementById('proof-kbd').hidden,
  }));
  check(!st.human && st.steps > 0 && st.hint && /Verified steps|Pași verificați/.test(st.label), `${tag}: without a human proof the step list shows as before (${st.steps} steps, tab "${st.label}")`);
}
