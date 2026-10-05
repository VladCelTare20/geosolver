//! Home-screen support: the web app manifest, the icons and launch images
//! rendered from the brand mark, and `robots.txt`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use axum::{
    extract::Path,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use resvg::{tiny_skia, usvg};

pub const LIGHT_BG: &str = "#f7f7f5";
pub const DARK_BG: &str = "#121417";
pub const ACCENT: &str = "#1f4fa0";
const DARK_ACCENT: &str = "#8db2e8";
const ICON_CACHE: &str = "public, max-age=604800";

pub const MANIFEST: &str = r##"{
  "id": "/",
  "name": "GeoSolver",
  "short_name": "GeoSolver",
  "description": "Draw the figure of a geometry problem and get a machine-checked proof.",
  "lang": "en",
  "dir": "ltr",
  "start_url": "/",
  "scope": "/",
  "display": "standalone",
  "orientation": "any",
  "background_color": "#f7f7f5",
  "theme_color": "#f7f7f5",
  "categories": ["education", "productivity"],
  "icons": [
    { "src": "/icons/icon-192.png", "sizes": "192x192", "type": "image/png", "purpose": "any" },
    { "src": "/icons/icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "any" },
    { "src": "/icons/icon-maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable" },
    { "src": "/favicon.svg", "sizes": "any", "type": "image/svg+xml" }
  ]
}
"##;

pub const FAVICON_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\"><circle cx=\"16\" cy=\"16\" r=\"13\" fill=\"none\" stroke=\"#1f4fa0\" stroke-width=\"2.2\"/><path d=\"M16 4.6 26.2 22H5.8Z\" fill=\"none\" stroke=\"#1f4fa0\" stroke-width=\"2.2\" stroke-linejoin=\"round\"/></svg>\n";

const ROBOTS: &str = "User-agent: *\nDisallow: /\n";

/// iPhone portrait screens in CSS pixels and their device pixel ratio, one
/// launch image (light and dark) each.
pub const SPLASH_SCREENS: &[(u32, u32, u32)] = &[
    (375, 667, 2),
    (375, 812, 3),
    (390, 844, 3),
    (393, 852, 3),
    (402, 874, 3),
    (428, 926, 3),
    (430, 932, 3),
    (440, 956, 3),
];

/// The mark (circle and inscribed triangle, a 32-unit square) centred at
/// (`cx`, `cy`) with outer diameter `size` pixels.
fn mark(cx: f32, cy: f32, size: f32, color: &str, stroke: f32) -> String {
    let s = size / (26.0 + stroke);
    format!(
        "<g transform=\"translate({cx} {cy}) scale({s}) translate(-16 -16)\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{stroke}\" stroke-linejoin=\"round\"><circle cx=\"16\" cy=\"16\" r=\"13\"/><path d=\"M16 4.6 26.2 22H5.8Z\"/></g>"
    )
}

fn icon_svg(w: u32, h: u32, bg: Option<&str>, mark_size: f32, color: &str, stroke: f32) -> String {
    let bg = bg.map_or(String::new(), |c| format!("<rect width=\"{w}\" height=\"{h}\" fill=\"{c}\"/>"));
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">{bg}{}</svg>",
        mark(w as f32 / 2.0, h as f32 / 2.0, mark_size, color, stroke)
    )
}

fn rasterize(svg: &str) -> Option<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    Some(pixmap)
}

/// An opaque picture as an RGB PNG (colour type 2: no alpha channel, so iOS
/// has no transparent pixels to fill with black).
fn opaque_png(svg: &str) -> Option<Vec<u8>> {
    let pixmap = rasterize(svg)?;
    let rgb: Vec<u8> = pixmap.data().chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, pixmap.width(), pixmap.height());
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Best);
        let mut w = enc.write_header().ok()?;
        w.write_image_data(&rgb).ok()?;
    }
    Some(out)
}

/// The same as an ICO file holding one PNG (supported since Windows Vista
/// and by every current browser).
fn ico_of_png(png: &[u8], size: u8) -> Vec<u8> {
    let mut out = vec![0, 0, 1, 0, 1, 0, size, size, 0, 0, 1, 0, 32, 0];
    out.extend_from_slice(&(png.len() as u32).to_le_bytes());
    out.extend_from_slice(&22u32.to_le_bytes());
    out.extend_from_slice(png);
    out
}

