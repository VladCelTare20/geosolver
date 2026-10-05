//! Rasterise SVG figures to PNG and export them (and a combined proof report)
//! to PDF — a pure-Rust pipeline (resvg + svg2pdf), no browser or external
//! tools, so it runs headlessly on a server.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use resvg::tiny_skia;
use resvg::usvg;

use serde_json::Value;

use crate::engine::Solution;
use crate::i18n::{self, Lang};
use crate::present;

/// A font database built once (system fonts), reused for every render. usvg
/// does per-glyph fallback, so the proof's math symbols (△ ∼ ∎ ⟂ ∥ √ …) are
/// resolved from whichever installed font carries them.
fn shared_fontdb() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        // Bundled fonts guarantee complete, consistent glyph coverage (math
        // symbols △ ∼ ∎ ⟂ ∥ ∠ √ …) regardless of what the host has installed,
        // so figures and proofs render identically everywhere.
        for font in [
            &include_bytes!("../assets/fonts/inter-regular.ttf")[..],
            &include_bytes!("../assets/fonts/inter-semibold.ttf")[..],
            &include_bytes!("../assets/fonts/stix-two-text-regular.ttf")[..],
            &include_bytes!("../assets/fonts/stix-two-text-semibold.ttf")[..],
            &include_bytes!("../assets/fonts/stix-two-text-italic.ttf")[..],
        ] {
            db.load_font_data(font.to_vec());
        }
        db.load_font_data(include_bytes!("../assets/fonts/DejaVuSans.ttf").to_vec());
        db.load_font_data(include_bytes!("../assets/fonts/DejaVuSansMono.ttf").to_vec());
        // Point the generic families at the bundled fonts, so figures and proofs
        // render even on a minimal server with no system fonts installed.
        db.set_sans_serif_family("DejaVu Sans");
        db.set_serif_family("DejaVu Sans");
        db.set_monospace_family("DejaVu Sans Mono");
        Arc::new(db)
    })
    .clone()
}

// usvg::Options is non-exhaustive, so the default-then-assign pattern is required.
#[allow(clippy::field_reassign_with_default)]
fn usvg_options() -> usvg::Options<'static> {
    let mut opt = usvg::Options::default();
    opt.fontdb = shared_fontdb();
    opt
}

/// Parse an SVG document into a usvg tree (shared by the PNG and PDF paths).
pub fn parse_svg(svg: &str) -> Result<usvg::Tree> {
    usvg::Tree::from_str(svg, &usvg_options()).map_err(|e| anyhow!("SVG parse error: {e}"))
}

/// Pixel budget for one raster (4 bytes each, so ~160 MB).
const MAX_PIXELS: u64 = 40_000_000;
/// Below this the text of a long proof is no longer legible; refuse instead.
const MIN_SCALE: f32 = 0.1;
/// Refuse SVGs taller/wider than this (CSS px) outright.
const MAX_SVG_EDGE: f32 = 4_000_000.0;

fn check_svg_size(tree: &usvg::Tree) -> Result<()> {
    let size = tree.size();
    let (w, h) = (size.width(), size.height());
    if !(w.is_finite() && h.is_finite()) || w > MAX_SVG_EDGE || h > MAX_SVG_EDGE {
        return Err(anyhow!("document too large to export ({w:.0}x{h:.0} px)"));
    }
    Ok(())
}

/// Render a parsed tree to PNG bytes at the given scale (1.0 = native size;
/// 2.0 = retina/high-DPI). A raster that would exceed the pixel budget (a long
/// proof) is rendered at a reduced scale that fits, rather than failing.
pub fn png_bytes(tree: &usvg::Tree, scale: f32) -> Result<Vec<u8>> {
    check_svg_size(tree)?;
    let size = tree.size();
    let area = (size.width() as f64) * (size.height() as f64) * (scale as f64).powi(2);
    let scale = if area > MAX_PIXELS as f64 {
        // Shave a little extra so ceil() rounding cannot overshoot the budget.
        scale * ((MAX_PIXELS as f64 / area).sqrt() * 0.999) as f32
    } else {
        scale
    };
    if scale < MIN_SCALE {
        return Err(anyhow!("figure too large to rasterise legibly"));
    }
    let w = ((size.width() * scale).ceil() as u32).max(1);
    let h = ((size.height() * scale).ceil() as u32).max(1);
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(anyhow!("figure too large to rasterise"));
    }
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or_else(|| anyhow!("pixmap allocation failed"))?;
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| anyhow!("PNG encode error: {e}"))
}

/// Convenience: SVG string → PNG bytes.
pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>> {
    png_bytes(&parse_svg(svg)?, scale)
}


// ---------------------------------------------------------------------------
// Report page: verdict, figure, given/prove, and the machine-checked steps.
// ---------------------------------------------------------------------------


struct Tone {
    ink: &'static str,
    bg: &'static str,
}

fn tone(status: &str, time_limited: bool) -> Tone {
    match status {
        "proved" => Tone { ink: "#17703a", bg: "#e8f4ec" },
        "refuted" => Tone { ink: "#b42318", bg: "#fdecea" },
        "holds-numerically" => Tone { ink: "#8a5a00", bg: "#fff4db" },
        _ if time_limited => Tone { ink: "#4b525a", bg: "#eef0f2" },
        _ => Tone { ink: "#4b525a", bg: "#eef0f2" },
    }
}

