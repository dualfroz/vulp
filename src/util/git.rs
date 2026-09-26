pub fn git_run(path: &std::path::Path, args: &[&str]) {
    match std::process::Command::new("git").arg("-C").arg(path).args(args).status() {
        Ok(s) if s.success() => {}
        Ok(s)  => eprintln!("  git {} → exit {}", args.join(" "), s),
        Err(e) => eprintln!("  git {}: {}", args.join(" "), e),
    }
}

pub fn git_run_ok(path: &std::path::Path, args: &[&str]) -> bool {
    std::process::Command::new("git").arg("-C").arg(path).args(args)
        .status().map(|s| s.success()).unwrap_or(false)
}
