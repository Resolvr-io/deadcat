use std::path::Path;
use std::process::Command;

fn main() {
    emit_git_info();
    tauri_build::build()
}

fn emit_git_info() {
    let hash = run_git(&["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = run_git(&["status", "--porcelain"])
        .map(|out| !out.is_empty())
        .unwrap_or(false);

    println!("cargo:rustc-env=GIT_HASH={hash}");
    println!(
        "cargo:rustc-env=GIT_DIRTY={}",
        if dirty { "1" } else { "0" }
    );

    // Re-run build.rs when the current commit changes. HEAD may be a symbolic
    // ref (branch) or a detached commit; watch the ref file it points to so
    // we catch commits that don't move HEAD itself.
    let git_dir = run_git(&["rev-parse", "--git-dir"]).unwrap_or_else(|| ".git".into());
    println!("cargo:rerun-if-changed={git_dir}/HEAD");
    if let Ok(head) = std::fs::read_to_string(Path::new(&git_dir).join("HEAD")) {
        if let Some(refname) = head.strip_prefix("ref: ").map(str::trim) {
            println!("cargo:rerun-if-changed={git_dir}/{refname}");
        }
    }
    // `git status` reads the index; rerun when it changes so GIT_DIRTY stays
    // accurate across stage/unstage.
    println!("cargo:rerun-if-changed={git_dir}/index");
}

fn run_git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
