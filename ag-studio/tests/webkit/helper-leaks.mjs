import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

const BASE = process.env.BASE || 'http://127.0.0.1:8787';
const PW = process.env.BASIC_PASSWORD || '';
const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), '../../..');
const JOBS = Number(process.env.JOBS || 3);
const DDAR = process.env.DDAR || 'ddar';
const OUT = process.env.OUT || '';
const auth = { Authorization: 'Basic ' + Buffer.from(':' + PW).toString('base64') };

function problems() {
  const out = [];
  const corpus = fs.readFileSync(path.join(ROOT, 'corpus/imo_ag_30.txt'), 'utf8').split('\n').map((l) => l.trim()).filter(Boolean);
  for (let i = 0; i + 1 < corpus.length; i += 2) out.push({ name: corpus[i], corpus: true });
  const walk = (dir) => fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name)).forEach((e) => {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p);
    else if (e.name.endsWith('.geo')) out.push({ name: path.relative(path.join(ROOT, 'alphageometry-rs'), p), input: fs.readFileSync(p, 'utf8') });
  });
  walk(path.join(ROOT, 'alphageometry-rs/examples'));
  const fixtures = path.join(ROOT, 'ag-studio/tests/fixtures/human');
  for (const f of fs.readdirSync(fixtures).filter((f) => f.endsWith('.json') && !f.startsWith('synthetic')).sort()) {
    out.push({ name: `fixture:${f.replace(/\.json$/, '')}`, input: JSON.parse(fs.readFileSync(path.join(fixtures, f), 'utf8')).program });
  }
  const app = fs.readFileSync(path.join(ROOT, 'ag-studio/assets/app.js'), 'utf8');
  for (const m of app.matchAll(/\{ id: "(\w+)",[\s\S]*?body: ("(?:[^"\\]|\\.)*")/g)) out.push({ name: `app-example:${m[1]}`, input: JSON.parse(m[2]) });
  const only = (process.env.ONLY || '').split(',').filter(Boolean);
  return only.length ? out.filter((p) => only.some((o) => p.name.includes(o))) : out;
}

const labels = (svg) => [...String(svg || '').matchAll(/<text class="f-lbl[^"]*"[^>]*data-p="([^"]*)"/g)].map((m) => m[1].replace(/&amp;/g, '&'));

function namesIn(value, known) {
  const sorted = [...known].sort((a, b) => b.length - a.length);
  const found = new Set();
  const visit = (v) => {
    if (typeof v === 'string') {
      for (const run of v.match(/[A-ZΩω][A-Za-z₀-₉′″]*/g) || []) {
        const parts = [];
        let rest = run;
        while (rest) {
          const n = sorted.find((k) => rest.startsWith(k));
          if (!n) break;
          parts.push(n);
          rest = rest.slice(n.length);
        }
        if (!rest) parts.forEach((p) => found.add(p));
      }
    } else if (Array.isArray(v)) v.forEach(visit);
    else if (v && typeof v === 'object') Object.values(v).forEach(visit);
  };
  visit(value);
  return found;
}

async function solve(p) {
  if (p.corpus) p.input = execFileSync(DDAR, ['--corpus-show', path.join(ROOT, 'corpus/imo_ag_30.txt'), p.name], { encoding: 'utf8' }).split('\n').find((l) => l.includes('@')) || '';
  const t0 = Date.now();
  let res, body;
  for (let tries = 0; ; tries++) {
    res = await fetch(`${BASE}/api/solve`, { method: 'POST', headers: { 'Content-Type': 'application/json', ...auth }, body: JSON.stringify({ input: p.input, record: false }) });
    body = await res.json().catch(() => ({}));
    if (res.status !== 503 || tries > 40) break;
    await new Promise((r) => setTimeout(r, 3000));
  }
  const v = body.view || {};
  const helpers = (v.helpers || []).map((h) => h.name);
  const aux = (v.aux || []).map((a) => a.name);
  const full = labels(body.svg);
  const proofSvg = body.svg_proof || body.svg;
  const proofLabels = labels(proofSvg);
  const known = new Set([...full, ...proofLabels, ...(v.points || []).map((x) => x.name), ...helpers]);
  const human = v.human && v.human.blocks && v.human.blocks.length ? v.human : null;
  const named = human ? namesIn(human, known) : new Set();
  const stated = namesIn([v.given_proof || v.given || [], v.goal || {}].map((f) => (Array.isArray(f) ? f.map((x) => x.points) : f.points)), known);
  const unnamed = human ? proofLabels.filter((n) => helpers.includes(n) && !named.has(n) && !aux.includes(n)) : [];
  const leaks = unnamed.filter((n) => !stated.has(n));
  const statedOnly = unnamed.filter((n) => stated.has(n));
  const derivationHelpers = full.filter((n) => helpers.includes(n));
  return {
    name: p.name, http: res.status, status: body.status || body.code || '', secs: ((Date.now() - t0) / 1000).toFixed(1),
    human: !!human, helpers, figure: full, proofFigure: proofLabels, separateProofFigure: !!body.svg_proof,
    hiddenInProof: v.proof_hidden || [], leaks, statedOnly, derivationHelpers,
  };
}

const list = problems();
const results = new Array(list.length);
let next = 0;
await Promise.all(Array.from({ length: JOBS }, async () => {
  while (next < list.length) {
    const i = next++;
    try { results[i] = await solve(list[i]); } catch (e) { results[i] = { name: list[i].name, error: e.message, leaks: [] }; }
    const r = results[i];
    console.log(`${r.leaks.length ? 'LEAK' : (r.statedOnly || []).length ? 'GIVN' : 'ok  '} ${r.name}  ${r.error || `${r.http} ${r.status} ${r.secs}s human=${r.human} helpers=[${r.helpers.join(' ')}] proof-figure=[${r.proofFigure.join(' ')}]${r.leaks.length ? ` leaked=[${r.leaks.join(' ')}]` : ''}${(r.statedOnly || []).length ? ` unnamed-by-the-proof-but-in-GIVEN=[${r.statedOnly.join(' ')}]` : ''}${(r.hiddenInProof || []).length ? ` hidden-in-proof=[${r.hiddenInProof.join(' ')}]` : ''}`}`);
  }
}));
const withHuman = results.filter((r) => r.human);
const leaking = results.filter((r) => r.leaks.length);
console.log(`\n${results.length} problems, ${withHuman.length} with a human proof, ${withHuman.filter((r) => r.helpers.length).length} of them with helper points, ${leaking.length} leaking a helper into the Proof figure`);
leaking.forEach((r) => console.log(`  LEAK ${r.name}: ${r.leaks.join(', ')}`));
const givenOnly = results.filter((r) => (r.statedOnly || []).length);
console.log(`${givenOnly.length} more draw a helper the human proof never names but the Proof tab's GIVEN states`);
givenOnly.forEach((r) => console.log(`  GIVN ${r.name}: ${r.statedOnly.join(', ')}`));
if (OUT) fs.writeFileSync(OUT, JSON.stringify(results, null, 1));
process.exit(leaking.length ? 1 : 0);
