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

/// Copy text by auto-detecting the available clipboard tool (wl-copy or xclip).
pub fn copy_auto(text: &str) -> bool {
    for (cmd, args) in [("wl-copy", &[][..]), ("xclip", &["-selection", "clipboard"][..])] {
        if let Ok(mut child) = std::process::Command::new(cmd)
            .args(args).stdin(std::process::Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(text.as_bytes()); }
            if child.wait().map(|s| s.success()).unwrap_or(false) { return true; }
        }
    }
    false
}
