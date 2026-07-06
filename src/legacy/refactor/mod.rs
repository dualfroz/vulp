//! Source/doc-file scanning for the hub's `[T]` "scan repos for TODO/FIXME/
//! HACK" action: two find functions tuned for different false-positive risks
//! — doc files (README, TODO.md, etc.) are matched as plain text, while
//! source files require the match to actually be inside a comment, so a
//! string literal containing the word "TODO" doesn't get scanned in as a todo.

use std::path::Path;

/// One pattern match found by either scanner.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FindResult {
    pub repo: String,
    pub file: String,
    pub line: usize,
    pub text: String,
}

// ── doc-files-only find (for TODO scanner — avoids self-referential hits in source) ──

pub fn find_in_doc_files(pattern: &str, root: &Path, repo_name: &str) -> Vec<FindResult> {
    let mut results = Vec::new();
    let rx = match regex_simple(pattern) {
        Some(r) => r,
        None    => return results,
    };
    walk_doc_files(root, &mut |file_path, rel| {
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            for (i, line) in content.lines().enumerate() {
                if rx(line) {
                    results.push(FindResult {
                        repo: repo_name.to_string(),
                        file: rel.clone(),
                        line: i + 1,
                        text: line.trim().chars().take(120).collect(),
                    });
                }
            }
        }
    });
    results
}

/// Recursively visits every doc/notes-style file under `root` (by extension
/// or recognized filename like `TODO`/`ROADMAP`), skipping the same
/// vendored/generated directories `find_in_source_files` skips.
fn walk_doc_files<F: FnMut(std::path::PathBuf, String)>(root: &Path, cb: &mut F) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !matches!(name.as_ref(), ".git" | "target" | "node_modules" | ".next" | "dist" | "build" | ".venv" | "vendor") {
                walk_doc_files(&path, cb);
            }
        } else {
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
            let fname = path.file_name().unwrap_or_default().to_string_lossy().to_uppercase();
            let is_doc = matches!(ext.as_deref(), Some("md") | Some("txt") | Some("todo") | Some("org") | Some("rst") | Some("taskpaper"))
                || matches!(fname.as_ref(), "TODO" | "TASKS" | "FIXME" | "NOTES" | "BUGS" | "ROADMAP");
            if is_doc {
                let rel = path.to_string_lossy().to_string();
                cb(path, rel);
            }
        }
    }
}

// ── comment-aware source-file find (for TODO scanner — avoids false positives
// on "TODO"/"FIXME"/"HACK" appearing in strings/identifiers/prose instead of
// an actual comment) ─────────────────────────────────────────────────────────

/// Comment marker(s) for a file extension, in the order they should be
/// checked. Empty slice means "not a recognized source extension" — those
/// files are skipped entirely by `find_in_source_files`.
fn comment_markers_for_ext(ext: &str) -> &'static [&'static str] {
    match ext {
        "rs" | "go" | "java" | "kt" | "kts" | "c" | "h" | "cpp" | "cc" | "cxx" | "hxx" | "hpp"
            | "cs" | "swift" | "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "php" | "scala"
            | "dart" | "zig" | "vue" | "svelte"
            => &["//", "/*"],
        "py" | "pyw" | "rb" | "sh" | "bash" | "zsh" | "fish" | "r" | "nim" | "tf" | "tfvars" | "yaml" | "yml" | "toml"
            => &["#"],
        "sql" | "hs" | "ex" | "exs" | "lua"
            => &["--"],
        "html" | "htm" | "xml"
            => &["<!--"],
        "clj" | "cljs"
            => &[";"],
        _ => &[],
    }
}

/// Whether `pattern` appears in `line` at a position preceded by one of
/// `markers` — a cheap same-line heuristic for "is this actually inside a
/// comment" that doesn't need a real per-language parser.
fn line_has_commented_pattern(line: &str, pattern: &str, markers: &[&str]) -> bool {
    let Some(pat_pos) = line.find(pattern) else { return false };
    markers.iter().any(|m| line[..pat_pos].contains(m))
}

/// Like `find_in_doc_files`, but for source code: only matches `pattern` when
/// it's preceded by a comment marker on the same line, so `TODO` appearing in
/// a string literal, identifier, or other non-comment code never matches.
pub fn find_in_source_files(pattern: &str, root: &Path, repo_name: &str) -> Vec<FindResult> {
    let mut results = Vec::new();
    walk_source_files(root, &mut |file_path, rel, markers| {
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            for (i, line) in content.lines().enumerate() {
                if line_has_commented_pattern(line, pattern, markers) {
                    results.push(FindResult {
                        repo: repo_name.to_string(),
                        file: rel.clone(),
                        line: i + 1,
                        text: line.trim().chars().take(120).collect(),
                    });
                }
            }
        }
    });
    results
}

/// Recursively visits every recognized source file under `root`, handing
/// each one's comment markers to `cb` alongside its path.
fn walk_source_files<F: FnMut(std::path::PathBuf, String, &'static [&'static str])>(root: &Path, cb: &mut F) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !matches!(name.as_ref(), ".git" | "target" | "node_modules" | ".next" | "dist" | "build" | ".venv" | "vendor") {
                walk_source_files(&path, cb);
            }
        } else if let Some(ext) = path.extension() {
            let markers = comment_markers_for_ext(&ext.to_string_lossy().to_lowercase());
            if !markers.is_empty() {
                let rel = path.to_string_lossy().to_string();
                cb(path, rel, markers);
            }
        }
    }
}

/// Builds a literal substring matcher for `pattern` — named for what it
/// stands in for, not because it's an actual regex (a real one isn't worth
/// the extra dependency for fixed strings like "TODO"/"FIXME"/"HACK").
type LineMatcher = Box<dyn Fn(&str) -> bool>;

fn regex_simple(pattern: &str) -> Option<LineMatcher> {
    // Simple substring/literal match (not full regex to avoid extra dep)
    let p = pattern.to_string();
    Some(Box::new(move |line: &str| line.contains(p.as_str())))
}
