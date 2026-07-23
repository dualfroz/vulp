//! Small, dependency-free helpers shared across the rest of the crate:
//! recursive file walking, opening paths in the system's default app, and
//! detecting which editor/clipboard tools are actually installed. Clipboard
//! and URL-rewriting logic each get their own submodule since both have
//! enough provider/platform-specific branching to be worth isolating.

pub mod clipboard;
pub mod git;
pub mod url;

/// Directories never walked when scanning a repo's source files — vendored
/// or generated content that must never be touched by rename/find-usages
/// (the whole reason codeintel uses tree-sitter over naive find/replace is
/// to stay correct, but staying out of dependency code entirely is the
/// first line of defense). Mirrors the exclusion list `legacy::refactor`
/// already used for the TODO scanner.
pub const EXCLUDED_DIRS: &[&str] = &[
    ".git", "target", "node_modules", ".next", "dist", "build", ".venv", "vendor",
];

/// Recursively visits every file under `root`, skipping `EXCLUDED_DIRS`.
pub fn walk_files<F: FnMut(&std::path::Path)>(root: &std::path::Path, cb: &mut F) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !EXCLUDED_DIRS.contains(&name.as_ref()) {
                walk_files(&path, cb);
            }
        } else {
            cb(&path);
        }
    }
}

/// Launches the configured editor against `target`, treating `editor` as a
/// whitespace-separated command line (program + flags) rather than a single
/// binary name — common editor configs like `"code --wait"` or `"vim -R"`
/// would otherwise fail to spawn entirely, since `Command::new` looks for a
/// literal executable named `"code --wait"`.
pub fn launch_editor(editor: &str, target: &std::path::Path) -> std::io::Result<std::process::ExitStatus> {
    let mut parts = editor.split_whitespace();
    let Some(prog) = parts.next() else {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty editor command"));
    };
    std::process::Command::new(prog).args(parts).arg(target).status()
}

/// Open a file, directory, or URL with the system default application.
pub fn open_path(target: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(target).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/c", "start", "", target]).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(target).spawn();
}

/// Whether `cmd` resolves to something on PATH, used to filter the
/// editor/clipboard-tool detection lists down to what's actually installed.
fn command_exists(cmd: &str) -> bool {
    #[cfg(target_os = "windows")]
    let finder = "where";
    #[cfg(not(target_os = "windows"))]
    let finder = "which";
    std::process::Command::new(finder).arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Editors actually present on this machine's PATH, in a sensible default order.
pub fn detect_editors() -> Vec<String> {
    ["code", "nvim", "vim", "nano", "hx", "micro", "subl", "zed"]
        .into_iter()
        .filter(|c| command_exists(c))
        .map(str::to_string)
        .collect()
}

/// Clipboard tools actually present on this machine's PATH.
pub fn detect_clipboard_tools() -> Vec<String> {
    let mut found = vec![];
    for cmd in &["wl-copy", "xclip", "pbcopy"] {
        if command_exists(cmd) {
            found.push(if *cmd == "xclip" { "xclip -selection clipboard".to_string() } else { cmd.to_string() });
        }
    }
    found
}
