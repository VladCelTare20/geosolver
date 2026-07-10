//! Photo / natural-language → `.geo` translation by driving the local `claude`
//! CLI in headless mode.
//!
//! This uses the Claude **subscription** (the CLI's stored OAuth credential) —
//! never an `ANTHROPIC_API_KEY`. If the CLI is not installed or not signed in,
//! translation is unavailable and callers fall back to manual entry; the rest
//! of the app (solving, drawing, PDF/PNG) works regardless.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use wait_timeout::ChildExt;

/// The `.geo` grammar taught to the model, bundled into the binary.
const GRAMMAR: &str = include_str!("../prompts/geo_grammar.md");

/// The bundled `.geo` grammar, exposed so other surfaces (the MCP server) can
/// teach it to a model too.
pub fn grammar() -> &'static str {
    GRAMMAR
}

/// A problem to translate.
pub enum Source {
    /// A free-text problem statement.
    Text(String),
    /// An image file on disk (PNG/JPG/…); the model reads it via the Read tool.
    Image(PathBuf),
}

/// The result of a translation.
#[derive(Clone, serde::Serialize)]
pub struct Translation {
    /// The `.geo` program.
    pub geo: String,
    /// A short title parsed from the program's leading `# comment`, if any.
    pub title: Option<String>,
}

/// Locate the `claude` executable once. Honours `AGSTUDIO_CLAUDE_BIN`, then the
/// usual names/locations. Returns `None` if the CLI is not installed.
pub fn claude_bin() -> Option<PathBuf> {
    static BIN: OnceLock<Option<PathBuf>> = OnceLock::new();
    BIN.get_or_init(find_claude).clone()
}

/// Is the `claude` CLI present (translation can be attempted)?
pub fn available() -> bool {
    claude_bin().is_some()
}

/// Installation + sign-in status of the translation backend.
#[derive(Clone, Copy, serde::Serialize)]
pub struct Status {
    /// The `claude` CLI is installed.
    pub installed: bool,
    /// The CLI is signed in (subscription OAuth present) — translation will work.
    pub logged_in: bool,
}

/// Report whether translation is installed and signed in.
pub fn status() -> Status {
    let installed = claude_bin().is_some();
    Status {
        installed,
        logged_in: installed && detect_login(),
    }
}

/// Check whether the `claude` CLI is signed in, authoritatively via
/// `claude auth status` (an auth env var also counts). Advisory only — the real
/// gate is the translate call itself.
fn detect_login() -> bool {
    if std::env::var_os("CLAUDE_CODE_OAUTH_TOKEN").is_some()
        || std::env::var_os("ANTHROPIC_API_KEY").is_some()
    {
        return true;
    }
    let Some(bin) = claude_bin() else {
        return false;
    };
    let mut cmd = Command::new(&bin);
    cmd.arg("auth")
        .arg("status")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    match run_with_timeout(cmd, Duration::from_secs(15)) {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            s.contains("\"loggedIn\": true") || s.contains("\"loggedIn\":true")
        }
        Err(_) => false,
    }
}

fn find_claude() -> Option<PathBuf> {
    // The npm `claude` / `claude.cmd` entries are shims; the real native binary
    // lives under node_modules. Spawning the real .exe avoids cmd.exe
    // metacharacter breakage on Windows when passing large prompts, so prefer it.
    const NESTED: &str = "node_modules/@anthropic-ai/claude-code/bin/claude.exe";
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("AGSTUDIO_CLAUDE_BIN") {
        candidates.push(PathBuf::from(p));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(Path::new(&appdata).join("npm").join(NESTED));
        candidates.push(Path::new(&appdata).join("npm").join("claude.cmd"));
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        candidates.push(Path::new(&home).join(".local/bin").join(NESTED));
        candidates.push(Path::new(&home).join(".local/bin/claude.exe"));
        candidates.push(Path::new(&home).join(".local/bin/claude"));
    }
    // Fall back to PATH-resolved names.
    for name in ["claude.exe", "claude.cmd", "claude"] {
        candidates.push(PathBuf::from(name));
    }
    candidates.into_iter().find(|c| probe(c))
}

