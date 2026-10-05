//! The shared-password page (`/gate`) and its signed cookie.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    body::Body,
    extract::{FromRequest, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    Form,
};
use base64::Engine as _;
use serde::Deserialize;

use crate::i18n::{self, Lang};
use crate::security::{self, Shared};

pub const GATE_HTML: &str = include_str!("../assets/gate.html");

/// A valid cookie older than this is re-issued on the next page load.
pub const RENEW_AFTER_SECS: u64 = 7 * 24 * 60 * 60;
/// How far in the future an `iat` may lie (clock skew between restarts).
const MAX_SKEW_SECS: u64 = 300;
const MAX_COOKIE_BYTES: usize = 128;
const MAX_NEXT_BYTES: usize = 2048;

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn cookie_name(secure: bool) -> &'static str {
    if secure {
        "__Host-gate"
    } else {
        "gate"
    }
}

fn mac(key: &[u8; 32], cred: &[u8; 32], iat: u64) -> [u8; 32] {
    let mut msg = b"agstudio-gate\0v1\0".to_vec();
    msg.extend_from_slice(iat.to_string().as_bytes());
    msg.push(0);
    msg.extend_from_slice(cred);
    security::hmac_sha256(key, &msg)
}

/// `v1.<iat>.<base64url(HMAC-SHA256(key, "agstudio-gate\0v1\0" iat "\0" cred))>`.
pub fn mint(key: &[u8; 32], cred: &[u8; 32], iat: u64) -> String {
    let m = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac(key, cred, iat));
    format!("v1.{iat}.{m}")
}

/// The cookie's issue time when `value` is a cookie minted under `key` and
/// `cred`, issued at most `max_age` seconds before `now` and not more than
/// [`MAX_SKEW_SECS`] after it.
pub fn verify(value: &str, key: &[u8; 32], cred: &[u8; 32], max_age: u64, now: u64) -> Option<u64> {
    if value.len() > MAX_COOKIE_BYTES {
        return None;
    }
    let (iat_s, mac_s) = value.strip_prefix("v1.")?.split_once('.')?;
    if iat_s.is_empty() || iat_s.len() > 12 || !iat_s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let iat: u64 = iat_s.parse().ok()?;
    if iat > now.saturating_add(MAX_SKEW_SECS) || now.saturating_sub(iat) > max_age {
        return None;
    }
    let got = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(mac_s).ok()?;
    let got: [u8; 32] = got.try_into().ok()?;
    security::ct_eq(&got, &mac(key, cred, iat)).then_some(iat)
}

fn max_age_secs(state: &Shared) -> u64 {
    u64::from(state.config.gate_days) * 86_400
}

/// The issue time of the request's gate cookie, when it carries exactly one
/// valid cookie for the current password and key.
pub fn cookie_iat(state: &Shared, headers: &HeaderMap) -> Option<u64> {
    let basic = state.config.basic_auth.as_ref()?;
    let value = crate::auth::single_cookie(headers, cookie_name(state.config.secure_cookies))?;
    verify(value, &state.gate_key, &basic.cred_digest(), max_age_secs(state), now_secs())
}

pub fn due_for_renewal(iat: u64) -> bool {
    now_secs().saturating_sub(iat) > RENEW_AFTER_SECS
}

fn set_cookie_value(state: &Shared) -> Option<String> {
    let basic = state.config.basic_auth.as_ref()?;
    let secure = state.config.secure_cookies;
    let value = mint(&state.gate_key, &basic.cred_digest(), now_secs());
    Some(format!(
        "{}={value}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{}",
        cookie_name(secure),
        max_age_secs(state),
        if secure { "; Secure" } else { "" }
    ))
}

/// Append a freshly minted gate cookie to `resp`.
pub fn append_cookie(state: &Shared, resp: &mut Response) {
    if let Some(v) = set_cookie_value(state).and_then(|v| HeaderValue::from_str(&v).ok()) {
        resp.headers_mut().append(header::SET_COOKIE, v);
    }
}

fn clear_cookie_value(secure: bool) -> String {
    format!(
        "{}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{}",
        cookie_name(secure),
        if secure { "; Secure" } else { "" }
    )
}

fn lang_cookie_value(lang: Lang, secure: bool) -> String {
    format!(
        "lang={}; Path=/; Max-Age=31536000; SameSite=Lax{}",
        lang.code(),
        if secure { "; Secure" } else { "" }
    )
}

