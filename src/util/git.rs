//! Thin wrappers around invoking `git` as a subprocess against a given
//! repo path. Used instead of a git library for commands where shelling out
//! to the user's actual `git` binary (with their real config, credential
//! helpers, hooks) is exactly the desired behavior.

/// Runs `git <args>` against `path`, inheriting stdio so output reaches the
/// terminal directly. On failure, prints a short summary instead of
/// propagating an error — most call sites treat a failed git command as
/// "nothing changed" rather than something to abort the TUI over.
pub fn git_run(path: &std::path::Path, args: &[&str]) {
    match std::process::Command::new("git").arg("-C").arg(path).args(args).status() {
        Ok(s) if s.success() => {}
        Ok(s)  => eprintln!("  git {} → exit {}", args.join(" "), s),
        Err(e) => eprintln!("  git {}: {}", args.join(" "), e),
    }
}

/// Like `git_run`, but silent and returns success/failure instead of
/// printing — used where the caller wants to branch on the result rather
/// than just report it.
pub fn git_run_ok(path: &std::path::Path, args: &[&str]) -> bool {
    std::process::Command::new("git").arg("-C").arg(path).args(args)
        .status().map(|s| s.success()).unwrap_or(false)
}
