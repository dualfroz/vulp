//! Tree-sitter-based code intelligence: list functions, find usages, rename
//! an identifier, convert naming convention, and aggregate repo stats.
//!
//! Tree-sitter (not regex/naive find-replace) is the explicit choice here:
//! it parses real syntax trees, so a rename only ever touches genuine
//! identifier nodes — never matches inside strings/comments, and never
//! confuses a function name with an unrelated object property of the same
//! name (see `lang::identifier_query`). Every walk also skips
//! `crate::util::EXCLUDED_DIRS` (`.git`, `target`, `node_modules`, `vendor`,
//! ...), so rename/find-usages never reach into dependency code.

pub mod lang;

use lang::{function_query, identifier_query, lang_for_ext, ts_language, Lang};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

/// Verbosity level for `list_functions`' display lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuncFormat {
    NamesOnly,
    WithArgs,
    WithMeta,
}

/// One function/method definition found in a repo.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub args: Vec<String>,
    pub file: String,
    pub line: usize,
    pub length_lines: usize,
    pub ref_count: usize,
}

/// One place an identifier is referenced, found by `find_usages`.
#[derive(Debug, Clone)]
pub struct UsageLocation {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub context: String,
}

/// One identifier-node rewrite: the byte range to replace in `file` and
/// what to replace it with. `start_byte`/`end_byte` are what `apply_rename_plan`
/// actually splices on; `line`/`col` are only for the human-readable preview.
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

/// A dry-run set of rename edits, shown to the user as a preview before
/// `apply_rename_plan` writes anything to disk.
#[derive(Debug, Clone, Default)]
pub struct RenamePlan {
    pub edits: Vec<RenameEdit>,
}

impl RenamePlan {
    /// Number of distinct files this plan touches.
    pub fn file_count(&self) -> usize {
        let mut files: Vec<&PathBuf> = self.edits.iter().map(|e| &e.file).collect();
        files.sort();
        files.dedup();
        files.len()
    }
}

/// A function-name casing convention, used as the target for
/// `build_convention_plan`'s bulk rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingConvention {
    Snake,
    Camel,
    Pascal,
}

impl NamingConvention {
    /// Display label shown in the convention picker.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Snake  => "snake_case",
            Self::Camel  => "camelCase",
            Self::Pascal => "PascalCase",
        }
    }

    /// Guesses a name's current convention from its shape (contains `_` →
    /// snake_case; starts uppercase → PascalCase; starts lowercase →
    /// camelCase). Returns `None` for ambiguous/empty names, so
    /// `build_convention_plan` can skip them rather than mis-convert.
    fn detect(name: &str) -> Option<Self> {
        if name.is_empty() { return None; }
        if name.contains('_') { return Some(Self::Snake); }
        match name.chars().next() {
            Some(c) if c.is_uppercase() => Some(Self::Pascal),
            Some(c) if c.is_lowercase() => Some(Self::Camel),
            _ => None,
        }
    }

    /// Splits `name` into words (via `split_words`) and rejoins them in this
    /// convention.
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

/// Reads and parses `path` with the tree-sitter grammar matching its
/// extension. Returns `None` for unsupported extensions or unreadable/
/// unparseable files, rather than an error — every caller is a best-effort
/// repo-wide walk where one bad file shouldn't abort the whole scan.
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

/// Every function/method definition found in `repo_path`. `ref_count` is the
/// number of identifier-reference nodes across the whole repo matching that
/// function's name (a coarse cross-reference count — not full call-graph
/// resolution, but enough to flag "this is referenced nowhere" or "this is
/// referenced everywhere").
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

/// Every place `identifier` appears as a plain identifier reference or
/// declaration (not a string, comment, or unrelated member-property name).
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

/// Single pass over the repo collecting a `RenameEdit` for every identifier
/// node whose text is a key of `renames`. Shared by `build_rename_plan`
/// (one old→new pair) and `build_convention_plan` (many pairs at once) so
/// converting an entire repo's naming convention costs one walk, not one
/// walk per renamed identifier.
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

/// Builds a dry-run plan for renaming `old` to `new`. Does not touch disk —
/// call `apply_rename_plan` after the caller has shown the preview and
/// gotten an explicit confirmation.
pub fn build_rename_plan(repo_path: &Path, old: &str, new: &str) -> RenamePlan {
    let mut renames = HashMap::new();
    renames.insert(old.to_string(), new.to_string());
    RenamePlan { edits: build_rename_edits(repo_path, &renames) }
}

/// Builds a dry-run plan converting every function name in `repo_path` that
/// doesn't already match `target` to that convention. Scoped to function
/// definitions found by `list_functions` (not every variable/binding in the
/// repo) — a deliberately bounded, predictable scope for a bulk rename.
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

/// Applies a previously-built plan: groups edits by file, applies each
/// file's edits back-to-front by byte offset (so earlier offsets in the same
/// file stay valid even when old/new names differ in length), and writes
/// each file once. Returns the number of edits actually applied.
pub fn apply_rename_plan(plan: &RenamePlan) -> std::io::Result<usize> {
    use std::collections::BTreeMap;
    let mut by_file: BTreeMap<&PathBuf, Vec<&RenameEdit>> = BTreeMap::new();
    for e in &plan.edits {
        by_file.entry(&e.file).or_default().push(e);
    }
    let mut applied = 0usize;
    for (file, mut edits) in by_file {
        edits.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));
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

/// Aggregate stats over a repo's source files, returned by `repo_stats`.
#[derive(Debug, Clone, Default)]
pub struct RepoCodeStats {
    pub total_functions: usize,
    pub total_files: usize,
    pub lang_counts: Vec<(String, usize)>,
    pub avg_fn_len: f64,
    pub longest_functions: Vec<(String, String, usize)>, // file, name, length_lines
    pub largest_files: Vec<(String, usize)>,             // file, loc
}

/// Read-only aggregate stats over every tree-sitter-supported source file in
/// `repo_path`. Unlike rename/find-usages, this is safe to run repo-wide
/// without any confirmation step — nothing is mutated.
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
    longest_functions.sort_by(|a, b| b.2.cmp(&a.2));
    longest_functions.truncate(10);

    let mut largest_files: Vec<(String, usize)> = file_loc.into_iter().collect();
    largest_files.sort_by(|a, b| b.1.cmp(&a.1));
    largest_files.truncate(10);

    let mut lang_counts: Vec<(String, usize)> = lang_counts.into_iter().collect();
    lang_counts.sort_by(|a, b| b.1.cmp(&a.1));

    RepoCodeStats { total_functions, total_files, lang_counts, avg_fn_len, longest_functions, largest_files }
}
