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

/// Locate the `claude` executable once: `AGSTUDIO_CLAUDE_BIN` if set, else the
/// first `claude` on `PATH`, else the usual install locations. Returns `None`
/// if the CLI is not installed.
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

/// Check whether the `claude` CLI is signed in to a subscription,
/// authoritatively via `claude auth status` (run with the same scrubbed env as
/// a translation, so an `ANTHROPIC_API_KEY` cannot make it look signed in).
/// Advisory only — the real gate is the translate call itself.
fn detect_login() -> bool {
    if env_login_hint(|k| std::env::var(k).ok()) {
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
    scrub_env(&mut cmd);
    match run_with_timeout(cmd, Duration::from_secs(15)) {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            s.contains("\"loggedIn\": true") || s.contains("\"loggedIn\":true")
        }
        Err(_) => false,
    }
}

/// A subscription OAuth token in the environment counts as signed in. An
/// `ANTHROPIC_API_KEY` does not: it is never passed to the child.
fn env_login_hint(var: impl Fn(&str) -> Option<String>) -> bool {
    var("CLAUDE_CODE_OAUTH_TOKEN").is_some_and(|v| !v.is_empty())
}

fn find_claude() -> Option<PathBuf> {
    const NESTED: &str = "node_modules/@anthropic-ai/claude-code/bin/claude.exe";
    let mut fallbacks: Vec<PathBuf> = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        fallbacks.push(Path::new(&appdata).join("npm").join(NESTED));
        fallbacks.push(Path::new(&appdata).join("npm").join("claude.cmd"));
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        fallbacks.push(Path::new(&home).join(".local/bin").join(NESTED));
        fallbacks.push(Path::new(&home).join(".local/bin/claude.exe"));
        fallbacks.push(Path::new(&home).join(".local/bin/claude"));
    }
    find_claude_from(
        std::env::var_os("AGSTUDIO_CLAUDE_BIN"),
        std::env::var_os("PATH"),
        &fallbacks,
        probe,
    )
}

/// The lookup order behind [`claude_bin`]. An explicit `env_bin` is
/// authoritative (a broken override is reported as "not installed", never
/// silently swapped for another binary).
fn find_claude_from(
    env_bin: Option<std::ffi::OsString>,
    path_var: Option<std::ffi::OsString>,
    fallbacks: &[PathBuf],
    probe: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(p) = env_bin.filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        return probe(&p).then_some(p);
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    for dir in path_var.iter().flat_map(std::env::split_paths) {
        for name in ["claude.exe", "claude", "claude.cmd"] {
            let c = dir.join(name);
            if c.is_file() {
                // The npm `claude.cmd` shim breaks on cmd.exe metacharacters in
                // a large prompt; the real exe beside it does not.
                if name == "claude.cmd" {
                    candidates.push(dir.join("node_modules/@anthropic-ai/claude-code/bin/claude.exe"));
                }
                candidates.push(c);
            }
        }
    }
    candidates.extend(fallbacks.iter().cloned());
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
    let mut cmd = Command::new(bin);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    run_with_timeout(cmd, Duration::from_secs(15))
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Wall-clock limit for one translation / humanization child.
const CHILD_TIMEOUT: Duration = Duration::from_secs(90);

/// An MCP config with no servers, used with `--strict-mcp-config`.
const EMPTY_MCP_CONFIG: &str = r#"{"mcpServers":{}}"#;

const NOT_FOUND: &str = "the `claude` CLI was not found. Install it with \
     `npm i -g @anthropic-ai/claude-code`, sign in with your subscription \
     (run `claude`, then `/login`), or set AGSTUDIO_CLAUDE_BIN to its path.";

const NOT_SIGNED_IN: &str = "the `claude` CLI is not signed in. Run `claude auth login` (it \
     uses your Claude subscription — no API key), then retry.";

/// Remove every credential/endpoint override except the subscription OAuth
/// token, plus the parent Claude Code session's own variables (when agstudio
/// itself runs under Claude Code), so the child authenticates only with the
/// operator's stored subscription login.
fn scrub_env(cmd: &mut Command) {
    for (key, _) in std::env::vars_os() {
        let Some(k) = key.to_str() else { continue };
        let keep = matches!(k, "CLAUDE_CODE_OAUTH_TOKEN" | "CLAUDE_CONFIG_DIR");
        if !keep && (k.starts_with("ANTHROPIC_") || k.starts_with("CLAUDE")) {
            cmd.env_remove(&key);
        }
    }
    // Belt and braces for the three that matter most, even if set later.
    for k in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_BASE_URL"] {
        cmd.env_remove(k);
    }
}