/// Write the bundled grammar to a temp file once and return its path, so it can
/// be passed via `--append-system-prompt-file` instead of as a huge,
/// metacharacter-laden command-line argument.
fn grammar_file() -> Option<PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        // A private, randomly-named temp file (created O_EXCL, mode 0600) avoids
        // the symlink race a fixed name would allow on a shared host. Persisted
        // for the process lifetime — the CLI subprocess reads it by path.
        let mut f = tempfile::Builder::new()
            .prefix("agstudio_grammar_")
            .suffix(".md")
            .tempfile()
            .ok()?;
        f.write_all(GRAMMAR.as_bytes()).ok()?;
        f.flush().ok()?;
        f.into_temp_path().keep().ok()
    })
    .clone()
}

/// Does `<bin> --version` run successfully?
fn probe(bin: &Path) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Translate a problem into a `.geo` program using the local Claude subscription.
pub fn translate(source: &Source) -> Result<Translation> {
    let bin = claude_bin().ok_or_else(|| {
        anyhow!(
            "the `claude` CLI was not found. Install it with \
             `npm i -g @anthropic-ai/claude-code`, sign in with your subscription \
             (run `claude`, then `/login`), or set AGSTUDIO_CLAUDE_BIN to its path."
        )
    })?;

    let prompt = match source {
        Source::Text(t) => {
            format!("Translate this geometry problem into a .geo program:\n\n{t}")
        }
        Source::Image(p) => format!(
            "Read the geometry problem in the image file at {} and translate it into a \
             .geo program. Output only the single ```geo code block.",
            p.display().to_string().replace('\\', "/")
        ),
    };

    let mut cmd = Command::new(&bin);
    cmd.arg("-p").arg(&prompt).arg("--strict-mcp-config");
    match grammar_file() {
        Some(path) => {
            cmd.arg("--append-system-prompt-file").arg(path);
        }
        None => {
            cmd.arg("--append-system-prompt").arg(GRAMMAR);
        }
    }
    if let Ok(model) = std::env::var("AGSTUDIO_CLAUDE_MODEL") {
        cmd.arg("--model").arg(model);
    }
    if let Source::Image(_) = source {
        // Allow reading the image and auto-approve so it never blocks headless.
        cmd.arg("--allowedTools")
            .arg("Read")
            .arg("--permission-mode")
            .arg("acceptEdits");
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = run_with_timeout(cmd, Duration::from_secs(75)).context("running the claude CLI")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        bail!("claude exited unsuccessfully: {}", err.trim());
    }
    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    // The CLI prints a login prompt (with exit 0) when not signed in.
    if raw.contains("Not logged in") || raw.contains("/login") || raw.contains("auth login") {
        bail!(
            "the `claude` CLI is not signed in. Run `claude auth login` (it uses your Claude \
             subscription — no API key), then retry."
        );
    }
    let geo = extract_geo(&raw)
        .ok_or_else(|| anyhow!("no .geo code block found in the model's reply:\n{}", raw.trim()))?;
    let title = extract_title(&geo);
    Ok(Translation { geo, title })
}

/// Spawn a command, capturing stdout/stderr on reader threads (so a full pipe
/// buffer can never deadlock the wait), and enforce a wall-clock timeout.
fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<Output> {
    let mut child = cmd.spawn()?;
    let out_reader = child.stdout.take().map(reader_thread);
    let err_reader = child.stderr.take().map(reader_thread);

    let status = match child.wait_timeout(timeout)? {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            bail!("claude timed out after {}s", timeout.as_secs());
        }
    };
    let stdout = out_reader.map(join).unwrap_or_default();
    let stderr = err_reader.map(join).unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn reader_thread<R: Read + Send + 'static>(mut r: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        buf
    })
}

fn join(h: std::thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    h.join().unwrap_or_default()
}

