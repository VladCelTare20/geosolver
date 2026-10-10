const APP_CSS: &str = include_str!("../assets/app.css");
const INDEX_HTML: &str = include_str!("../assets/index.html");
const AUTH_HTML: &str = include_str!("../assets/auth.html");
const LANDING_HTML: &str = include_str!("../assets/landing.html");
const GATE_HTML: &str = include_str!("../assets/gate.html");
const APP_JS: &str = include_str!("../assets/app.js");
const SITE_JS: &str = include_str!("../assets/site.js");
const I18N_JS: &str = include_str!("../assets/i18n.js");
const LANDING_JS: &str = include_str!("../assets/landing.js");
const AUTH_JS: &str = include_str!("../assets/auth.js");

const PAGES: [(&str, &str); 4] = [
    ("index.html", INDEX_HTML),
    ("auth.html", AUTH_HTML),
    ("landing.html", LANDING_HTML),
    ("gate.html", GATE_HTML),
];
const SCRIPTS: [(&str, &str); 5] = [
    ("app.js", APP_JS),
    ("site.js", SITE_JS),
    ("i18n.js", I18N_JS),
    ("landing.js", LANDING_JS),
    ("auth.js", AUTH_JS),
];

const LATE_CSS_VALUES: &[&str] = &[
    "color-mix(", "oklch(", "oklab(", "lab(", "lch(", "light-dark(", "color(", "round(", "mod(", "rem(",
    "anchor(", "anchor-size(", "calc-size(", "dvh", "svh", "lvh", "dvw", "svw", "lvw", "dvi", "svi",
    "lvi", "dvb", "svb", "lvb", "dvmin", "dvmax", "svmin", "svmax", "lvmin", "lvmax", "cqw", "cqh",
    "cqi", "cqb", "cqmin", "cqmax", "rlh", "1lh",
];
const LATE_SELECTORS: &[&str] = &[
    ":has(", ":user-invalid", ":user-valid", "::target-text", ":popover-open", ":state(", "::details-content",
    ":open", "::scroll-marker", "::scroll-button", "::picker(", "::view-transition", ":active-view-transition",
];
const BANNED_AT_RULES: &[&str] = &[
    "@layer", "@scope", "@property", "@starting-style", "@view-transition", "@position-try", "@function",
    "@custom-media",
];
const BANNED_JS: &[&str] = &[
    ".findLast(", ".findLastIndex(", ".toSorted(", ".toReversed(", ".toSpliced(", "Object.groupBy",
    "Map.groupBy", "Array.fromAsync", "Promise.withResolvers", "Promise.try", "AbortSignal.timeout",
    "AbortSignal.any", "requestIdleCallback", "Intl.Segmenter", "Intl.DurationFormat", "showPopover",
    "togglePopover", "hidePopover", "startViewTransition", "checkVisibility", "navigator.userActivation",
    "CloseWatcher", "scheduler.", "setHTMLUnsafe", "EyeDropper", ".union(", ".intersection(",
    ".isWellFormed(", ".toWellFormed(", "Iterator.from", "structuredClone", "showModal", "import(",
    "import.meta", "static {", "Error.isError", "RegExp.escape", "Float16Array", "Atomics.pause",
    ".at(-",
];

