pub mod lang;

use lang::{function_query, identifier_query, lang_for_ext, ts_language, Lang};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuncFormat {
    NamesOnly,
    WithArgs,
    WithMeta,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub args: Vec<String>,
    pub file: String,
    pub line: usize,
    pub length_lines: usize,
    pub ref_count: usize,
}

#[derive(Debug, Clone)]
pub struct UsageLocation {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub context: String,
}

#[derive(Debug, Clone)]
pub struct RenameEdit {
    pub file: PathBuf,
    pub line: usize,
    pub col: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub old_text: String,
    pub new_text: String,
}

#[derive(Debug, Clone, Default)]
pub struct RenamePlan {
    pub edits: Vec<RenameEdit>,
}

impl RenamePlan {
    pub fn file_count(&self) -> usize {
        let mut files: Vec<&PathBuf> = self.edits.iter().map(|e| &e.file).collect();
        files.sort();
        files.dedup();
        files.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingConvention {
    Snake,
    Camel,
    Pascal,
}

impl NamingConvention {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Snake  => "snake_case",
            Self::Camel  => "camelCase",
            Self::Pascal => "PascalCase",
        }
    }

    fn detect(name: &str) -> Option<Self> {
        if name.is_empty() { return None; }
        if name.contains('_') { return Some(Self::Snake); }
        match name.chars().next() {
            Some(c) if c.is_uppercase() => Some(Self::Pascal),
            Some(c) if c.is_lowercase() => Some(Self::Camel),
            _ => None,
        }
    }

    fn convert(&self, name: &str) -> String {
        let words = split_words(name);
        match self {
            Self::Snake  => words.join("_"),
            Self::Camel  => join_camel_pascal(&words, false),
            Self::Pascal => join_camel_pascal(&words, true),
        }
    }
}

fn split_words(name: &str) -> Vec<String> {
    if name.contains('_') {
        return name.split('_').filter(|s| !s.is_empty()).map(|s| s.to_lowercase()).collect();
    }
    let mut words = vec![];
    let mut cur = String::new();
    for ch in name.chars() {
        if ch.is_uppercase() && !cur.is_empty() {
            words.push(std::mem::take(&mut cur).to_lowercase());
        }
        cur.push(ch);
    }
    if !cur.is_empty() { words.push(cur.to_lowercase()); }
    words
}

fn join_camel_pascal(words: &[String], pascal: bool) -> String {
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        if w.is_empty() { continue; }
        let mut chars = w.chars();
        let first = chars.next().unwrap();
        if i == 0 && !pascal {
            out.push(first.to_ascii_lowercase());
        } else {
            out.push(first.to_ascii_uppercase());
        }
        out.push_str(chars.as_str());
    }
    out
}

fn parse_file(path: &Path) -> Option<(Lang, String, tree_sitter::Tree)> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    let lang = lang_for_ext(&ext)?;
    let source = std::fs::read_to_string(path).ok()?;
    let mut parser = Parser::new();
    parser.set_language(&ts_language(lang)).ok()?;
    let tree = parser.parse(&source, None)?;
    Some((lang, source, tree))
}

fn rel_path(repo_path: &Path, path: &Path) -> String {
    path.strip_prefix(repo_path).unwrap_or(path).to_string_lossy().to_string()
}

