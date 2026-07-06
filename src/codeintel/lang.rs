//! Per-language tree-sitter wiring: which grammar handles which extension,
//! and the query used to find function/method definitions in each one.
//! Adding a language means adding one match arm here — `mod.rs` never needs
//! to know about grammar internals.

/// A source language with tree-sitter support wired up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Go,
}

impl Lang {
    /// Display name shown in repo-stats language breakdowns.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Rust       => "Rust",
            Self::Python     => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Tsx        => "TSX",
            Self::Go         => "Go",
        }
    }
}

/// Maps a lowercased file extension to its `Lang`, if supported.
pub fn lang_for_ext(ext: &str) -> Option<Lang> {
    match ext {
        "rs"            => Some(Lang::Rust),
        "py" | "pyw"    => Some(Lang::Python),
        "js" | "mjs" | "cjs" | "jsx" => Some(Lang::JavaScript),
        "ts"            => Some(Lang::TypeScript),
        "tsx"           => Some(Lang::Tsx),
        "go"            => Some(Lang::Go),
        _ => None,
    }
}

/// The tree-sitter grammar for `lang`.
pub fn ts_language(lang: Lang) -> tree_sitter::Language {
    match lang {
        Lang::Rust       => tree_sitter_rust::LANGUAGE.into(),
        Lang::Python     => tree_sitter_python::LANGUAGE.into(),
        Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Lang::Tsx        => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Lang::Go         => tree_sitter_go::LANGUAGE.into(),
    }
}

/// Query that captures every function/method definition: `@function` for the
/// whole node (used for line span + body text), `@name` for its identifier,
/// `@params` for its parameter list.
pub fn function_query(lang: Lang) -> &'static str {
    match lang {
        Lang::Rust => {
            "(function_item name: (identifier) @name parameters: (parameters) @params) @function"
        }
        Lang::Python => {
            "(function_definition name: (identifier) @name parameters: (parameters) @params) @function"
        }
        Lang::JavaScript => {
            "[
                (function_declaration name: (identifier) @name parameters: (formal_parameters) @params) @function
                (method_definition name: (property_identifier) @name parameters: (formal_parameters) @params) @function
                (variable_declarator name: (identifier) @name value: (arrow_function parameters: (formal_parameters) @params)) @function
            ]"
        }
        Lang::TypeScript | Lang::Tsx => {
            "[
                (function_declaration name: (identifier) @name parameters: (formal_parameters) @params) @function
                (method_definition name: (property_identifier) @name parameters: (formal_parameters) @params) @function
                (variable_declarator name: (identifier) @name value: (arrow_function parameters: (formal_parameters) @params)) @function
            ]"
        }
        Lang::Go => {
            "[
                (function_declaration name: (identifier) @name parameters: (parameter_list) @params) @function
                (method_declaration name: (field_identifier) @name parameters: (parameter_list) @params) @function
            ]"
        }
    }
}

/// Query that captures every plain identifier reference/declaration node —
/// deliberately excludes member/property-name node kinds (e.g. JS's
/// `property_identifier` used for `obj.foo`), so a rename of a function
/// named `foo` won't also touch an unrelated object property called `foo`.
pub fn identifier_query(_lang: Lang) -> &'static str {
    "(identifier) @id"
}
