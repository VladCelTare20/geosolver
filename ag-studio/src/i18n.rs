//! Server-side message localization (English/Romanian).
//!
//! The language is taken from the `lang` cookie the browser sets (see
//! `assets/i18n.js`); absent or unrecognized, it defaults to English. Handlers
//! return `t(lang, key)` for every user-facing string so a Romanian visitor
//! sees Romanian errors too. Inherently-technical detail (parser/compiler
//! output) is passed through verbatim by the caller.

use axum::http::{header, HeaderMap};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
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
        "solve.empty" => "The program is empty.",
        "solve.failed" => "The prover stopped unexpectedly. Try again; if it happens again, simplify the problem.",
        // history
        "history.load_fail" => "Could not load your history. Try again.",
        "history.none" => "That history entry no longer exists.",
        "history.del_fail" => "Could not delete that history entry. Try again.",
        // translate
        "translate.too_long" => "The description is too long. Shorten it and try again.",
        "translate.bad_image" => "The image could not be read. Try a PNG, JPG or WebP photo.",
        "translate.need_input" => "Describe the problem or choose a photo first.",
        "translate.failed" => "The translation stopped unexpectedly. Try again, or write the .geo program directly.",
        // humanize
        "humanize.need_proof" => "There is no proof to explain.",
        "humanize.unavailable" => "AI explanations are not enabled on this server.",
        "humanize.failed" => "The explanation could not be written. Try again.",
        // export
        "export.failed" => "The export failed. Try again.",
        // shared / security
        "err.too_long" => "The input is too long. Shorten it and try again.",
        "err.too_large" => "This program is longer than the server accepts. Shorten it, or use fewer points.",
        "server.busy" => "Every solver slot is in use. Try again in a moment.",
        "server.busy_self" => "You already have as many solves running as one account may. Wait for one to finish, then try again.",
        "rate.exceeded" => "Too many requests in a short time. Wait a minute, then try again.",
        "origin.refused" => "This request came from another site and was refused.",
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
        "compile.unknown_shape" => "“{tok}” is not a known construction. Several points can be introduced at once only with triangle (A B C = triangle) or segment (A B = segment); define the others one at a time.",
        "compile.trailing" => "The equation should end before “{tok}”.",
        "compile.trailing.eol" => "The equation has extra text at the end.",
        "compile.degenerate_goal" => "The goal repeats the point {tok}, so it is degenerate or trivially true and proves nothing. Use distinct points.",
        "compile.arity" => "“{tok}” takes {expected}, but {got} given.",
        "compile.args.one" => "{n} argument",
        "compile.args.other" => "{n} arguments",
        "compile.given.one" => "was",
        "compile.given.other" => "were",
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
        "report.verdict.as_drawn" => "Proved for the configuration shown",
        "report.explain.as_drawn" => "Every numbered step below is machine-checked. One step uses the order of points as drawn, so the proof covers this configuration.",
        "report.explain.refuted" => "A sampled figure contradicts it, so no proof can exist.",
        "report.explain.holds-numerically.one" => "True in the sampled figure, but GeoSolver found no Euclidean proof. That is evidence, not a proof.",
        "report.explain.holds-numerically.other" => "True in all {n} sampled figures, but GeoSolver found no Euclidean proof. That is evidence, not a proof.",
        "report.explain.not-proved" => "The search ended without a proof. The statement may still be true.",
        "report.explain.budget" => "The search used its whole budget ({runs}) without finding a proof. The statement may still be true.",
        "report.explain.metric_error" => "GeoSolver cannot handle this kind of goal yet. This says nothing about whether the statement is true.",
        "report.explain.unsound" => "A derivation was found but rejected, because a sampled figure contradicts it (likely a degenerate figure). The statement may still be true.",
        "report.explain.replay" => "The search reported a proof, but its constructions could not be replayed. The statement may still be true.",
        "report.explain.time_limit" => "The search stopped at the {secs} s limit without finding a proof. The statement may still be true.",
        "report.runs.one" => "{n} deductive run",
        "report.runs.other" => "{n} deductive runs",
        "report.method.ddar" => "Deductive search",
        "report.method.aux.one" => "Deductive search + {n} auxiliary point",
        "report.method.aux.other" => "Deductive search + {n} auxiliary points",
        "report.method.aux_search" => "Deductive search + auxiliary-point search",
        "report.method.euclid" => "Euclidean theorems",
        "report.method.numeric" => "Numerical check",
        "report.method.counter" => "Numerical counterexample",
        "report.as_drawn" => "uses the drawn order of points",
        "fact.para_ratio" => "{par} and {ratio}",
        "report.given" => "Given",
        "report.prove" => "Prove",
        "report.proof" => "Proof — machine-checked steps",
        "report.aux" => "Auxiliary points added by the search",
        "report.counter" => "Counterexample",
        "report.footer" => "GeoSolver · machine-checked geometry proofs",
        "report.steps.one" => "{n} deduction step",
        "report.steps.other" => "{n} deduction steps",
        "report.samples.one" => "{n} sampled figure",
        "report.samples.other" => "{n} sampled figures",
        "fact.oncircle" => "{pts} lie on a circle centered at {o}",
        "fact.ratio_fixed" => "the ratio {a} : {b} is fixed",
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
        "aux.intersect2" => "second intersection of line {0} with {1}",
        "aux.parallelogram" => "completes the parallelogram {0}{1}{2}",
        "aux.spiral_center" => "center of the spiral similarity taking {0} to {1}",
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
        "rule.transfer" => "length ratios",
        "rule.arcchord" => "equal arcs ⇔ equal chords",
        "rule.algebra" | "rule.combine" => "combining relations",
        "rule.closure" => "deductive closure",
        "rule.trig" => "trigonometry",
        "rule.rearrange" => "equivalent form",
        "rule.area" => "area addition",
        "rule.segments" => "segment addition",
        "rule.other" => "deduction",
        "report.hyps" => "Steps {list} restate the hypotheses and constructions.",
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
        "solve.empty" => "Programul este gol.",
        "solve.failed" => "Motorul de demonstrare s-a oprit neașteptat. Încearcă din nou; dacă se repetă, simplifică problema.",
        // history
        "history.load_fail" => "Istoricul nu a putut fi încărcat. Încearcă din nou.",
        "history.none" => "Această intrare din istoric nu mai există.",
        "history.del_fail" => "Intrarea din istoric nu a putut fi ștearsă. Încearcă din nou.",
        // translate
        "translate.too_long" => "Descrierea este prea lungă. Scurteaz-o și încearcă din nou.",
        "translate.bad_image" => "Imaginea nu a putut fi citită. Încearcă o fotografie PNG, JPG sau WebP.",
        "translate.need_input" => "Descrie problema sau alege mai întâi o fotografie.",
        "translate.failed" => "Traducerea s-a oprit neașteptat. Încearcă din nou sau scrie direct programul .geo.",
        // humanize
        "humanize.need_proof" => "Nu există o demonstrație de explicat.",
        "humanize.unavailable" => "Explicațiile AI nu sunt activate pe acest server.",
        "humanize.failed" => "Explicația nu a putut fi scrisă. Încearcă din nou.",
        // export
        "export.failed" => "Exportul a eșuat. Încearcă din nou.",
        // shared / security
        "err.too_long" => "Textul introdus este prea lung. Scurtează-l și încearcă din nou.",
        "err.too_large" => "Programul este mai lung decât acceptă serverul. Scurtează-l sau folosește mai puține puncte.",
        "server.busy" => "Toate locurile de rezolvare sunt ocupate. Încearcă din nou în câteva momente.",
        "server.busy_self" => "Ai deja în curs câte rezolvări poate avea un cont. Așteaptă să se termine una, apoi încearcă din nou.",
        "rate.exceeded" => "Prea multe cereri într-un timp scurt. Așteaptă un minut, apoi încearcă din nou.",
        "origin.refused" => "Cererea a venit de pe alt site și a fost refuzată.",
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
        "compile.unknown_shape" => "„{tok}” nu este o construcție cunoscută. Mai multe puncte pot fi introduse deodată doar cu triangle (A B C = triangle) sau segment (A B = segment); definește-le pe celelalte pe rând.",
        "compile.trailing" => "Ecuația ar trebui să se încheie înainte de „{tok}”.",
        "compile.trailing.eol" => "Ecuația are text în plus la final.",
        "compile.degenerate_goal" => "Concluzia repetă punctul {tok}, deci este degenerată sau trivial adevărată și nu demonstrează nimic. Folosește puncte distincte.",
        "compile.arity" => "„{tok}” primește {expected}, dar a primit {got}.",
        "compile.args.one" => "{n} argument",
        "compile.args.few" => "{n} argumente",
        "compile.args.other" => "{n} de argumente",
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
        "report.verdict.as_drawn" => "Demonstrat pentru configurația din figură",
        "report.explain.as_drawn" => "Fiecare pas numerotat de mai jos este verificat automat. Un pas folosește ordinea punctelor din figură, deci demonstrația acoperă această configurație.",
        "report.explain.refuted" => "O figură eșantionată o contrazice, deci nu poate exista o demonstrație.",
        "report.explain.holds-numerically.one" => "Adevărat în figura eșantionată, dar GeoSolver nu a găsit o demonstrație euclidiană. Este un indiciu numeric, nu o demonstrație.",
        "report.explain.holds-numerically.few" => "Adevărat în toate cele {n} figuri eșantionate, dar GeoSolver nu a găsit o demonstrație euclidiană. Este un indiciu numeric, nu o demonstrație.",
        "report.explain.holds-numerically.other" => "Adevărat în toate cele {n} de figuri eșantionate, dar GeoSolver nu a găsit o demonstrație euclidiană. Este un indiciu numeric, nu o demonstrație.",
        "report.explain.not-proved" => "Căutarea s-a încheiat fără demonstrație. Afirmația poate fi totuși adevărată.",
        "report.explain.budget" => "Căutarea și-a folosit tot bugetul ({runs}) fără a găsi o demonstrație. Afirmația poate fi totuși adevărată.",
        "report.explain.metric_error" => "GeoSolver nu poate trata încă acest tip de concluzie. Asta nu spune nimic despre adevărul afirmației.",
        "report.explain.unsound" => "S-a găsit o derivare, dar a fost respinsă: o figură eșantionată o contrazice (probabil o figură degenerată). Afirmația poate fi totuși adevărată.",
        "report.explain.replay" => "Căutarea a raportat o demonstrație ale cărei construcții nu au putut fi reconstruite. Afirmația poate fi totuși adevărată.",
        "report.explain.time_limit" => "Căutarea s-a oprit la limita de {secs} s fără a găsi o demonstrație. Afirmația poate fi totuși adevărată.",
        "report.runs.one" => "{n} rulare deductivă",
        "report.runs.few" => "{n} rulări deductive",
        "report.runs.other" => "{n} de rulări deductive",
        "report.method.ddar" => "Căutare deductivă",
        "report.method.aux.one" => "Căutare deductivă + {n} punct auxiliar",
        "report.method.aux.few" => "Căutare deductivă + {n} puncte auxiliare",
        "report.method.aux.other" => "Căutare deductivă + {n} de puncte auxiliare",
        "report.method.aux_search" => "Căutare deductivă + căutare de puncte auxiliare",
        "report.method.euclid" => "Teoreme euclidiene",
        "report.method.numeric" => "Verificare numerică",
        "report.method.counter" => "Contraexemplu numeric",
        "report.as_drawn" => "folosește ordinea punctelor din figură",
        "fact.para_ratio" => "{par} și {ratio}",
        "report.given" => "Ipoteze",
        "report.prove" => "De demonstrat",
        "report.proof" => "Demonstrație — pași verificați automat",
        "report.aux" => "Puncte auxiliare adăugate de căutare",
        "report.counter" => "Contraexemplu",
        "report.footer" => "GeoSolver · demonstrații de geometrie verificate automat",
        "report.steps.one" => "{n} pas de deducție",
        "report.steps.few" => "{n} pași de deducție",
        "report.steps.other" => "{n} de pași de deducție",
        "report.samples.one" => "{n} figură eșantionată",
        "report.samples.few" => "{n} figuri eșantionate",
        "report.samples.other" => "{n} de figuri eșantionate",
        "fact.oncircle" => "{pts} se află pe un cerc cu centrul {o}",
        "fact.ratio_fixed" => "raportul {a} : {b} este fix",
        "report.default_title" => "Problemă de geometrie",
        "fact.coll" => "{pts} sunt coliniare",
        "fact.cyclic" => "{pts} sunt conciclice",
        "fact.midp" => "{m} este mijlocul segmentului {seg}",
        "fact.circle" => "{o} este centrul cercului circumscris triunghiului {tri}",
        "rule.given" => "ipoteză",
        "rule.construction" => "construcție",
        "rule.aux" => "construcție auxiliară",
        "aux.midpoint" => "mijlocul segmentului {0}{1}",
        "aux.circumcenter" => "centrul cercului circumscris triunghiului {0}{1}{2}",
        "aux.orthocenter" => "ortocentrul △{0}{1}{2}",
        "aux.foot" => "piciorul perpendicularei din {0} pe {1}",
        "aux.reflect" => "simetricul lui {0} față de {1}",
        "aux.intersect" => "intersecția dintre {0} și {1}",
        "aux.intersect2" => "a doua intersecție a dreptei {0} cu {1}",
        "aux.parallelogram" => "completează paralelogramul {0}{1}{2}",
        "aux.spiral_center" => "centrul asemănării spirale care duce {0} în {1}",
        "aux.isogonal" => "conjugatul izogonal al lui {0} în △{1}",
        "aux.antipode" => "punctul diametral opus lui {0}",
        "aux.arc_midpoint" => "mijlocul arcului {0}{1}",
        "aux.bisector_foot" => "piciorul bisectoarei din {0} în △{1}",
        "aux.circumcircle" => "cercul circumscris triunghiului {0}{1}{2}",
        "aux.circle" => "cercul ({0}, {0}{1})",
        "rule.similar" => "triunghiuri asemenea",
        "rule.collinear" => "coliniaritate",
        "rule.concyclic" => "unghiuri înscrise",
        "rule.eqradius" => "raze egale",
        "rule.coincide" => "puncte confundate",
        "rule.transfer" => "rapoarte de lungimi",
        "rule.arcchord" => "arce egale ⇔ coarde egale",
        "rule.algebra" | "rule.combine" => "combinarea relațiilor",
        "rule.closure" => "închidere deductivă",
        "rule.trig" => "trigonometrie",
        "rule.rearrange" => "formă echivalentă",
        "rule.area" => "aditivitatea ariei",
        "rule.segments" => "adunarea segmentelor",
        "rule.other" => "deducție",
        "report.hyps" => "Pașii {list} reiau ipotezele și construcțiile.",
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

