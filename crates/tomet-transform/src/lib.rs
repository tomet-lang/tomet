//! Pure in-memory AST refactoring and structural transformations for Tomet documents.
//!
//! Provides:
//! - (expressions)| Every `{$ ... }` in a document resolved once, in one pass, with the
//!                | evaluator's typed values put back according to where each expression sits.
//! - (source)     | The clock and the random source a blueprint's `default: auto` is made from, behind
//!                | one trait: the real ones, and a fixed one for a test. Nothing else in the crate
//!                | reads a clock or draws.
//! - (seats)      | Every type seat (`<$type(...)>`) of a blueprint filled from its source, or each
//!                | one that cannot be reported, so a document made from a blueprint has none.
//! - (macro_rewrite) Reverse URL-to-macro pattern matching and rewriting.
//! - (directive) Directive promotion (`::meta.type` -> `::kind`) and Value DSL normalization.
//! - (structural) Structural AST query matching and in-place transformations (rename tag, rename key, replace value).
//! - (meta) Batch `::meta` key/value updates.

#![allow(clippy::doc_overindented_list_items)]

pub mod blueprint;
pub mod directive;
pub mod expressions;
pub mod links;
pub mod macro_rewrite;
pub mod meta;
pub mod seats;
pub mod source;
pub mod structural;

pub use blueprint::*;
pub use directive::*;
pub use expressions::{Unresolved, UnresolvedKind, resolve_expressions, resolve_expressions_where};
pub use links::*;
pub use macro_rewrite::*;
pub use meta::*;
pub use seats::{SeatProblem, SeatReason};
pub use source::{FixedSource, Source, SystemSource};
pub use structural::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tomet_ast::Value;

    fn parse_doc(src: &str) -> tomet_ast::File {
        tomet_ast_parser::parse_file(src).expect("valid source")
    }

    /// The value `call` stands for where an element's args hold it.
    fn macro_call(call: &str) -> Value {
        parse_doc(&format!("@x({call})\n"))
            .body
            .into_iter()
            .find_map(|block| match block {
                tomet_ast::Block::Element(el) => el.data.args,
                _ => None,
            })
            .expect("an element with args")
    }

    #[test]
    fn an_argument_stays_an_integer_only_when_it_is_written_as_one() {
        let macros = HashMap::from([("p".to_string(), Value::String("https://x/$(1)".into()))]);
        let set = MacroSet::from_map(&macros);
        for (url, call) in [
            ("https://x/7", "$macro.p(7)"),
            ("https://x/007", "$macro.p(\"007\")"),
            ("https://x/+7", "$macro.p(\"+7\")"),
            ("https://x/a\"b)", "$macro.p(\"a\\\"b)\")"),
        ] {
            assert_eq!(
                set.rewrite_url(url).map(Value::Interp),
                Some(macro_call(call)),
                "{url}"
            );
        }
    }

    #[test]
    fn test_macro_pattern_and_set() {
        let mut macros = HashMap::new();
        macros.insert("gh".into(), Value::String("https://github.com/$(1)".into()));
        macros.insert(
            "gh_issue".into(),
            Value::String("https://github.com/$(1)/issues/$(2)".into()),
        );

        let set = MacroSet::from_map(&macros);
        assert_eq!(
            set.rewrite_url("https://github.com/tomet/tomet")
                .map(Value::Interp),
            Some(macro_call("$macro.gh(\"tomet/tomet\")"))
        );
        assert_eq!(
            set.rewrite_url("https://github.com/tomet/tomet/issues/42")
                .map(Value::Interp),
            Some(macro_call("$macro.gh_issue(\"tomet/tomet\", 42)"))
        );

        let mut doc = parse_doc(
            "@link(\"https://github.com/tomet/tomet\")[repo]\n@embed(target: \"https://github.com/tomet/tomet/issues/42\")\n@note(\"https://github.com/tomet/tomet\")\n",
        );
        assert_eq!(transform_link_targets_with_macros(&mut doc, &set), 2);

        use tomet_tree::DataExt;
        let mut results = Vec::new();
        tomet_tree::for_each_element(&doc, |el| {
            let val = el
                .data
                .get_attr("target")
                .or(el.data.args.as_ref())
                .cloned();
            results.push((el.name.name.as_str().to_string(), val));
        });
        assert_eq!(
            results,
            vec![
                (
                    "link".into(),
                    Some(macro_call("$macro.gh(\"tomet/tomet\")"))
                ),
                (
                    "embed".into(),
                    Some(macro_call("$macro.gh_issue(\"tomet/tomet\", 42)"))
                ),
                (
                    "note".into(),
                    Some(Value::String("https://github.com/tomet/tomet".into()))
                ),
            ]
        );
    }

    #[test]
    fn test_promote_meta_type_to_kind() {
        let mut doc = parse_doc("::version(1.0)\n::meta(\n  type: task\n  id: doc-1\n)\n");
        assert!(promote_meta_type_to_kind(&mut doc));

        let mut names = Vec::new();
        tomet_tree::for_each_declaration(&doc, |declaration| {
            names.push(declaration.name.clone());
        });
        assert!(names.iter().any(|n| n.is_bare_named("kind")));
        assert!(names.iter().any(|n| n.is_bare_named("version")));
        assert!(names.iter().any(|n| n.is_bare_named("meta")));
    }

    #[test]
    fn test_promote_meta_id_to_id() {
        let mut doc = parse_doc("::meta(\n  id: doc-1\n  title: T\n)\n");
        assert!(promote_meta_id_to_id(&mut doc));

        let mut id = None;
        let mut meta_keys = Vec::new();
        tomet_tree::for_each_declaration(&doc, |declaration| {
            if declaration.name.is_bare_named("id") {
                id = declaration.data.args.clone();
            }
            if declaration.name.is_bare_named("meta")
                && let Some(Value::Map(entries)) =
                    tomet_semantics::data::declaration_entries(declaration)
            {
                meta_keys = entries.into_iter().map(|(key, _)| key).collect();
            }
        });
        assert_eq!(id, Some(Value::String("doc-1".into())));
        assert_eq!(meta_keys, vec!["title".to_string()]);

        // Nothing to promote: no change.
        assert!(!promote_meta_id_to_id(&mut parse_doc("::meta(title: T)\n")));
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
