//! Server-side message localization (English/Romanian).
//!
//! The language is taken from the `lang` cookie the browser sets (see
//! `assets/i18n.js`); absent or unrecognized, it defaults to English. Handlers
//! return `t(lang, key)` for every user-facing string so a Romanian visitor
//! sees Romanian errors too. Inherently-technical detail (parser/compiler
//! output) is passed through verbatim by the caller.

use axum::http::{header, HeaderMap};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Ro,
}

/// Read the preferred language from the request's `lang` cookie.
pub fn lang_from_headers(headers: &HeaderMap) -> Lang {
    if let Some(cookie) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for part in cookie.split(';') {
            if let Some(v) = part.trim().strip_prefix("lang=") {
                if v.eq_ignore_ascii_case("ro") {
                    return Lang::Ro;
                }
                if v.eq_ignore_ascii_case("en") {
                    return Lang::En;
                }
            }
        }
    }
    Lang::En
}

/// Localized message for `key`. Unknown keys return the key itself (so a missing
/// translation is visible rather than silently blank).
pub fn t(lang: Lang, key: &str) -> &'static str {
    match lang {
        Lang::En => en(key),
        Lang::Ro => ro(key),
    }
}

fn en(key: &str) -> &'static str {
    match key {
        // auth
        "auth.username_rule" => "username must be 3–32 characters: letters, digits, - or _",
        "auth.pw_len" => "password must be 8–128 characters",
        "auth.user_taken" => "that username is already taken",
        "auth.create_fail" => "could not create the account",
        "auth.bad_creds" => "invalid username or password",
        "auth.login_fail" => "login failed",
        "auth.not_signed_in" => "not signed in",
        "auth.sign_in_required" => "sign in required",
        // solve
        "solve.empty" => "empty program",
        "solve.failed" => "the solver failed",
        // history
        "history.load_fail" => "could not load history",
        "history.none" => "no such history entry",
        "history.del_fail" => "could not delete history entry",
        // translate
        "translate.too_long" => "description is too long",
        "translate.bad_image" => "the uploaded image could not be read",
        "translate.need_input" => "provide a description or an image",
        "translate.failed" => "the translator failed",
        // humanize
        "humanize.need_proof" => "provide a proof to rewrite",
        "humanize.unavailable" => "AI proofs are not enabled on this server",
        "humanize.failed" => "could not write the proof — please retry",
        // export
        "export.failed" => "export failed",
        // shared / security
        "err.too_long" => "input is too long",
        "err.too_large" => "the program is too large for this server",
        "server.busy" => "the server is busy — please retry in a moment",
        "rate.exceeded" => "rate limit exceeded — please slow down",
        "origin.refused" => "cross-origin request refused",
        _ => "something went wrong",
    }
}

fn ro(key: &str) -> &'static str {
    match key {
        // auth
        "auth.username_rule" => {
            "numele de utilizator trebuie să aibă 3–32 de caractere: litere, cifre, - sau _"
        }
        "auth.pw_len" => "parola trebuie să aibă între 8 și 128 de caractere",
        "auth.user_taken" => "acest nume de utilizator este deja folosit",
        "auth.create_fail" => "contul nu a putut fi creat",
        "auth.bad_creds" => "nume de utilizator sau parolă incorecte",
        "auth.login_fail" => "autentificarea a eșuat",
        "auth.not_signed_in" => "neautentificat",
        "auth.sign_in_required" => "este necesară autentificarea",
        // solve
        "solve.empty" => "program gol",
        "solve.failed" => "rezolvitorul a eșuat",
        // history
        "history.load_fail" => "istoricul nu a putut fi încărcat",
        "history.none" => "nu există această intrare în istoric",
        "history.del_fail" => "intrarea din istoric nu a putut fi ștearsă",
        // translate
        "translate.too_long" => "descrierea este prea lungă",
        "translate.bad_image" => "imaginea încărcată nu a putut fi citită",
        "translate.need_input" => "furnizează o descriere sau o imagine",
        "translate.failed" => "traducătorul a eșuat",
        // humanize
        "humanize.need_proof" => "furnizează o demonstrație de rescris",
        "humanize.unavailable" => "demonstrațiile AI nu sunt activate pe acest server",
        "humanize.failed" => "demonstrația nu a putut fi redactată — te rog reîncearcă",
        // export
        "export.failed" => "exportul a eșuat",
        // shared / security
        "err.too_long" => "textul introdus este prea lung",
        "err.too_large" => "programul este prea mare pentru acest server",
        "server.busy" => "serverul este ocupat — te rog reîncearcă în scurt timp",
        "rate.exceeded" => "limită de solicitări depășită — te rog încetinește",
        "origin.refused" => "cerere din altă origine refuzată",
        _ => "ceva n-a mers",
    }
}