/// A verdict glyph drawn as paths (no font dependency), 20×20 at (x, y).
fn verdict_icon(status: &str, time_limited: bool, x: f32, y: f32, color: &str) -> String {
    let c = format!(
        "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"9\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1.8\"/>",
        x + 10.0,
        y + 10.0
    );
    let path = match (status, time_limited) {
        ("proved", _) => format!("M{:.1},{:.1} l3.2,3.4 l6,-7", x + 5.6, y + 10.4),
        ("refuted", _) => format!(
            "M{:.1},{:.1} l7,7 M{:.1},{:.1} l-7,7",
            x + 6.5,
            y + 6.5,
            x + 13.5,
            y + 6.5
        ),
        ("holds-numerically", _) => format!(
            "M{:.1},{:.1} q2.5,-2.5 5,0 t5,0 M{:.1},{:.1} q2.5,-2.5 5,0 t5,0",
            x + 5.0,
            y + 8.5,
            x + 5.0,
            y + 13.0
        ),
        (_, true) => format!("M{:.1},{:.1} v5 l3.5,2.5", x + 10.0, y + 5.0),
        _ => format!("M{:.1},{:.1} h9", x + 5.5, y + 10.0),
    };
    format!(
        "{c}<path d=\"{path}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"
    )
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Map the few math glyphs the bundled font lacks to equivalents it carries.
fn normalize_glyphs(s: &str) -> String {
    s.replace('\u{27c2}', "\u{22a5}").replace('\u{2225}', "\u{2016}")
}

fn is_operator(tok: &str) -> bool {
    matches!(tok, "=" | "+" | "\u{2212}" | "-" | "\u{b7}" | ":" | "/" | "\u{2044}" | "<" | ">" | "\u{2264}" | "\u{2265}" | "\u{2260}" | "\u{223c}" | "\u{2245}" | "\u{27c2}" | "\u{22a5}" | "\u{2225}" | "\u{2016}")
}

/// Words of `text`, with a relation or operator and its operands kept as one
/// unbreakable run (joined by U+00A0).
fn math_atoms(text: &str) -> Vec<String> {
    let mut atoms: Vec<String> = Vec::new();
    let mut glue_next = false;
    for tok in text.split_whitespace() {
        let op = is_operator(tok);
        match atoms.last_mut() {
            Some(last) if op || glue_next => {
                last.push('\u{a0}');
                last.push_str(tok);
            }
            _ => atoms.push(tok.to_string()),
        }
        glue_next = op;
    }
    atoms
}

/// Greedy word wrap to at most `cols` characters per line, breaking only
/// outside equations where it can; a run longer than a line is broken at its
/// spaces, a word longer than a line at the line length.
fn wrap(text: &str, cols: usize) -> Vec<String> {
    let cols = cols.max(4);
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for atom in math_atoms(text) {
        let n = atom.chars().count();
        let need = cur.chars().count() + usize::from(!cur.is_empty()) + n;
        if n > cols {
            let joined = if cur.is_empty() { atom.replace('\u{a0}', " ") } else { format!("{cur} {}", atom.replace('\u{a0}', " ")) };
            let mut parts = wrap_words(&joined, cols);
            cur = parts.pop().unwrap_or_default();
            lines.extend(parts);
            continue;
        }
        if !cur.is_empty() && need > cols {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&atom);
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines.into_iter().map(|l| l.replace('\u{a0}', " ")).collect()
}

fn wrap_words(text: &str, cols: usize) -> Vec<String> {
    let cols = cols.max(4);
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut words: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let chars: Vec<char> = word.chars().collect();
        if chars.len() <= cols {
            words.push(word.to_string());
        } else {
            words.extend(chars.chunks(cols).map(|c| c.iter().collect::<String>()));
        }
    }
    for word in words.iter().map(String::as_str) {
        let need = cur.chars().count() + usize::from(!cur.is_empty()) + word.chars().count();
        if !cur.is_empty() && need > cols {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

fn args_of(f: &Value) -> Vec<String> {
    f["args"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// A typed fact (see `present::Fact`) as a sentence in `lang`.
pub fn fact_text(f: &Value, lang: Lang) -> String {
    let a = args_of(f);
    let g = |i: usize| a.get(i).cloned().unwrap_or_default();
    match f["kind"].as_str().unwrap_or("") {
        "coll" => i18n::tf(lang, "fact.coll", &[("pts", a.join(", "))]),
        "cyclic" => i18n::tf(lang, "fact.cyclic", &[("pts", a.join(", "))]),
        "midp" => i18n::tf(lang, "fact.midp", &[("m", g(0)), ("seg", g(1))]),
        "circle" => i18n::tf(lang, "fact.circle", &[("o", g(0)), ("tri", g(1))]),
        "oncircle" => i18n::tf(lang, "fact.oncircle", &[("o", g(0)), ("pts", a[1..].join(", "))]),
        "bisector" => i18n::tf(lang, "fact.bisector", &[("p", g(0)), ("angle", g(1))]),
        "concur" if !a.is_empty() => i18n::tf(lang, "fact.concur", &[("lines", a[..a.len() - 1].join(", ")), ("p", g(a.len() - 1))]),
        k @ ("incenter" | "excenter" | "in_or_excenter") => i18n::tf(lang, &format!("fact.{k}"), &[("i", g(0)), ("tri", g(1))]),
        "prose" => {
            let ro = f["ro"].as_array().and_then(|r| r.first()).and_then(Value::as_str);
            match (lang, ro) {
                (Lang::Ro, Some(r)) => r.to_string(),
                _ => g(0),
            }
        }
        "cong" | "length" | "eqangle" | "coincide" => format!("{} = {}", g(0), g(1)),
        "perp" => format!("{} \u{27c2} {}", g(0), g(1)),
        "para" => format!("{} \u{2225} {}", g(0), g(1)),
        "eqratio" => a.chunks(2).map(|c| c.join(" : ")).collect::<Vec<_>>().join(" = "),
        "para_ratio" => i18n::tf(
            lang,
            "fact.para_ratio",
            &[("par", format!("{} \u{2225} {}", g(0), g(1))), ("ratio", format!("{} : {} = {} : {}", g(2), g(3), g(4), g(5)))],
        ),
        "aconst" => format!("{} = {}\u{b0}", g(0), g(1)),
        "rconst" => format!("{} : {} = {}", g(0), g(1), g(2)),
        "simtri" => format!("\u{25b3}{} \u{223c} \u{25b3}{}", g(0), g(1)),
        "contri" => format!("\u{25b3}{} \u{2245} \u{25b3}{}", g(0), g(1)),
        "eqdist" => a.join(" = "),
        "points" => a.join(", "),
        _ => a.join(" "),
    }
}

/// An auxiliary construction in words (`midpoint of BC`), falling back to the
/// engine's own notation for kinds without a template.
pub fn shape_words(x: &str, lang: Lang) -> String {
    let shapes = [
        ("circumcircle(", "aux.circumcircle", ","),
        ("circle(", "aux.circle", ","),
        ("para(", "aux.line.para", ","),
        ("perp(", "aux.line.perp", ","),
        ("tangent_at(", "aux.line.tangent_at", ","),
        ("isogonal(", "aux.line.isogonal", " in "),
    ];
    for (f, k, sep) in shapes {
        if let Some(inner) = x.trim().strip_prefix(f).and_then(|r| r.strip_suffix(')')) {
            let mut s = i18n::t(lang, k).to_string();
            for (i, p) in inner.split(sep).map(str::trim).enumerate() {
                let p = p.strip_prefix("centre ").or_else(|| p.strip_prefix("center ")).unwrap_or(p);
                s = s.replace(&format!("{{{i}}}"), p);
            }
            if !s.contains('{') {
                return s;
            }
        }
    }
    x.to_string()
}

pub fn aux_text(a: &Value, lang: Lang) -> String {
    let raw = a["text"].as_str().unwrap_or("").to_string();
    let kind = a["kind"].as_str().unwrap_or("");
    let mut key = format!("aux.{kind}");
    let raw_args: Vec<&str> = a["args"].as_array().map(|v| v.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if kind == "intersect" && raw_args.len() == 2 && raw_args[1].starts_with("circ") {
        let on: Vec<&str> = raw_args[1]
            .trim_end_matches(')')
            .split_once('(')
            .map(|(_, inner)| inner.split(',').map(str::trim).collect())
            .unwrap_or_default();
        if on.iter().any(|p| !p.is_empty() && raw_args[0].contains(p)) {
            key = "aux.intersect2".to_string();
        }
    }
    let template = i18n::t(lang, &key);
    if template == i18n::t(lang, "aux.__none__") {
        return raw;
    }
    let mut args: Vec<String> = a["args"]
        .as_array()
        .map(|v| v.iter().filter_map(|x| x.as_str()).map(|x| shape_words(x, lang)).collect())
        .unwrap_or_default();
    if matches!(kind, "midpoint" | "circumcenter" | "orthocenter" | "parallelogram") {
        args = args.iter().flat_map(|x| x.split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>()).collect();
    }
    let mut s = template.to_string();
    for (i, p) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), p);
    }
    if s.contains('{') { raw } else { s }
}

fn rule_text(step: &Value, lang: Lang) -> String {
    match (step["rule"].as_str().unwrap_or("other"), step["rule_name"].as_str()) {
        ("theorem", Some(name)) => match (lang, step["rule_name_ro"].as_str()) {
            (Lang::Ro, Some(ro)) => ro.to_string(),
            _ => name.to_string(),
        },
        (key, _) => i18n::t(lang, &format!("rule.{key}")).to_string(),
    }
}

fn fmt_num(x: f64, lang: Lang, digits: usize) -> String {
    let s = format!("{x:.digits$}");
    if lang == Lang::Ro {
        s.replace('.', ",")
    } else {
        s
    }
}

fn fmt_secs(x: f64, lang: Lang) -> String {
    if x < 0.001 {
        "< 1 ms".to_string()
    } else if x < 1.0 {
        format!("{} ms", (x * 1000.0).round())
    } else {
        format!("{} s", fmt_num(x, lang, if x < 10.0 { 1 } else { 0 }))
    }
}

fn counter_text(c: &Value, lang: Lang) -> Option<String> {
    let kind = c["kind"].as_str()?;
    let labels: Vec<String> = c["labels"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let l = |i: usize| labels.get(i).cloned().unwrap_or_default();
    let (lhs, rhs) = (c["lhs"].as_f64()?, c["rhs"].as_f64()?);
    let digits = if matches!(kind, "values" | "length" | "ratios") { 4 } else { 1 };
    Some(i18n::tf(
        lang,
        &format!("counter.{kind}"),
        &[
            ("a", l(0)),
            ("b", l(1)),
            ("lhs", fmt_num(lhs, lang, digits)),
            ("rhs", fmt_num(rhs, lang, digits)),
        ],
    ))
}

/// The export report for a solution as the web app returns it (engine fields
/// plus `view` and `title`), as one tall page (the PNG report).
pub fn report_from_json(v: &Value, lang: Lang) -> String {
    report_pages(v, lang, false).into_iter().next().unwrap_or_default()
}

/// The report as A4 pages: the figure on page 1, breaks only between blocks
/// (never inside a step), a running header and page numbers.
pub fn report_pdf_from_json(v: &Value, lang: Lang) -> Result<Vec<u8>> {
    pdf_from_pages(&report_pages(v, lang, true))
}

/// The PNG report: the tall single page at print resolution.
pub fn report_png_from_json(v: &Value, lang: Lang) -> Result<Vec<u8>> {
    svg_to_png(&report_from_json(v, lang), 2.75)
}

/// The report for a [`Solution`] (CLI and MCP exports), in English, as one
/// tall page.
pub fn report_svg(sol: &Solution, title: Option<&str>, _light: bool) -> String {
    report_from_json(&present::solution_json(sol, title), Lang::En)
}

/// [`report_svg`] as paginated A4 PDF.
pub fn report_pdf(sol: &Solution, title: Option<&str>) -> Result<Vec<u8>> {
    report_pdf_from_json(&present::solution_json(sol, title), Lang::En)
}

/// Assemble one PDF from page SVGs (each converted with svg2pdf and placed as
/// a full-page form XObject).
pub fn pdf_from_pages(pages: &[String]) -> Result<Vec<u8>> {
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
    let mut alloc = Ref::new(1);
    let catalog = alloc.bump();
    let tree_id = alloc.bump();
    let mut pdf = Pdf::new();
    let mut kids = Vec::new();
    let mut chunks = Vec::new();
    let mut page_objs = Vec::new();
    for svg in pages {
        let tree = parse_svg(svg)?;
        check_svg_size(&tree)?;
        let (w, h) = (tree.size().width(), tree.size().height());
        let (chunk, root) = svg2pdf::to_chunk(&tree, svg2pdf::ConversionOptions::default())
            .map_err(|e| anyhow!("PDF conversion error: {e}"))?;
        let mut map = std::collections::HashMap::new();
        let chunk = chunk.renumber(|old| *map.entry(old).or_insert_with(|| alloc.bump()));
        let root = *map.get(&root).ok_or_else(|| anyhow!("PDF conversion lost the page"))?;
        let (page_id, content_id) = (alloc.bump(), alloc.bump());
        kids.push(page_id);
        page_objs.push((page_id, content_id, root, w, h));
        chunks.push(chunk);
    }
    pdf.catalog(catalog).pages(tree_id);
    pdf.pages(tree_id).kids(kids.iter().copied()).count(kids.len() as i32);
    for (page_id, content_id, root, w, h) in page_objs {
        let name = Name(b"P0");
        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, w, h));
        page.parent(tree_id);
        page.contents(content_id);
        page.resources().x_objects().pair(name, root);
        page.finish();
        let mut content = Content::new();
        content.transform([w, 0.0, 0.0, h, 0.0, 0.0]).x_object(name);
        pdf.stream(content_id, &content.finish());
    }
    for c in &chunks {
        pdf.extend(c);
    }
    Ok(pdf.finish())
}

const PAGE_W: f32 = 595.0;
const PAGE_H: f32 = 842.0;
const MARGIN: f32 = 48.0;
const BODY_FS: f32 = 11.0;
const LEADING: f32 = 16.0;
const FIG_MAX_H: f32 = 330.0;
/// A report that would spill onto a second page gets a figure down to this
/// height instead.
const FIG_MIN_H: f32 = 190.0;
const FIG_ONE_PAGE_MIN_H: f32 = 150.0;
const BESIDE_GAIN: f32 = 220.0;
const UI: &str = "'GeoSolver Sans', 'DejaVu Sans', sans-serif";
const MATH: &str = "'GeoSolver Math', 'DejaVu Serif', serif";
const INK: &str = "#16191d";
const MUTED: &str = "#4b525a";
const RULE: &str = "#dadcd8";
const ACCENT: &str = "#1f4fa0";
const AUX_INK: &str = "#b45309";

struct Block {
    h: f32,
    body: String,
    keep_next: usize,
}

fn fmt_int(n: u64, lang: Lang) -> String {
    let digits = n.to_string();
    if digits.len() <= 3 {
        return digits;
    }
    let sep = if lang == Lang::Ro { '.' } else { ',' };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

fn tpn(lang: Lang, key: &str, n: u64) -> String {
    i18n::tp(lang, key, n, &[]).replace(&n.to_string(), &fmt_int(n, lang))
}

fn method_text(v: &Value, lang: Lang) -> Option<String> {
    let status = v["status"].as_str().unwrap_or("");
    let method = v["method"].as_str().unwrap_or("");
    match status {
        "holds-numerically" => return Some(i18n::t(lang, "report.method.numeric").to_string()),
        "refuted" => {
            let key = if v["view"]["counterexample"].is_null() { "report.method.numeric" } else { "report.method.counter" };
            return Some(i18n::t(lang, key).to_string());
        }
        _ => {}
    }
    if method == "euclidean" {
        return (status == "proved").then(|| i18n::t(lang, "report.method.euclid").to_string());
    }
    let aux = v["aux_constructions"].as_array().map_or(0, Vec::len) as u64;
    Some(if aux > 0 {
        tpn(lang, "report.method.aux", aux)
    } else if method == "aux-search" {
        i18n::t(lang, "report.method.aux_search").to_string()
    } else {
        i18n::t(lang, "report.method.ddar").to_string()
    })
}

fn verdict_copy(v: &Value, lang: Lang) -> (String, String) {
    let status = v["status"].as_str().unwrap_or("not-proved");
    let view = &v["view"];
    let note = view["note"]["key"].as_str().unwrap_or("");
    let samples = v["numeric_samples"].as_u64().unwrap_or(0);
    let secs = fmt_num(view["note"]["secs"].as_f64().unwrap_or(60.0), lang, 0);
    let t = |k: &str| i18n::t(lang, k).to_string();
    match status {
        "proved" if view["as_drawn"].as_bool() == Some(true) => (t("report.verdict.as_drawn"), t("report.explain.as_drawn")),
        "proved" if view["proof"]["steps"].as_array().is_none_or(|a| a.is_empty()) => (t("report.verdict.proved"), t("report.explain.immediate")),
        "proved" => (t("report.verdict.proved"), t("report.explain.proved")),
        "refuted" => (t("report.verdict.refuted"), t("report.explain.refuted")),
        "holds-numerically" => (t("report.verdict.holds-numerically"), tpn(lang, "report.explain.holds-numerically", samples)),
        _ if note == "time_limit" => (t("report.verdict.time_limit"), i18n::tf(lang, "report.explain.time_limit", &[("secs", secs)])),
        _ => {
            let explain = match (note, view["note"]["runs"].as_u64()) {
                ("budget", Some(runs)) => i18n::tf(lang, "report.explain.budget", &[("runs", tpn(lang, "report.runs", runs))]),
                ("metric_error" | "unsound" | "replay", _) => t(&format!("report.explain.{note}")),
                _ => t("report.explain.not-proved"),
            };
            (t("report.verdict.not-proved"), explain)
        }
    }
}

struct Typeset<'a> {
    names: Vec<&'a str>,
}

impl Typeset<'_> {
    fn is_name_run(&self, tok: &str) -> bool {
        let mut rest = tok.trim_end_matches(['²', '³']);
        if rest.is_empty() || !rest.starts_with(|c: char| c.is_uppercase()) {
            return false;
        }
        while !rest.is_empty() {
            let Some(n) = self.names.iter().filter(|n| rest.starts_with(**n)).max_by_key(|n| n.len()) else {
                return false;
            };
            rest = &rest[n.len()..];
        }
        true
    }

    fn spans(&self, text: &str) -> String {
        let text = normalize_glyphs(text);
        let mut out = String::new();
        let mut tok = String::new();
        let flush = |tok: &mut String, out: &mut String| {
            if tok.is_empty() {
                return;
            }
            let fn_len = ["sin", "cos", "tan"].iter().find(|f| tok.starts_with(**f) && tok[f.len()..].starts_with(['∠', '△'])).map_or(0, |f| f.len());
            let lead: String = tok[..fn_len].to_string() + &tok[fn_len..].chars().take_while(|c| matches!(c, '△' | '∠' | '|' | '(' | '[')).collect::<String>();
            let core = &tok[lead.len()..];
            let trail_len = core.chars().rev().take_while(|c| matches!(c, '|' | ')' | ']' | '²' | '³')).map(char::len_utf8).sum::<usize>();
            let (name, trail) = core.split_at(core.len() - trail_len);
            if self.is_name_run(name) {
                out.push_str(&escape_xml(&lead));
                out.push_str(&format!("<tspan font-style=\"italic\">{}</tspan>", escape_xml(name)));
                out.push_str(&escape_xml(trail));
            } else {
                out.push_str(&escape_xml(tok));
            }
            tok.clear();
        };
        for c in text.chars() {
            if c.is_whitespace() || matches!(c, ',' | '.' | ';' | ':' | '=' | '+' | '−' | '·' | '∼' | '≅' | '⊥' | '‖' | '∥' | '⟂' | '/') {
                flush(&mut tok, &mut out);
                out.push_str(&escape_xml(&c.to_string()));
            } else {
                tok.push(c);
            }
        }
        flush(&mut tok, &mut out);
        out
    }
}

fn txt(x: f32, y: f32, size: f32, weight: u32, fill: &str, family: &str, inner: &str) -> String {
    format!(
        "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"{size}\" font-weight=\"{weight}\" fill=\"{fill}\" font-family=\"{family}\" xml:space=\"preserve\">{inner}</text>\n"
    )
}

fn plain(x: f32, y: f32, size: f32, weight: u32, fill: &str, family: &str, s: &str) -> String {
    txt(x, y, size, weight, fill, family, &escape_xml(&normalize_glyphs(s)))
}

fn wordmark(x: f32, y: f32) -> String {
    format!(
        "<g transform=\"translate({x:.1},{:.1}) scale(0.75)\" fill=\"none\" stroke=\"{ACCENT}\" stroke-width=\"2\" stroke-linejoin=\"round\"><circle cx=\"16\" cy=\"16\" r=\"13\"/><path d=\"M16 4.6 26.2 22H5.8Z\"/></g>\n{}",
        y - 18.0,
        plain(x + 31.0, y, 16.0, 600, INK, MATH, "GeoSolver")
    )
}

fn report_pages(v: &Value, lang: Lang, paginate: bool) -> Vec<String> {
    report_pages_fit(v, lang, paginate, FIG_MAX_H)
}

fn report_pages_fit(v: &Value, lang: Lang, paginate: bool, fig_max_h: f32) -> Vec<String> {
    let status = v["status"].as_str().unwrap_or("not-proved");
    let view = &v["view"];
    let time_limited = view["note"]["key"].as_str() == Some("time_limit");
    let t = tone(status, time_limited);
    let (headline, explain) = verdict_copy(v, lang);
    let samples = v["numeric_samples"].as_u64().unwrap_or(0);
    let title = v["title"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| i18n::t(lang, "report.default_title").to_string());
    let mut point_names: Vec<String> = view["points"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p["name"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    for st in view["proof"]["steps"].as_array().into_iter().flatten() {
        for f in std::iter::once(&st["fact"]).chain(st["subs"].as_array().into_iter().flatten().map(|u| &u["fact"])) {
            for p in f["points"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                if !point_names.iter().any(|n| n == p) {
                    point_names.push(p.to_string());
                }
            }
        }
    }
    let ts = Typeset { names: point_names.iter().map(String::as_str).collect() };
    let content_w = PAGE_W - 2.0 * MARGIN;
    let cols = (content_w / (BODY_FS * 0.5)) as usize;
    let mut blocks: Vec<Block> = Vec::new();

    let mut head = wordmark(MARGIN, 18.0);
    let mut y = 52.0;
    for line in wrap(&title, 44) {
        head.push_str(&plain(MARGIN, y, 19.0, 600, INK, UI, &line));
        y += 25.0;
    }
    y += 2.0;
    let explain_lines = wrap(&explain, ((content_w - 56.0) / (10.0 * 0.5)) as usize);
    let card_h = 40.0 + explain_lines.len() as f32 * 15.0;
    head.push_str(&format!(
        "<clipPath id=\"verdict-card\"><rect x=\"{MARGIN}\" y=\"{y:.1}\" width=\"{content_w}\" height=\"{card_h:.1}\" rx=\"8\"/></clipPath>\n<rect x=\"{MARGIN}\" y=\"{y:.1}\" width=\"{content_w}\" height=\"{card_h:.1}\" rx=\"8\" fill=\"{}\" stroke=\"{}\" stroke-opacity=\"0.35\"/>\n<rect x=\"{MARGIN}\" y=\"{y:.1}\" width=\"4\" height=\"{card_h:.1}\" fill=\"{}\" clip-path=\"url(#verdict-card)\"/>\n",
        t.bg, t.ink, t.ink
    ));
    head.push_str(&verdict_icon(status, time_limited, MARGIN + 14.0, y + 10.0, t.ink));
    head.push_str(&plain(MARGIN + 42.0, y + 24.0, 14.0, 600, t.ink, UI, &headline));
    for (i, l) in explain_lines.iter().enumerate() {
        head.push_str(&plain(MARGIN + 42.0, y + 41.0 + i as f32 * 15.0, 10.0, 400, INK, UI, l));
    }
    y += card_h + 18.0;
    let mut meta: Vec<String> = method_text(v, lang).into_iter().collect();
    let secs = fmt_secs(v["elapsed_secs"].as_f64().unwrap_or(0.0), lang);
    meta.push(if v["search"] == "shortest" {
        i18n::tf(lang, "report.shortest_search", &[("t", secs)])
    } else {
        secs
    });
    let shown_steps = view["proof"]["steps"].as_array().map_or(0, |a| a.iter().filter(|s| s["kind"] == "step").count());
    if status == "proved" && shown_steps > 0 {
        meta.push(tpn(lang, "report.steps", shown_steps as u64));
    }
    if status == "holds-numerically" && samples > 0 {
        meta.push(tpn(lang, "report.samples", samples));
    }
    if status == "proved" && view["as_drawn"].as_bool() == Some(true) {
        meta.push(i18n::t(lang, "report.as_drawn").to_string());
    }
    head.push_str(&plain(MARGIN, y, 9.5, 400, MUTED, UI, &meta.join("  \u{b7}  ")));
    y += 12.0;
    let given: Vec<String> = view["given"]
        .as_array()
        .map(|a| a.iter().map(|f| fact_text(f, lang)).collect())
        .unwrap_or_default();
    let goal_text = (!view["goal"].is_null()).then(|| fact_text(&view["goal"], lang));
    let inner_w = content_w - 24.0;
    let svg = fit_figure(v["svg"].as_str().unwrap_or(""), inner_w, fig_max_h);
    let svg = svg.as_str();
    let mut fig_h = 0.0;
    let mut beside = false;
    if let Some((fw, fh)) = svg_dimensions(svg).or_else(|| viewbox_size(svg)) {
        let scale = (inner_w / fw).min(fig_max_h / fh);
        let (w, h) = (fw * scale, fh * scale);
        let vb = viewbox_attr(svg).unwrap_or_else(|| format!("0 0 {fw} {fh}"));
        let figure = |x: f32, y: f32, frame_x: f32, frame_w: f32| {
            format!(
                "<rect x=\"{frame_x:.1}\" y=\"{y:.1}\" width=\"{frame_w:.1}\" height=\"{:.1}\" rx=\"8\" fill=\"#ffffff\" stroke=\"{RULE}\"/>\n<svg x=\"{x:.1}\" y=\"{:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" viewBox=\"{vb}\" preserveAspectRatio=\"xMidYMid meet\" font-family=\"{MATH}\">\n{}\n</svg>\n",
                h + 24.0,
                y + 12.0,
                strip_svg_root(svg)
            )
        };
        let frame_w = w + 24.0;
        let text_w = content_w - frame_w - 20.0;
        let side = beside_text(&given, goal_text.as_deref(), text_w, lang, &ts);
        if frame_w <= content_w * 0.62 && text_w >= 170.0 && side.1 <= h + 24.0 + 40.0 {
            beside = true;
            blocks.push(Block { h: y, body: head, keep_next: 1 });
            let mut b = figure(MARGIN + content_w - frame_w + 12.0, 0.0, MARGIN + content_w - frame_w, frame_w);
            b.push_str(&side.0);
            blocks.push(Block { h: (h + 24.0).max(side.1) + 10.0, body: b, keep_next: 0 });
            head = String::new();
        } else {
            head.push_str(&figure(MARGIN + (content_w - w) / 2.0, y, MARGIN, content_w));
            y += h + 24.0 + 10.0;
        }
        fig_h = h;
    }
    if !beside {
        blocks.push(Block { h: y, body: head, keep_next: 0 });
    }

    let section = |blocks: &mut Vec<Block>, label: &str, items: usize| {
        let mut b = plain(MARGIN, 22.0, 9.0, 600, MUTED, UI, &label.to_uppercase());
        b.push_str(&format!(
            "<line x1=\"{MARGIN}\" y1=\"28\" x2=\"{:.1}\" y2=\"28\" stroke=\"{RULE}\"/>\n",
            PAGE_W - MARGIN
        ));
        let keep = if items <= 3 { items } else { 2 };
        blocks.push(Block { h: 44.0, body: b, keep_next: keep });
    };
    let lines_block = |blocks: &mut Vec<Block>, lines: &[String], fill: &str, weight: u32, bullet: bool, ts: &Typeset, keep_next: usize| {
        let mut b = String::new();
        for (i, l) in lines.iter().enumerate() {
            let lead = if bullet && i == 0 { "\u{2022}  " } else if bullet { "   " } else { "" };
            b.push_str(&txt(MARGIN, 12.0 + i as f32 * LEADING, BODY_FS, weight, fill, MATH, &format!("{}{}", escape_xml(lead), ts.spans(l))));
        }
        blocks.push(Block { h: lines.len() as f32 * LEADING + 2.0, body: b, keep_next });
    };
    let penultimate = |i: usize, n: usize| usize::from(n >= 2 && i + 2 == n);

    if !given.is_empty() && !beside {
        section(&mut blocks, i18n::t(lang, "report.given"), given.len());
        for (i, g) in given.iter().enumerate() {
            lines_block(&mut blocks, &wrap(g, cols - 4), INK, 400, true, &ts, penultimate(i, given.len()));
        }
    }
    if let Some(goal) = goal_text.as_ref().filter(|_| !beside) {
        section(&mut blocks, i18n::t(lang, "report.prove"), 1);
        lines_block(&mut blocks, &wrap(goal, cols), INK, 600, false, &ts, 0);
    }
    if let Some(c) = view.get("counterexample").filter(|c| !c.is_null()).and_then(|c| counter_text(c, lang)) {
        section(&mut blocks, i18n::t(lang, "report.counter"), 1);
        lines_block(&mut blocks, &wrap(&c, cols), t.ink, 400, false, &ts, 0);
    }
    if let Some(aux) = view["aux"].as_array().filter(|a| !a.is_empty()) {
        section(&mut blocks, i18n::t(lang, "report.aux"), aux.len());
        for (i, a) in aux.iter().enumerate() {
            let name = a["name"].as_str().unwrap_or("");
            let lines = wrap(&format!("{name}: {}", aux_text(a, lang)), cols);
            let mut b = String::new();
            for (k, l) in lines.iter().enumerate() {
                let inner = match l.strip_prefix(&format!("{name}:")).filter(|_| k == 0) {
                    Some(rest) => format!("<tspan fill=\"{AUX_INK}\" font-weight=\"600\">{}</tspan>:{}", ts.spans(name), ts.spans(rest)),
                    None => ts.spans(l),
                };
                b.push_str(&txt(MARGIN, 12.0 + k as f32 * LEADING, BODY_FS, 400, INK, MATH, &inner));
            }
            blocks.push(Block { h: lines.len() as f32 * LEADING + 2.0, body: b, keep_next: penultimate(i, aux.len()) });
        }
    }
    let steps = view["proof"]["steps"].as_array().cloned().unwrap_or_default();
    if status == "proved" && !steps.is_empty() {
        section(&mut blocks, i18n::t(lang, "report.proof"), steps.len());
        let gutter = 28.0;
        let step_cols = ((content_w - gutter) / (BODY_FS * 0.5)) as usize;
        let restated: Vec<u64> = steps.iter().filter(|s| s["kind"] == "given").filter_map(|s| s["n"].as_u64()).collect();
        let fold = restated.len() >= 2;
        for st in &steps {
            if fold && st["kind"] == "given" {
                if st["n"].as_u64() == restated.first().copied() {
                    let text = i18n::t(lang, "report.hyps").replace("{list}", &ranges(&restated));
                    let mut body = String::new();
                    let lines = wrap(&text, (step_cols as f32 * BODY_FS / 8.5) as usize);
                    for (i, l) in lines.iter().enumerate() {
                        body.push_str(&plain(MARGIN + gutter, 10.0 + i as f32 * 12.0, 8.5, 400, MUTED, UI, l));
                    }
                    blocks.push(Block { h: lines.len() as f32 * 12.0 + 8.0, body, keep_next: 0 });
                }
                continue;
            }
            let fact = fact_text(&st["fact"], lang);
            let rule = rule_text(st, lang);
            let deps: Vec<String> = st["deps"]
                .as_array()
                .map(|d| d.iter().filter_map(|x| x.as_u64().map(|n| n.to_string())).collect())
                .unwrap_or_default();
            let mut meta = rule;
            if !deps.is_empty() {
                meta.push_str(&format!("  \u{2190} {}", deps.join(", ")));
            }
            let lines = wrap(&fact, step_cols);
            let n = st["n"].as_u64().unwrap_or(0);
            let mut b = plain(MARGIN, 12.0, 9.5, 600, MUTED, UI, &format!("{n}."));
            let mut y = 12.0;
            for l in &lines {
                b.push_str(&txt(MARGIN + gutter, y, BODY_FS, 400, INK, MATH, &ts.spans(l)));
                y += LEADING;
            }
            for sub in st["subs"].as_array().into_iter().flatten() {
                let rule = rule_text(sub, lang);
                let sub_lines = wrap(&fact_text(&sub["fact"], lang), step_cols - 4);
                let last = sub_lines.len() - 1;
                let inline = sub_lines[last].chars().count() + rule.chars().count() * 8 / 10 + 3 <= step_cols - 4;
                for (i, l) in sub_lines.iter().enumerate() {
                    let lead = if i == 0 { "\u{2022}  " } else { "    " };
                    let tail = if i == last && inline {
                        format!("<tspan font-family=\"{UI}\" font-size=\"7.5\" fill=\"{MUTED}\">   {}</tspan>", escape_xml(&rule))
                    } else {
                        String::new()
                    };
                    b.push_str(&txt(MARGIN + gutter + 6.0, y, BODY_FS - 0.5, 400, INK, MATH, &format!("{}{}{tail}", escape_xml(lead), ts.spans(l))));
                    y += LEADING - 1.0;
                }
                if !inline {
                    b.push_str(&plain(MARGIN + gutter + 18.0, y - 3.0, 7.5, 400, MUTED, UI, &rule));
                    y += 9.0;
                }
            }
            let my = y - 2.0;
            b.push_str(&plain(MARGIN + gutter, my, 8.5, 400, MUTED, UI, &meta));
            blocks.push(Block { h: y - 12.0 + 16.0, body: b, keep_next: 0 });
        }
        if let Some(c) = view["proof"].get("conclusion").filter(|c| !c.is_null()) {
            if let Some(last) = blocks.last_mut() {
                last.keep_next = 1;
            }
            let lines = wrap(&fact_text(c, lang), step_cols);
            let mut b = txt(MARGIN + gutter - 8.0, 12.0, BODY_FS, 600, "#17703a", MATH, "\u{220e}").replace("<text ", "<text text-anchor=\"end\" ");
            for (i, l) in lines.iter().enumerate() {
                b.push_str(&txt(MARGIN + gutter, 12.0 + i as f32 * LEADING, BODY_FS, 600, INK, MATH, &ts.spans(l)));
            }
            blocks.push(Block { h: lines.len() as f32 * LEADING + 2.0, body: b, keep_next: 0 });
        }
    }

    let footer_text = i18n::t(lang, "report.footer");
    let page_svg = |body: &str, h: f32| {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{PAGE_W:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {PAGE_W:.0} {h:.0}\" font-family=\"{UI}\">\n<rect width=\"{PAGE_W:.0}\" height=\"{h:.0}\" fill=\"#ffffff\"/>\n{body}</svg>\n"
        )
    };
    let footer = |h: f32, page: Option<(usize, usize)>| {
        let fy = h - 28.0;
        let mut f = format!(
            "<line x1=\"{MARGIN}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{RULE}\"/>\n",
            fy - 12.0,
            PAGE_W - MARGIN,
            fy - 12.0
        );
        f.push_str(&plain(MARGIN, fy, 8.5, 400, MUTED, UI, footer_text));
        if let Some((i, n)) = page {
            let s = format!("{i} / {n}");
            f.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{fy:.1}\" font-size=\"8.5\" fill=\"{MUTED}\" font-family=\"{UI}\" text-anchor=\"end\">{s}</text>\n",
                PAGE_W - MARGIN
            ));
        }
        f
    };

    if !paginate {
        let mut body = String::new();
        let mut y = MARGIN - 10.0;
        for b in &blocks {
            body.push_str(&format!("<g transform=\"translate(0,{y:.1})\">\n{}</g>\n", b.body));
            y += b.h;
        }
        let h = y + 70.0;
        body.push_str(&footer(h, None));
        return vec![page_svg(&body, h)];
    }

    let rule_y = PAGE_H - 40.0;
    let bottom = rule_y - 8.0;
    let total: f32 = blocks.iter().map(|b| b.h).sum();
    let overflow = total - (bottom - (MARGIN - 10.0));
    if paginate && overflow > 0.0 && fig_h > 0.0 && fig_max_h >= FIG_MAX_H {
        if overflow <= fig_h - FIG_ONE_PAGE_MIN_H + BESIDE_GAIN {
            let mut h = fig_h - 10.0;
            while h >= FIG_ONE_PAGE_MIN_H {
                let pages = report_pages_fit(v, lang, paginate, h);
                if pages.len() == 1 {
                    return pages;
                }
                h -= 15.0;
            }
        }
        let smaller = fig_h - overflow - 8.0;
        if smaller >= FIG_MIN_H {
            return report_pages_fit(v, lang, paginate, smaller);
        }
    }
    let running = |title: &str| {
        let short: String = if title.chars().count() > 70 {
            format!("{}\u{2026}", title.chars().take(69).collect::<String>().trim_end())
        } else {
            title.to_string()
        };
        let mut r = plain(MARGIN, 34.0, 8.5, 500, MUTED, UI, &short);
        r.push_str(&format!(
            "<text x=\"{:.1}\" y=\"34\" font-size=\"8.5\" font-weight=\"600\" fill=\"{}\" font-family=\"{UI}\" text-anchor=\"end\">{}</text>\n<line x1=\"{MARGIN}\" y1=\"42\" x2=\"{:.1}\" y2=\"42\" stroke=\"{RULE}\"/>\n",
            PAGE_W - MARGIN,
            t.ink,
            escape_xml(&headline),
            PAGE_W - MARGIN
        ));
        r
    };
    let breaks_for = |force: Option<usize>| -> Vec<usize> {
        let mut starts = vec![0usize];
        let mut y = MARGIN - 10.0;
        for i in 0..blocks.len() {
            let b = &blocks[i];
            let need: f32 = b.h + (1..=b.keep_next).filter_map(|k| blocks.get(i + k)).map(|n| n.h).sum::<f32>();
            let top = if starts.len() == 1 { MARGIN - 10.0 } else { 54.0 };
            let tail: f32 = blocks[i..].iter().map(|b| b.h).sum();
            let short_tail = blocks.len() - i <= 3 && y + tail <= rule_y - 2.0;
            let forced = force == Some(i) && i > *starts.last().unwrap_or(&0);
            if forced || (y + need > bottom && y > top + 1.0 && !short_tail) {
                starts.push(i);
                y = 54.0;
            }
            y += b.h;
        }
        starts
    };
    let mut starts = breaks_for(None);
    let last = *starts.last().unwrap_or(&0);
    if starts.len() > 1 && blocks.len() - last < 3 {
        let prev = starts[starts.len() - 2];
        let mut pull = blocks.len().saturating_sub(3).max(prev + 1);
        while pull > prev + 1 && blocks[pull - 1].keep_next > 0 {
            pull -= 1;
        }
        if pull < last {
            starts = breaks_for(Some(pull));
        }
    }
    let mut pages: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut y = MARGIN - 10.0;
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 && starts.contains(&i) {
            pages.push(std::mem::take(&mut cur));
            y = 54.0;
        }
        cur.push_str(&format!("<g transform=\"translate(0,{y:.1})\">\n{}</g>\n", b.body));
        y += b.h;
    }
    pages.push(cur);
    let n = pages.len();
    pages
        .into_iter()
        .enumerate()
        .map(|(k, body)| {
            let mut full = if k > 0 { running(&title) } else { String::new() };
            full.push_str(&body);
            full.push_str(&footer(PAGE_H, Some((k + 1, n))));
            page_svg(&full, PAGE_H)
        })
        .collect()
}

/// Point labels on the page: this size in points, whatever the figure's scale.
const LABEL_PT: f32 = 10.5;

fn fit_figure(svg: &str, max_w: f32, max_h: f32) -> String {
    let scale_of = |s: &str| svg_dimensions(s).or_else(|| viewbox_size(s)).map(|(w, h)| (max_w / w).min(max_h / h));
    let Some(s0) = scale_of(svg) else { return svg.to_string() };
    let first = crate::figure::relabel(svg, (LABEL_PT / (18.0 * s0)) as f64);
    match scale_of(&first) {
        Some(s1) => crate::figure::relabel(svg, (LABEL_PT / (18.0 * s1)) as f64),
        None => first,
    }
}

/// GIVEN and PROVE set in a column `width` wide (beside a narrow figure):
/// the markup and its height.
fn beside_text(given: &[String], goal: Option<&str>, width: f32, lang: Lang, ts: &Typeset) -> (String, f32) {
    let cols = (width / (BODY_FS * 0.5)) as usize;
    let mut b = String::new();
    let mut y = 0.0;
    let section = |b: &mut String, y: &mut f32, label: &str| {
        b.push_str(&plain(MARGIN, *y + 10.0, 9.0, 600, MUTED, UI, &label.to_uppercase()));
        b.push_str(&format!(
            "<line x1=\"{MARGIN}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{RULE}\"/>\n",
            *y + 16.0,
            MARGIN + width,
            *y + 16.0
        ));
        *y += 32.0;
    };
    if !given.is_empty() {
        section(&mut b, &mut y, i18n::t(lang, "report.given"));
        for g in given {
            for (i, l) in wrap(g, cols.saturating_sub(4)).iter().enumerate() {
                let lead = if i == 0 { "\u{2022}  " } else { "   " };
                b.push_str(&txt(MARGIN, y, BODY_FS, 400, INK, MATH, &format!("{}{}", escape_xml(lead), ts.spans(l))));
                y += LEADING;
            }
        }
        y += 14.0;
    }
    if let Some(goal) = goal {
        section(&mut b, &mut y, i18n::t(lang, "report.prove"));
        for l in wrap(goal, cols) {
            b.push_str(&txt(MARGIN, y, BODY_FS, 600, INK, MATH, &ts.spans(&l)));
            y += LEADING;
        }
    }
    (b, y)
}

/// Step numbers as ranges: `[1, 2, 3, 5]` → `1–3, 5`.
fn ranges(ns: &[u64]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < ns.len() {
        let mut j = i;
        while j + 1 < ns.len() && ns[j + 1] == ns[j] + 1 {
            j += 1;
        }
        out.push(if j > i { format!("{}\u{2013}{}", ns[i], ns[j]) } else { ns[i].to_string() });
        i = j + 1;
    }
    out.join(", ")
}

/// Read the `width`/`height` attributes off an SVG document's root tag.
fn svg_dimensions(svg: &str) -> Option<(f32, f32)> {
    let (w, h) = (root_attr(svg, "width")?, root_attr(svg, "height")?);
    let (w, h): (f32, f32) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()).then_some((w, h))
}

