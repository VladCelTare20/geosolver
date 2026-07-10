//! Rasterise SVG figures to PNG and export them (and a combined proof report)
//! to PDF — a pure-Rust pipeline (resvg + svg2pdf), no browser or external
//! tools, so it runs headlessly on a server.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use resvg::tiny_skia;
use resvg::usvg;

use crate::engine::Solution;

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

/// Render a parsed tree to PNG bytes at the given scale (1.0 = native size;
/// 2.0 = retina/high-DPI).
pub fn png_bytes(tree: &usvg::Tree, scale: f32) -> Result<Vec<u8>> {
    let size = tree.size();
    let w = ((size.width() * scale).ceil() as u32).max(1);
    let h = ((size.height() * scale).ceil() as u32).max(1);
    // Guard against a pathological allocation (each pixel is 4 bytes).
    if (w as u64) * (h as u64) > 40_000_000 {
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

/// Export a parsed tree to a single-page PDF.
pub fn pdf_bytes(tree: &usvg::Tree) -> Result<Vec<u8>> {
    svg2pdf::to_pdf(
        tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
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
// Combined "report" page: figure + numbered proof, laid out as one SVG that can
// be exported to PDF or PNG — the deliverable a human keeps.
// ---------------------------------------------------------------------------

const PAGE_W: f32 = 820.0;
const MARGIN: f32 = 40.0;
// Fallback figure dimensions when the embedded SVG's own aren't parseable.
const FIG_NATIVE_W: f32 = 1020.0;
const FIG_NATIVE_H: f32 = 640.0;
const PROOF_FONT: f32 = 14.0;
const PROOF_LEADING: f32 = 20.0;
const WRAP_COLS: usize = 92;

struct Palette {
    page_bg: &'static str,
    ink: &'static str,
    subtle: &'static str,
    accent: &'static str,
    rule: &'static str,
}

fn palette(light: bool) -> Palette {
    if light {
        Palette {
            page_bg: "#ffffff",
            ink: "#12161c",
            subtle: "#5c6b7a",
            accent: "#1a7f37",
            rule: "#d7dee6",
        }
    } else {
        Palette {
            page_bg: "#0b0e13",
            ink: "#e6edf3",
            subtle: "#8b98a5",
            accent: "#3fb950",
            rule: "#222a35",
        }
    }
}

/// Map the few math glyphs the bundled monospace font lacks (`⟂`, `∥`) to
/// equivalents it carries (`⊥`, `‖`), so the whole proof renders in one font
/// instead of falling back to a proportional face mid-line.
fn normalize_glyphs(s: &str) -> String {
    s.replace('⟂', "⊥").replace('∥', "‖")
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Word-wrap a proof line to `WRAP_COLS` characters, indenting continuation
/// lines so they sit under the step text.
fn wrap_line(line: &str, cols: usize) -> Vec<String> {
    if line.chars().count() <= cols {
        return vec![line.to_string()];
    }
    let indent = "     "; // aligns under "001. "
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in line.split(' ') {
        let prospective = if cur.is_empty() {
            word.chars().count()
        } else {
            cur.chars().count() + 1 + word.chars().count()
        };
        if !cur.is_empty() && prospective > cols {
            out.push(std::mem::take(&mut cur));
            cur.push_str(indent);
        }
        if !cur.is_empty() && !cur.ends_with(' ') {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Build a standalone report page (figure + proof) as an SVG document.
///
/// `light` controls the *page* colours; the embedded figure keeps whatever
/// theme it was rendered with (pass a light figure for a print-friendly PDF).
pub fn report_svg(sol: &Solution, title: Option<&str>, light: bool) -> String {
    let pal = palette(light);

    // Layout geometry. The figure canvas varies (its legend panel grows with
    // the hypothesis count), so read its real dimensions — a fixed viewBox
    // would crop tall figures.
    let (fig_native_w, fig_native_h) =
        svg_dimensions(&sol.svg).unwrap_or((FIG_NATIVE_W, FIG_NATIVE_H));
    let fig_w = PAGE_W - 2.0 * MARGIN;
    let fig_h = fig_w * (fig_native_h / fig_native_w);
    let header_h = 84.0;
    let fig_y = header_h;

    // Assemble the proof body lines (status note, aux points, then the proof).
    let mut body: Vec<String> = Vec::new();
    if !sol.aux_constructions.is_empty() {
        body.push("Auxiliary constructions:".to_string());
        for c in &sol.aux_constructions {
            body.push(format!("  + {c}"));
        }
        body.push(String::new());
    }
    match &sol.proof {
        Some(p) => {
            for raw in p.lines() {
                let raw = normalize_glyphs(raw);
                for w in wrap_line(&raw, WRAP_COLS) {
                    body.push(w);
                }
            }
        }
        None => {
            if !sol.proved {
                body.push("The engine did not find a proof within its search budget.".to_string());
            }
        }
    }

    let proof_top = fig_y + fig_h + 34.0;
    let proof_h = body.len() as f32 * PROOF_LEADING;
    let page_h = proof_top + proof_h + MARGIN + 22.0;

    // Embed the figure SVG as a nested, scaled <svg> (strip its own root tag so
    // the inner geometry inherits our coordinate box via viewBox).
    let inner_fig = strip_svg_root(&sol.svg);

    let status = if sol.proved { "Proven" } else { "Not proven" };
    let title_text = escape_xml(title.unwrap_or("GeoSolver proof"));

    let mut s = String::new();
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{PAGE_W:.0}\" height=\"{page_h:.0}\" \
         viewBox=\"0 0 {PAGE_W:.0} {page_h:.0}\" font-family=\"Helvetica, Arial, sans-serif\">\n"
    ));
    s.push_str(&format!(
        "<rect width=\"{PAGE_W:.0}\" height=\"{page_h:.0}\" fill=\"{}\"/>\n",
        pal.page_bg
    ));

    // Header.
    s.push_str(&format!(
        "<text x=\"{MARGIN:.0}\" y=\"46\" font-size=\"26\" font-weight=\"700\" fill=\"{}\">{}</text>\n",
        pal.ink, title_text
    ));
    s.push_str(&format!(
        "<text x=\"{MARGIN:.0}\" y=\"70\" font-size=\"14\" fill=\"{}\">\
         <tspan fill=\"{}\" font-weight=\"700\">{}</tspan> · {} · {:.3}s</text>\n",
        pal.subtle,
        pal.accent,
        status,
        escape_xml(&sol.note),
        sol.elapsed_secs
    ));

    // Figure (nested, scaled to the content box; bordered card).
    s.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1\" rx=\"8\"/>\n",
        MARGIN, fig_y, fig_w, fig_h, pal.rule
    ));
    s.push_str(&format!(
        "<svg x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" viewBox=\"0 0 {:.0} {:.0}\" preserveAspectRatio=\"xMidYMid meet\">\n{}\n</svg>\n",
        MARGIN, fig_y, fig_w, fig_h, fig_native_w, fig_native_h, inner_fig
    ));

    // Divider above the proof.
    s.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
        MARGIN,
        proof_top - 18.0,
        PAGE_W - MARGIN,
        proof_top - 18.0,
        pal.rule
    ));

    // Proof body (monospace for aligned numbering; usvg falls back per glyph).
    let mut y = proof_top;
    for line in &body {
        if !line.is_empty() {
            s.push_str(&format!(
                "<text x=\"{MARGIN:.0}\" y=\"{y:.1}\" font-size=\"{PROOF_FONT}\" \
                 font-family=\"'DejaVu Sans Mono', Consolas, monospace\" fill=\"{}\" xml:space=\"preserve\">{}</text>\n",
                pal.ink,
                escape_xml(line)
            ));
        }
        y += PROOF_LEADING;
    }

    // Footer.
    s.push_str(&format!(
        "<text x=\"{MARGIN:.0}\" y=\"{:.1}\" font-size=\"11\" fill=\"{}\">Generated by GeoSolver · {} points · low-level: {}</text>\n",
        page_h - 18.0,
        pal.subtle,
        count_points(&sol.low_level),
        // At 11px the footer fits ~110 chars on the 820px page; the fixed
        // prefix uses ~60 of them.
        escape_xml(&truncate(&sol.low_level, 48))
    ));

    s.push_str("</svg>\n");
    s
}

/// Read the `width`/`height` attributes off an SVG document's root tag.
fn svg_dimensions(svg: &str) -> Option<(f32, f32)> {
    let open = svg.find("<svg")?;
    let end = svg[open..].find('>')? + open;
    let tag = &svg[open..end];
    let attr = |name: &str| -> Option<f32> {
        let key = format!("{name}=\"");
        let at = tag.find(&key)? + key.len();
        let rest = &tag[at..];
        let close = rest.find('"')?;
        rest[..close].trim().parse().ok()
    };
    let (w, h) = (attr("width")?, attr("height")?);
    (w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()).then_some((w, h))
}

/// Strip the outer `<svg ...>` … `</svg>` wrapper, returning just the inner
/// markup (which we re-wrap in a nested, scaled <svg>).
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

fn count_points(low_level: &str) -> usize {
    low_level.split('@').count().saturating_sub(1)
}

fn truncate(s: &str, n: usize) -> String {
    let one_line = s.replace(['\n', '\r'], " ");
    if one_line.chars().count() <= n {
        one_line
    } else {
        let cut: String = one_line.chars().take(n).collect();
        format!("{cut}…")
    }
}