/// Pull the `.geo` program out of a model reply: prefer a ```geo fenced block,
/// then any fenced block, then the raw text if it contains a `prove` line.
fn extract_geo(text: &str) -> Option<String> {
    if let Some(b) = fenced_block(text, Some("geo")) {
        return Some(b);
    }
    if let Some(b) = fenced_block(text, None) {
        return Some(b);
    }
    if text.lines().any(|l| l.trim_start().starts_with("prove ")) {
        return Some(text.trim().to_string());
    }
    None
}

/// Extract the contents of the first fenced code block. If `tag` is given, only
/// a fence opening with that tag matches; otherwise any ``` fence matches.
fn fenced_block(text: &str, tag: Option<&str>) -> Option<String> {
    let mut lines = text.lines();
    let mut inside = false;
    let mut collected: Vec<&str> = Vec::new();
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if !inside {
            if let Some(rest) = trimmed.strip_prefix("```") {
                let matches = match tag {
                    Some(t) => rest.trim().eq_ignore_ascii_case(t),
                    None => true,
                };
                if matches {
                    inside = true;
                }
            }
        } else if trimmed.starts_with("```") {
            let body = collected.join("\n").trim().to_string();
            return if body.is_empty() { None } else { Some(body) };
        } else {
            collected.push(line);
        }
    }
    None
}

/// Parse a title from the program's first `# comment` line.
fn extract_title(geo: &str) -> Option<String> {
    for line in geo.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix('#') {
            let title = rest.trim();
            if !title.is_empty() && !title.to_lowercase().starts_with("cannot translate") {
                return Some(title.to_string());
            }
        } else if !t.is_empty() {
            break; // a non-comment statement before any title comment
        }
    }
    None
}

/// Output language for a humanized proof.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Ro,
}

impl Lang {
    pub fn from_code(s: &str) -> Lang {
        if s.eq_ignore_ascii_case("ro") {
            Lang::Ro
        } else {
            Lang::En
        }
    }
}

/// System prompt: turn a machine (DDAR) proof into a flowing, textbook-style
/// English proof — like a solutions manual, not a line-by-line transcript.
const HUMANIZE_SYSTEM_EN: &str = "You are a mathematician writing the solution to an olympiad \
    geometry problem for a printed solutions manual. You are given a machine-generated proof (from \
    the GeoSolver / AlphaGeometry DDAR engine) and must rewrite it as ONE clean, flowing proof in \
    natural mathematical prose, the way a human author would.\n\
    Rules:\n\
    - Write in English. Begin with the word 'Solution.' then the proof.\n\
    - Flowing prose in full sentences. NO step numbers, NO lists of points after each statement, \
    NO citation brackets like [002 & 004], NO grading marks like '3p'.\n\
    - Be concise: merge trivial steps and foreground the key ideas, especially any auxiliary \
    construction. Aim for a short, readable argument, not a step-by-step transcript.\n\
    - Stay faithful to the given machine proof's logic; do not invent a different proof or \
    unstated facts. If the machine added auxiliary points, introduce them naturally ('Let ω be \
    the circumcircle of ABC, and let X be its second intersection with line BQ ...').\n\
    - Use proper mathematical typography with Unicode symbols: ∠ for angles, △ for \
    triangles, ∥ parallel, ⊥ perpendicular, ° degrees, ⇒, and fraction/ratio \
    notation. End the proof with ∎.\n\
    - Format with lightweight Markdown: wrap point names and short math expressions in *single \
    asterisks* for italics (e.g. *ABC*, *∠PP₁C = ∠BAC*), and use **double asterisks** \
    sparingly for one key phrase. Separate paragraphs with a blank line. Do NOT use headings, code \
    fences, or LaTeX.\n\
    - Machine notation to decode (do NOT echo it): coll = collinear, cyclic = concyclic, eqangle = \
    equal angles, para = parallel, perp = perpendicular, cong = equal segments, midp = midpoint; \
    auxiliary points the solver introduced carry textbook-style names (M, O, P′, B′ …; \
    auxₙ only as a fallback) — keep those names.\n\
    Output only the proof.";