fn strip_css_comments(css: &str) -> String {
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

struct StyleRule {
    selector: String,
    decls: Vec<(String, String)>,
    supports: bool,
    nested: bool,
}

struct Sheet {
    rules: Vec<StyleRule>,
    at_rules: Vec<String>,
}

fn matching_brace(s: &[u8], open: usize) -> usize {
    let mut depth = 0;
    let mut quote = 0u8;
    for (i, &c) in s.iter().enumerate().skip(open) {
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
            continue;
        }
        match c {
            b'"' | b'\'' => quote = c,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len()
}

fn split_decls(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut quote = None;
    let mut paren = 0;
    let mut start = 0;
    let b = body.as_bytes();
    for i in 0..=b.len() {
        let c = if i < b.len() { b[i] } else { b';' };
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            b'"' | b'\'' => quote = Some(c),
            b'(' => paren += 1,
            b')' => paren -= 1,
            b';' if paren == 0 => {
                let d = body[start..i.min(b.len())].trim();
                if let Some((p, v)) = d.split_once(':') {
                    out.push((p.trim().to_ascii_lowercase(), v.trim().to_string()));
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    out
}

fn parse_block(css: &str, supports: bool, sheet: &mut Sheet) {
    let b = css.as_bytes();
    let mut i = 0;
    let mut start = 0;
    while i < b.len() {
        match b[i] {
            b';' if css[start..i].trim_start().starts_with('@') => {
                sheet.at_rules.push(css[start..i].trim().to_string());
                start = i + 1;
            }
            b'{' => {
                let head = css[start..i].trim().to_string();
                let end = matching_brace(b, i);
                let body = &css[i + 1..end.min(b.len())];
                if head.starts_with('@') {
                    sheet.at_rules.push(head.clone());
                    let inner_supports = supports || head.starts_with("@supports");
                    if head.starts_with("@keyframes") || head.starts_with("@-webkit-keyframes") {
                        let mut frames = Sheet { rules: Vec::new(), at_rules: Vec::new() };
                        parse_block(body, true, &mut frames);
                        sheet.rules.extend(frames.rules.into_iter().map(|mut r| {
                            r.selector = format!("{head} {}", r.selector);
                            r
                        }));
                    } else if head.starts_with("@font-face") {
                        sheet.rules.push(StyleRule { selector: head, decls: split_decls(body), supports, nested: false });
                    } else {
                        parse_block(body, inner_supports, sheet);
                    }
                } else {
                    let nested = body.contains('{');
                    sheet.rules.push(StyleRule { selector: head, decls: split_decls(body), supports, nested });
                }
                i = end + 1;
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
}

fn sheet(css: &str) -> Sheet {
    let mut s = Sheet { rules: Vec::new(), at_rules: Vec::new() };
    parse_block(&strip_css_comments(css), false, &mut s);
    s
}

fn page_styles(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("<style>") {
        let tail = &rest[i + 7..];
        let end = tail.find("</style>").unwrap_or(tail.len());
        out.push(tail[..end].to_string());
        rest = &tail[end..];
    }
    out
}

fn all_sheets() -> Vec<(String, Sheet)> {
    let mut v = vec![("app.css".to_string(), sheet(APP_CSS))];
    for (name, html) in PAGES {
        for (k, css) in page_styles(html).iter().enumerate() {
            v.push((format!("{name} <style> #{k}"), sheet(css)));
        }
    }
    v
}

fn late_value(v: &str) -> Option<&'static str> {
    let low = v.to_ascii_lowercase();
    LATE_CSS_VALUES.iter().copied().find(|f| {
        if f.ends_with('(') {
            low.match_indices(f).any(|(i, _)| i == 0 || !low.as_bytes()[i - 1].is_ascii_alphanumeric() && low.as_bytes()[i - 1] != b'-')
        } else {
            low.match_indices(f).any(|(i, _)| {
                let before = i > 0 && low.as_bytes()[i - 1].is_ascii_digit();
                let after = low.as_bytes().get(i + f.len()).is_none_or(|c| !c.is_ascii_alphanumeric());
                before && after
            })
        }
    })
}

fn top_level_selectors(sel: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in sel.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(sel[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(sel[start..].trim().to_string());
    out
}

#[test]
fn css_values_newer_than_the_baseline_have_a_fallback() {
    let mut bad = Vec::new();
    for (name, s) in all_sheets() {
        for r in &s.rules {
            for (k, (prop, val)) in r.decls.iter().enumerate() {
                let Some(f) = late_value(val) else { continue };
                if r.supports {
                    continue;
                }
                let fallback = r.decls[..k].iter().any(|(p, v)| p == prop && late_value(v).is_none());
                if !fallback {
                    bad.push(format!("{name}: `{}` {{ {prop}: {val} }} uses {f} with no earlier `{prop}` fallback and no @supports", r.selector));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn selectors_newer_than_the_baseline_stand_alone() {
    let mut bad = Vec::new();
    for (name, s) in all_sheets() {
        for r in &s.rules {
            let list = top_level_selectors(&r.selector);
            for f in LATE_SELECTORS {
                if r.selector.contains(f) && list.len() > 1 {
                    bad.push(format!("{name}: `{}` puts {f} in a selector list; a browser without it drops the whole rule", r.selector));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn no_css_nesting_or_late_at_rules() {
    let mut bad = Vec::new();
    for (name, s) in all_sheets() {
        for r in &s.rules {
            if r.nested {
                bad.push(format!("{name}: `{}` nests a rule (CSS nesting is Chrome 112 / Safari 16.5 / Firefox 117)", r.selector));
            }
        }
        for a in &s.at_rules {
            for banned in BANNED_AT_RULES {
                if a.starts_with(banned) {
                    bad.push(format!("{name}: {a}"));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn container_queries_have_a_media_query_fallback() {
    for (name, s) in all_sheets() {
        let containers = s.at_rules.iter().filter(|a| a.starts_with("@container")).count();
        let fallbacks = s.at_rules.iter().filter(|a| a.replace(' ', "").starts_with("@supportsnot(container-type:inline-size)")).count();
        assert!(fallbacks >= containers, "{name}: {containers} @container rule(s) but {fallbacks} `@supports not (container-type: inline-size)` fallback(s)");
    }
}

fn js_code(src: &str) -> (String, Vec<String>) {
    let b = src.as_bytes();
    let mut code = String::with_capacity(src.len());
    let mut regexes = Vec::new();
    let mut i = 0;
    let mut last_sig = b'(';
    let mut last_word = String::new();
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == b'"' || c == b'\'' || c == b'`' {
            let q = c;
            i += 1;
            while i < b.len() && b[i] != q {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            code.push_str("\"\"");
            last_sig = b'"';
            continue;
        }
        let regex_ok = b"(,=:[!&|?{};+-*%<>~^".contains(&last_sig) || matches!(last_word.as_str(), "return" | "typeof" | "case" | "in" | "of");
        if c == b'/' && regex_ok {
            let start = i;
            i += 1;
            let mut class = false;
            while i < b.len() {
                match b[i] {
                    b'\\' => i += 1,
                    b'[' => class = true,
                    b']' => class = false,
                    b'/' if !class => break,
                    b'\n' => break,
                    _ => {}
                }
                i += 1;
            }
            i += 1;
            while i < b.len() && b[i].is_ascii_alphabetic() {
                i += 1;
            }
            regexes.push(src[start..i.min(b.len())].to_string());
            code.push_str("/r/");
            last_sig = b'r';
            continue;
        }
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$') {
                i += 1;
            }
            last_word = src[s..i].to_string();
            code.push_str(&last_word);
            last_sig = b'w';
            continue;
        }
        if !c.is_ascii_whitespace() {
            last_sig = c;
            last_word.clear();
        }
        code.push(c as char);
        i += 1;
    }
    (code, regexes)
}

fn inline_scripts(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("<script>") {
        let tail = &rest[i + 8..];
        let end = tail.find("</script>").unwrap_or(tail.len());
        out.push(tail[..end].to_string());
        rest = &tail[end..];
    }
    out
}

#[test]
fn scripts_use_no_api_or_syntax_newer_than_the_baseline() {
    let mut bad = Vec::new();
    let mut sources: Vec<(String, String)> = SCRIPTS.iter().map(|(n, s)| (n.to_string(), s.to_string())).collect();
    for (name, html) in PAGES {
        for (k, s) in inline_scripts(html).into_iter().enumerate() {
            sources.push((format!("{name} inline #{k}"), s));
        }
    }
    for (name, src) in &sources {
        let (code, regexes) = js_code(src);
        for f in BANNED_JS {
            if code.contains(f) {
                bad.push(format!("{name}: {f}"));
            }
        }
        for r in &regexes {
            let flags = &r[r.rfind('/').unwrap_or(0) + 1..];
            if r.contains("(?<=") || r.contains("(?<!") {
                bad.push(format!("{name}: regex lookbehind {r} (Safari 16.4)"));
            }
            if flags.contains('v') {
                bad.push(format!("{name}: regex v flag {r}"));
            }
        }
        if code.contains("navigator.share(") && !code.contains("navigator.share)") && !code.contains("!navigator.share") {
            bad.push(format!("{name}: navigator.share without a presence check"));
        }
        if code.contains(".inert") && !code.contains("\"inert\" in HTMLElement.prototype") && !code.contains("\"\" in HTMLElement.prototype") {
            bad.push(format!("{name}: .inert without the no-inert fallback"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn inline_page_scripts_are_es5() {
    for (name, html) in PAGES {
        for s in inline_scripts(html) {
            let (code, _) = js_code(&s);
            for f in ["=>", "let ", "const ", "class ", "...", "?.", "??", "async ", "await "] {
                assert!(!code.contains(f), "{name}: inline script uses `{f}`; an old browser must be able to run it");
            }
        }
    }
}

#[test]
fn every_scripted_page_has_the_old_browser_guard_and_a_noscript_note() {
    for (name, html) in PAGES.iter().filter(|(n, _)| *n != "gate.html") {
        assert!(html.contains("C.supports(\"aspect-ratio\",\"1 / 1\")"), "{name}: guard script missing");
        assert!(html.contains("id=\"gs-old\""), "{name}: unsupported-browser note missing");
        assert!(html.contains("id=\"gs-fail\""), "{name}: did-not-load note missing");
        assert!(html.contains("<noscript><div class=\"gs-note gs-show gs-both\""), "{name}: <noscript> note missing");
        let guard = html.find("C.supports(\"aspect-ratio\"").unwrap();
        let first_src = html.find("<script src=").unwrap_or(html.len());
        assert!(guard < first_src, "{name}: the guard must run before any external script");
        for lang in ["lang=\"en\"><h2>", "lang=\"ro\"><h2>"] {
            assert!(html.matches(lang).count() >= 3, "{name}: every note needs EN and RO text");
        }
    }
    assert!(GATE_HTML.contains("id=\"gs-old\""), "gate.html: CSS-only old-browser note missing");
    assert!(GATE_HTML.contains("@supports (aspect-ratio: 1 / 1) { #gs-old { display: none; } }"));
    for (name, js) in [("app.js", APP_JS), ("landing.js", LANDING_JS), ("auth.js", AUTH_JS)] {
        assert!(js.contains("GS.booted = true;"), "{name} must mark the page booted");
    }
}

#[test]
fn the_checker_catches_what_it_should() {
    let s = sheet(".a { height: 100dvh; } .b { height: 100vh; height: 100dvh; } @supports (height: 1dvh) { .c { height: 1dvh; } } .d, .e:has(x) { color: red; } .f { .g { color: red; } }");
    let flagged: Vec<_> = s.rules.iter().filter(|r| {
        r.decls.iter().enumerate().any(|(k, (p, v))| late_value(v).is_some() && !r.supports && !r.decls[..k].iter().any(|(q, w)| q == p && late_value(w).is_none()))
    }).map(|r| r.selector.clone()).collect();
    assert_eq!(flagged, vec![".a".to_string()]);
    assert!(s.rules.iter().any(|r| r.selector.contains(":has(") && top_level_selectors(&r.selector).len() == 2));
    assert!(s.rules.iter().any(|r| r.nested));
    assert_eq!(late_value("2px solid color-mix(in srgb, red 5%, blue)"), Some("color-mix("));
    assert_eq!(late_value("calc(100svh - 4px)"), Some("svh"));
    assert_eq!(late_value("var(--vh-dyn)"), None);
    assert_eq!(late_value("translate(-50%)"), None);
    let (code, re) = js_code("var a = 'x.findLast(' + b; // .toSorted(\nvar r = /(?<=a)b/g; c = d / e / f;");
    assert!(!code.contains("findLast") && !code.contains("toSorted"));
    assert_eq!(re, vec!["/(?<=a)b/g".to_string()]);
}
