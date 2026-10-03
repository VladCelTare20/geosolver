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
        // translate (structured)
        "translate.disabled" => "AI translation is not enabled on this server.",
        "translate.no_problem" => "The translator could not turn that into a geometry problem. Try rephrasing it, or write the .geo program directly.",
        "history.no_solution" => "This entry was saved before results were stored; it will be solved again.",
        "export.expired" => "This result is no longer cached on the server. Solve it again to export it.",
        // compile errors
        "compile.expect_comma_paren" => "Expected a comma or a closing parenthesis before “{tok}”.",
        "compile.expect_comma_paren.eol" => "A closing parenthesis is missing at the end of this line.",
        "compile.expect_point" => "Expected a point name here.",
        "compile.expect_def" => "A definition looks like “A B C = triangle”: expected a point name or “=”.",
        "compile.expect_expr" => "Expected a construction or an expression here.",
        "compile.expect_exponent" => "An exponent must be a number, as in dist(A, B)^2.",
        "compile.unexpected_token" => "“{tok}” is not expected here.",
        "compile.unexpected_token.eol" => "This line ends too early.",
        "compile.bad_char" => "The character “{tok}” cannot be used in a .geo program.",
        "compile.unknown_relation" => "“{tok}” is not a relation GeoSolver knows (try coll, perp, para, cyclic, cong or eqangle).",
        "compile.unknown_name" => "The point “{tok}” is used before it is defined.",
        "compile.unknown_construction" => "“{tok}” is not a known construction (for example midpoint, foot, circumcenter or meet).",
        "compile.arity" => "“{tok}” takes {expected} arguments, but {got} were given.",
        "compile.redefined" => "The name “{tok}” is defined twice.",
        "compile.bad_number" => "“{tok}” is not a valid number.",
        "compile.degenerate" => "No valid figure could be drawn: some points coincide, or a construction is impossible.",
        "compile.empty" => "The program is empty.",
        "compile.no_goal" => "Add a goal line, for example “prove perp(A, H, B, C)”.",
        "compile.point_one" => "“point:” defines exactly one point.",
        "compile.other" => "The program could not be compiled.",
        // export report
        "report.verdict.proved" => "Proved",
        "report.verdict.refuted" => "The statement is false",
        "report.verdict.holds-numerically" => "Holds numerically — no Euclidean proof",
        "report.verdict.not-proved" => "Not proved",
        "report.verdict.time_limit" => "Not proved — time limit reached",
        "report.explain.proved" => "Every numbered step below is machine-checked.",
        "report.explain.refuted" => "A sampled figure contradicts the goal, so it cannot be proved.",
        "report.explain.holds-numerically" => "The goal holds in {n} sampled figures, but no synthetic proof was found. That is evidence, not a proof.",
        "report.explain.not-proved" => "The search ended without a proof. The statement may still be true.",
        "report.explain.time_limit" => "The search stopped at the {secs} s time limit without finding a proof.",
        "report.given" => "Given",
        "report.prove" => "Prove",
        "report.proof" => "Proof — machine-checked steps",
        "report.aux" => "Auxiliary points added by the search",
        "report.counter" => "Counterexample",
        "report.footer" => "GeoSolver · deductive geometry prover",
        "report.steps" => "{n} steps",
        "report.default_title" => "Geometry problem",
        "fact.coll" => "{pts} are collinear",
        "fact.cyclic" => "{pts} are concyclic",
        "fact.midp" => "{m} is the midpoint of {seg}",
        "fact.circle" => "{o} is the circumcenter of △{tri}",
        "rule.given" => "given",
        "rule.construction" => "construction",
        "rule.aux" => "auxiliary construction",
        "aux.midpoint" => "midpoint of {0}{1}",
        "aux.circumcenter" => "circumcenter of △{0}{1}{2}",
        "aux.orthocenter" => "orthocenter of △{0}{1}{2}",
        "aux.foot" => "foot of the perpendicular from {0} to {1}",
        "aux.reflect" => "reflection of {0} in {1}",
        "aux.intersect" => "intersection of {0} and {1}",
        "aux.parallelogram" => "completes the parallelogram {0}{1}{2}",
        "aux.spiral_center" => "centre of the spiral similarity taking {0} to {1}",
        "aux.isogonal" => "isogonal conjugate of {0} in △{1}",
        "aux.antipode" => "antipode of {0}",
        "aux.arc_midpoint" => "midpoint of arc {0}{1}",
        "aux.bisector_foot" => "foot of the bisector from {0} in △{1}",
        "aux.circumcircle" => "circumcircle of △{0}{1}{2}",
        "aux.circle" => "circle ({0}, {0}{1})",
        "rule.similar" => "similar triangles",
        "rule.collinear" => "collinearity",
        "rule.concyclic" => "inscribed angles",
        "rule.eqradius" => "equal radii",
        "rule.coincide" => "coincident points",
        "rule.transfer" => "segment arithmetic",
        "rule.arcchord" => "equal arcs ⇔ equal chords",
        "rule.algebra" => "algebra",
        "rule.other" => "deduction",
        "counter.angle" => "In the sampled figure the angle between {a} and {b} is {lhs}°, not {rhs}°.",
        "counter.length" => "In the sampled figure {a} = {lhs} but {b} = {rhs}.",
        "counter.angles" => "In the sampled figure {a} = {lhs}° but {b} = {rhs}°.",
        "counter.values" => "In the sampled figure the left side is {lhs} and the right side is {rhs}.",
        "counter.off_line" => "In the sampled figure {a} is not on line {b}.",
        "counter.off_circle" => "In the sampled figure {a} is not on the circle through {b}.",
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
        // translate (structured)
        "translate.disabled" => "Traducerea AI nu este activată pe acest server.",
        "translate.no_problem" => "Traducătorul nu a putut transforma textul într-o problemă de geometrie. Reformulează sau scrie direct programul .geo.",
        "history.no_solution" => "Această intrare a fost salvată înainte ca rezultatele să fie păstrate; va fi rezolvată din nou.",
        "export.expired" => "Rezultatul nu mai este păstrat pe server. Rezolvă din nou pentru a-l exporta.",
        // compile errors
        "compile.expect_comma_paren" => "Lipsește o virgulă sau o paranteză închisă înainte de „{tok}”.",
        "compile.expect_comma_paren.eol" => "Lipsește o paranteză închisă la finalul acestui rând.",
        "compile.expect_point" => "Aici trebuie un nume de punct.",
        "compile.expect_def" => "O definiție arată ca „A B C = triangle”: se aștepta un nume de punct sau „=”.",
        "compile.expect_expr" => "Aici trebuie o construcție sau o expresie.",
        "compile.expect_exponent" => "Exponentul trebuie să fie un număr, ca în dist(A, B)^2.",
        "compile.unexpected_token" => "„{tok}” nu este așteptat aici.",
        "compile.unexpected_token.eol" => "Rândul se termină prea devreme.",
        "compile.bad_char" => "Caracterul „{tok}” nu poate fi folosit într-un program .geo.",
        "compile.unknown_relation" => "„{tok}” nu este o relație cunoscută (încearcă coll, perp, para, cyclic, cong sau eqangle).",
        "compile.unknown_name" => "Punctul „{tok}” este folosit înainte de a fi definit.",
        "compile.unknown_construction" => "„{tok}” nu este o construcție cunoscută (de exemplu midpoint, foot, circumcenter sau meet).",
        "compile.arity" => "„{tok}” primește {expected} argumente, dar au fost date {got}.",
        "compile.redefined" => "Numele „{tok}” este definit de două ori.",
        "compile.bad_number" => "„{tok}” nu este un număr valid.",
        "compile.degenerate" => "Nu s-a putut desena o figură validă: unele puncte coincid sau o construcție este imposibilă.",
        "compile.empty" => "Programul este gol.",
        "compile.no_goal" => "Adaugă un rând cu concluzia, de exemplu „prove perp(A, H, B, C)”.",
        "compile.point_one" => "„point:” definește exact un punct.",
        "compile.other" => "Programul nu a putut fi compilat.",
        // export report
        "report.verdict.proved" => "Demonstrat",
        "report.verdict.refuted" => "Afirmația este falsă",
        "report.verdict.holds-numerically" => "Adevărat numeric — fără demonstrație euclidiană",
        "report.verdict.not-proved" => "Nedemonstrat",
        "report.verdict.time_limit" => "Nedemonstrat — limita de timp a fost atinsă",
        "report.explain.proved" => "Fiecare pas numerotat de mai jos este verificat automat.",
        "report.explain.refuted" => "O figură eșantionată contrazice concluzia, deci ea nu poate fi demonstrată.",
        "report.explain.holds-numerically" => "Concluzia se verifică în {n} figuri eșantionate, dar nu s-a găsit o demonstrație sintetică. Este o dovadă numerică, nu o demonstrație.",
        "report.explain.not-proved" => "Căutarea s-a încheiat fără demonstrație. Afirmația poate fi totuși adevărată.",
        "report.explain.time_limit" => "Căutarea s-a oprit la limita de {secs} s fără a găsi o demonstrație.",
        "report.given" => "Ipoteze",
        "report.prove" => "De demonstrat",
        "report.proof" => "Demonstrație — pași verificați automat",
        "report.aux" => "Puncte auxiliare adăugate de căutare",
        "report.counter" => "Contraexemplu",
        "report.footer" => "GeoSolver · demonstrator deductiv de geometrie",
        "report.steps" => "{n} pași",
        "report.default_title" => "Problemă de geometrie",
        "fact.coll" => "{pts} sunt coliniare",
        "fact.cyclic" => "{pts} sunt conciclice",
        "fact.midp" => "{m} este mijlocul segmentului {seg}",
        "fact.circle" => "{o} este centrul cercului circumscris △{tri}",
        "rule.given" => "ipoteză",
        "rule.construction" => "construcție",
        "rule.aux" => "construcție auxiliară",
        "aux.midpoint" => "mijlocul segmentului {0}{1}",
        "aux.circumcenter" => "centrul cercului circumscris △{0}{1}{2}",
        "aux.orthocenter" => "ortocentrul △{0}{1}{2}",
        "aux.foot" => "piciorul perpendicularei din {0} pe {1}",
        "aux.reflect" => "simetricul lui {0} față de {1}",
        "aux.intersect" => "intersecția dintre {0} și {1}",
        "aux.parallelogram" => "completează paralelogramul {0}{1}{2}",
        "aux.spiral_center" => "centrul asemănării spirale care duce {0} în {1}",
        "aux.isogonal" => "conjugatul izogonal al lui {0} în △{1}",
        "aux.antipode" => "punctul diametral opus lui {0}",
        "aux.arc_midpoint" => "mijlocul arcului {0}{1}",
        "aux.bisector_foot" => "piciorul bisectoarei din {0} în △{1}",
        "aux.circumcircle" => "cercul circumscris △{0}{1}{2}",
        "aux.circle" => "cercul ({0}, {0}{1})",
        "rule.similar" => "triunghiuri asemenea",
        "rule.collinear" => "coliniaritate",
        "rule.concyclic" => "unghiuri înscrise",
        "rule.eqradius" => "raze egale",
        "rule.coincide" => "puncte confundate",
        "rule.transfer" => "aritmetica segmentelor",
        "rule.arcchord" => "arce egale ⇔ coarde egale",
        "rule.algebra" => "calcul",
        "rule.other" => "deducție",
        "counter.angle" => "În figura eșantionată unghiul dintre {a} și {b} este {lhs}°, nu {rhs}°.",
        "counter.length" => "În figura eșantionată {a} = {lhs}, dar {b} = {rhs}.",
        "counter.angles" => "În figura eșantionată {a} = {lhs}°, dar {b} = {rhs}°.",
        "counter.values" => "În figura eșantionată membrul stâng este {lhs}, iar cel drept {rhs}.",
        "counter.off_line" => "În figura eșantionată {a} nu se află pe dreapta {b}.",
        "counter.off_circle" => "În figura eșantionată {a} nu se află pe cercul prin {b}.",
        _ => "ceva n-a mers",
    }
}

/// [`t`] with `{name}` placeholders filled in.
pub fn tf(lang: Lang, key: &str, vars: &[(&str, String)]) -> String {
    let mut s = t(lang, key).to_string();
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// The sentence explaining a compile error, in the reader's language.
pub fn compile_message(lang: Lang, d: &crate::present::Diagnosis) -> String {
    let key = format!("compile.{}", d.key);
    let key = match (d.key, &d.token) {
        ("expect_comma_paren" | "unexpected_token", None) => format!("{key}.eol"),
        _ => key,
    };
    tf(
        lang,
        &key,
        &[
            ("tok", d.token.clone().unwrap_or_default()),
            ("expected", d.expected.map(|n| n.to_string()).unwrap_or_default()),
            ("got", d.got.map(|n| n.to_string()).unwrap_or_default()),
        ],
    )
}