/// Same, in Romanian — matching a Romanian textbook's register and diacritics.
const HUMANIZE_SYSTEM_RO: &str = "Ești matematician și scrii soluția unei probleme de \
    geometrie de olimpiadă pentru o culegere tipărită cu rezolvări. Primești o \
    demonstrație generată automat (de motorul DDAR GeoSolver / AlphaGeometry) și trebuie \
    să o rescrii ca O SINGURĂ demonstrație curată și curgătoare, în limbaj \
    matematic natural, așa cum ar scrie un autor uman.\n\
    Reguli:\n\
    - Scrie în limba română, cu diacritice corecte (ă, â, î, ș, ț). \
    Începe cu cuvântul „Soluție.” urmat de demonstrație.\n\
    - Proză curgătoare, în fraze complete. FĂRĂ numere de pas, FĂRĂ liste de \
    puncte după fiecare afirmație, FĂRĂ paranteze de citare precum [002 & 004], \
    FĂRĂ punctaje precum „3p”.\n\
    - Fii concis: unește pașii banali și scoate în evidență ideile-cheie, mai \
    ales construcția auxiliară. Un raționament scurt și lizibil, nu o transcriere pas \
    cu pas.\n\
    - Rămâi fidel logicii demonstrației automate; nu inventa altă demonstrație sau \
    fapte neprecizate. Dacă motorul a adăugat puncte auxiliare, introdu-le natural („Fie \
    ω cercul circumscris al triunghiului ABC și fie X a doua intersecție a lui cu dreapta \
    BQ ...”).\n\
    - Folosește tipografie matematică corectă, cu simboluri Unicode: ∠ pentru \
    unghiuri, △ pentru triunghiuri, ∥ paralel, ⊥ perpendicular, ° grade, ⇒, și \
    notație de fracție/raport. Încheie demonstrația cu ∎.\n\
    - Formatează cu Markdown ușor: pune numele de puncte și expresiile matematice scurte \
    între *asteriscuri simple* pentru cursive (ex. *ABC*, *∠PP₁C = ∠BAC*), și \
    folosește **asteriscuri duble** cu moderație pentru o expresie-cheie. Separă \
    paragrafele cu un rând gol. NU folosi titluri, blocuri de cod sau LaTeX.\n\
    - Notația automată de decodat (NU o repeta): coll = coliniare, cyclic = conciclice, \
    eqangle = unghiuri egale, para = paralel, perp = perpendicular, cong = segmente egale, midp = \
    mijloc; punctele auxiliare introduse de motor poartă nume de manual (M, O, P′, B′ …; \
    auxₙ doar ca rezervă) — păstrează aceste nume.\n\
    Scrie doar demonstrația.";

/// Build the user prompt: the readable problem, the machine proof to rewrite,
/// and any auxiliary points the search introduced.
fn humanize_prompt(problem: &str, proof: &str, aux: &[String], lang: Lang) -> String {
    let aux_block = if aux.is_empty() {
        String::new()
    } else {
        let list = aux
            .iter()
            .map(|a| format!("- {a}"))
            .collect::<Vec<_>>()
            .join("\n");
        match lang {
            Lang::En => format!("\n\nAuxiliary points the solver added:\n{list}"),
            Lang::Ro => format!("\n\nPuncte auxiliare adăugate de motor:\n{list}"),
        }
    };
    match lang {
        Lang::En => {
            format!("Problem:\n{problem}\n\nMachine proof to rewrite:\n{proof}{aux_block}")
        }
        Lang::Ro => {
            format!("Problema:\n{problem}\n\nDemonstrația automată de rescris:\n{proof}{aux_block}")
        }
    }
}

