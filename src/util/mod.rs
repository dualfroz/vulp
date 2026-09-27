pub mod clipboard;
pub mod git;
#[cfg(test)]
pub mod testgit;
pub mod url;

pub const EXCLUDED_DIRS: &[&str] = &[
    ".git", "target", "node_modules", ".next", "dist", "build", ".venv", "vendor",
];

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

pub fn launch_editor(editor: &str, target: &std::path::Path) -> std::io::Result<std::process::ExitStatus> {
    let mut parts = editor.split_whitespace();
    let Some(prog) = parts.next() else {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty editor command"));
    };
    std::process::Command::new(prog).args(parts).arg(target).status()
}

pub fn open_path(target: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(target).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/c", "start", "", target]).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(target).spawn();
}

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

pub fn detect_editors() -> Vec<String> {
    ["code", "nvim", "vim", "nano", "hx", "micro", "subl", "zed"]
        .into_iter()
        .filter(|c| command_exists(c))
        .map(str::to_string)
        .collect()
}

pub fn detect_clipboard_tools() -> Vec<String> {
    let mut found = vec![];
    for cmd in &["wl-copy", "xclip", "pbcopy"] {
        if command_exists(cmd) {
            found.push(if *cmd == "xclip" { "xclip -selection clipboard".to_string() } else { cmd.to_string() });
        }
    }
    found
}
