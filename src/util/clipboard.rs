//! Clipboard writers: a configured-command variant (`copy_to_clipboard`,
//! used when the user has set `clipboard_cmd` in config) and an
//! auto-detecting fallback (`copy_auto`) that tries the common Linux tools.

use std::io::Write as _;

/// Copy text using a configured command string (e.g. "wl-copy" or "xclip -selection clipboard").
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

/// Copy text by auto-detecting the available clipboard tool: `wl-copy`/`xclip`
/// on Linux, `pbcopy` on macOS, `clip` on Windows. Tries every candidate for
/// the current OS in order and returns on the first one that actually
/// launches and exits successfully — callers should surface a status message
/// on `false` (nothing copied) rather than assume success, since on Linux
/// without either tool installed, or on an unexpected platform, this is a
/// real "no-op" outcome the user needs to know about.
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