/// Rewrite a machine (DDAR) proof into a flowing, human-readable proof in the
/// requested language, using the local Claude **Opus** subscription. Reuses the
/// same subprocess pattern as [`translate`].
pub fn humanize_proof(problem: &str, proof: &str, aux: &[String], lang: Lang) -> Result<String> {
    let bin = claude_bin().ok_or_else(|| {
        anyhow!(
            "the `claude` CLI was not found. Install it with \
             `npm i -g @anthropic-ai/claude-code`, sign in with your subscription \
             (run `claude`, then `/login`), or set AGSTUDIO_CLAUDE_BIN to its path."
        )
    })?;

    let sys = match lang {
        Lang::En => HUMANIZE_SYSTEM_EN,
        Lang::Ro => HUMANIZE_SYSTEM_RO,
    };
    let prompt = humanize_prompt(problem, proof, aux, lang);
    // Opus, made fast: the slow part of `claude -p --model opus` is *extended
    // thinking* (minutes on a big proof). Rewriting an already-found proof needs
    // no deep deliberation, so we turn the thinking budget off — Opus then writes
    // an equally rigorous proof in ~10–30 s. Also skip loading MCP servers, since
    // this is a one-shot text task.
    let model =
        std::env::var("AGSTUDIO_HUMANIZE_MODEL").unwrap_or_else(|_| "claude-opus-4-8".to_string());
    let mut cmd = Command::new(&bin);
    cmd.arg("-p")
        .arg(&prompt)
        .arg("--append-system-prompt")
        .arg(sys)
        .arg("--model")
        .arg(&model)
        .arg("--strict-mcp-config")
        .env("MAX_THINKING_TOKENS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output =
        run_with_timeout(cmd, Duration::from_secs(90)).context("running the claude CLI")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        bail!("claude exited unsuccessfully: {}", err.trim());
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.contains("Not logged in") || raw.contains("/login") || raw.contains("auth login") {
        bail!(
            "the `claude` CLI is not signed in. Run `claude auth login` (it uses your Claude \
             subscription — no API key), then retry."
        );
    }
    if raw.is_empty() {
        bail!("empty proof from the model");
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_prompt_carries_problem_proof_and_aux() {
        let p = humanize_prompt(
            "In triangle ABC ... prove cyclic P Q P1 Q1",
            "001. assumption: coll B C A1\n002. collinear: B A1 C [001]",
            &["aux9 = intersect(BQ, circumcircle(A,B,C))".to_string()],
            Lang::En,
        );
        assert!(p.contains("prove cyclic P Q P1 Q1"));
        assert!(p.contains("002. collinear"));
        assert!(p.contains("aux9 = intersect"));
        assert!(p.contains("Auxiliary points"));
    }

    #[test]
    fn humanize_prompt_omits_aux_block_when_empty() {
        let p = humanize_prompt("prob", "proof", &[], Lang::Ro);
        assert!(!p.contains("Puncte auxiliare"));
        assert!(p.contains("Demonstrația automată"));
    }

    #[test]
    fn lang_from_code() {
        assert_eq!(Lang::from_code("ro"), Lang::Ro);
        assert_eq!(Lang::from_code("RO"), Lang::Ro);
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("xx"), Lang::En);
    }

    #[test]
    fn extracts_tagged_geo_block() {
        let reply = "Here you go:\n\n```geo\n# Thales\nA = free\nprove perp(C,A,C,B)\n```\nDone.";
        let geo = extract_geo(reply).unwrap();
        assert!(geo.starts_with("# Thales"));
        assert!(geo.contains("prove perp"));
        assert_eq!(extract_title(&geo).as_deref(), Some("Thales"));
    }

    #[test]
    fn extracts_untagged_block() {
        let reply = "```\nA B C = triangle\nprove coll(A,B,C)\n```";
        assert!(extract_geo(reply).unwrap().contains("prove coll"));
    }

    #[test]
    fn falls_back_to_raw_with_prove() {
        let reply = "A B C = triangle\nprove coll(A,B,C)";
        assert!(extract_geo(reply).unwrap().contains("prove coll"));
    }

    #[test]
    fn none_without_program() {
        assert!(extract_geo("I cannot help with that.").is_none());
    }
}
