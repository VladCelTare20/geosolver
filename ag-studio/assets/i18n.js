/* GeoSolver i18n — English/Romanian, client-side.
 *
 * Usage in markup:
 *   <span data-i18n="hero.h1">Olympiad geometry…</span>      → textContent
 *   <span data-i18n="x" data-i18n-html>…</span>              → innerHTML (trusted strings only)
 *   <input data-i18n-attr="placeholder:app.input_ph">        → attribute(s), ";"-separated
 *   <button data-lang-set="ro">RO</button>                    → sets language on click
 *   <button data-lang-toggle>EN·RO</button>                   → toggles language on click
 *   <el data-lang-only="ro">…</el>                            → shown only in that language
 *
 * Dynamic strings (JS): i18n.t('key', {name: 'X'}).
 * Re-render dynamic content on the 'langchange' event.
 *
 * Default is English; a saved choice (localStorage) wins and is mirrored to a
 * `lang` cookie so the backend answers in the same language.
 */
(function () {
  "use strict";

  var DICT = {
    en: {
      // — shared / nav —
      "nav.signin": "Sign in",
      "nav.create": "Create account",
      "nav.back": "← Back to GeoSolver",
      "lang.name": "EN",
      "lang.switch": "Schimbă în română",

      // — landing: hero —
      "hero.h1": "Geometry, proved while you watch.",
      "hero.lede": "Photograph or describe a geometry problem and GeoSolver returns a labeled figure and a complete, human-readable proof — powered by a from-scratch Rust implementation of the AlphaGeometry deduction engine.",
      "hero.cta_create": "Create a free account",
      "hero.cta_signin": "Sign in",
      "trust.ms": "proofs in milliseconds",
      "trust.local": "written in plain language",
      "trust.checked": "every step machine-checked",
      "cap.proven": "Proven ∎",
      "cap.text": "IMO 2023, Problem 2 — solved by GeoSolver in 0.94 s",

      // — landing: showcase —
      "show.title": "Watch it crack a real IMO problem.",
      "show.sub": "This is not a mock-up. GeoSolver proved IMO 2023 Problem 2 — a medal-level contest problem — discovering the key auxiliary constructions on its own. The full machine-checked proof is below.",
      "case.title": "IMO 2023 · Problem 2",
      "badge.hard": "olympiad hard",
      "badge.proven": "Proven ∎ in 0.94 s",
      "case.statement": "Let <em>ABC</em> be an acute-angled triangle with <em>AB</em> &lt; <em>AC</em>, and let <em>Ω</em> be its circumcircle. Let <em>S</em> be the midpoint of the arc <em>CB</em> of <em>Ω</em> containing <em>A</em>. The perpendicular from <em>A</em> to <em>BC</em> meets <em>BS</em> at <em>D</em> and meets <em>Ω</em> again at <em>E ≠ A</em>. The line through <em>D</em> parallel to <em>BC</em> meets line <em>BE</em> at <em>L</em>, and the circumcircle <em>ω</em> of triangle <em>BDL</em> meets <em>Ω</em> again at <em>P ≠ B</em>. <em>Prove that the tangent to ω at P meets line BS on the internal bisector of ∠BAC.</em>",
      "stat.wall_v": "0.94 s",
      "stat.wall": "to the first proof",
      "stat.steps": "steps in the final proof",
      "stat.aux": "auxiliary point needed",
      "stat.ddar": "DDAR runs",
      "show.insight": "<b>The search found the key constructions on its own.</b> To reason about the tangent at all, the engine names its meeting point with <code>BS</code> as <code>X</code> and hunts for auxiliary points that make the figure speak: in under a second it proved the theorem by extending <code>BO₁</code> and the tangent <code>XP</code> to their second intersections with <code>Ω</code>. A 30-second refinement pass then distilled the shortest proof it could find — 60 machine-checked steps resting on a single auxiliary point: the reflection of <code>P</code> across line <code>BS</code>.",
      "show.summary": "The full proof, exactly as GeoSolver wrote it",

      // — landing: features —
      "feat.title": "Everything a geometry student needs.",
      "feat.sub": "From a photo to a proof you can actually read — rigorous, and written in plain language.",
      "feat.photo.h": "Photo → proof",
      "feat.photo.p": "Snap a textbook problem or describe it in plain English; the built-in translator turns it into a solvable geometry program.",
      "feat.rigor.h": "Every step verified",
      "feat.rigor.p": "The engine machine-checks each deduction, so the proof is sound — not a plausible-looking guess.",
      "feat.proof.h": "Human-readable proof",
      "feat.proof.p": "Every proof is rewritten by Claude Opus into a clear, flowing argument — like a solutions manual, not a wall of symbols.",
      "feat.pdf.h": "PDF export",
      "feat.pdf.p": "Export a print-ready PDF or PNG report combining the figure and proof for homework, handouts, or study notes.",

      // — landing: how —
      "how.title": "Three steps, no setup.",
      "how.sub": "Solves stay in your private history so you can reopen any proof later.",
      "how.s1.h": "State the problem",
      "how.s1.p": "Type it in plain English, paste a <code>.geo</code> program, or upload a photo of the page.",
      "how.s2.h": "GeoSolver proves it",
      "how.s2.p": "Deductive closure plus an auxiliary-point search — the same recipe as AlphaGeometry, reimplemented in fast native Rust.",
      "how.s3.h": "Read and export",
      "how.s3.p": "Read the proof in plain language, check the labeled figure, and save a print-ready PDF.",

      // — landing: closing / footer —
      "close.h": "Your next proof is seconds away.",
      "close.p": "Free account, no email required — pick a username and start solving.",
      "close.cta": "Create a free account",
      "footer": "GeoSolver — a geometry proof assistant for students.",

      // — auth —
      "auth.doctitle": "Sign in · GeoSolver",
      "auth.welcome": "Welcome back",
      "auth.create_head": "Create your account",
      "auth.sub_login": "Sign in to solve, save, and revisit your proofs.",
      "auth.sub_register": "Free — no email required. Just pick a name and password.",
      "auth.username": "Username",
      "auth.password": "Password",
      "auth.user_hint": "3–32 characters: letters, digits, - or _",
      "auth.pass_hint": "At least 8 characters",
      "auth.err_user": "Please enter a username (3–32 characters).",
      "auth.err_pass": "Password must be at least 8 characters.",
      "auth.signed_in": "Signed in as {name}. Redirecting…",
      "auth.generic_err": "Something went wrong. Please try again.",
      "auth.net_err": "Could not reach the server. Please try again.",

      // — app: header / input —
      "app.tag": "type or photograph a geometry problem — get the labeled figure and a step-by-step proof",
      "app.ai_on": "AI translation on",
      "app.ai_signin": "sign in to enable AI",
      "app.ai_off": "AI translation off",
      "app.logout": "Log out",
      "app.problem": "Problem",
      "app.mode_describe": "Describe",
      "app.mode_geo": ".geo code",
      "app.figure": "Figure",
      "app.theme_light": "Light",
      "app.theme_dark": "Dark",
      "app.describe_label": "Describe the problem in words, or upload a photo of it:",
      "app.geo_label": "Write a .geo program:",
      "app.input_ph_describe": "e.g. In triangle ABC, H is the orthocenter. Prove the reflection of H over line BC lies on the circumcircle of ABC.",
      "app.input_ph_geo": "# a .geo program\nA B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))",
      "app.drop_text": "Drop a photo of the problem here, or click to choose",
      "app.drop_replace": "{name} — click to replace",
      "app.best": "Find the shortest proof",
      "app.best_hint": "— compares every proof it can reach in up to 20 seconds",
      "app.solve": "Solve",
      "app.clear": "Clear",

      // — app: output —
      "app.solution": "Solution",
      "app.empty": "Enter your problem and press <b>Solve</b>. The labeled figure and the step-by-step proof appear here.",
      "app.zoom_out": "Zoom out",
      "app.zoom_in": "Zoom in",
      "app.fit": "Fit to view",
      "app.fullscreen": "Fullscreen",
      "app.exit_fullscreen": "Exit fullscreen",
      "app.fig_hint": "scroll / pinch to zoom · drag to pan · double-tap to reset",
      "app.save_pdf": "Save PDF",
      "app.save_png": "Save PNG",
      "app.given": "Given",
      "app.prove": "Prove",
      "app.aux_label": "Auxiliary constructions the search introduced",
      "app.compiled_geo": "Compiled .geo program",
      "app.proof": "Proof.",
      "app.search_result": "Search result.",
      "app.writing_proof": "Writing a clean proof…",
      "app.proof_omitted": "No proof was found within the search budget.",
      "app.proof_fallback": "The proof writer is unavailable — showing the machine-verified steps.",

      // — app: dynamic status —
      "app.working": "Working…",
      "app.translating": "Translating…",
      "app.finding_shortest": "Finding the shortest proof…",
      "app.solving": "Solving…",
      "app.verdict_proven": "Proven ∎",
      "app.verdict_not": "Not proven",
      "app.method_ddar": "DDAR",
      "app.method_aux": "DDAR + aux search",
      "app.method_euclid": "Euclidean prover",
      "app.n_points": "{n} points",
      "app.n_steps": "{n} steps",
      "app.n_examined": "examined {n} constructions",
      "app.false_stmt": "The goal does not hold in the sampled figure — the statement appears to be <b>false</b>. The engine attempted it anyway.",

      // — app: errors —
      "app.err_write_geo": "Write a .geo program first.",
      "app.err_describe": "Describe the problem or upload a photo.",
      "app.err_translation": "Translation failed: {msg}",
      "app.err_cannot_translate": "The model could not turn this into a geometry problem:",
      "app.err_solve": "Solve error: {msg}",
      "app.err_generic": "Something went wrong: {msg}",
      "app.err_export": "Export failed: {msg}",
      "app.warn_login": "One-time setup: sign the Claude CLI into your subscription — run <code>claude auth login</code> in a terminal, then reload this page. (No API key — it uses your Claude subscription.) Meanwhile you can switch to <b>.geo code</b> and solve directly.",
      "app.warn_install": "AI translation needs the Claude CLI. Run <code>npm i -g @anthropic-ai/claude-code</code>, then <code>claude auth login</code>, then reload — or switch to <b>.geo code</b> and write the program directly.",

      // — app: history —
      "app.history": "History",
      "app.loading": "Loading…",
      "app.no_solves": "No solves yet.",
      "app.untitled": "Untitled",
      "app.proven": "Proven",
      "app.not_proven": "Not proven",
      "app.reopen": "Reopen: {label}",
      "app.delete": "Delete",
      "app.del_fail": "Could not delete that entry.",
      "app.footer": "GeoSolver · a from-scratch Rust implementation of the AlphaGeometry deduction engine",
    },

    ro: {
      // — shared / nav —
      "nav.signin": "Autentificare",
      "nav.create": "Creează cont",
      "nav.back": "← Înapoi la GeoSolver",
      "lang.name": "RO",
      "lang.switch": "Switch to English",

      // — landing: hero —
      "hero.h1": "Geometrie, demonstrată sub ochii tăi.",
      "hero.lede": "Fotografiază sau descrie o problemă de geometrie, iar GeoSolver îți întoarce o figură etichetată și o demonstrație completă, pe înțelesul tău — totul pornit de la o implementare în Rust, scrisă de la zero, a motorului de deducție AlphaGeometry.",
      "hero.cta_create": "Creează un cont gratuit",
      "hero.cta_signin": "Autentificare",
      "trust.ms": "demonstrații în milisecunde",
      "trust.local": "scrise pe înțelesul tău",
      "trust.checked": "fiecare pas verificat automat",
      "cap.proven": "Demonstrat ∎",
      "cap.text": "IMO 2023, Problema 2 — rezolvată de GeoSolver în 0,94 s",

      // — landing: showcase —
      "show.title": "Vezi-l cum rezolvă o problemă reală de la IMO.",
      "show.sub": "Nu este o simulare. GeoSolver a demonstrat Problema 2 de la IMO 2023 — o problemă de concurs de nivel de medalie — descoperind singur construcțiile auxiliare cheie. Demonstrația completă, verificată automat, este mai jos.",
      "case.title": "IMO 2023 · Problema 2",
      "badge.hard": "nivel olimpiadă",
      "badge.proven": "Demonstrat ∎ în 0,94 s",
      "case.statement": "Fie <em>ABC</em> un triunghi ascuțitunghic cu <em>AB</em> &lt; <em>AC</em> și fie <em>Ω</em> cercul său circumscris. Fie <em>S</em> mijlocul arcului <em>CB</em> al lui <em>Ω</em> care îl conține pe <em>A</em>. Perpendiculara din <em>A</em> pe <em>BC</em> intersectează <em>BS</em> în <em>D</em> și intersectează <em>Ω</em> a doua oară în <em>E ≠ A</em>. Paralela prin <em>D</em> la <em>BC</em> intersectează dreapta <em>BE</em> în <em>L</em>, iar cercul circumscris <em>ω</em> al triunghiului <em>BDL</em> intersectează <em>Ω</em> a doua oară în <em>P ≠ B</em>. <em>Demonstrați că tangenta la ω în P intersectează dreapta BS pe bisectoarea interioară a unghiului ∠BAC.</em>",
      "stat.wall_v": "0,94 s",
      "stat.wall": "până la prima demonstrație",
      "stat.steps": "pași în demonstrația finală",
      "stat.aux": "punct auxiliar necesar",
      "stat.ddar": "rulări DDAR",
      "show.insight": "<b>Căutarea a descoperit singură construcțiile cheie.</b> Ca să poată raționa despre tangentă, motorul îi dă un nume punctului în care aceasta întâlnește <code>BS</code> — <code>X</code> — și caută puncte auxiliare care să facă figura să vorbească: în mai puțin de o secundă a demonstrat teorema prelungind <code>BO₁</code> și tangenta <code>XP</code> până la a doua lor intersecție cu <code>Ω</code>. O trecere de rafinare de 30 de secunde a distilat apoi cea mai scurtă demonstrație găsită — 60 de pași verificați automat, sprijiniți pe un singur punct auxiliar: reflexia lui <code>P</code> față de dreapta <code>BS</code>.",
      "show.summary": "Demonstrația completă, exact așa cum a scris-o GeoSolver",

      // — landing: features —
      "feat.title": "Tot ce îi trebuie unui elev la geometrie.",
      "feat.sub": "De la o fotografie la o demonstrație pe care chiar o poți citi — riguroasă și scrisă pe înțelesul tău.",
      "feat.photo.h": "Fotografie → demonstrație",
      "feat.photo.p": "Fotografiază o problemă din manual sau descrie-o în cuvinte; traducătorul integrat o transformă într-un program de geometrie ce poate fi rezolvat.",
      "feat.rigor.h": "Fiecare pas verificat",
      "feat.rigor.p": "Motorul verifică automat fiecare deducție, așa că demonstrația este corectă — nu o presupunere care doar pare plauzibilă.",
      "feat.proof.h": "Demonstrație pe înțelesul tău",
      "feat.proof.p": "Fiecare demonstrație este rescrisă de Claude Opus într-un raționament clar și curgător — ca într-o culegere cu rezolvări, nu un șir de simboluri.",
      "feat.pdf.h": "Export PDF",
      "feat.pdf.p": "Exportă un raport PDF sau PNG gata de tipărit, care îmbină figura și demonstrația — pentru teme, fișe sau notițe.",

      // — landing: how —
      "how.title": "Trei pași, fără nicio configurare.",
      "how.sub": "Rezolvările rămân în istoricul tău privat, ca să poți redeschide oricând orice demonstrație.",
      "how.s1.h": "Enunță problema",
      "how.s1.p": "Scrie-o în cuvinte, lipește un program <code>.geo</code> sau încarcă o fotografie a paginii.",
      "how.s2.h": "GeoSolver o demonstrează",
      "how.s2.p": "Închidere deductivă plus o căutare de puncte auxiliare — aceeași rețetă ca AlphaGeometry, reimplementată în Rust nativ și rapid.",
      "how.s3.h": "Citește și exportă",
      "how.s3.p": "Citește demonstrația în cuvinte simple, verifică figura etichetată și salvează un PDF gata de tipărit.",

      // — landing: closing / footer —
      "close.h": "Următoarea ta demonstrație e la câteva secunde distanță.",
      "close.p": "Cont gratuit, fără e-mail — alege un nume de utilizator și începe să rezolvi.",
      "close.cta": "Creează un cont gratuit",
      "footer": "GeoSolver — un asistent de demonstrații de geometrie pentru elevi.",

      // — auth —
      "auth.doctitle": "Autentificare · GeoSolver",
      "auth.welcome": "Bine ai revenit",
      "auth.create_head": "Creează-ți contul",
      "auth.sub_login": "Autentifică-te ca să rezolvi, să salvezi și să revii la demonstrațiile tale.",
      "auth.sub_register": "Gratuit — fără e-mail. Alege doar un nume și o parolă.",
      "auth.username": "Nume de utilizator",
      "auth.password": "Parolă",
      "auth.user_hint": "3–32 de caractere: litere, cifre, - sau _",
      "auth.pass_hint": "Cel puțin 8 caractere",
      "auth.err_user": "Introdu un nume de utilizator (3–32 de caractere).",
      "auth.err_pass": "Parola trebuie să aibă cel puțin 8 caractere.",
      "auth.signed_in": "Autentificat ca {name}. Redirecționare…",
      "auth.generic_err": "Ceva n-a mers. Încearcă din nou.",
      "auth.net_err": "Serverul nu poate fi contactat. Încearcă din nou.",

      // — app: header / input —
      "app.tag": "scrie sau fotografiază o problemă de geometrie — primești figura etichetată și o demonstrație pas cu pas",
      "app.ai_on": "traducere AI activă",
      "app.ai_signin": "autentifică Claude pentru AI",
      "app.ai_off": "traducere AI dezactivată",
      "app.logout": "Deconectare",
      "app.problem": "Problemă",
      "app.mode_describe": "Descrie",
      "app.mode_geo": "cod .geo",
      "app.figure": "Figură",
      "app.theme_light": "Deschisă",
      "app.theme_dark": "Întunecată",
      "app.describe_label": "Descrie problema în cuvinte sau încarcă o fotografie a ei:",
      "app.geo_label": "Scrie un program .geo:",
      "app.input_ph_describe": "ex.: În triunghiul ABC, H este ortocentrul. Arată că reflexia lui H față de dreapta BC se află pe cercul circumscris triunghiului ABC.",
      "app.input_ph_geo": "# un program .geo\nA B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))",
      "app.drop_text": "Trage aici o fotografie a problemei sau apasă pentru a alege",
      "app.drop_replace": "{name} — apasă pentru a înlocui",
      "app.best": "Găsește cea mai scurtă demonstrație",
      "app.best_hint": "— compară toate demonstrațiile posibile în cel mult 20 de secunde",
      "app.solve": "Rezolvă",
      "app.clear": "Șterge",

      // — app: output —
      "app.solution": "Soluție",
      "app.empty": "Introdu problema și apasă <b>Rezolvă</b>. Figura etichetată și demonstrația pas cu pas apar aici.",
      "app.zoom_out": "Micșorează",
      "app.zoom_in": "Mărește",
      "app.fit": "Încadrează în vizor",
      "app.fullscreen": "Tot ecranul",
      "app.exit_fullscreen": "Ieși din modul tot ecranul",
      "app.fig_hint": "derulează / apropie degetele pentru zoom · trage pentru a deplasa · dublu-atinge pentru resetare",
      "app.save_pdf": "Salvează PDF",
      "app.save_png": "Salvează PNG",
      "app.given": "Ipoteze",
      "app.prove": "De demonstrat",
      "app.aux_label": "Construcțiile auxiliare introduse de căutare",
      "app.compiled_geo": "Programul .geo compilat",
      "app.proof": "Demonstrație.",
      "app.search_result": "Rezultatul căutării.",
      "app.writing_proof": "Se redactează o demonstrație clară…",
      "app.proof_omitted": "Nu s-a găsit nicio demonstrație în bugetul de căutare.",
      "app.proof_fallback": "Redactorul de demonstrații nu este disponibil — se afișează pașii verificați automat.",

      // — app: dynamic status —
      "app.working": "Se lucrează…",
      "app.translating": "Se traduce…",
      "app.finding_shortest": "Caut cea mai scurtă demonstrație…",
      "app.solving": "Se rezolvă…",
      "app.verdict_proven": "Demonstrat ∎",
      "app.verdict_not": "Nedemonstrat",
      "app.method_ddar": "DDAR",
      "app.method_aux": "DDAR + căutare auxiliară",
      "app.method_euclid": "Demonstrator euclidian",
      "app.n_points": "{n} puncte",
      "app.n_steps": "{n} pași",
      "app.n_examined": "{n} construcții examinate",
      "app.false_stmt": "Enunțul nu se verifică în figura eșantionată — afirmația pare <b>falsă</b>. Motorul a încercat totuși.",

      // — app: errors —
      "app.err_write_geo": "Scrie mai întâi un program .geo.",
      "app.err_describe": "Descrie problema sau încarcă o fotografie.",
      "app.err_translation": "Traducerea a eșuat: {msg}",
      "app.err_cannot_translate": "Modelul nu a putut transforma asta într-o problemă de geometrie:",
      "app.err_solve": "Eroare la rezolvare: {msg}",
      "app.err_generic": "Ceva n-a mers: {msg}",
      "app.err_export": "Exportul a eșuat: {msg}",
      "app.warn_login": "Configurare unică: autentifică CLI-ul Claude în abonamentul tău — rulează <code>claude auth login</code> într-un terminal, apoi reîncarcă pagina. (Fără cheie API — folosește abonamentul tău Claude.) Între timp poți trece la <b>cod .geo</b> și rezolva direct.",
      "app.warn_install": "Traducerea AI are nevoie de CLI-ul Claude. Rulează <code>npm i -g @anthropic-ai/claude-code</code>, apoi <code>claude auth login</code>, apoi reîncarcă — sau treci la <b>cod .geo</b> și scrie programul direct.",

      // — app: history —
      "app.history": "Istoric",
      "app.loading": "Se încarcă…",
      "app.no_solves": "Încă nicio rezolvare.",
      "app.untitled": "Fără titlu",
      "app.proven": "Demonstrat",
      "app.not_proven": "Nedemonstrat",
      "app.reopen": "Redeschide: {label}",
      "app.delete": "Șterge",
      "app.del_fail": "Nu am putut șterge intrarea.",
      "app.footer": "GeoSolver · o implementare în Rust, scrisă de la zero, a motorului de deducție AlphaGeometry",
    },
  };

  function stored() {
    try { return localStorage.getItem("lang"); } catch (e) { return null; }
  }
  function current() {
    var l = stored();
    return l === "ro" ? "ro" : "en"; // English-first default
  }
  function setCookie(l) {
    try { document.cookie = "lang=" + l + ";path=/;max-age=31536000;samesite=lax"; } catch (e) {}
  }
  function t(key, vars) {
    var l = current();
    var s = (DICT[l] && DICT[l][key]);
    if (s == null) s = (DICT.en && DICT.en[key]);
    if (s == null) s = key;
    if (vars) for (var k in vars) s = s.split("{" + k + "}").join(vars[k]);
    return s;
  }

  function applyLang(l) {
    if (l !== "en" && l !== "ro") l = "en";
    try { localStorage.setItem("lang", l); } catch (e) {}
    setCookie(l);
    document.documentElement.lang = l;
    document.documentElement.setAttribute("data-lang", l);

    document.querySelectorAll("[data-i18n]").forEach(function (el) {
      var v = t(el.getAttribute("data-i18n"));
      if (el.hasAttribute("data-i18n-html")) el.innerHTML = v;
      else el.textContent = v;
    });
    document.querySelectorAll("[data-i18n-attr]").forEach(function (el) {
      el.getAttribute("data-i18n-attr").split(";").forEach(function (pair) {
        var i = pair.indexOf(":");
        if (i < 0) return;
        var attr = pair.slice(0, i).trim();
        var key = pair.slice(i + 1).trim();
        if (attr && key) el.setAttribute(attr, t(key));
      });
    });
    document.querySelectorAll("[data-lang-only]").forEach(function (el) {
      el.hidden = el.getAttribute("data-lang-only") !== l;
    });
    document.querySelectorAll("[data-lang-set]").forEach(function (b) {
      b.setAttribute("aria-pressed", b.getAttribute("data-lang-set") === l ? "true" : "false");
    });
    document.querySelectorAll("[data-lang-label]").forEach(function (b) {
      b.textContent = l === "en" ? "RO" : "EN";
      b.setAttribute("title", t("lang.switch"));
      b.setAttribute("aria-label", t("lang.switch"));
    });

    document.dispatchEvent(new CustomEvent("langchange", { detail: { lang: l } }));
    document.documentElement.style.visibility = ""; // reveal (FOUC guard)
  }
  function toggle() { applyLang(current() === "en" ? "ro" : "en"); }

  window.i18n = { t: t, applyLang: applyLang, toggle: toggle, current: current };

  function init() {
    document.querySelectorAll("[data-lang-set]").forEach(function (b) {
      b.addEventListener("click", function () { applyLang(b.getAttribute("data-lang-set")); });
    });
    document.querySelectorAll("[data-lang-toggle]").forEach(function (b) {
      b.addEventListener("click", toggle);
    });
    applyLang(current());
  }
  if (document.readyState !== "loading") init();
  else document.addEventListener("DOMContentLoaded", init);
})();