/// Percent-encode everything but RFC 3986 unreserved characters.
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A same-site path to return to after the gate, or `/`: it must start with
/// one `/` (not `//` or `/\`, which browsers read as another host), hold no
/// backslash or control character, fit in 2 KB, and not lead back to `/gate`.
pub fn sanitize_next(next: &str) -> String {
    let b = next.as_bytes();
    let ok = b.first() == Some(&b'/')
        && !matches!(b.get(1), Some(b'/') | Some(b'\\'))
        && !b.iter().any(|&c| c == b'\\' || c.is_ascii_control())
        && b.len() <= MAX_NEXT_BYTES
        && !next.starts_with("/gate");
    if ok {
        next.to_string()
    } else {
        "/".to_string()
    }
}

/// `next` as a `Location` value: bytes outside visible ASCII are escaped.
fn location_of(next: &str) -> HeaderValue {
    let mut out = String::with_capacity(next.len());
    for b in next.bytes() {
        if (0x21..0x7f).contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    HeaderValue::from_str(&out).unwrap_or(HeaderValue::from_static("/"))
}

fn see_other(location: HeaderValue) -> Response {
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
    )
        .into_response()
}

/// 303 to the gate page, returning to `target` (a path and query) afterwards.
pub fn redirect_to_gate(target: &str) -> Response {
    let loc = format!("/gate?next={}", percent_encode(target));
    see_other(HeaderValue::from_str(&loc).unwrap_or(HeaderValue::from_static("/gate")))
}

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Which message the page shows under the password field.
#[derive(Clone, Copy)]
enum Problem {
    Wrong,
    Empty,
    Rate,
}

impl Problem {
    fn key(self) -> &'static str {
        match self {
            Problem::Wrong => "gate.wrong",
            Problem::Empty => "gate.empty",
            Problem::Rate => "gate.rate",
        }
    }
}

fn render(lang: Lang, next: &str, ask_user: bool, problem: Option<Problem>) -> String {
    let t = |k: &str| html_escape(i18n::t(lang, k));
    let username = if ask_user {
        format!(
            "<div class=\"field\"><label for=\"user\">{}</label><input class=\"input\" id=\"user\" name=\"username\" type=\"text\" autocomplete=\"username\" autocapitalize=\"none\" autocorrect=\"off\" spellcheck=\"false\" maxlength=\"256\" required></div>",
            t("gate.username")
        )
    } else {
        "<input class=\"sr-only\" type=\"text\" name=\"username\" value=\"GeoSolver\" autocomplete=\"username\" readonly tabindex=\"-1\" aria-hidden=\"true\">".to_string()
    };
    let (pw_attrs, error) = match problem {
        Some(p) => (
            " aria-invalid=\"true\" aria-describedby=\"pw-err\" autofocus".to_string(),
            format!("<p class=\"gate-err\" id=\"pw-err\" role=\"alert\">{}</p>", t(p.key())),
        ),
        None => (String::new(), String::new()),
    };
    let enc_next = html_escape(&percent_encode(next));
    let links: Vec<String> = [Lang::En, Lang::Ro]
        .iter()
        .map(|&l| {
            let code = l.code();
            let current = if l == lang { " aria-current=\"true\"" } else { "" };
            format!(
                "<a href=\"/gate?lang={code}&amp;next={enc_next}\" hreflang=\"{code}\" lang=\"{code}\"{current}>{}</a>",
                code.to_ascii_uppercase()
            )
        })
        .collect();
    GATE_HTML
        .replace("<!--pwa-splash-->", crate::pwa::splash_links())
        .replace("{{lang}}", lang.code())
        .replace("{{doctitle}}", &t("gate.doctitle"))
        .replace("{{title}}", &t("gate.title"))
        .replace("{{sub}}", &t("gate.sub"))
        .replace("{{label}}", &t("gate.label"))
        .replace("{{submit}}", &t("gate.submit"))
        .replace("{{remember}}", &t("gate.remember"))
        .replace("{{lang_label}}", &t("gate.lang"))
        .replace("{{next}}", &html_escape(next))
        .replace("{{username}}", &username)
        .replace("{{pw_attrs}}", &pw_attrs)
        .replace("{{error}}", &error)
        .replace("{{lang_links}}", &links.join("\n      "))
}