/// CLDR plural category: `one`, `few` (Romanian 0 and 2–19 mod 100) or `other`.
pub fn plural_cat(lang: Lang, n: u64) -> &'static str {
    match lang {
        Lang::En if n == 1 => "one",
        Lang::En => "other",
        Lang::Ro if n == 1 => "one",
        Lang::Ro if n == 0 || (1..=19).contains(&(n % 100)) => "few",
        Lang::Ro => "other",
    }
}

/// `key.one` / `key.few` / `key.other` for `n`, with `{n}` and `vars` filled.
pub fn tp(lang: Lang, key: &str, n: u64, vars: &[(&str, String)]) -> String {
    let full = format!("{key}.{}", plural_cat(lang, n));
    let missing = t(lang, "__missing__");
    let template = match t(lang, &full) {
        x if x == missing => t(lang, &format!("{key}.other")),
        x => x,
    };
    let mut s = template.replace("{n}", &n.to_string());
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// The sentence explaining a compile error, in the reader's language.
pub fn compile_message(lang: Lang, d: &crate::present::Diagnosis) -> String {
    let key = format!("compile.{}", d.key);
    let key = match (d.key, &d.token) {
        ("expect_comma_paren" | "unexpected_token" | "trailing", None) => format!("{key}.eol"),
        _ => key,
    };
    let (expected, got) = (d.expected.unwrap_or(0) as u64, d.got.unwrap_or(0) as u64);
    let got_text = match lang {
        Lang::En => format!("{got} {}", tp(lang, "compile.given", got, &[])),
        Lang::Ro => tp(lang, "compile.args", got, &[]),
    };
    tf(
        lang,
        &key,
        &[
            ("tok", d.token.clone().unwrap_or_default()),
            ("expected", tp(lang, "compile.args", expected, &[])),
            ("got", got_text),
        ],
    )
}

const PROSE_RO: &[(&str, &str)] = &[
    ("Combining the equations above gives ", "Combinând egalitățile de mai sus obținem "),
    ("Combining the proportions above gives ", "Combinând proporțiile de mai sus obținem "),
    ("Multiplying the sine relations above gives ", "Înmulțind relațiile cu sinusuri de mai sus obținem "),
    ("Adding the relations above and dividing by ", "Adunând relațiile de mai sus și împărțind la "),
    ("Adding the relations above gives ", "Adunând relațiile de mai sus obținem "),
    ("Facts derived from the hypotheses by the deductive closure:", "Fapte deduse din ipoteze prin închiderea deductivă:"),
    ("From the sine relations below: ", "Din relațiile cu sinusuri de mai jos: "),
    ("Extended law of sines in circle ", "Teorema sinusurilor (forma extinsă) în cercul "),
    ("Law of sines in ", "Teorema sinusurilor în "),
    (". By the law of sines in ", ". Din teorema sinusurilor în "),
    ("Law of cosines in ", "Teorema cosinusului în "),
    ("Sine area formula: ", "Formula ariei cu sinus: "),
    ("Ratio lemma in ", "Lema rapoartelor în "),
    (" (With the other two ratio lemmas in ", " (Împreună cu celelalte două leme ale rapoartelor din "),
    (" this is trigonometric Ceva.)", " aceasta este teorema lui Ceva în formă trigonometrică.)"),
    (" with cevian ", " cu ceviana "),
    ("Power of the point ", "Puterea punctului "),
    (" inside the circle)", " în interiorul cercului)"),
    (" outside the circle)", " în exteriorul cercului)"),
    ("Multiply both sides by ", "Înmulțim ambii membri cu "),
    (", which is not 0 (distinct points in the figure).", ", care nu este 0 (puncte distincte în figură)."),
    (
        ", which is not 0: every angle in it is strictly between 0° and 180° in the figure (non-degenerate triangles).",
        ", care nu este 0: fiecare unghi din expresie este strict între 0° și 180° în figură (triunghiuri nedegenerate).",
    ),
    (" is convex in the figure, so its area splits along either diagonal: ", " este convex în figură, deci aria sa se descompune după oricare diagonală: "),
    ("; multiplying both sides by ", "; înmulțind ambii membri cu "),
    (" (both sides multiplied by ", " (ambii membri înmulțiți cu "),
    (" (derived from the hypotheses)", " (dedus din ipoteze)"),
    (" (derived below)", " (dedus mai jos)"),
    (" (as drawn)", " (ca în figură)"),
    (" (given), so ", " (ipoteză), deci "),
    (" (given)", " (ipoteză)"),
    (" (hypothesis)", " (ipoteză)"),
    ("Given: ", "Ipoteză: "),
    ("In right triangle ", "În triunghiul dreptunghic "),
    (" (right angle at ", " (unghi drept în "),
    (", by the Pythagorean theorem ", ", din teorema lui Pitagora, "),
    (" (the right angle at ", " (unghiul drept din "),
    (" is given)", " este ipoteză)"),
    ("By Apollonius's median theorem in triangle ", "Din teorema medianei în triunghiul "),
    (" with median ", " cu mediana "),
    ("By Stewart's theorem for the cevian ", "Din teorema lui Stewart pentru ceviana "),
    ("By the geometric-mean (altitude) relation in right triangle ", "Din teorema înălțimii în triunghiul dreptunghic "),
    ("By the geometric-mean (leg) relation in right triangle ", "Din teorema catetei în triunghiul dreptunghic "),
    ("By the power of the point ", "Din puterea punctului "),
    (" with respect to the circle", " față de cerc"),
    ("By the tangent–secant power of ", "Din puterea punctului "),
    (" (tangent ", " (tangenta "),
    (", secant ", ", secanta "),
    ("By the basic proportionality (intercept) theorem, since ", "Din teorema lui Thales, deoarece "),
    (", so by the angle-bisector theorem ", ", deci, din teorema bisectoarei, "),
    ("By the angle-bisector theorem (", "Din teorema bisectoarei ("),
    (" bisects ", " este bisectoarea "),
    ("By Menelaus's theorem for the transversal ", "Din teorema lui Menelaus pentru transversala "),
    ("Drop perpendiculars from ", "Ducem perpendicularele din "),
    (" onto the transversal ", " pe transversala "),
    (" is tangent at ", " este tangentă în "),
    (" is a secant: ", " este o secantă: "),
    (" (tangent–chord = inscribed angle)", " (unghiul dintre tangentă și coardă = unghiul înscris)"),
    (" — so triangles ", " — deci triunghiurile "),
    (" are similar (AA), giving ", " sunt asemenea (UU), de unde "),
    (" (AA): ", " (UU): "),
    (" (AA)", " (UU)"),
    (" are concyclic: ", " sunt conciclice: "),
    ("inscribed angles on the same arc", "unghiuri înscrise care subîntind același arc"),
    ("an exterior angle of a cyclic quadrilateral equals the interior opposite angle", "unghiul exterior al unui patrulater inscriptibil este egal cu unghiul interior opus"),
    ("vertical angles", "unghiuri opuse la vârf"),
    ("the same angle", "același unghi"),
    (" is a diameter of the circle, so ", " este diametru al cercului, deci "),
    (
        " (Thales' theorem — an angle in a semicircle is right)",
        " (teorema lui Thales — un unghi înscris într-un semicerc este drept)",
    ),
    (" be the point diametrically opposite ", " punctul diametral opus lui "),
    (" on the circle (so ", " pe cerc (deci "),
    (" is a diameter through the center ", " este un diametru care trece prin centrul "),
    (" are parallel chords of the circle, so they cut equal arcs and ", " sunt coarde paralele ale cercului, deci delimitează arce egale și "),
    (" (equal chords): ", " (coarde egale): "),
    (" of triangle ", " a triunghiului "),
    (" in triangle ", " în triunghiul "),
    (" be the intersection of ", " intersecția dreptelor "),
    (" be the reflection of ", " simetricul lui "),
    (" be the midpoint of ", " mijlocul segmentului "),
    (" be the foot of the perpendicular from ", " piciorul perpendicularei din "),
    (" be the circumcentre of ", " centrul cercului circumscris triunghiului "),
    (" be the circumcenter of ", " centrul cercului circumscris triunghiului "),
    (" be the second meet of line ", " a doua intersecție a dreptei "),
    (" be the point on ", " punctul de pe "),
    (" with the circle", " cu cercul"),
    (" is the midpoint of ", " este mijlocul segmentului "),
    (" are collinear", " sunt coliniare"),
    (" lies on ", " se află pe "),
    (" between ", " între "),
    (" (shared at ", " (comun în "),
    (" and the right angles at ", " și unghiurile drepte din "),
    ("  (two lines perpendicular to the same line are parallel)", "  (două drepte perpendiculare pe aceeași dreaptă sunt paralele)"),
    (" through ", " prin "),
    (" (since ", " (deoarece "),
    (" since ", " deoarece "),
    (" and in ", " și în "),
    (" gives ", " obținem "),
    (", with ", ", cu "),
    (" with ", " cu "),
    (" at ", " în "),
    (" on ", " pe "),
    (", so ", ", deci "),
    (", since ", ", deoarece "),
    ("Hence ", "Deci "),
    ("So ", "Deci "),
    ("Let ", "Fie "),
    (" and ", " și "),
];

/// The euclidean prover's English sentences, US spelling throughout.
pub fn prose_en(s: &str) -> String {
    s.replace("centre", "center")
}

/// The euclidean prover's English sentences in Romanian (its templates are a
/// closed set; anything unrecognized stays as written).
pub fn prose_ro(s: &str) -> String {
    let mut out = s.replace("centre", "center");
    for (en, ro) in PROSE_RO {
        out = out.replace(en, ro);
    }
    if out.contains("simetricul lui ") {
        out = out.replace(" in ", " față de ");
    }
    if out.contains("piciorul perpendicularei ") {
        out = out.replace(" to ", " pe ");
    }
    out
}

/// A cited theorem's Romanian name, for the theorems the provers cite.
pub fn theorem_ro(name: &str) -> Option<&'static str> {
    Some(match name.to_lowercase().as_str() {
        "stewart's theorem" => "teorema lui Stewart",
        "pythagorean theorem" => "teorema lui Pitagora",
        "apollonius's median theorem" => "teorema medianei",
        "menelaus's theorem" => "teorema lui Menelaus",
        "ceva's theorem" => "teorema lui Ceva",
        "angle-bisector theorem" => "teorema bisectoarei",
        "basic proportionality (intercept) theorem" => "teorema lui Thales",
        "geometric-mean (altitude) relation" => "teorema înălțimii",
        "geometric-mean (leg) relation" => "teorema catetei",
        "ptolemy's theorem" => "teorema lui Ptolemeu",
        "thales' theorem" => "teorema lui Thales",
        "angle bisector theorem" => "teorema bisectoarei",
        "angle bisector theorem (converse)" => "reciproca teoremei bisectoarei",
        "radical axis" => "axa radicală",
        "monge–d'alembert" => "teorema Monge–d'Alembert",
        "homothety at a center of similitude" | "homothety at a centre of similitude" => "omotetie cu centrul într-un centru de asemănare",
        "intercept theorem (parallel rungs)" => "teorema lui Thales (paralele)",
        "law of sines" => "teorema sinusurilor",
        "extended law of sines" => "teorema sinusurilor (forma extinsă)",
        "law of cosines" => "teorema cosinusului",
        "sine area formula" => "formula ariei cu sinus",
        "ratio lemma" => "lema rapoartelor",
        "trigonometric ceva" => "teorema lui Ceva (forma trigonometrică)",
        "power of a point" => "puterea punctului",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals_follow_each_language() {
        let steps = |l, n| tp(l, "report.steps", n, &[]);
        assert_eq!(steps(Lang::En, 1), "1 deduction step");
        assert_eq!(steps(Lang::En, 2), "2 deduction steps");
        assert_eq!(steps(Lang::Ro, 1), "1 pas de deducție");
        assert_eq!(steps(Lang::Ro, 2), "2 pași de deducție");
        assert_eq!(steps(Lang::Ro, 20), "20 de pași de deducție");
        assert_eq!(steps(Lang::Ro, 21), "21 de pași de deducție");
        assert_eq!(steps(Lang::Ro, 101), "101 pași de deducție");
        assert_eq!(tp(Lang::Ro, "report.runs", 5000, &[]), "5000 de rulări deductive");
        assert_eq!(tp(Lang::Ro, "report.samples", 48, &[]), "48 de figuri eșantionate");
    }

    #[test]
    fn arity_messages_agree_in_number() {
        let d = |e, g| crate::present::Diagnosis {
            key: "arity",
            line: 1,
            col: 1,
            len: 1,
            token: Some("midpoint".into()),
            expected: Some(e),
            got: Some(g),
        };
        assert_eq!(compile_message(Lang::En, &d(2, 1)), "“midpoint” takes 2 arguments, but 1 was given.");
        assert_eq!(compile_message(Lang::En, &d(1, 3)), "“midpoint” takes 1 argument, but 3 were given.");
        assert_eq!(compile_message(Lang::Ro, &d(2, 20)), "„midpoint” primește 2 argumente, dar a primit 20 de argumente.");
    }

    #[test]
    fn euclidean_prose_reads_in_romanian() {
        assert_eq!(prose_ro("|BC| = 6 (given), so BC² = 36."), "|BC| = 6 (ipoteză), deci BC² = 36.");
        let ro = prose_ro("Combining the equations above gives AD² = 14.");
        assert!(ro.starts_with("Combinând"), "{ro}");
        assert_eq!(theorem_ro("Stewart's theorem"), Some("teorema lui Stewart"));
    }
}
