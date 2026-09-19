//! Pure in-memory AST refactoring and structural transformations for Tomet documents.
//!
//! Provides:
//! - **[`macro_rewrite`]**: Reverse URL-to-macro pattern matching and rewriting.
//! - **[`directive`]**: Directive promotion (`@meta.type` -> `@kind`) and Value DSL normalization.
//! - **[`structural`]**: Structural AST query matching and in-place transformations (rename tag, rename key, replace value).
//! - **[`meta`]**: Batch `@meta` key/value updates.
//! - **[`index_query`]**: `${filter(...)}` in an `@kind(index)` document,
//!   expanded into the `@file` entries it selects.

pub mod blueprint;
pub mod directive;
pub mod index_query;
pub mod interp;
pub mod links;
pub mod macro_expand;
pub mod macro_rewrite;
pub mod meta;
pub mod structural;

pub use blueprint::*;
pub use directive::*;
pub use index_query::*;
pub use interp::*;
pub use links::*;
pub use macro_expand::*;
pub use macro_rewrite::*;
pub use meta::*;
pub use structural::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tomet_ast::{Block, Value};

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
        // `@version` and `@meta` sit on adjacent lines with no blank line
        // or `;` between them, so they join into one `Block::Paragraph`
        // under the default placement rule (`docs/spec/syntax.tmt`'s
        // `##[ 区切り ]`) -- `doc.blocks.len()` no longer tracks "how many
        // top-level declarations", only `for_each_top_level_element` does.
        let mut doc = parse_doc("@version(1.0)\n@meta{\n  type: task\n  id: doc-1\n}\n");
        assert!(promote_meta_type_to_kind(&mut doc));

        let mut sigils = Vec::new();
        tomet_tree::for_each_top_level_element(&doc, |el| {
            sigils.push(el.sigil.clone());
        });
        assert!(sigils.iter().any(|s| s.is_bare_named("kind")));
        assert!(sigils.iter().any(|s| s.is_bare_named("version")));
        assert!(sigils.iter().any(|s| s.is_bare_named("meta")));
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

    #[test]
    fn test_expand_document_macros() {
        let mut doc = parse_doc("- @link($macro.https(\"www.kaggle.com\"))\n");
        let mut config = tomet_semantics::DocumentConfig::default();
        config.macros.insert("https".into(), "https://${1}".into());

        expand_document_macros(&mut doc, &config);

        let Block::Element(ul_el) = &doc.blocks[0] else {
            panic!()
        };
        let items = ul_el.value.as_ref().unwrap().as_children();
        let item_el = items[0];
        let content = item_el.content.as_ref().unwrap();
        let tomet_ast::Inline::Element(link_el) = &content[0] else {
            panic!()
        };
        assert_eq!(
            link_el.args,
            Some(Value::String("https://www.kaggle.com".into()))
        );
    }
}