/// The icon at `name` (`apple-touch-icon.png`, `icon-192.png`, …), rendered
/// on first request and kept for the life of the process.
pub fn icon(name: &str) -> Option<&'static [u8]> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static [u8]>>> = OnceLock::new();
    let (key, build): (&'static str, fn() -> Option<Vec<u8>>) = match name {
        "apple-touch-icon.png" => ("apple-touch-icon.png", || {
            opaque_png(&icon_svg(180, 180, Some(LIGHT_BG), 180.0 * 0.68, ACCENT, 2.4))
        }),
        "icon-192.png" => ("icon-192.png", || {
            opaque_png(&icon_svg(192, 192, Some(LIGHT_BG), 192.0 * 0.68, ACCENT, 2.4))
        }),
        "icon-512.png" => ("icon-512.png", || {
            opaque_png(&icon_svg(512, 512, Some(LIGHT_BG), 512.0 * 0.68, ACCENT, 2.2))
        }),
        "icon-maskable-512.png" => ("icon-maskable-512.png", || {
            opaque_png(&icon_svg(512, 512, Some(ACCENT), 300.0, "#ffffff", 2.2))
        }),
        "favicon.ico" => ("favicon.ico", || {
            let png = rasterize(&icon_svg(32, 32, None, 31.0, ACCENT, 2.6))?.encode_png().ok()?;
            Some(ico_of_png(&png, 32))
        }),
        _ => return None,
    };
    let mut m = CACHE.get_or_init(Default::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(b) = m.get(key) {
        return Some(b);
    }
    let bytes: &'static [u8] = Box::leak(build()?.into_boxed_slice());
    m.insert(key, bytes);
    Some(bytes)
}

/// `750x1334-light.png` → (750, 1334, dark?, ratio), for the listed screens only.
fn splash_spec(name: &str) -> Option<(u32, u32, bool, u32)> {
    let (dims, theme) = name.strip_suffix(".png")?.split_once('-')?;
    let dark = match theme {
        "light" => false,
        "dark" => true,
        _ => return None,
    };
    let (w, h) = dims.split_once('x')?;
    let (w, h): (u32, u32) = (w.parse().ok()?, h.parse().ok()?);
    let &(_, _, r) = SPLASH_SCREENS.iter().find(|&&(cw, ch, r)| cw * r == w && ch * r == h)?;
    Some((w, h, dark, r))
}

/// A launch image: the page background with the mark centred, 120 pt wide.
pub fn splash(name: &str) -> Option<&'static [u8]> {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static [u8]>>> = OnceLock::new();
    let (w, h, dark, r) = splash_spec(name)?;
    let mut m = CACHE.get_or_init(Default::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(b) = m.get(name) {
        return Some(b);
    }
    let (bg, fg) = if dark { (DARK_BG, DARK_ACCENT) } else { (LIGHT_BG, ACCENT) };
    let png = opaque_png(&icon_svg(w, h, Some(bg), 120.0 * r as f32, fg, 2.0))?;
    let bytes: &'static [u8] = Box::leak(png.into_boxed_slice());
    m.insert(name.to_string(), bytes);
    Some(bytes)
}

/// The `apple-touch-startup-image` links for [`SPLASH_SCREENS`], one per
/// screen and colour scheme.
pub fn splash_links() -> &'static str {
    static LINKS: OnceLock<String> = OnceLock::new();
    LINKS.get_or_init(|| {
        let mut out = Vec::new();
        for &(w, h, r) in SPLASH_SCREENS {
            for scheme in ["light", "dark"] {
                out.push(format!(
                    "<link rel=\"apple-touch-startup-image\" media=\"(device-width: {w}px) and (device-height: {h}px) and (-webkit-device-pixel-ratio: {r}) and (orientation: portrait) and (prefers-color-scheme: {scheme})\" href=\"/splash/{}x{}-{scheme}.png\">",
                    w * r,
                    h * r
                ));
            }
        }
        out.join("\n")
    })
}

/// `html` with its `<!--pwa-splash-->` marker replaced by [`splash_links`].
pub fn with_splash(html: &str) -> String {
    html.replace("<!--pwa-splash-->", splash_links())
}

fn bytes(ctype: &'static str, cache: &'static str, body: &'static [u8]) -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(ctype)),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
        ],
        body,
    )
        .into_response()
}

pub async fn manifest() -> Response {
    bytes("application/manifest+json", "public, max-age=86400", MANIFEST.as_bytes())
}

pub async fn favicon_svg() -> Response {
    bytes("image/svg+xml", ICON_CACHE, FAVICON_SVG.as_bytes())
}