/// Build a locked-down headless `claude -p` invocation: no settings sources,
/// hooks, CLAUDE.md, skills or MCP servers; only the built-in `tools` listed
/// (`""` = none), denied anything not pre-approved, no session written to disk,
/// run in `cwd`.
fn child_command(bin: &Path, prompt: &str, tools: &str, allowed: Option<&str>, cwd: &Path) -> Command {
    let mut cmd = Command::new(bin);
    cmd.arg("-p")
        .arg(prompt)
        .arg("--safe-mode")
        .arg("--setting-sources")
        .arg("")
        .arg("--strict-mcp-config")
        .arg("--mcp-config")
        .arg(EMPTY_MCP_CONFIG)
        .arg("--no-session-persistence")
        .arg("--disable-slash-commands")
        .arg("--permission-mode")
        .arg("dontAsk")
        .arg("--tools")
        .arg(tools);
    if let Some(rule) = allowed {
        cmd.arg("--allowedTools").arg(rule);
    }
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    scrub_env(&mut cmd);
    cmd
}

/// A fresh, private, empty working directory for one child run (removed on
/// drop). Canonicalized so the path given to the model matches what it sees.
fn scratch_dir() -> Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::Builder::new()
        .prefix("agstudio-claude-")
        .tempdir()
        .context("creating a scratch directory for the claude CLI")?;
    let path = dir.path().canonicalize().unwrap_or_else(|_| dir.path().to_path_buf());
    Ok((dir, path))
}

/// Does CLI output look like its "please sign in" message?
fn looks_signed_out(s: &str) -> bool {
    s.contains("Not logged in") || s.contains("/login") || s.contains("auth login")
}

/// Translate a problem into a `.geo` program using the local Claude subscription.
pub fn translate(source: &Source) -> Result<Translation> {
    let bin = claude_bin().ok_or_else(|| anyhow!(NOT_FOUND))?;
    translate_with(&bin, source, CHILD_TIMEOUT)
}

fn translate_with(bin: &Path, source: &Source, timeout: Duration) -> Result<Translation> {
    let (_scratch, cwd) = scratch_dir()?;
    let (prompt, tools, allowed) = match source {
        Source::Text(t) => (
            format!("Translate this geometry problem into a .geo program:\n\n{t}"),
            "",
            None,
        ),
        Source::Image(p) => {
            // Copy the image into the child's otherwise-empty cwd under a fixed
            // name, and allow Read on exactly that file.
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .filter(|e| !e.is_empty() && e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
                .unwrap_or_else(|| "img".to_string());
            let local = cwd.join(format!("problem.{ext}"));
            std::fs::copy(p, &local)
                .with_context(|| format!("copying the image {}", p.display()))?;
            let shown = local.display().to_string().replace('\\', "/");
            (
                format!(
                    "Read the geometry problem in the image file at {shown} and translate it \
                     into a .geo program. Output only the single ```geo code block."
                ),
                "Read",
                Some(format!("Read(/{shown})")),
            )
        }
    };

    let mut cmd = child_command(bin, &prompt, tools, allowed.as_deref(), &cwd);
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

    let output = run_with_timeout(cmd, timeout).context("running the claude CLI")?;
    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        if looks_signed_out(&raw) || looks_signed_out(&err) {
            bail!(NOT_SIGNED_IN);
        }
        bail!("claude exited unsuccessfully: {}", err.trim());
    }
    let Some(geo) = extract_geo(&raw) else {
        // The CLI prints a login prompt (with exit 0) when not signed in.
        if looks_signed_out(&raw) {
            bail!(NOT_SIGNED_IN);
        }
        bail!("no .geo code block found in the model's reply:\n{}", raw.trim());
    };
    if let Some(reason) = cannot_translate_reason(&geo) {
        bail!("the model could not translate this problem: {reason}");
    }
    let title = extract_title(&geo);
    Ok(Translation { geo, title })
}

