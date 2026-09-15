//! Supported programming languages and their comment syntax rules.

use std::path::Path;

/// Supported languages for comment extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Java,
    C,
    Cpp,
    Go,
    Shell,
    Toml,
    Yaml,
    Sql,
    Lua,
    Html,
}

impl Language {
    /// Detects language from a file path based on its extension or filename.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase());
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_ascii_lowercase());

        if let Some(name) = file_name.as_deref() {
            match name {
                "dockerfile" | "containerfile" => return Some(Language::Shell),
                "justfile" => return Some(Language::Shell),
                _ => {}
            }
        }

        match ext.as_deref() {
            Some("rs") => Some(Language::Rust),
            Some("py" | "pyi") => Some(Language::Python),
            Some("js" | "mjs" | "cjs" | "jsx") => Some(Language::JavaScript),
            Some("ts" | "mts" | "cts" | "tsx") => Some(Language::TypeScript),
            Some("java") => Some(Language::Java),
            Some("c" | "h") => Some(Language::C),
            Some("cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx") => Some(Language::Cpp),
            Some("go") => Some(Language::Go),
            Some("sh" | "bash" | "zsh") => Some(Language::Shell),
            Some("toml") => Some(Language::Toml),
            Some("yaml" | "yml") => Some(Language::Yaml),
            Some("sql") => Some(Language::Sql),
            Some("lua") => Some(Language::Lua),
            Some("html" | "xml" | "svg") => Some(Language::Html),
            _ => None,
        }
    }

    /// Returns the comment syntax configuration for this language.
    pub fn syntax(&self) -> CommentSyntax {
        match self {
            Language::Rust => CommentSyntax {
                doc_line_prefixes: vec!["///", "//!"],
                regular_line_prefixes: vec!["//"],
                doc_block_delimiters: vec![("/**", "*/")],
                regular_block_delimiters: vec![("/*", "*/")],
            },
            Language::Python => CommentSyntax {
                doc_line_prefixes: vec![],
                regular_line_prefixes: vec!["#"],
                doc_block_delimiters: vec![("\"\"\"", "\"\"\""), ("'''", "'''")],
                regular_block_delimiters: vec![],
            },
            Language::JavaScript | Language::TypeScript | Language::Java => CommentSyntax {
                doc_line_prefixes: vec![],
                regular_line_prefixes: vec!["//"],
                doc_block_delimiters: vec![("/**", "*/")],
                regular_block_delimiters: vec![("/*", "*/")],
            },
            Language::C | Language::Cpp | Language::Go => CommentSyntax {
                doc_line_prefixes: vec![],
                regular_line_prefixes: vec!["//"],
                doc_block_delimiters: vec![],
                regular_block_delimiters: vec![("/*", "*/")],
            },
            Language::Shell | Language::Toml | Language::Yaml => CommentSyntax {
                doc_line_prefixes: vec![],
                regular_line_prefixes: vec!["#"],
                doc_block_delimiters: vec![],
                regular_block_delimiters: vec![],
            },
            Language::Sql => CommentSyntax {
                doc_line_prefixes: vec![],
                regular_line_prefixes: vec!["--"],
                doc_block_delimiters: vec![],
                regular_block_delimiters: vec![("/*", "*/")],
            },
            Language::Lua => CommentSyntax {
                doc_line_prefixes: vec![],
                regular_line_prefixes: vec!["--"],
                doc_block_delimiters: vec![],
                regular_block_delimiters: vec![("--[[", "]]")],
            },
            Language::Html => CommentSyntax {
                doc_line_prefixes: vec![],
                regular_line_prefixes: vec![],
                doc_block_delimiters: vec![],
                regular_block_delimiters: vec![("<!--", "-->")],
            },
        }
    }
}

/// Comment syntax definitions for a language.
#[derive(Debug, Clone)]
pub struct CommentSyntax {
    /// Line prefixes treated specifically as documentation (e.g. `///`, `//!`).
    pub doc_line_prefixes: Vec<&'static str>,
    /// Line prefixes for regular line comments (e.g. `//`, `#`, `--`).
    pub regular_line_prefixes: Vec<&'static str>,
    /// Block delimiters treated as documentation (e.g. `/** ... */`, `""" ... """`).
    pub doc_block_delimiters: Vec<(&'static str, &'static str)>,
    /// Regular block comment delimiters (e.g. `/* ... */`, `<!-- ... -->`).
    pub regular_block_delimiters: Vec<(&'static str, &'static str)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(
            Language::from_path(Path::new("src/lib.rs")),
            Some(Language::Rust)
        );
        assert_eq!(
            Language::from_path(Path::new("app/main.py")),
            Some(Language::Python)
        );
        assert_eq!(
            Language::from_path(Path::new("web/index.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            Language::from_path(Path::new("web/index.js")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            Language::from_path(Path::new("server/App.java")),
            Some(Language::Java)
        );
        assert_eq!(
            Language::from_path(Path::new("script.sh")),
            Some(Language::Shell)
        );
        assert_eq!(
            Language::from_path(Path::new("config.toml")),
            Some(Language::Toml)
        );
        assert_eq!(Language::from_path(Path::new("unknown.xyz")), None);
    }
}