fn page_response(status: StatusCode, html: String) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (header::HeaderName::from_static("x-robots-tag"), HeaderValue::from_static("noindex")),
        ],
        Html(html),
    )
        .into_response()
}

#[derive(Deserialize, Default)]
struct GateQuery {
    #[serde(default)]
    next: Option<String>,
    #[serde(default)]
    lang: Option<String>,
}

/// `GET /gate`: the password page, or straight on to `next` when the cookie
/// is already valid (or there is no form gate).
pub async fn page(State(state): State<Shared>, uri: Uri, headers: HeaderMap) -> Response {
    let q = Query::<GateQuery>::try_from_uri(&uri).map(|q| q.0).unwrap_or_default();
    let next = sanitize_next(q.next.as_deref().unwrap_or("/"));
    if !state.config.form_gate() || cookie_iat(&state, &headers).is_some() {
        return see_other(location_of(&next));
    }
    let chosen = q.lang.as_deref().and_then(Lang::from_code);
    let lang = chosen.unwrap_or_else(|| i18n::lang_from_headers(&headers));
    let ask_user = state.config.basic_auth.as_ref().is_some_and(|b| b.has_user());
    let mut resp = page_response(StatusCode::OK, render(lang, &next, ask_user, None));
    if let Some(l) = chosen {
        if let Ok(v) = HeaderValue::from_str(&lang_cookie_value(l, state.config.secure_cookies)) {
            resp.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    resp
}

#[derive(Deserialize, Default)]
struct GateForm {
    #[serde(default)]
    password: String,
    #[serde(default)]
    next: String,
    #[serde(default)]
    username: String,
}

/// `POST /gate`: check the shared password against the per-IP wrong-password
/// budget it shares with Basic auth; on success set the cookie and go to
/// `next`.
pub async fn submit(State(state): State<Shared>, req: Request<Body>) -> Response {
    let Some(basic) = state.config.basic_auth.clone().filter(|_| state.config.form_gate()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ip = security::client_ip(&state.config, &req);
    let headers = req.headers().clone();
    let lang = i18n::lang_from_headers(&headers);
    let ask_user = basic.has_user();
    let form = Form::<GateForm>::from_request(req, &state).await.map(|f| f.0);
    let next = sanitize_next(form.as_ref().map_or("/", |f| f.next.as_str()));
    if state.password_guesses_exhausted(ip) {
        let mut resp = page_response(StatusCode::TOO_MANY_REQUESTS, render(lang, &next, ask_user, Some(Problem::Rate)));
        resp.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        return resp;
    }
    let form = match form {
        Ok(f) if !f.password.is_empty() => f,
        Ok(_) => return page_response(StatusCode::BAD_REQUEST, render(lang, &next, ask_user, Some(Problem::Empty))),
        Err(rejection) => {
            let status = rejection.status();
            return page_response(status, render(lang, &next, ask_user, Some(Problem::Empty)));
        }
    };
    if !basic.credentials_match(form.username.as_bytes(), form.password.as_bytes()) {
        state.count_wrong_password(ip);
        return page_response(StatusCode::UNAUTHORIZED, render(lang, &next, ask_user, Some(Problem::Wrong)));
    }
    let mut resp = see_other(location_of(&next));
    append_cookie(&state, &mut resp);
    crate::web::with_guest_id(&state, &headers, resp)
}

/// `POST /gate/forget`: drop this device's gate cookie.
pub async fn forget(State(state): State<Shared>) -> Response {
    let mut resp = see_other(HeaderValue::from_static("/gate"));
    if let Ok(v) = HeaderValue::from_str(&clear_cookie_value(state.config.secure_cookies)) {
        resp.headers_mut().append(header::SET_COOKIE, v);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7u8; 32];
    const NOW: u64 = 1_800_000_000;
    const DAY: u64 = 86_400;

    fn cred(spec: &str) -> [u8; 32] {
        security::BasicAuth::parse(spec).unwrap().cred_digest()
    }

    #[test]
    fn minted_cookies_verify_and_tampering_fails() {
        let c = cred(":correct-horse-9");
        let v = mint(&KEY, &c, NOW);
        assert!(v.starts_with(&format!("v1.{NOW}.")));
        assert!(v.len() <= MAX_COOKIE_BYTES);
        assert_eq!(verify(&v, &KEY, &c, 180 * DAY, NOW), Some(NOW));
        assert_eq!(verify(&v, &KEY, &c, 180 * DAY, NOW + 179 * DAY), Some(NOW));

        for i in "v1.1800000000.".len()..v.len() {
            let mut b = v.clone().into_bytes();
            b[i] = if b[i] == b'A' { b'B' } else { b'A' };
            let t = String::from_utf8(b).unwrap();
            assert_eq!(verify(&t, &KEY, &c, 180 * DAY, NOW), None, "MAC char {i} changed");
        }
        let future = mint(&KEY, &c, NOW + 301);
        assert_eq!(verify(&future, &KEY, &c, 180 * DAY, NOW), None, "iat > now + 300");
        let skewed = mint(&KEY, &c, NOW + 299);
        assert!(verify(&skewed, &KEY, &c, 180 * DAY, NOW).is_some());
        assert_eq!(verify(&v, &KEY, &c, 180 * DAY, NOW + 181 * DAY), None, "expired");
        assert_eq!(verify(&v, &KEY, &cred(":another-password"), 180 * DAY, NOW), None, "rotated password");
        assert_eq!(verify(&v, &KEY, &cred("someone:correct-horse-9"), 180 * DAY, NOW), None, "user added");
        assert_eq!(verify(&v, &[8u8; 32], &c, 180 * DAY, NOW), None, "rotated key");
        let relabeled = v.replacen(&NOW.to_string(), &(NOW + 1).to_string(), 1);
        assert_eq!(verify(&relabeled, &KEY, &c, 180 * DAY, NOW + 1), None, "iat is signed");
        for junk in ["", "v1", "v1..", "v2.1.x", &format!("v1.{NOW}"), &format!("v1.+{NOW}.x"), &"v1.1.".repeat(40)] {
            assert_eq!(verify(junk, &KEY, &c, 180 * DAY, NOW), None, "{junk:?}");
        }
    }

    #[test]
    fn next_is_kept_only_when_it_stays_on_this_site() {
        assert_eq!(sanitize_next("/app?x=1"), "/app?x=1");
        assert_eq!(sanitize_next("/"), "/");
        for bad in [
            "//evil.com", "/\\evil.com", "https://evil.com", "/gate", "/gate?next=/app", "//evil",
            "/app\r\nSet-Cookie: x=1", "/a\nb", "", "app", "/a\\b", "/\u{7f}",
        ] {
            assert_eq!(sanitize_next(bad), "/", "{bad:?}");
        }
        assert_eq!(sanitize_next(&format!("/{}", "a".repeat(2047))).len(), 2048);
        assert_eq!(sanitize_next(&format!("/{}", "a".repeat(2048))), "/");
    }

    #[test]
    fn percent_encoding_round_trips_through_the_query() {
        assert_eq!(percent_encode("/"), "%2F");
        assert_eq!(percent_encode("/app?x=1&y=ă"), "%2Fapp%3Fx%3D1%26y%3D%C4%83");
    }

    #[test]
    fn gate_page_has_no_inline_script_and_escapes_its_values() {
        assert!(security::inline_scripts(GATE_HTML).is_empty());
        assert!(!GATE_HTML.contains("<script"));
        let html = render(Lang::En, "/app?a=\"><script>x</script>", false, Some(Problem::Wrong));
        assert!(!html.contains("<script"), "next must be escaped");
        assert!(!html.contains("{{"), "every placeholder is filled");
        assert!(html.contains("aria-invalid=\"true\""));
        assert!(html.contains("role=\"alert\""));
        assert!(html.contains(i18n::t(Lang::En, "gate.wrong")));
        assert!(html.contains("autocomplete=\"current-password\""));
        assert!(html.contains("name=\"username\" value=\"GeoSolver\""));
        let ro = render(Lang::Ro, "/", true, None);
        assert!(ro.contains("<html lang=\"ro\">"));
        assert!(ro.contains(&html_escape(i18n::t(Lang::Ro, "gate.title"))));
        assert!(ro.contains("autocomplete=\"username\" autocapitalize"), "user:pass asks for the user");
        assert!(!ro.contains("aria-invalid"));
        assert!(!ro.contains("role=\"alert\""));
    }
}
