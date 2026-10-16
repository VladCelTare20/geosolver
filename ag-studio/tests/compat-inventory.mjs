import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';

const DEPS = process.env.COMPAT_DEPS;
if (!DEPS) {
  console.error('set COMPAT_DEPS to a node_modules holding @mdn/browser-compat-data, acorn, acorn-walk and postcss');
  process.exit(2);
}
const require = createRequire(path.join(DEPS, 'x.js'));
const bcd = require('@mdn/browser-compat-data');
const acorn = require('acorn');
const walk = require('acorn-walk');
const postcss = require('postcss');

const ASSETS = path.resolve(path.dirname(new URL(import.meta.url).pathname), '../assets');
const BASELINE = {
  chrome: '100', chrome_android: '100', edge: '100', opera: '86', opera_android: '69', webview_android: '100',
  samsunginternet_android: '17.0', firefox: '100', firefox_android: '100', safari: '15.4', safari_ios: '15.4',
};
const JS = ['i18n.js', 'site.js', 'app.js', 'landing.js', 'auth.js'];
const HTML = ['index.html', 'landing.html', 'auth.html', 'gate.html'];

const cmp = (a, b) => {
  const x = a.split('.').map(Number), y = b.split('.').map(Number);
  for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) - (y[i] || 0);
  return 0;
};
const node = (key) => key.split('.').reduce((n, k) => (n && n[k]) || null, bcd);
function since(stmts) {
  let best = null;
  for (const s of [].concat(stmts || [])) {
    if (s.flags || s.prefix || s.alternative_name || s.version_removed || s.partial_implementation) continue;
    if (s.version_added === false || s.version_added == null || s.version_added === 'preview') continue;
    const v = s.version_added === true ? '0' : String(s.version_added).replace(/^[≤<]/, '');
    if (!best || cmp(v, best) < 0) best = v;
  }
  return best;
}
function verdict(key) {
  const n = node(key);
  if (!n || !n.__compat) return null;
  const misses = [];
  for (const [b, base] of Object.entries(BASELINE)) {
    const v = since(n.__compat.support[b]);
    if (v == null) misses.push(`${b} none`);
    else if (cmp(v, base) > 0) misses.push(`${b} ${v}`);
  }
  return misses;
}
const rows = [];
const seen = new Set();
function report(kind, feature, key, where) {
  const id = `${kind}|${feature}|${key}`;
  if (seen.has(id)) return;
  seen.add(id);
  const v = verdict(key);
  rows.push({ kind, feature, key, where, status: v == null ? 'no data' : v.length ? 'BELOW' : 'ok', misses: v || [] });
}