pub fn list_functions(repo_path: &Path, _format: FuncFormat) -> Vec<FunctionInfo> {
    let mut out: Vec<FunctionInfo> = Vec::new();
    crate::util::walk_files(repo_path, &mut |path| {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { return };
        if lang_for_ext(&ext.to_lowercase()).is_none() { return; }
        let Some((lang, source, tree)) = parse_file(path) else { return };
        let query_src = function_query(lang);
        let Ok(query) = Query::new(&ts_language(lang), query_src) else { return };
        let name_idx = query.capture_index_for_name("name");
        let params_idx = query.capture_index_for_name("params");
        let func_idx = query.capture_index_for_name("function");
        let mut cursor = QueryCursor::new();
        let src_bytes = source.as_bytes();
        let mut matches = cursor.matches(&query, tree.root_node(), src_bytes);
        let rel = rel_path(repo_path, path);
        while let Some(m) = matches.next() {
            let mut name = None;
            let mut params_node = None;
            let mut func_node = None;
            for cap in m.captures {
                if Some(cap.index) == name_idx { name = cap.node.utf8_text(src_bytes).ok().map(str::to_string); }
                if Some(cap.index) == params_idx { params_node = Some(cap.node); }
                if Some(cap.index) == func_idx { func_node = Some(cap.node); }
            }
            let (Some(name), Some(fnode)) = (name, func_node) else { continue };
            let args = params_node.map(|p| {
                let mut tc = p.walk();
                p.named_children(&mut tc)
                    .filter_map(|c| c.utf8_text(src_bytes).ok().map(|t| t.trim().to_string()))
                    .collect::<Vec<_>>()
            }).unwrap_or_default();
            let start_line = fnode.start_position().row + 1;
            let end_line = fnode.end_position().row + 1;
            out.push(FunctionInfo {
                name, args,
                file: rel.clone(),
                line: start_line,
                length_lines: end_line.saturating_sub(start_line) + 1,
                ref_count: 0,
            });
        }
    });

    let mut counts: HashMap<String, usize> = HashMap::new();
    crate::util::walk_files(repo_path, &mut |path| {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { return };
        let Some(lang) = lang_for_ext(&ext.to_lowercase()) else { return };
        let Some((_, source, tree)) = parse_file(path) else { return };
        let Ok(query) = Query::new(&ts_language(lang), identifier_query(lang)) else { return };
        let id_idx = query.capture_index_for_name("id");
        let mut cursor = QueryCursor::new();
        let src_bytes = source.as_bytes();
        let mut matches = cursor.matches(&query, tree.root_node(), src_bytes);
        while let Some(m) = matches.next() {
            for cap in m.captures {
                if Some(cap.index) != id_idx { continue; }
                if let Ok(text) = cap.node.utf8_text(src_bytes) {
                    *counts.entry(text.to_string()).or_insert(0) += 1;
                }
            }
        }
    });
    for f in &mut out {
        f.ref_count = counts.get(&f.name).copied().unwrap_or(0);
    }

    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

pub fn find_usages(repo_path: &Path, identifier: &str) -> Vec<UsageLocation> {
    let mut out = Vec::new();
    crate::util::walk_files(repo_path, &mut |path| {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { return };
        let Some(lang) = lang_for_ext(&ext.to_lowercase()) else { return };
        let Some((_, source, tree)) = parse_file(path) else { return };
        let Ok(query) = Query::new(&ts_language(lang), identifier_query(lang)) else { return };
        let id_idx = query.capture_index_for_name("id");
        let mut cursor = QueryCursor::new();
        let src_bytes = source.as_bytes();
        let mut matches = cursor.matches(&query, tree.root_node(), src_bytes);
        let rel = rel_path(repo_path, path);
        let lines: Vec<&str> = source.lines().collect();
        while let Some(m) = matches.next() {
            for cap in m.captures {
                if Some(cap.index) != id_idx { continue; }
                let Ok(text) = cap.node.utf8_text(src_bytes) else { continue };
                if text != identifier { continue; }
                let pos = cap.node.start_position();
                let context: String = lines.get(pos.row).map(|s| s.trim()).unwrap_or("").chars().take(100).collect();
                out.push(UsageLocation { file: rel.clone(), line: pos.row + 1, col: pos.column + 1, context });
            }
        }
    });
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

fn build_rename_edits(repo_path: &Path, renames: &HashMap<String, String>) -> Vec<RenameEdit> {
    let mut edits = Vec::new();
    crate::util::walk_files(repo_path, &mut |path| {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { return };
        let Some(lang) = lang_for_ext(&ext.to_lowercase()) else { return };
        let Some((_, source, tree)) = parse_file(path) else { return };
        let Ok(query) = Query::new(&ts_language(lang), identifier_query(lang)) else { return };
        let id_idx = query.capture_index_for_name("id");
        let mut cursor = QueryCursor::new();
        let src_bytes = source.as_bytes();
        let mut matches = cursor.matches(&query, tree.root_node(), src_bytes);
        while let Some(m) = matches.next() {
            for cap in m.captures {
                if Some(cap.index) != id_idx { continue; }
                let Ok(text) = cap.node.utf8_text(src_bytes) else { continue };
                let Some(new_name) = renames.get(text) else { continue };
                let pos = cap.node.start_position();
                edits.push(RenameEdit {
                    file: path.to_path_buf(),
                    line: pos.row + 1,
                    col: pos.column + 1,
                    start_byte: cap.node.start_byte(),
                    end_byte: cap.node.end_byte(),
                    old_text: text.to_string(),
                    new_text: new_name.clone(),
                });
            }
        }
    });
    edits.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    edits
}

pub fn build_rename_plan(repo_path: &Path, old: &str, new: &str) -> RenamePlan {
    let mut renames = HashMap::new();
    renames.insert(old.to_string(), new.to_string());
    RenamePlan { edits: build_rename_edits(repo_path, &renames) }
}

pub fn build_convention_plan(repo_path: &Path, target: NamingConvention) -> RenamePlan {
    let funcs = list_functions(repo_path, FuncFormat::NamesOnly);
    let mut renames: HashMap<String, String> = HashMap::new();
    for f in &funcs {
        if renames.contains_key(&f.name) { continue; }
        if NamingConvention::detect(&f.name) == Some(target) { continue; }
        let new_name = target.convert(&f.name);
        if new_name != f.name && !new_name.is_empty() {
            renames.insert(f.name.clone(), new_name);
        }
    }
    RenamePlan { edits: build_rename_edits(repo_path, &renames) }
}

pub fn apply_rename_plan(plan: &RenamePlan) -> std::io::Result<usize> {
    use std::collections::BTreeMap;
    let mut by_file: BTreeMap<&PathBuf, Vec<&RenameEdit>> = BTreeMap::new();
    for e in &plan.edits {
        by_file.entry(&e.file).or_default().push(e);
    }
    let mut applied = 0usize;
    for (file, mut edits) in by_file {
        edits.sort_by_key(|e| std::cmp::Reverse(e.start_byte));
        let mut bytes = std::fs::read(file)?;
        for e in edits {
            if e.start_byte > e.end_byte || e.end_byte > bytes.len() { continue; }
            bytes.splice(e.start_byte..e.end_byte, e.new_text.bytes());
            applied += 1;
        }
        std::fs::write(file, bytes)?;
    }
    Ok(applied)
}

#[derive(Debug, Clone, Default)]
pub struct RepoCodeStats {
    pub total_functions: usize,
    pub total_files: usize,
    pub lang_counts: Vec<(String, usize)>,
    pub avg_fn_len: f64,
    pub longest_functions: Vec<(String, String, usize)>,
    pub largest_files: Vec<(String, usize)>,
}

pub fn repo_stats(repo_path: &Path) -> RepoCodeStats {
    let funcs = list_functions(repo_path, FuncFormat::NamesOnly);

    let mut lang_counts: HashMap<String, usize> = HashMap::new();
    let mut file_loc: HashMap<String, usize> = HashMap::new();
    let mut total_files = 0usize;
    crate::util::walk_files(repo_path, &mut |path| {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { return };
        let Some(lang) = lang_for_ext(&ext.to_lowercase()) else { return };
        total_files += 1;
        *lang_counts.entry(lang.label().to_string()).or_insert(0) += 1;
        if let Ok(content) = std::fs::read_to_string(path) {
            file_loc.insert(rel_path(repo_path, path), content.lines().count());
        }
    });

    let total_functions = funcs.len();
    let avg_fn_len = if total_functions > 0 {
        funcs.iter().map(|f| f.length_lines).sum::<usize>() as f64 / total_functions as f64
    } else { 0.0 };

    let mut longest_functions: Vec<(String, String, usize)> = funcs.iter()
        .map(|f| (f.file.clone(), f.name.clone(), f.length_lines)).collect();
    longest_functions.sort_by_key(|(_, _, lines)| std::cmp::Reverse(*lines));
    longest_functions.truncate(10);

    let mut largest_files: Vec<(String, usize)> = file_loc.into_iter().collect();
    largest_files.sort_by_key(|(_, loc)| std::cmp::Reverse(*loc));
    largest_files.truncate(10);

    let mut lang_counts: Vec<(String, usize)> = lang_counts.into_iter().collect();
    lang_counts.sort_by_key(|(_, files)| std::cmp::Reverse(*files));

    RepoCodeStats { total_functions, total_files, lang_counts, avg_fn_len, longest_functions, largest_files }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST_SRC: &str = "fn fetch_data(url: &str, retries: u32) -> String {\n    let _ = retries;\n    url.to_string()\n}\n\nfn main() {\n    let label = \"fetch_data\";\n    // fetch_data is called below\n    fetch_data(label, 1);\n}\n";

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn sample_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "src/lib.rs", RUST_SRC);
        write(dir.path(), "app.py", "def load_config(path, strict=False):\n    return path\n");
        write(dir.path(), "main.go", "package main\n\nfunc Add(a int, b int) int { return a + b }\n");
        write(dir.path(), "web/ui.ts", "export const toUpper = (s: string): string => s.toUpperCase();\nfunction greet(name: string) { return name; }\n");
        write(dir.path(), "target/gen.rs", "fn generated() {}\n");
        write(dir.path(), "README.md", "fetch_data\n");
        dir
    }

    #[test]
    fn every_language_query_compiles() {
        for lang in [Lang::Rust, Lang::Python, Lang::JavaScript, Lang::TypeScript, Lang::Tsx, Lang::Go] {
            Query::new(&ts_language(lang), function_query(lang)).unwrap_or_else(|e| panic!("{lang:?}: {e}"));
            Query::new(&ts_language(lang), identifier_query(lang)).unwrap_or_else(|e| panic!("{lang:?}: {e}"));
        }
    }

    #[test]
    fn functions_are_listed_across_languages() {
        let dir = sample_repo();
        let funcs = list_functions(dir.path(), FuncFormat::WithMeta);
        let summary: Vec<(&str, &str, usize)> = funcs.iter().map(|f| (f.file.as_str(), f.name.as_str(), f.line)).collect();
        assert_eq!(summary, [
            ("app.py", "load_config", 1),
            ("main.go", "Add", 3),
            ("src/lib.rs", "fetch_data", 1),
            ("src/lib.rs", "main", 6),
            ("web/ui.ts", "toUpper", 1),
            ("web/ui.ts", "greet", 2),
        ]);
        let fetch = &funcs[2];
        assert_eq!(fetch.args, ["url: &str", "retries: u32"]);
        assert_eq!(fetch.length_lines, 4);
        assert_eq!(fetch.ref_count, 2);
        assert_eq!(funcs[0].args, ["path", "strict=False"]);
        assert_eq!(funcs[1].args, ["a int", "b int"]);
    }

    #[test]
    fn usages_skip_strings_and_comments() {
        let dir = sample_repo();
        let usages = find_usages(dir.path(), "fetch_data");
        let places: Vec<(&str, usize, usize)> = usages.iter().map(|u| (u.file.as_str(), u.line, u.col)).collect();
        assert_eq!(places, [("src/lib.rs", 1, 4), ("src/lib.rs", 9, 5)]);
        assert_eq!(usages[1].context, "fetch_data(label, 1);");
        assert!(find_usages(dir.path(), "generated").is_empty());
    }

    #[test]
    fn rename_rewrites_identifiers_only() {
        let dir = sample_repo();
        let plan = build_rename_plan(dir.path(), "fetch_data", "download");
        assert_eq!(plan.edits.len(), 2);
        assert_eq!(plan.file_count(), 1);
        assert_eq!(apply_rename_plan(&plan).unwrap(), 2);

        let src = std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
        assert!(src.starts_with("fn download(url: &str"));
        assert!(src.contains("    download(label, 1);"));
        assert!(src.contains("\"fetch_data\""));
        assert!(src.contains("// fetch_data is called below"));
        assert_eq!(std::fs::read_to_string(dir.path().join("README.md")).unwrap(), "fetch_data\n");
        assert!(build_rename_plan(dir.path(), "fetch_data", "x").edits.is_empty());
    }

    #[test]
    fn convention_plan_renames_functions_and_call_sites() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "lib.rs", "fn fetchUserData() {}\nfn already_snake() {}\nfn main() { fetchUserData(); already_snake(); }\n");
        let plan = build_convention_plan(dir.path(), NamingConvention::Snake);
        assert!(plan.edits.iter().all(|e| e.old_text == "fetchUserData" && e.new_text == "fetch_user_data"));
        assert_eq!(apply_rename_plan(&plan).unwrap(), 2);
        let src = std::fs::read_to_string(dir.path().join("lib.rs")).unwrap();
        assert_eq!(src, "fn fetch_user_data() {}\nfn already_snake() {}\nfn main() { fetch_user_data(); already_snake(); }\n");
    }

    #[test]
    fn naming_conventions_convert_between_each_other() {
        assert_eq!(NamingConvention::Camel.convert("fetch_user_id"), "fetchUserId");
        assert_eq!(NamingConvention::Pascal.convert("fetchUserId"), "FetchUserId");
        assert_eq!(NamingConvention::Snake.convert("FetchUserId"), "fetch_user_id");
        assert_eq!(NamingConvention::detect("load_all"), Some(NamingConvention::Snake));
        assert_eq!(NamingConvention::detect("Loader"), Some(NamingConvention::Pascal));
        assert_eq!(NamingConvention::detect("loader"), Some(NamingConvention::Camel));
        assert_eq!(NamingConvention::detect(""), None);
    }

    #[test]
    fn stats_aggregate_supported_files() {
        let dir = sample_repo();
        let stats = repo_stats(dir.path());
        assert_eq!(stats.total_functions, 6);
        assert_eq!(stats.total_files, 4);
        assert_eq!(stats.largest_files[0], ("src/lib.rs".to_string(), 10));
        assert_eq!(stats.longest_functions[0].1, "main");
        assert_eq!(stats.lang_counts.len(), 4);
    }
}
