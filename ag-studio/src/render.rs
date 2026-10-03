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
/// Largest page edge, in points, that common PDF viewers accept (200 in).
const MAX_PDF_PAGE_PT: f32 = 14_400.0;
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

/// Export a parsed tree to a single-page PDF. A very tall page (a long proof)
/// is scaled down, via the DPI, so neither edge exceeds what viewers accept.
pub fn pdf_bytes(tree: &usvg::Tree) -> Result<Vec<u8>> {
    check_svg_size(tree)?;
    let size = tree.size();
    let longest = size.width().max(size.height());
    let mut page = svg2pdf::PageOptions::default();
    if longest > MAX_PDF_PAGE_PT {
        page.dpi = 72.0 * longest / MAX_PDF_PAGE_PT * 1.001;
    }
    svg2pdf::to_pdf(tree, svg2pdf::ConversionOptions::default(), page)
    .map_err(|e| anyhow!("PDF conversion error: {e}"))
}

/// Convenience: SVG string → PNG bytes.
pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>> {
    png_bytes(&parse_svg(svg)?, scale)
}

/// Convenience: SVG string → PDF bytes.
pub fn svg_to_pdf(svg: &str) -> Result<Vec<u8>> {
    pdf_bytes(&parse_svg(svg)?)
}

// ---------------------------------------------------------------------------
// Report page: verdict, figure, given/prove, and the machine-checked steps.
// ---------------------------------------------------------------------------

const PAGE_W: f32 = 820.0;
const MARGIN: f32 = 48.0;
const BODY_FS: f32 = 13.5;
const LEADING: f32 = 20.0;
const FIG_MAX_H: f32 = 520.0;

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

