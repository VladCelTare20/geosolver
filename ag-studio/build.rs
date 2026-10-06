use std::path::PathBuf;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn watch(git_path: &str) {
    if let Some(p) = git(&["rev-parse", "--path-format=absolute", "--git-path", git_path]) {
        if PathBuf::from(&p).exists() {
            println!("cargo:rerun-if-changed={p}");
        }
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=GEOSOLVER_REVISION");
    let revision = match std::env::var("GEOSOLVER_REVISION") {
        Ok(r) if !r.trim().is_empty() => r.trim().to_string(),
        _ => {
            watch("HEAD");
            watch("packed-refs");
            if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
                watch(&head_ref);
            }
            git(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".to_string())
        }
    };
    println!("cargo:rustc-env=GEOSOLVER_REVISION={revision}");
}