/// The reason in a `# cannot translate: <reason>` reply, if the program is
/// only that.
fn cannot_translate_reason(geo: &str) -> Option<String> {
    let mut lines = geo.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next()?;
    let rest = first.strip_prefix('#')?.trim();
    let lower = rest.to_lowercase();
    let reason = lower.strip_prefix("cannot translate")?;
    let has_statements = lines.any(|l| !l.starts_with('#'));
    if has_statements {
        return None;
    }
    let reason = rest[rest.len() - reason.len()..].trim_start_matches([':', ' ']).trim();
    Some(if reason.is_empty() { "no reason given".to_string() } else { reason.to_string() })
}

/// Spawn a command (in its own process group on Unix), capturing stdout/stderr
/// on reader threads (so a full pipe buffer can never deadlock the wait), and
/// enforce a wall-clock timeout. On timeout — or if waiting fails — the whole
/// group is killed and the child reaped.
fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<Output> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    let out_reader = child.stdout.take().map(reader_thread);
    let err_reader = child.stderr.take().map(reader_thread);

    let status = match child.wait_timeout(timeout) {
        Ok(Some(status)) => status,
        Ok(None) => {
            kill_tree(&mut child);
            bail!("claude timed out after {}s", timeout.as_secs_f64().round());
        }
        Err(e) => {
            kill_tree(&mut child);
            return Err(anyhow!(e).context("waiting for the claude CLI"));
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

/// Kill a child and everything in its process group, then reap it.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // The child leads its own group (`process_group(0)`), so -pid names it.
        // std has no killpg; the `kill` utility is everywhere this runs.
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg("--")
            .arg(format!("-{}", child.id()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
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
/// requested language, using the local Claude **Opus** subscription. Runs the
/// same locked-down child as [`translate`], with no tools at all.
pub fn humanize_proof(problem: &str, proof: &str, aux: &[String], lang: Lang) -> Result<String> {
    let bin = claude_bin().ok_or_else(|| anyhow!(NOT_FOUND))?;
    humanize_with(&bin, problem, proof, aux, lang, CHILD_TIMEOUT)
}

fn humanize_with(
    bin: &Path,
    problem: &str,
    proof: &str,
    aux: &[String],
    lang: Lang,
    timeout: Duration,
) -> Result<String> {
    let sys = match lang {
        Lang::En => HUMANIZE_SYSTEM_EN,
        Lang::Ro => HUMANIZE_SYSTEM_RO,
    };
    let prompt = humanize_prompt(problem, proof, aux, lang);
    // Opus, made fast: the slow part of `claude -p --model opus` is *extended
    // thinking* (minutes on a big proof). Rewriting an already-found proof needs
    // no deep deliberation, so we turn the thinking budget off — Opus then writes
    // an equally rigorous proof in ~10–30 s.
    let model =
        std::env::var("AGSTUDIO_HUMANIZE_MODEL").unwrap_or_else(|_| "claude-opus-4-8".to_string());
    let (_scratch, cwd) = scratch_dir()?;
    let mut cmd = child_command(bin, &prompt, "", None, &cwd);
    cmd.arg("--append-system-prompt")
        .arg(sys)
        .arg("--model")
        .arg(&model)
        .env("MAX_THINKING_TOKENS", "0");

    let output = run_with_timeout(cmd, timeout).context("running the claude CLI")?;
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        if looks_signed_out(&raw) || looks_signed_out(&err) {
            bail!(NOT_SIGNED_IN);
        }
        bail!("claude exited unsuccessfully: {}", err.trim());
    }
    if raw.is_empty() {
        bail!("empty proof from the model");
    }
    if raw.len() < 200 && looks_signed_out(&raw) {
        bail!(NOT_SIGNED_IN);
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

    // ---- the child `claude` process, exercised against a fake binary --------

    const GOOD_REPLY: &str = "```geo\n# Thales\nA = free\nprove coll(A, A, A)\n```\n";

    /// A fake `claude` written under `target/`: logs argv (NUL-separated), env,
    /// cwd and the cwd listing into its own directory, then prints `reply` and
    /// exits with `code`.
    struct FakeClaude {
        dir: tempfile::TempDir,
    }

    impl FakeClaude {
        fn new(reply: &str, code: i32) -> FakeClaude {
            use std::os::unix::fs::PermissionsExt;
            let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target");
            std::fs::create_dir_all(&target).unwrap();
            let dir = tempfile::Builder::new()
                .prefix("fake-claude-")
                .tempdir_in(&target)
                .unwrap();
            std::fs::write(dir.path().join("reply"), reply).unwrap();
            let d = dir.path().display();
            let script = format!(
                "#!/bin/sh\nprintf '%s\\0' \"$@\" > '{d}/argv'\nenv > '{d}/env'\n\
                 pwd > '{d}/cwd'\nls -A > '{d}/ls'\ncat '{d}/reply'\nexit {code}\n"
            );
            let bin = dir.path().join("claude");
            std::fs::write(&bin, script).unwrap();
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            FakeClaude { dir }
        }
        fn bin(&self) -> PathBuf {
            self.dir.path().join("claude")
        }
        fn read(&self, name: &str) -> String {
            std::fs::read_to_string(self.dir.path().join(name)).unwrap_or_default()
        }
        fn argv(&self) -> Vec<String> {
            let raw = self.read("argv");
            let mut v: Vec<String> = raw.split('\0').map(str::to_string).collect();
            v.pop();
            v
        }
        fn value_after(&self, flag: &str) -> Option<String> {
            let a = self.argv();
            let i = a.iter().position(|x| x == flag)?;
            a.get(i + 1).cloned()
        }
    }

    fn assert_locked_down(fake: &FakeClaude, tools: &str) {
        let argv = fake.argv();
        assert_eq!(argv.first().map(String::as_str), Some("-p"), "{argv:?}");
        assert_eq!(fake.value_after("--tools").as_deref(), Some(tools), "{argv:?}");
        assert_eq!(fake.value_after("--setting-sources").as_deref(), Some(""), "{argv:?}");
        assert_eq!(
            fake.value_after("--mcp-config").as_deref(),
            Some(EMPTY_MCP_CONFIG),
            "{argv:?}"
        );
        assert_eq!(fake.value_after("--permission-mode").as_deref(), Some("dontAsk"));
        for flag in ["--strict-mcp-config", "--no-session-persistence", "--safe-mode"] {
            assert!(argv.iter().any(|a| a == flag), "missing {flag}: {argv:?}");
        }
        for bad in ["acceptEdits", "bypassPermissions", "--bare", "--dangerously-skip-permissions"] {
            assert!(!argv.iter().any(|a| a == bad), "unexpected {bad}: {argv:?}");
        }
        let env = fake.read("env");
        for var in ["ANTHROPIC_API_KEY=", "ANTHROPIC_AUTH_TOKEN=", "ANTHROPIC_BASE_URL=", "CLAUDECODE="] {
            assert!(!env.lines().any(|l| l.starts_with(var)), "{var} leaked into the child env");
        }
        assert!(env.lines().any(|l| l.starts_with("HOME=")), "HOME must survive for OAuth");
        let cwd = fake.read("cwd");
        let cwd = cwd.trim();
        assert_ne!(Path::new(cwd), std::env::current_dir().unwrap().as_path());
        assert!(!Path::new(cwd).exists(), "the child's scratch dir must be removed afterwards");
    }

    fn poison_env() {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-should-not-leak");
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "tok-should-not-leak");
        std::env::set_var("ANTHROPIC_BASE_URL", "http://127.0.0.1:9");
    }

    #[test]
    fn text_translation_runs_with_no_tools_in_an_empty_dir() {
        poison_env();
        let fake = FakeClaude::new(GOOD_REPLY, 0);
        let t = translate_with(&fake.bin(), &Source::Text("prove Thales".into()), CHILD_TIMEOUT)
            .unwrap();
        assert!(t.geo.contains("prove coll"));
        assert_locked_down(&fake, "");
        assert_eq!(fake.read("ls").trim(), "", "the text-mode cwd must be empty");
        assert!(fake.value_after("--allowedTools").is_none());
    }

    #[test]
    fn image_translation_may_read_only_the_copied_image() {
        poison_env();
        let fake = FakeClaude::new(GOOD_REPLY, 0);
        let src_dir = tempfile::tempdir().unwrap();
        let img = src_dir.path().join("photo of problem.PNG");
        std::fs::write(&img, b"\x89PNG fake").unwrap();
        translate_with(&fake.bin(), &Source::Image(img.clone()), CHILD_TIMEOUT).unwrap();
        assert_locked_down(&fake, "Read");
        assert_eq!(fake.read("ls").trim(), "problem.png");
        let cwd = fake.read("cwd").trim().to_string();
        let allowed = fake.value_after("--allowedTools").unwrap();
        assert_eq!(allowed, format!("Read(/{cwd}/problem.png)"));
        let prompt = fake.value_after("-p").unwrap();
        assert!(prompt.contains(&format!("{cwd}/problem.png")), "{prompt}");
        assert!(!prompt.contains("photo of problem"), "{prompt}");
    }

    #[test]
    fn humanize_runs_with_no_tools_in_an_empty_dir() {
        poison_env();
        let fake = FakeClaude::new("Solution. Trivial. ∎", 0);
        let out = humanize_with(&fake.bin(), "prob", "001. x", &[], Lang::En, CHILD_TIMEOUT)
            .unwrap();
        assert!(out.starts_with("Solution."));
        assert_locked_down(&fake, "");
    }

    #[test]
    fn login_words_inside_a_good_reply_are_not_a_sign_in_failure() {
        let reply = "```geo\n# Proof that /login and auth login are fine here\nA = free\n\
                     prove coll(A, A, A)\n```";
        let fake = FakeClaude::new(reply, 0);
        let t = translate_with(&fake.bin(), &Source::Text("x".into()), CHILD_TIMEOUT).unwrap();
        assert!(t.geo.contains("prove coll"));
    }

    #[test]
    fn a_login_prompt_without_a_program_is_reported_as_not_signed_in() {
        let fake = FakeClaude::new("Not logged in · Please run /login", 0);
        let err = translate_with(&fake.bin(), &Source::Text("x".into()), CHILD_TIMEOUT)
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("not signed in"), "{err}");

        let failed = FakeClaude::new("Please run /login", 1);
        let err = translate_with(&failed.bin(), &Source::Text("x".into()), CHILD_TIMEOUT)
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("not signed in"), "{err}");
    }

    #[test]
    fn cannot_translate_block_is_an_error() {
        let fake = FakeClaude::new("```geo\n# cannot translate: this is a 3D problem\n```", 0);
        let err = translate_with(&fake.bin(), &Source::Text("x".into()), CHILD_TIMEOUT)
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("could not translate"), "{err}");
        assert!(err.contains("3D problem"), "{err}");
    }

    #[test]
    fn timeout_kills_the_whole_process_group() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("grandchild.pid");
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(format!("sleep 30 & echo $! > '{}'; wait", pidfile.display()))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let t = std::time::Instant::now();
        let err = run_with_timeout(cmd, Duration::from_millis(1500)).err().unwrap();
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(t.elapsed() < Duration::from_secs(10));
        let pid = std::fs::read_to_string(&pidfile).unwrap();
        // Dead = gone or a zombie awaiting its (re)parent's reap.
        let alive = || {
            std::fs::read_to_string(format!("/proc/{}/stat", pid.trim()))
                .map(|st| st.rsplit(')').next().unwrap_or("").trim_start().chars().next() != Some('Z'))
                .unwrap_or(false)
        };
        let until = std::time::Instant::now() + Duration::from_secs(3);
        while alive() && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(), "grandchild {pid} survived the timeout");
    }

    #[test]
    fn find_claude_prefers_env_then_path_then_fallbacks() {
        let a = tempfile::tempdir().unwrap();
        let on_path = a.path().join("claude");
        std::fs::write(&on_path, "").unwrap();
        let fallback = PathBuf::from("/opt/fallback/claude");
        let env_bin = PathBuf::from("/opt/env/claude");
        let all = |_: &Path| true;
        let path_var = std::env::join_paths([a.path()]).unwrap();

        let got = find_claude_from(Some(env_bin.clone().into()), Some(path_var.clone()), &[fallback.clone()], all);
        assert_eq!(got, Some(env_bin.clone()));
        let got = find_claude_from(None, Some(path_var.clone()), &[fallback.clone()], all);
        assert_eq!(got, Some(on_path.clone()));
        let got = find_claude_from(None, None, &[fallback.clone()], all);
        assert_eq!(got, Some(fallback.clone()));
        // An explicit AGSTUDIO_CLAUDE_BIN that does not run is not silently replaced.
        let none = |p: &Path| p != env_bin.as_path();
        assert_eq!(find_claude_from(Some(env_bin.clone().into()), Some(path_var), &[fallback], none), None);
    }

    #[test]
    fn an_api_key_alone_does_not_count_as_signed_in() {
        assert!(!env_login_hint(|k| (k == "ANTHROPIC_API_KEY").then(|| "sk".into())));
        assert!(env_login_hint(|k| (k == "CLAUDE_CODE_OAUTH_TOKEN").then(|| "t".into())));
    }
}