/// Greedy word wrap to at most `cols` characters per line.
fn wrap(text: &str, cols: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
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
        "cong" | "length" | "eqangle" | "coincide" => format!("{} = {}", g(0), g(1)),
        "perp" => format!("{} \u{27c2} {}", g(0), g(1)),
        "para" => format!("{} \u{2225} {}", g(0), g(1)),
        "eqratio" => format!("{} : {} = {} : {}", g(0), g(1), g(2), g(3)),
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
pub fn aux_text(a: &Value, lang: Lang) -> String {
    let raw = a["text"].as_str().unwrap_or("").to_string();
    let kind = a["kind"].as_str().unwrap_or("");
    let key = format!("aux.{kind}");
    let template = i18n::t(lang, &key);
    if template == i18n::t(lang, "aux.__none__") {
        return raw;
    }
    let circle_words = |x: &str| -> String {
        for (f, k) in [("circumcircle(", "aux.circumcircle"), ("circle(", "aux.circle")] {
            if let Some(inner) = x.strip_prefix(f).and_then(|r| r.strip_suffix(')')) {
                let mut s = i18n::t(lang, k).to_string();
                for (i, p) in inner.split(',').map(str::trim).enumerate() {
                    s = s.replace(&format!("{{{i}}}"), p);
                }
                return s;
            }
        }
        x.to_string()
    };
    let mut args: Vec<String> = a["args"]
        .as_array()
        .map(|v| v.iter().filter_map(|x| x.as_str()).map(circle_words).collect())
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
        ("theorem", Some(name)) => name.to_string(),
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
    let digits = if kind == "values" || kind == "length" { 4 } else { 1 };
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
/// plus `view` and `title`), on a light, print-friendly page.
pub fn report_from_json(v: &Value, lang: Lang) -> String {
    let status = v["status"].as_str().unwrap_or("not-proved");
    let view = &v["view"];
    let time_limited = view["note"]["key"].as_str() == Some("time_limit");
    let t = tone(status, time_limited);
    let verdict_key = if status != "proved" && time_limited {
        "time_limit".to_string()
    } else {
        status.to_string()
    };
    let headline = i18n::t(lang, &format!("report.verdict.{verdict_key}")).to_string();
    let explain = i18n::tf(
        lang,
        &format!("report.explain.{verdict_key}"),
        &[
            ("n", v["numeric_samples"].as_u64().unwrap_or(0).to_string()),
            ("secs", fmt_num(view["note"]["secs"].as_f64().unwrap_or(60.0), lang, 0)),
        ],
    );
    let title = v["title"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| i18n::t(lang, "report.default_title").to_string());
    let content_w = PAGE_W - 2.0 * MARGIN;
    let cols = (content_w / (BODY_FS * 0.56)) as usize;

    let mut body = String::new();
    let mut y = MARGIN + 6.0;
    let text = |s: &mut String, x: f32, y: f32, size: f32, weight: u32, fill: &str, family: &str, content: &str| {
        let _ = std::fmt::Write::write_fmt(
            s,
            format_args!(
                "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"{size}\" font-weight=\"{weight}\" fill=\"{fill}\" font-family=\"{family}\" xml:space=\"preserve\">{}</text>\n",
                escape_xml(&normalize_glyphs(content))
            ),
        );
    };
    const SANS: &str = "'DejaVu Sans', Helvetica, Arial, sans-serif";
    const INK: &str = "#16191d";
    const MUTED: &str = "#4b525a";
    const RULE: &str = "#dadcd8";

    for (i, line) in wrap(&title, 46).iter().enumerate() {
        y += if i == 0 { 22.0 } else { 30.0 };
        text(&mut body, MARGIN, y, 25.0, 700, INK, SANS, line);
    }
    y += 18.0;
    let explain_lines = wrap(&explain, cols.saturating_sub(8));
    let card_h = 46.0 + explain_lines.len() as f32 * 19.0;
    let _ = std::fmt::Write::write_fmt(
        &mut body,
        format_args!(
            "<rect x=\"{MARGIN}\" y=\"{y:.1}\" width=\"{content_w}\" height=\"{card_h:.1}\" rx=\"10\" fill=\"{}\" stroke=\"{}\" stroke-opacity=\"0.35\"/>\n",
            t.bg, t.ink
        ),
    );
    body.push_str(&verdict_icon(status, time_limited, MARGIN + 16.0, y + 13.0, t.ink));
    text(&mut body, MARGIN + 46.0, y + 29.0, 17.0, 700, t.ink, SANS, &headline);
    for (i, l) in explain_lines.iter().enumerate() {
        text(&mut body, MARGIN + 46.0, y + 50.0 + i as f32 * 19.0, BODY_FS, 400, INK, SANS, l);
    }
    y += card_h + 22.0;

    let mut meta: Vec<String> = vec![fmt_secs(v["elapsed_secs"].as_f64().unwrap_or(0.0), lang)];
    let shown_steps = view["proof"]["steps"].as_array().map_or(0, Vec::len);
    if status == "proved" && shown_steps > 0 {
        meta.push(i18n::tf(lang, "report.steps", &[("n", shown_steps.to_string())]));
    }
    text(&mut body, MARGIN, y, 12.0, 400, MUTED, SANS, &meta.join("  \u{b7}  "));
    y += 16.0;

    let svg = v["svg"].as_str().unwrap_or("");
    if let Some((fw, fh)) = svg_dimensions(svg).or_else(|| viewbox_size(svg)) {
        let scale = (content_w / fw).min(FIG_MAX_H / fh);
        let (w, h) = (fw * scale, fh * scale);
        let x = MARGIN + (content_w - w) / 2.0;
        let vb = viewbox_attr(svg).unwrap_or_else(|| format!("0 0 {fw} {fh}"));
        let _ = std::fmt::Write::write_fmt(
            &mut body,
            format_args!(
                "<rect x=\"{MARGIN}\" y=\"{y:.1}\" width=\"{content_w}\" height=\"{:.1}\" rx=\"10\" fill=\"#ffffff\" stroke=\"{RULE}\"/>\n<svg x=\"{x:.1}\" y=\"{:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" viewBox=\"{vb}\" preserveAspectRatio=\"xMidYMid meet\" font-family=\"'DejaVu Serif', 'DejaVu Sans', serif\">\n{}\n</svg>\n",
                h + 24.0,
                y + 12.0,
                strip_svg_root(svg)
            ),
        );
        y += h + 24.0 + 28.0;
    } else {
        y += 12.0;
    }

    let section = |body: &mut String, y: &mut f32, label: &str| {
        *y += 8.0;
        text(body, MARGIN, *y, 11.5, 700, MUTED, SANS, &label.to_uppercase());
        let _ = std::fmt::Write::write_fmt(
            body,
            format_args!(
                "<line x1=\"{MARGIN}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{RULE}\"/>\n",
                *y + 7.0,
                PAGE_W - MARGIN,
                *y + 7.0
            ),
        );
        *y += 26.0;
    };

    let given: Vec<String> = view["given"]
        .as_array()
        .map(|a| a.iter().map(|f| fact_text(f, lang)).collect())
        .unwrap_or_default();
    if !given.is_empty() {
        section(&mut body, &mut y, i18n::t(lang, "report.given"));
        for g in &given {
            for (i, l) in wrap(g, cols - 4).iter().enumerate() {
                text(&mut body, MARGIN + if i == 0 { 0.0 } else { 14.0 }, y, BODY_FS, 400, INK, SANS, &if i == 0 { format!("\u{2022} {l}") } else { l.clone() });
                y += LEADING;
            }
        }
        y += 6.0;
    }
    if !view["goal"].is_null() {
        section(&mut body, &mut y, i18n::t(lang, "report.prove"));
        for l in wrap(&fact_text(&view["goal"], lang), cols) {
            text(&mut body, MARGIN, y, BODY_FS + 0.5, 700, INK, SANS, &l);
            y += LEADING;
        }
        y += 6.0;
    }
    if let Some(c) = view.get("counterexample").filter(|c| !c.is_null()).and_then(|c| counter_text(c, lang)) {
        section(&mut body, &mut y, i18n::t(lang, "report.counter"));
        for l in wrap(&c, cols) {
            text(&mut body, MARGIN, y, BODY_FS, 400, t.ink, SANS, &l);
            y += LEADING;
        }
        y += 6.0;
    }
    if let Some(aux) = view["aux"].as_array().filter(|a| !a.is_empty()) {
        section(&mut body, &mut y, i18n::t(lang, "report.aux"));
        for a in aux {
            let line = format!("{}: {}", a["name"].as_str().unwrap_or(""), aux_text(a, lang));
            for l in wrap(&line, cols) {
                text(&mut body, MARGIN, y, BODY_FS, 400, "#b45309", SANS, &l);
                y += LEADING;
            }
        }
        y += 6.0;
    }
    let steps = view["proof"]["steps"].as_array().cloned().unwrap_or_default();
    if status == "proved" && !steps.is_empty() {
        section(&mut body, &mut y, i18n::t(lang, "report.proof"));
        let gutter = 34.0;
        let step_cols = ((content_w - gutter) / (BODY_FS * 0.56)) as usize;
        for st in &steps {
            let mut line = fact_text(&st["fact"], lang);
            let rule = rule_text(st, lang);
            let deps: Vec<String> = st["deps"]
                .as_array()
                .map(|d| d.iter().filter_map(|x| x.as_u64().map(|n| n.to_string())).collect())
                .unwrap_or_default();
            line.push_str(&format!("   \u{2014} {rule}"));
            if !deps.is_empty() {
                line.push_str(&format!(" [{}]", deps.join(", ")));
            }
            let n = st["n"].as_u64().unwrap_or(0);
            text(&mut body, MARGIN, y, BODY_FS, 700, MUTED, SANS, &format!("{n}."));
            for l in wrap(&line, step_cols) {
                text(&mut body, MARGIN + gutter, y, BODY_FS, 400, INK, SANS, &l);
                y += LEADING;
            }
        }
        if let Some(c) = view["proof"].get("conclusion").filter(|c| !c.is_null()) {
            y += 4.0;
            for l in wrap(&format!("\u{220e}  {}", fact_text(c, lang)), cols) {
                text(&mut body, MARGIN, y, BODY_FS, 700, INK, SANS, &l);
                y += LEADING;
            }
        }
    }
    y += 18.0;
    let _ = std::fmt::Write::write_fmt(
        &mut body,
        format_args!(
            "<line x1=\"{MARGIN}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" stroke=\"{RULE}\"/>\n",
            PAGE_W - MARGIN
        ),
    );
    y += 22.0;
    text(&mut body, MARGIN, y, 11.0, 400, MUTED, SANS, i18n::t(lang, "report.footer"));
    let page_h = y + MARGIN - 12.0;

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{PAGE_W:.0}\" height=\"{page_h:.0}\" viewBox=\"0 0 {PAGE_W:.0} {page_h:.0}\" font-family=\"{SANS}\">\n<rect width=\"{PAGE_W:.0}\" height=\"{page_h:.0}\" fill=\"#ffffff\"/>\n{body}</svg>\n"
    )
}

/// The report for a [`Solution`] (CLI and MCP exports), in English.
pub fn report_svg(sol: &Solution, title: Option<&str>, _light: bool) -> String {
    report_from_json(&present::solution_json(sol, title), Lang::En)
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
    fn long_proof_pdf_pages_stay_within_viewer_limits() {
        let pdf = svg_to_pdf(&long_report(3000)).expect("pdf");
        let text = String::from_utf8_lossy(&pdf);
        let at = text.find("/MediaBox").expect("MediaBox");
        let nums: Vec<f32> = text[at + 9..]
            .trim_start_matches([' ', '['])
            .split(']')
            .next()
            .unwrap()
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        assert_eq!(nums.len(), 4, "{nums:?}");
        assert!(nums[3] - nums[1] <= MAX_PDF_PAGE_PT + 1.0, "{nums:?}");
    }

    #[test]
    fn absurd_svgs_are_refused() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"90000000\"/>";
        assert!(svg_to_pdf(svg).is_err());
        assert!(svg_to_png(svg, 1.0).is_err());
    }
}
