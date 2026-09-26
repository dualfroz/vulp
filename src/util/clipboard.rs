use std::io::Write as _;

pub fn copy_to_clipboard(text: &str, cmd: &str) -> bool {
    if cmd.is_empty() { return false; }
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let (prog, args) = parts.split_first().unwrap_or((&"", &[]));
    let Ok(mut child) = std::process::Command::new(prog)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .spawn() else { return false; };
    if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(text.as_bytes()); }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

pub fn copy_auto(text: &str) -> bool {
    #[cfg(target_os = "macos")]
    let candidates: &[(&str, &[&str])] = &[("pbcopy", &[])];
    #[cfg(target_os = "windows")]
    let candidates: &[(&str, &[&str])] = &[("clip", &[])];
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let candidates: &[(&str, &[&str])] = &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])];

    for (cmd, args) in candidates {
        if let Ok(mut child) = std::process::Command::new(cmd)
            .args(*args).stdin(std::process::Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(text.as_bytes()); }
            if child.wait().map(|s| s.success()).unwrap_or(false) { return true; }
        }
    }
    false
}
