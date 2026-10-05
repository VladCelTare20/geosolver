const APP_CSS: &str = include_str!("../assets/app.css");
const SITE_JS: &str = include_str!("../assets/site.js");
const APP_JS: &str = include_str!("../assets/app.js");
const I18N_JS: &str = include_str!("../assets/i18n.js");
const LANDING_HTML: &str = include_str!("../assets/landing.html");

fn block_after<'a>(src: &'a str, marker: &str) -> &'a str {
    let at = src.find(marker).unwrap_or_else(|| panic!("{marker} not found"));
    let open = at + src[at..].find('{').expect("block opens");
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open + 1..open + i];
                }
            }
            _ => {}
        }
    }
    panic!("{marker} block never closes")
}

#[test]
fn print_shows_the_result_in_light_colours_without_controls() {
    let print = block_after(APP_CSS, "@media print");
    for hidden in [".site-header", ".fig-tools", ".verdict-actions", ".has-result .composer", ".site-footer", ".toast-region", ".kbd-hint"] {
        assert!(print.contains(hidden), "print must hide {hidden}");
    }
    let tokens = block_after(print, "html:root:not(#print)");
    for t in ["color-scheme: light", "--text: #000000", "--fig-ink: #1b1f24", "--fig-halo: #ffffff", "--fig-bg: #ffffff", "--bg: #ffffff"] {
        assert!(tokens.contains(t), "print must force {t} whatever the theme");
    }
    assert!(print.contains(".figure-panel") && print.contains("break-inside: avoid"), "the figure must not split across pages");
}

#[test]
fn the_figure_wheel_never_traps_page_scrolling() {
    let wheel = block_after(SITE_JS, "vp.addEventListener(\"wheel\"");
    let zoom = wheel.find("me.zoomAt").expect("wheel zooms somewhere");
    let before = &wheel[..zoom];
    assert!(before.contains("me.pinned()"), "an in-page figure must let a plain wheel scroll the page");
    assert!(before.contains("e.ctrlKey || e.metaKey"), "Ctrl/⌘ + wheel and trackpad pinches must zoom");
    assert!(before.contains("e.deltaMode"), "line/page wheel deltas must be scaled to pixels");
    assert!(wheel.contains("dy > 0 && !me.zoomed()"), "scrolling down over a figure at fit must scroll the page");
    assert!(SITE_JS.contains("\"gesturechange\""), "Safari trackpad pinches must zoom the figure, not the page");
    assert!(APP_JS.contains("fig.hint.wheel"), "the hint must say how to zoom an in-page figure");
}

#[test]
fn images_can_be_pasted_or_dropped_anywhere() {
    assert!(APP_JS.contains("document.addEventListener(\"paste\""));
    assert!(APP_JS.contains("document.addEventListener(\"drop\""), "a drop outside the drop zone must not navigate to the image");
    for key in ["\"fig.hint.wheel\"", "\"photo.drop.paste\""] {
        assert_eq!(I18N_JS.matches(key).count(), 2, "{key} needs EN and RO");
    }
}

#[test]
fn wide_screens_give_the_extra_width_to_the_figure() {
    let wide = block_after(APP_CSS, "@media (min-width: 1800px)");
    assert!(wide.contains("--maxw: 1760px"));
    assert!(wide.contains("minmax(0, 780px) minmax(0, 1fr)"), "the text column stays readable; the figure takes the rest");
    let mid = block_after(APP_CSS, "@media (min-width: 1024px) and (max-width: 1199px)");
    assert!(mid.contains("minmax(0, 1.1fr) minmax(0, 1fr)"), "iPad landscape keeps a usable figure column");
    assert!(LANDING_HTML.contains(".site-header .inner { max-width: 1120px; }"), "the landing header lines up with its content");
}

#[test]
fn contrast_and_forced_colours_keep_state_visible() {
    let more = block_after(APP_CSS, "@media (prefers-contrast: more)");
    assert!(more.contains("--border: var(--border-strong)"));
    let forced = block_after(APP_CSS, "@media (forced-colors: active)");
    assert!(forced.contains(".status-pill.on .dot"), "the AI pill dot must survive forced colours");
}