pub async fn favicon_ico() -> Response {
    match icon("favicon.ico") {
        Some(b) => bytes("image/x-icon", ICON_CACHE, b),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn apple_touch_icon() -> Response {
    match icon("apple-touch-icon.png") {
        Some(b) => bytes("image/png", ICON_CACHE, b),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn icon_file(Path(file): Path<String>) -> Response {
    let known = matches!(file.as_str(), "icon-192.png" | "icon-512.png" | "icon-maskable-512.png");
    match icon(&file).filter(|_| known) {
        Some(b) => bytes("image/png", ICON_CACHE, b),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn splash_file(Path(file): Path<String>) -> Response {
    match splash(&file) {
        Some(b) => bytes("image/png", ICON_CACHE, b),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn robots() -> Response {
    bytes("text/plain; charset=utf-8", "public, max-age=86400", ROBOTS.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (width, height, colour type) from a PNG's IHDR.
    pub fn ihdr(png: &[u8]) -> (u32, u32, u8) {
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        let w = u32::from_be_bytes(png[16..20].try_into().unwrap());
        let h = u32::from_be_bytes(png[20..24].try_into().unwrap());
        (w, h, png[25])
    }

    #[test]
    fn icons_match_their_declared_sizes_and_are_opaque() {
        for (name, size) in [
            ("apple-touch-icon.png", 180),
            ("icon-192.png", 192),
            ("icon-512.png", 512),
            ("icon-maskable-512.png", 512),
        ] {
            let png = icon(name).unwrap();
            assert_eq!(ihdr(png), (size, size, 2), "{name}: RGB, no alpha");
        }
        let ico = icon("favicon.ico").unwrap();
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        assert_eq!(ihdr(&ico[22..]).0, 32);
        assert!(icon("nope.png").is_none());
    }

    #[test]
    fn the_mark_is_drawn_with_a_visible_stroke() {
        let p = rasterize(&icon_svg(180, 180, Some(LIGHT_BG), 180.0 * 0.68, ACCENT, 2.4)).unwrap();
        let px = |x: u32, y: u32| {
            let c = p.pixel(x, y).unwrap();
            (c.red(), c.green(), c.blue())
        };
        assert_eq!(px(2, 2), (0xf7, 0xf7, 0xf5), "corner is the light background");
        let accent = (0x1f, 0x4f, 0xa0);
        let ring: Vec<u32> = (0..90).filter(|&x| px(x, 90) == accent).collect();
        assert!(ring.len() >= 6, "the circle's stroke is at least 6 px wide: {ring:?}");
        let outer = 90 - ring[0];
        assert!((58..=64).contains(&outer), "mark ≈ 68 % of the width: radius {outer}");
        let m = rasterize(&icon_svg(512, 512, Some(ACCENT), 300.0, "#ffffff", 2.2)).unwrap();
        let white: Vec<u32> = (0..256).filter(|&x| m.pixel(x, 256).unwrap().red() == 255).collect();
        assert!(256 - white[0] <= 204, "maskable mark inside the 80 % safe circle");
    }

    #[test]
    fn manifest_is_valid_and_lists_the_png_icons() {
        let m: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(m["display"], "standalone");
        assert_eq!(m["start_url"], "/");
        assert_eq!(m["name"], "GeoSolver");
        let pngs: Vec<&serde_json::Value> =
            m["icons"].as_array().unwrap().iter().filter(|i| i["type"] == "image/png").collect();
        assert_eq!(pngs.len(), 3);
        for i in pngs {
            let file = i["src"].as_str().unwrap().strip_prefix("/icons/").unwrap();
            let (w, h, _) = ihdr(icon(file).unwrap());
            assert_eq!(i["sizes"], format!("{w}x{h}"));
        }
    }

    #[test]
    fn splash_images_exist_for_every_link() {
        let links = splash_links();
        assert_eq!(links.matches("apple-touch-startup-image").count(), 16);
        let (w, h, dark, r) = splash_spec("750x1334-dark.png").unwrap();
        assert_eq!((w, h, dark, r), (750, 1334, true, 2));
        assert!(splash_spec("751x1334-dark.png").is_none());
        assert!(splash_spec("750x1334-sepia.png").is_none());
        assert!(splash_spec("../x.png").is_none());
        let png = splash("1170x2532-light.png").unwrap();
        assert_eq!(ihdr(png), (1170, 2532, 2));
        for href in links.split("href=\"/splash/").skip(1) {
            let file = href.split('"').next().unwrap();
            assert!(splash_spec(file).is_some(), "{file}");
        }
    }
}
