//! iPhone and Android layout invariants of the shipped stylesheet and pages, checked on the
//! source text (the browser-level checks are `tests/webkit/iphone-layout.mjs`).

const APP_CSS: &str = include_str!("../assets/app.css");
const INDEX_HTML: &str = include_str!("../assets/index.html");
const AUTH_HTML: &str = include_str!("../assets/auth.html");
const LANDING_HTML: &str = include_str!("../assets/landing.html");
const GATE_HTML: &str = include_str!("../assets/gate.html");

struct Rule {
    selector: String,
    body: String,
    media: Vec<String>,
    at: usize,
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = match rest[i + 2..].find("*/") {
            Some(j) => &rest[i + 2 + j + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

fn rules(css: &str) -> Vec<Rule> {
    let css = strip_comments(css);
    let bytes = css.as_bytes();
    let mut out = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                let head = css[start..i].trim().to_string();
                if head.starts_with('@') {
                    stack.push(head);
                    start = i + 1;
                } else {
                    let end = css[i..].find('}').map(|j| i + j).unwrap_or(css.len());
                    out.push(Rule { selector: head, body: css[i + 1..end].to_string(), media: stack.clone(), at: i });
                    i = end;
                    start = end + 1;
                }
            }
            b'}' => {
                stack.pop();
                start = i + 1;
            }
            b';' if stack.last().is_none_or(|s| !s.starts_with("@media") && !s.starts_with("@supports")) => {
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn style_blocks(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(i) = rest.find("<style>") {
        let tail = &rest[i + 7..];
        let end = tail.find("</style>").unwrap_or(tail.len());
        out.push_str(&tail[..end]);
        out.push('\n');
        rest = &tail[end..];
    }
    out
}

fn decl<'a>(r: &'a Rule, prop: &str) -> Option<&'a str> {
    r.body.split(';').map(str::trim).find_map(|d| d.strip_prefix(prop).and_then(|v| v.trim_start().strip_prefix(':')).map(str::trim))
}

#[test]
fn every_hover_style_is_limited_to_hover_capable_pointers() {
    for (name, css) in [("app.css", APP_CSS.to_string()), ("auth.html", style_blocks(AUTH_HTML)), ("landing.html", style_blocks(LANDING_HTML))] {
        let all = rules(&css);
        assert!(!all.is_empty(), "{name}: no rules parsed");
        for r in all.iter().filter(|r| r.selector.contains(":hover")) {
            assert!(
                r.media.iter().any(|m| m.contains("hover: hover")),
                "{name}: `{}` is not inside @media (hover: hover), so it sticks after a tap on iPhone",
                r.selector
            );
        }
    }
}

#[test]
fn focused_fields_are_16px_on_touch_so_ios_does_not_zoom() {
    let all = rules(APP_CSS);
    let last_small = |sel: &str| {
        all.iter()
            .filter(|r| r.selector.split(',').any(|s| s.trim() == sel) && decl(r, "font-size").is_some_and(|v| v != "16px"))
            .map(|r| r.at)
            .max()
            .unwrap_or(0)
    };
    for sel in [".editor textarea", ".editor .hl", ".editor .gutter", ".rail-search .input", ".input", ".textarea"] {
        let fix = all
            .iter()
            .filter(|r| r.media.iter().any(|m| m.contains("pointer: coarse")))
            .filter(|r| r.selector.split(',').any(|s| s.trim() == sel))
            .find(|r| decl(r, "font-size") == Some("16px"))
            .unwrap_or_else(|| panic!("no 16px rule for `{sel}` under (pointer: coarse)"));
        assert!(fix.at > last_small(sel), "`{sel}`: the 16px touch rule comes before a smaller font-size rule and loses");
    }
    assert!(!INDEX_HTML.contains("maximum-scale"), "never disable user zoom");
}

#[test]
fn syntax_help_cannot_widen_the_page() {
    let all = rules(APP_CSS);
    let find = |sel: &str| all.iter().filter(|r| r.media.is_empty() && r.selector == sel).collect::<Vec<_>>();
    assert!(find(".panel").iter().any(|r| decl(r, "grid-template-columns") == Some("minmax(0, 1fr)") && decl(r, "min-width") == Some("0")));
    assert!(find(".syntax-help").iter().any(|r| decl(r, "min-width") == Some("0")));
    assert!(find(".syntax-help pre").iter().any(|r| decl(r, "overflow-x") == Some("auto")));
    assert!(find(".syntax-help p code").iter().any(|r| decl(r, "overflow-wrap") == Some("anywhere")));
}

#[test]
fn content_gutters_respect_left_and_right_safe_areas() {
    let all = rules(APP_CSS);
    let root = all.iter().find(|r| r.selector == ":root" && r.media.is_empty()).unwrap();
    for (var, env) in [("--sai-l", "safe-area-inset-left"), ("--sai-r", "safe-area-inset-right"), ("--sai-t", "safe-area-inset-top"), ("--sai-b", "safe-area-inset-bottom")] {
        assert!(decl(root, var).is_some_and(|v| v.contains(env)), "{var} must come from env({env})");
    }
    for sel in [".site-header .inner", ".shell", ".site-footer", ".fig-frame.is-full", ".rail", ".toast-region"] {
        let both = all
            .iter()
            .filter(|r| r.selector.split(',').any(|s| s.trim() == sel))
            .any(|r| r.body.contains("--sai-l") && (r.body.contains("--sai-r") || sel == ".rail"));
        assert!(both, "`{sel}` never pads for the landscape notch (--sai-l/--sai-r)");
    }
    for (name, html) in [("auth.html", AUTH_HTML), ("landing.html", LANDING_HTML)] {
        assert!(style_blocks(html).contains("--sai-l"), "{name}: page gutter ignores the left safe area");
    }
    for env in ["safe-area-inset-top", "safe-area-inset-bottom", "safe-area-inset-left", "safe-area-inset-right"] {
        assert_eq!(APP_CSS.matches(env).count(), 1, "read {env} only through its --sai-* token, so a test build can simulate it");
    }
}

#[test]
fn full_height_blocks_use_dynamic_viewport_units_with_a_fallback() {
    let all = rules(APP_CSS);
    assert!(all.iter().any(|r| r.media.iter().any(|m| m.contains("100dvh")) && decl(r, "--vh-dyn") == Some("100dvh")));
    let body = all.iter().find(|r| r.selector == "body" && r.media.is_empty()).unwrap();
    assert_eq!(decl(body, "min-height"), Some("var(--vh-dyn)"));
    assert!(style_blocks(AUTH_HTML).contains("min-height: var(--vh-dyn)"));
    for r in &all {
        for d in r.body.split(';') {
            let d = d.trim();
            if d.starts_with("min-height") || d.starts_with("max-height") || d.starts_with("height") {
                assert!(!d.contains("100vh -"), "`{}`: `{d}` measures from the large iOS viewport; use var(--vh-dyn)", r.selector);
            }
        }
    }
}

#[test]
fn touch_controls_reach_48px() {
    let all = rules(APP_CSS);
    let coarse: Vec<&Rule> = all.iter().filter(|r| r.media.iter().any(|m| m.contains("pointer: coarse"))).collect();
    let has = |sel: &str, prop: &str, val: &str| coarse.iter().any(|r| r.selector.split(',').any(|s| s.trim() == sel) && decl(r, prop) == Some(val));
    assert!(has(":root", "--target", "48px"));
    assert!(has(":root", "--hdr-ctl", "48px"));
    assert!(has(".btn-sm", "min-height", "48px"));
    assert!(has(".toast .btn", "min-height", "48px"));
    assert!(has(".menu [role=\"menuitem\"]", "min-height", "48px"));
    assert!(has(".filter", "min-height", "48px"));
    assert!(has(".input", "min-height", "48px"));
    assert!(has(".err-detail summary", "min-height", "48px"));
    assert!(has(".cite::after", "inset", "-6px"), "36px cite + 6px each side");
    assert!(has(".step-meta .cites", "gap", "12px"), "cite hit areas must not overlap");
    assert!(has(".seg > button", "min-height", "42px"));
    assert!(has(".seg > button::after", "inset", "-3px -1px"), "42px segment + 3px each side");
    assert!(has(".lang-seg > button", "min-width", "48px"));
    assert!(has(":root", "--pan", "44px"));
    let narrow = coarse.iter().any(|r| r.media.iter().any(|m| m.contains("max-width: 359px")) && r.selector == ":root" && decl(r, "--hdr-ctl") == Some("44px"));
    assert!(narrow, "below 360 px the header controls stay at the 44px minimum so the header fits");
    let auth = rules(&style_blocks(AUTH_HTML));
    assert!(auth.iter().any(|r| r.media.iter().any(|m| m.contains("pointer: coarse")) && r.selector == ".back" && decl(r, "min-height") == Some("48px")));
    assert!(GATE_HTML.contains(".gate-submit { width: 100%; min-height: 48px;"));
}

#[test]
fn human_proof_controls_reach_48px_and_chains_fold_on_phones() {
    let all = rules(APP_CSS);
    let coarse: Vec<&Rule> = all.iter().filter(|r| r.media.iter().any(|m| m.contains("pointer: coarse"))).collect();
    let has = |sel: &str, prop: &str, val: &str| coarse.iter().any(|r| r.selector.split(',').any(|s| s.trim() == sel) && decl(r, prop) == Some(val));
    assert!(has(".hp-cites .cites", "gap", "12px"), "derivation chips keep their 48px hit areas apart");
    assert!(has(".hp-why-toggle::after", "inset", "-14px -8px"));
    assert!(has(".hp-compute-toggle::after", "inset", "-14px -8px"));
    assert!(has(".menu [role=\"menuitemcheckbox\"]", "min-height", "48px"));
    let narrow: Vec<&Rule> = all.iter().filter(|r| r.media.iter().any(|m| m.contains("max-width: 599px"))).collect();
    let at = |sel: &str, prop: &str, val: &str| narrow.iter().any(|r| r.selector.split(',').any(|s| s.trim() == sel) && decl(r, prop) == Some(val));
    assert!(at(".hp-chain td", "display", "inline"), "a chain becomes stacked lines, never a wide table");
    assert!(at(".hp-chain .hp-why", "display", "none"), "reasons fold behind \"Show reasons\" on phones");
    assert!(at(".hp-why-toggle", "display", "inline-block"));
    for r in all.iter().filter(|r| r.selector.contains(".hp-") && r.selector.contains(":hover")) {
        assert!(r.media.iter().any(|m| m.contains("hover: hover")), "{} must sit inside @media (hover: hover)", r.selector);
    }
    assert!(INDEX_HTML.contains("id=\"ptab-human\"") && INDEX_HTML.contains("id=\"proof-human-panel\"") && INDEX_HTML.contains("id=\"hp-kbd\""));
}

#[test]
fn narrow_screens_wrap_instead_of_scrolling_sideways() {
    let all = rules(APP_CSS);
    let plain = |sel: &str, prop: &str, val: &str| all.iter().any(|r| r.media.is_empty() && r.selector.split(',').any(|s| s.trim() == sel) && decl(r, prop).is_some_and(|v| v.contains(val)));
    assert!(plain(".site-header .inner", "display", "flex") && plain(".site-header .inner", "flex-wrap", "wrap"));
    assert!(plain(".header-tools", "flex-wrap", "wrap"));
    assert!(plain(".composer-title", "flex-wrap", "wrap"));
    assert!(plain(".tabs", "flex-wrap", "wrap") && plain(".tabs > button", "min-width", "max-content"));
    assert!(plain("#effort", "flex-wrap", "wrap"));
    assert!(plain(".actions", "flex-wrap", "wrap"));
    assert!(plain(".fig-tools", "flex-wrap", "wrap"));
    assert!(plain(".example-chips .btn", "white-space", "normal"));
    let at = |w: &str, sel: &str| all.iter().any(|r| r.media.iter().any(|m| m.contains(w)) && r.selector.contains(sel));
    assert!(at("max-width: 339px", ".signin-btn .lbl"), "sign-in becomes an icon below 340 px");
    assert!(at("max-width: 299px", ".shell"), "tighter gutters below 300 px (Galaxy Fold cover screen, page zoom)");
    assert!(at("max-width: 259px", ".site-header"), "the header stops being sticky once it may wrap");
    assert!(INDEX_HTML.contains("class=\"btn btn-secondary btn-sm signin-btn\" id=\"signin\""));
    assert!(LANDING_HTML.contains("signin-btn"));
}

#[test]
fn back_button_closes_drawer_and_full_screen() {
    let site = include_str!("../assets/site.js");
    let app = include_str!("../assets/app.js");
    assert!(site.contains("addEventListener(\"popstate\"") && site.contains("history.pushState({ gsLayer"));
    assert!(site.contains("backLayer(this.closeByBack)") && site.contains("dropLayer(this.closeByBack)"));
    assert!(app.contains("GS.backLayer(closeDrawerByBack)") && app.contains("GS.dropLayer(closeDrawerByBack)"));
}

#[test]
fn ios_field_attributes() {
    let user = AUTH_HTML.split("id=\"username\"").nth(1).and_then(|s| s.split('>').next()).unwrap();
    assert!(user.contains("autocorrect=\"off\"") && user.contains("autocapitalize=\"off\""));
    assert!(user.contains("enterkeyhint=\"next\""));
    let search = INDEX_HTML.split("id=\"hist-search\"").nth(1).and_then(|s| s.split('>').next()).unwrap();
    assert!(search.contains("enterkeyhint=\"search\""));
    assert!(APP_CSS.contains("-webkit-tap-highlight-color: transparent"));
}
