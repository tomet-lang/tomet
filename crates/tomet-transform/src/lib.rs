//! Pure in-memory AST refactoring and structural transformations for Tomet documents.
//!
//! Provides:
//! - **[`macro_rewrite`]**: Reverse URL-to-macro pattern matching and rewriting.
//! - **[`directive`]**: Directive promotion (`@meta.type` -> `@kind`) and Value DSL normalization.
//! - **[`structural`]**: Structural AST query matching and in-place transformations (rename tag, rename key, replace value).
//! - **[`meta`]**: Batch `@meta` key/value updates.

pub mod blueprint;
pub mod directive;
pub mod links;
pub mod macro_rewrite;
pub mod meta;
pub mod structural;

pub use blueprint::*;
pub use directive::*;
pub use links::*;
pub use macro_rewrite::*;
pub use meta::*;
pub use structural::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn parse_doc(src: &str) -> tomet_ast::Document {
        tomet_parser::parse_document(src).expect("valid source")
    }

    #[test]
    fn test_macro_pattern_and_set() {
        let mut macros = HashMap::new();
        macros.insert("gh".into(), "https://github.com/${1}".into());
        macros.insert(
            "gh_issue".into(),
            "https://github.com/${1}/issues/${2}".into(),
        );

        let set = MacroSet::from_map(&macros);
        assert_eq!(
            set.rewrite_url("https://github.com/tomet/tomet"),
            Some("$gh(\"tomet/tomet\")".into())
        );
        assert_eq!(
            set.rewrite_url("https://github.com/tomet/tomet/issues/42"),
            Some("$gh_issue(\"tomet/tomet\", 42)".into())
        );
    }

    #[test]
    fn test_promote_meta_type_to_kind() {
        let mut doc = parse_doc("@version(1.0)\n@meta{\n  type: task\n  id: doc-1\n}\n");
        assert!(promote_meta_type_to_kind(&mut doc));
        assert_eq!(doc.blocks.len(), 3);
    }

    #[test]
    fn test_structural_transform() {
        let mut doc = parse_doc("@note[Check this]\n");
        let action = StructuralAction::RenameTag {
            from: "note".into(),
            to: "warning".into(),
        };
        let count = apply_structural_action(&mut doc, &action);
        assert_eq!(count, 1);
    }
}
