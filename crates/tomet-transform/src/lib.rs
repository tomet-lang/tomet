//! Pure in-memory AST refactoring and structural transformations for Tomet documents.
//!
//! Provides:
//! - (expressions)| Every `${...}` in a document resolved once, in one pass, with the
//!                | evaluator's typed values put back according to where each expression sits.
//! - (macro_rewrite) Reverse URL-to-macro pattern matching and rewriting.
//! - (directive) Directive promotion (`::meta.type` -> `::kind`) and Value DSL normalization.
//! - (structural) Structural AST query matching and in-place transformations (rename tag, rename key, replace value).
//! - (meta) Batch `::meta` key/value updates.

pub mod blueprint;
pub mod directive;
pub mod expressions;
pub mod links;
pub mod macro_rewrite;
pub mod meta;
pub mod structural;

pub use blueprint::*;
pub use directive::*;
pub use expressions::{Unresolved, UnresolvedKind, resolve_expressions, resolve_expressions_where};
pub use links::*;
pub use macro_rewrite::*;
pub use meta::*;
pub use structural::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tomet_ast::Value;

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
            Some("$macro.gh(\"tomet/tomet\")".into())
        );
        assert_eq!(
            set.rewrite_url("https://github.com/tomet/tomet/issues/42"),
            Some("$macro.gh_issue(\"tomet/tomet\", 42)".into())
        );
    }

    #[test]
    fn test_promote_meta_type_to_kind() {
        let mut doc = parse_doc("::version(1.0)\n::meta{\n  type: task\n  id: doc-1\n}\n");
        assert!(promote_meta_type_to_kind(&mut doc));

        let mut names = Vec::new();
        tomet_tree::for_each_top_level_declaration(&doc, |declaration| {
            names.push(declaration.name.clone());
        });
        assert!(names.iter().any(|n| n.is_bare_named("kind")));
        assert!(names.iter().any(|n| n.is_bare_named("version")));
        assert!(names.iter().any(|n| n.is_bare_named("meta")));
    }

    #[test]
    fn test_promote_meta_id_to_id() {
        let mut doc = parse_doc("::meta{\n  id: doc-1\n  title: T\n}\n");
        assert!(promote_meta_id_to_id(&mut doc));

        let mut id = None;
        let mut meta_keys = Vec::new();
        tomet_tree::for_each_top_level_declaration(&doc, |declaration| {
            if declaration.name.is_bare_named("id") {
                id = declaration.data.args.clone();
            }
            if declaration.name.is_bare_named("meta")
                && let Some(Value::Map(entries)) =
                    tomet_semantics::data::declaration_data(declaration)
            {
                meta_keys = entries.into_iter().map(|(key, _)| key).collect();
            }
        });
        assert_eq!(id, Some(Value::String("doc-1".into())));
        assert_eq!(meta_keys, vec!["title".to_string()]);

        // Nothing to promote: no change.
        assert!(!promote_meta_id_to_id(&mut parse_doc(
            "::meta{ title: T }\n"
        )));
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