fn root_attr(svg: &str, name: &str) -> Option<String> {
    let open = svg.find("<svg")?;
    let end = svg[open..].find('>')? + open;
    let tag = &svg[open..end];
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    let rest = &tag[at..];
    let close = rest.find('"')?;
    Some(rest[..close].to_string())
}

fn viewbox_attr(svg: &str) -> Option<String> {
    root_attr(svg, "viewBox")
}

fn viewbox_size(svg: &str) -> Option<(f32, f32)> {
    let vb = viewbox_attr(svg)?;
    let n: Vec<f32> = vb.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| (n[2], n[3]))
}

/// Strip the outer `<svg ...>` … `</svg>` wrapper, returning the inner markup.
fn strip_svg_root(svg: &str) -> String {
    let after_open = match svg.find("<svg") {
        Some(i) => match svg[i..].find('>') {
            Some(j) => i + j + 1,
            None => return svg.to_string(),
        },
        None => return svg.to_string(),
    };
    let before_close = svg.rfind("</svg>").unwrap_or(svg.len());
    svg.get(after_open..before_close).unwrap_or("").trim().to_string()
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{solve, SolveOptions};

    fn long_report(lines: usize) -> String {
        let mut sol = solve("A B C = triangle\nprove coll(A, B, A)", &SolveOptions::default())
            .expect("solve");
        let proof: Vec<String> = (1..=lines)
            .map(|i| format!("{i:03}. cong A B C D [{:03}]", i.saturating_sub(1)))
            .collect();
        sol.proof = Some(proof.join("\n"));
        report_svg(&sol, Some("long"), true)
    }

    fn png_dims(png: &[u8]) -> (u32, u32) {
        let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        (be(&png[16..20]), be(&png[20..24]))
    }

    #[test]
    fn long_proofs_still_rasterise_at_a_reduced_scale() {
        let svg = long_report(3000);
        let png = svg_to_png(&svg, 2.0).expect("a long proof must still export to PNG");
        let (w, h) = png_dims(&png);
        assert!((w as u64) * (h as u64) <= MAX_PIXELS, "{w}x{h}");
        assert!(h > w, "scaled, not cropped: {w}x{h}");
    }

    #[test]
    fn short_reports_keep_the_requested_scale() {
        let tree = parse_svg(&long_report(5)).unwrap();
        let png = png_bytes(&tree, 2.0).unwrap();
        let (w, _) = png_dims(&png);
        assert_eq!(w, (tree.size().width() * 2.0).ceil() as u32);
    }

    #[test]
    fn pdf_reports_are_paginated_a4() {
        let mut sol = solve(
            "A B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))",
            &SolveOptions::default(),
        )
        .expect("solve");
        let short = report_pdf(&sol, Some("short")).unwrap();
        let text = String::from_utf8_lossy(&short);
        assert!(text.contains("/MediaBox [0 0 595 842]"), "A4 pages");
        assert!(text.contains("/Count 1") || text.contains("/Count 2"), "a short proof fits on a page or two");
        let proof: Vec<String> = (1..=150usize).map(|i| format!("{i:03}. cong A B C D [{:03}]", i.saturating_sub(1))).collect();
        sol.proof = Some(proof.join("\n"));
        let pages = report_pages(&present::solution_json(&sol, Some("long")), Lang::En, true);
        assert!(pages.len() >= 3, "{} pages", pages.len());
        assert!(pages.iter().all(|p| p.contains("height=\"842\"")));
        assert!(pages[1].contains(" / "), "page numbers");
        let pdf = pdf_from_pages(&pages).unwrap();
        assert!(String::from_utf8_lossy(&pdf).contains(&format!("/Count {}", pages.len())));
    }

    #[test]
    fn long_proof_pdf_pages_stay_within_viewer_limits() {
        let mut sol = solve(
            "A B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))",
            &SolveOptions::default(),
        )
        .expect("solve");
        let proof: Vec<String> = (1..=3000usize).map(|i| format!("{i:03}. cong A B C D [{:03}]", i.saturating_sub(1))).collect();
        sol.proof = Some(proof.join("\n"));
        let pdf = report_pdf(&sol, Some("long")).expect("a long proof must still export to PDF");
        let text = String::from_utf8_lossy(&pdf);
        let boxes = text.matches("/MediaBox").count();
        assert!(boxes > 1, "paginated, not one tall page");
        assert_eq!(text.matches("/MediaBox [0 0 595 842]").count(), boxes, "every page is A4");
    }

    #[test]
    fn report_copy_matches_the_app() {
        let refuted = serde_json::json!({"status": "refuted", "method": "ddar", "view": {"note": {"key": "false"}}});
        assert_eq!(verdict_copy(&refuted, Lang::En).1, "A sampled figure contradicts it, so no proof can exist.");
        assert_eq!(method_text(&refuted, Lang::En).as_deref(), Some("Numerical check"), "no counterexample to show");
        let shown = serde_json::json!({"status": "refuted", "method": "ddar", "view": {"counterexample": {"kind": "ratios", "lhs": 1.0, "rhs": 2.0, "labels": ["AM : MB", "AC : CB"]}}});
        assert_eq!(method_text(&shown, Lang::En).as_deref(), Some("Numerical counterexample"));
        assert_eq!(counter_text(&shown["view"]["counterexample"], Lang::En).as_deref(), Some("In the sampled figure AM : MB = 1.0000 but AC : CB = 2.0000."));
        let numeric = serde_json::json!({"status": "holds-numerically", "numeric_samples": 48, "view": {"note": {"key": "no_euclid"}}});
        assert_eq!(
            verdict_copy(&numeric, Lang::Ro).1,
            "Adevărat în toate cele 48 de figuri eșantionate, dar GeoSolver nu a găsit o demonstrație euclidiană. Este un indiciu numeric, nu o demonstrație."
        );
        let budget = serde_json::json!({"status": "not-proved", "method": "aux-search", "view": {"note": {"key": "budget", "runs": 5000}}});
        assert!(verdict_copy(&budget, Lang::Ro).1.contains("(5.000 de rulări deductive)"), "{:?}", verdict_copy(&budget, Lang::Ro));
        let drawn = serde_json::json!({"status": "proved", "method": "euclidean", "view": {"as_drawn": true, "note": {"key": "euclid"}}});
        assert_eq!(verdict_copy(&drawn, Lang::En).0, "Proved for the configuration shown");
    }

    fn label_sizes_on_page(v: &Value) -> Vec<f32> {
        let page = &report_pages(v, Lang::En, true)[0];
        let svg_scale: Vec<(f32, f32)> = page
            .lines()
            .filter(|l| l.starts_with("<svg x="))
            .filter_map(|l| {
                let w: f32 = root_attr(l, "width")?.parse().ok()?;
                let vb = root_attr(l, "viewBox")?;
                let vw: f32 = vb.split_whitespace().nth(2)?.parse().ok()?;
                Some((w / vw, 0.0))
            })
            .collect();
        let scale = svg_scale.first().map(|s| s.0).unwrap_or(1.0);
        page.lines()
            .filter(|l| l.contains("class=\"f-lbl"))
            .filter_map(|l| {
                let at = l.find(" font-size=\"")? + 12;
                let fs: f32 = l[at..].split('"').next()?.parse().ok()?;
                Some(fs * scale)
            })
            .collect()
    }

    #[test]
    fn report_labels_are_a_fixed_size_on_the_page() {
        let small = present::solution_json(&solve("A B C = triangle\nM = midpoint(A, B)\nprove perp(C, M, A, B)", &SolveOptions::default()).unwrap(), None);
        let euler = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../alphageometry-rs/examples/named/euler_formula_oi.geo")).unwrap();
        let large = present::solution_json(&solve(&euler, &SolveOptions::default()).unwrap(), None);
        for v in [&small, &large] {
            let sizes = label_sizes_on_page(v);
            assert!(!sizes.is_empty());
            assert!(sizes.iter().all(|s| (9.5..=11.5).contains(s)), "{sizes:?}");
        }
    }

    #[test]
    fn equations_are_not_broken_inside() {
        let lines = wrap("D lies on BC between B and C, so BD : DC = 1 : 2 and AD\u{b2} = 2\u{2044}3 \u{b7} AB\u{b2} + 1\u{2044}3 \u{b7} AC\u{b2}", 40);
        assert!(lines.iter().any(|l| l.contains("BD : DC = 1 : 2")), "{lines:?}");
        assert!(lines.iter().all(|l| !l.ends_with('=') && !l.ends_with('\u{b7}') && !l.ends_with('+')), "{lines:?}");
    }

    #[test]
    fn a_short_tail_stays_on_the_previous_page() {
        let mut sol = solve("A B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))", &SolveOptions::default()).unwrap();
        for n in 20..90usize {
            let proof: Vec<String> = (1..=n).map(|i| format!("{i:03}. cong A B C D [{:03}]", i.saturating_sub(1))).collect();
            sol.proof = Some(proof.join("\n"));
            let pages = report_pages(&present::solution_json(&sol, Some("t")), Lang::En, true);
            let last = pages.last().unwrap();
            let blocks = last.matches("<g transform=").count();
            assert!(pages.len() == 1 || blocks > 2, "{n} steps: the last page holds only {blocks} blocks");
        }
    }

    #[test]
    fn long_words_are_broken_to_fit() {
        let lines = wrap("Supercalifragilisticexpialidocious_reflection_of_the_orthocenter", 20);
        assert!(lines.iter().all(|l| l.chars().count() <= 20), "{lines:?}");
        assert_eq!(lines.concat(), "Supercalifragilisticexpialidocious_reflection_of_the_orthocenter");
    }

    #[test]
    fn absurd_svgs_are_refused() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"90000000\"/>";
        assert!(pdf_from_pages(&[svg.to_string()]).is_err());
        assert!(svg_to_png(svg, 1.0).is_err());
    }

    #[test]
    fn a_short_proof_fits_one_page() {
        let src = "# The reflection of the orthocenter in a side lies on the circumcircle.\nA B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))";
        let v = present::solution_json(&solve(src, &SolveOptions::default()).unwrap(), None);
        for lang in [Lang::En, Lang::Ro] {
            assert_eq!(report_pages(&v, lang, true).len(), 1);
        }
    }
}