const memberFiles = new Map();
for (const f of JS) {
  const src = fs.readFileSync(path.join(ASSETS, f), 'utf8');
  let es = null;
  for (const v of [5, 2015, 2016, 2017, 2018, 2019, 2020, 2021, 2022, 2023, 2024]) {
    try { acorn.parse(src, { ecmaVersion: v }); es = v; break; } catch (e) {}
  }
  rows.push({ kind: 'js-syntax', feature: `${f} parses as ES${es}`, key: '-', where: f, status: es && es <= 2020 ? 'ok' : 'CHECK', misses: [] });
  walk.full(acorn.parse(src, { ecmaVersion: 'latest' }), (n) => {
    if (n.type === 'MemberExpression' && !n.computed && n.property.type === 'Identifier') {
      if (!memberFiles.has(n.property.name)) memberFiles.set(n.property.name, new Set());
      memberFiles.get(n.property.name).add(f);
    }
    if (n.type === 'NewExpression' && n.callee.type === 'Identifier') report('js-api', `new ${n.callee.name}`, bcd.api[n.callee.name] ? `api.${n.callee.name}` : `javascript.builtins.${n.callee.name}`, f);
    if (n.type === 'MemberExpression' && n.object.type === 'Identifier' && !n.computed) {
      const o = n.object.name, p = n.property.name;
      if (o === 'navigator') report('js-api', `navigator.${p}`, `api.Navigator.${p}`, f);
      else if (bcd.javascript.builtins[o] && node(`javascript.builtins.${o}.${p}`)) report('js-api', `${o}.${p}`, `javascript.builtins.${o}.${p}`, f);
      else if (bcd.api[o] && node(`api.${o}.${p}`)) report('js-api', `${o}.${p}`, `api.${o}.${p}`, f);
    }
    if (n.type === 'Literal' && n.regex) {
      if (/\(\?<[=!]/.test(n.regex.pattern)) rows.push({ kind: 'js-syntax', feature: `regex lookbehind ${n.regex.pattern}`, key: '-', where: f, status: 'BELOW', misses: ['safari 16.4'] });
    }
  });
}
const index = new Map();
for (const group of ['api', 'javascript.builtins']) {
  const root = node(group);
  for (const [iface, n] of Object.entries(root)) for (const m of Object.keys(n)) if (m !== '__compat') {
    if (!index.has(m)) index.set(m, []);
    index.get(m).push(`${group}.${iface}.${m}`);
  }
}
for (const [m, files] of memberFiles) {
  const keys = index.get(m) || [];
  if (!keys.length) continue;
  const ok = keys.filter((k) => (verdict(k) || []).length === 0);
  if (!ok.length) {
    const best = keys.map((k) => ({ k, v: verdict(k) })).sort((a, b) => a.v.length - b.v.length)[0];
    rows.push({ kind: 'js-member', feature: `.${m}`, key: best.k, where: [...files].join(','), status: 'BELOW?', misses: best.v });
  }
}

const sheets = [{ name: 'app.css', css: fs.readFileSync(path.join(ASSETS, 'app.css'), 'utf8') }];
for (const h of HTML) for (const m of fs.readFileSync(path.join(ASSETS, h), 'utf8').matchAll(/<style>([\s\S]*?)<\/style>/g)) sheets.push({ name: `${h} <style>`, css: m[1] });
const unitKey = { dvh: 'viewport_percentage_units_dynamic', svh: 'viewport_percentage_units_small', lvh: 'viewport_percentage_units_large', vmin: 'vmin', vmax: 'vmax' };
const fnKey = { 'color-mix': 'css.types.color.color-mix', min: 'css.types.min', max: 'css.types.max', clamp: 'css.types.clamp', env: 'css.types.env', 'conic-gradient': 'css.types.gradient.conic-gradient', 'light-dark': 'css.types.color.light-dark', oklch: 'css.types.color.oklch' };
for (const { name, css } of sheets) {
  const root = postcss.parse(css);
  root.walkAtRules((r) => {
    report('css-at-rule', `@${r.name}`, `css.at-rules.${r.name}`, name);
    if (r.name === 'media') for (const m of r.params.matchAll(/\(\s*([a-z-]+)\s*[:)]/g)) report('css-media', `(${m[1]})`, `css.at-rules.media.${m[1]}`, name);
    if (r.name === 'supports' && /selector\(/.test(r.params)) report('css-at-rule', '@supports selector()', 'css.at-rules.supports.selector', name);
  });
  root.walkRules((r) => {
    for (const m of r.selector.matchAll(/::?([a-z-]+)/g)) report('css-selector', `:${m[1]}`, `css.selectors.${m[1]}`, name);
  });
  root.walkDecls((d) => {
    const p = d.prop.toLowerCase();
    if (p.startsWith('--')) return;
    report('css-property', p, `css.properties.${p}`, name);
    const prop = node(`css.properties.${p}`) || {};
    for (const word of d.value.toLowerCase().replace(/var\([^)]*\)/g, '').match(/[a-z][a-z-]*/g) || []) if (prop[word] && prop[word].__compat) report('css-value', `${p}: ${word}`, `css.properties.${p}.${word}`, name);
    if (/^(gap|row-gap|column-gap)$/.test(p) && node('css.properties.gap.flex_context')) report('css-value', 'gap in flex layout', 'css.properties.gap.flex_context', name);
    for (const m of d.value.matchAll(/\d(dvh|svh|lvh|vmin|vmax)\b/g)) report('css-unit', m[1], `css.types.length.${unitKey[m[1]]}`, name);
    for (const m of d.value.matchAll(/([a-z-]+)\(/g)) if (fnKey[m[1]]) report('css-function', `${m[1]}()`, fnKey[m[1]], name);
  });
}
for (const h of HTML) {
  const s = fs.readFileSync(path.join(ASSETS, h), 'utf8');
  for (const m of s.matchAll(/<([a-z][a-z0-9]*)\b/g)) if (node(`html.elements.${m[1]}`)) report('html-element', `<${m[1]}>`, `html.elements.${m[1]}`, h);
  for (const a of ['inert', 'enterkeyhint', 'autocapitalize', 'inputmode', 'popover', 'loading', 'fetchpriority']) if (new RegExp(`\\s${a}[=\\s>]`).test(s)) report('html-attr', a, `html.global_attributes.${a}`, h);
  if (/rel="preload"/.test(s)) report('html-attr', 'link rel=preload', 'html.elements.link.rel.preload', h);
}
for (const f of JS) {
  const s = fs.readFileSync(path.join(ASSETS, f), 'utf8');
  if (/\.inert\b/.test(s)) report('html-attr', 'inert (via JS)', 'api.HTMLElement.inert', f);
}

const out = process.env.JSON_OUT;
if (out) fs.writeFileSync(out, JSON.stringify(rows, null, 1));
const order = { BELOW: 0, 'BELOW?': 1, CHECK: 2, 'no data': 3, ok: 4 };
rows.sort((a, b) => order[a.status] - order[b.status] || a.kind.localeCompare(b.kind) || a.feature.localeCompare(b.feature));
for (const r of rows) console.log(`${r.status.padEnd(8)} ${r.kind.padEnd(13)} ${r.feature.padEnd(36)} ${r.misses.join(', ')}${r.status === 'ok' ? '' : `  [${r.where}]`}`);
console.log(`\n${rows.length} features: ${rows.filter((r) => r.status === 'ok').length} ok, ${rows.filter((r) => r.status.startsWith('BELOW')).length} below the baseline (each needs a fallback or a harmless-degrade note in assets/CODEMAP.md)`);
