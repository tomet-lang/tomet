//! Rust binding for the TypedMark tree-sitter grammar (`grammar.js` at
//! the crate root; `src/parser.c` etc. are generated from it via
//! `npx tree-sitter-cli@0.26.12 generate`, then compiled by `build.rs`).
//!
//! This grammar exists for editor syntax highlighting (Zed and other
//! tree-sitter-based editors), not as a source of truth -- that's
//! `typedmark-parser`'s job. It's a **reasonable approximation of the
//! real grammar, not a byte-for-byte match**; several things a
//! context-free grammar can't cheaply express are simplified away or
//! left as known-narrow parse-error cases:
//!
//! - **No flanking-delimiter whitespace rules.** `*em*`/`**strong**`/
//!   `==mark==` don't reject e.g. `* not em *` the way the real parser
//!   does; `_`/`__` also skip the "must follow a non-alphanumeric
//!   character" check.
//! - **No lazy paragraph continuation.** A paragraph ends at a blank
//!   line or EOF only, not at the next line merely *looking* like a new
//!   block. Every real fixture this grammar was tested against already
//!   puts a blank line between blocks, so this hasn't been observed to
//!   matter in practice.
//! - **`(input)`/`[area]`/`{value}` aren't capped at one each, in any
//!   order.** The real grammar allows each group at most once, in any
//!   order; here they're just `repeat(choice(...))`, so e.g. two
//!   `(input)` groups back to back would (harmlessly) both parse.
//! - **`em`/`strong` don't nest in each other.** `*`/`**`/`_`/`__` all
//!   share a delimiter character, and letting them nest is the classic
//!   Markdown emphasis/strong ambiguity that needs real flanking-rule
//!   lookahead (or an external scanner) to resolve properly.
//! - **A bare (keyless, colon-less) scalar directly inside `(...)`/
//!   `{...}`** -- e.g. `@meta(yaml)`'s `yaml`, or `{required}` -- parses
//!   as an `ERROR`-wrapped node instead of a clean `scalar`. Root cause:
//!   tree-sitter's lexer only uses declared precedence to break *equal-
//!   length* token ties, so an identifier-shaped bare word (which could
//!   be either a `map_entry` key or a standalone `scalar`) can't be
//!   disambiguated without knowing whether a `:` follows -- one token of
//!   lookahead this grammar doesn't have without an external scanner.
//!   `map_entry`'s key still correctly wins over `scalar` whenever a
//!   `:` genuinely follows (the common, important case), since
//!   `scalar`'s own colon-inclusive value-position variant makes that a
//!   longest-match win rather than a tie. See `grammar.js`'s comments on
//!   `scalar`/`_value_scalar`/`map_entry` for the full mechanism.
//! - **A trailing blank line right before the closing `}` of a multi-
//!   entry, newline-separated `map`/`children` block** (no comma on the
//!   last line) can produce a small, locally-contained `ERROR` node
//!   right at the closing brace, even though every entry before it
//!   parses correctly. Same one-token-lookahead limitation as above,
//!   applied to "is this blank line another entry, or the group's own
//!   trailing gap?".
//! - **Stray/unmatched `]`** (e.g. literal `[area]` written as prose,
//!   not as a real `area_group`) has no fallback token and produces a
//!   small `ERROR` -- unlike `(`/`[`/`{`/`-`, which all fall back to a
//!   `punctuation` node when nothing opens with them.
//! - **`<T>`/`@name` written with no group at all** (e.g. `<T>` used as
//!   a literal placeholder in prose, as `docs/tmt/typedmark.tm` itself
//!   does when explaining the grammar) still parses as an `element`
//!   here, where the real parser requires an immediately-following
//!   group to even recognize it as one (see
//!   `document.rs::is_type_element_start`/`is_at_element_start`) and
//!   otherwise treats it as plain text. Considered an acceptable v1
//!   trade-off: it doesn't error, and highlighting `<T>`-shaped text as
//!   an element is a reasonable default regardless.
//!
//! Verified against real content: `cargo test` in this crate parses
//! `docs/tmt/typedmark.tm` and `docs/tmt/image_meta.tm` and checks that
//! the only `ERROR`/`MISSING` nodes are the known, narrow cases above --
//! not that there are none. `docs/tmt/typedmark.tm` also has one
//! pre-existing case (`###[ [] のルール ]`, a `[]` immediately inside a
//! `[...]` content-span) that the *real* hand-written parser mishandles
//! too (confirmed separately against `typedmark-parser`), so this
//! grammar erroring on it as well isn't a regression.

use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_typedmark() -> *const ();
}

/// The tree-sitter language function for TypedMark. Convert to a
/// `tree_sitter::Language` via `.into()` to build a `tree_sitter::Parser`.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_typedmark) };

/// The grammar's `node-types.json`, e.g. for tooling that wants to
/// inspect the node type set without parsing anything.
pub const NODE_TYPES: &str = include_str!("node-types.json");

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use tree_sitter::{Node, Parser};

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser.set_language(&LANGUAGE.into()).expect("failed to load the TypedMark grammar");
        parser.parse(src, None).expect("parse returned None")
    }

    /// Collects the source text of every `ERROR`/`MISSING` node in the
    /// tree, so a fixture-level assertion can compare against exactly
    /// the known, documented cases rather than just counting them.
    fn error_texts(src: &str, tree: &tree_sitter::Tree) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut cursor = tree.walk();
        let mut visit = |node: Node| {
            if node.is_error() || node.is_missing() {
                out.insert(node.utf8_text(src.as_bytes()).unwrap_or_default().to_string());
            }
        };
        loop {
            visit(cursor.node());
            if cursor.goto_first_child() {
                continue;
            }
            loop {
                if cursor.goto_next_sibling() {
                    break;
                }
                if !cursor.goto_parent() {
                    return out;
                }
            }
        }
    }

    #[test]
    fn parses_a_heading() {
        let tree = parse("#[ Hello ]{ id:header1 }\n");
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parses_lists_ordered_and_unordered() {
        for src in ["- one\n- two\n- three\n", "-. one\n-. two\n"] {
            let tree = parse(src);
            assert!(!tree.root_node().has_error(), "expected no errors for {src:?}");
        }
    }

    #[test]
    fn merges_multiline_paragraphs_into_one_node() {
        let tree = parse("line one\nline two\nline three\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        assert_eq!(root.child(0).unwrap().kind(), "paragraph");
        assert_eq!(root.child_count(), 1);
    }

    #[test]
    fn parses_emphasis_strong_mark() {
        let tree = parse("a *em* b **strong** c ==mark==\n");
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parses_elements_with_all_group_kinds() {
        for src in [
            "<caution>[ be careful ]\n",
            "<input>(name:email, type:email){status:required}\n",
            "@(url:https://example.com)[Wiki]\n",
            "<embed>(file:assets/pic.png)[alt text]\n",
        ] {
            let tree = parse(src);
            assert!(!tree.root_node().has_error(), "expected no errors for {src:?}");
        }
    }

    #[test]
    fn bare_non_colon_scalar_is_a_known_limitation_not_silent_data_loss() {
        // Documented in this module's doc comment: `scalar`/`map_entry`'s
        // key both being identifier-shaped, equal-length matches at this
        // position means precedence can't disambiguate without knowing
        // whether a `:` follows -- this asserts the failure mode is a
        // visible `ERROR` node (so a caller can tell something's off),
        // not a silently wrong-but-clean parse.
        let tree = parse("@meta(yaml)\n");
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn parses_a_group_separated_from_its_element_by_whitespace() {
        // Regression test: `token(prec(1, "["))`/`"("`/`"{"` on the
        // group-opening literals in `grammar.js` -- without it, any
        // whitespace between an element and its first group caused the
        // element to parse with zero groups and the group's own text to
        // fall out as unrelated paragraph content.
        let tree = parse("@links {\n  (1)[ note ]\n  (anotation1)[ note2 ]\n}\n");
        let root = tree.root_node();
        let element = root.child(0).unwrap().child(0).unwrap();
        assert_eq!(element.kind(), "element");
        let at_element = element.child(0).expect("expected element to wrap an at_element");
        assert_eq!(at_element.kind(), "at_element");
        let value_group = at_element
            .named_child(1)
            .expect("expected a value_group as the at_element's second named child");
        assert_eq!(value_group.kind(), "value_group");
        let children = value_group.named_child(0).expect("expected a children node");
        assert_eq!(children.kind(), "children");
        assert_eq!(children.named_child_count(), 2);
    }

    #[test]
    fn typedmark_tm_fixture_has_only_known_error_cases() {
        let src = include_str!("../../../docs/tmt/typedmark.tm");
        let tree = parse(src);
        let errors = error_texts(src, &tree);
        // Every error node's text contains (or exactly is) one of these
        // markers, each tied to one documented case in this module's
        // doc comment: `yaml`/`required` are bare non-colon scalars,
        // `anotation1` is the same inside `@links{}`'s bare_element,
        // a bare `"\n"` is the trailing-blank-line-before-`}` case, and
        // the `のうち.../のルール` snippets are the stray-`]`-in-prose and
        // the pre-existing `[]`-inside-`[...]` parser bug, both from
        // this file's self-referential grammar-explanation prose.
        let known_markers = ["yaml", "required", "anotation1", "のうち必要なものを付ける", "のルール", "\n"];
        for text in &errors {
            assert!(
                known_markers.iter().any(|marker| text.contains(marker)),
                "unexpected error node text: {text:?}"
            );
        }
    }

    #[test]
    fn image_meta_tm_fixture_has_only_known_error_cases() {
        let src = include_str!("../../../docs/tmt/image_meta.tm");
        let tree = parse(src);
        let errors = error_texts(src, &tree);
        // "yaml" is `@meta(yaml)`'s bare non-colon scalar (see the
        // module doc); an empty string is a zero-width `MISSING` node
        // from the same error's recovery, not a separate case.
        for text in &errors {
            assert!(text.is_empty() || text.contains("yaml"), "unexpected error node text: {text:?}");
        }
    }

    #[test]
    fn highlights_query_is_valid() {
        let query_src = include_str!("../queries/highlights.scm");
        tree_sitter::Query::new(&LANGUAGE.into(), query_src)
            .expect("queries/highlights.scm should be a valid query against this grammar");
    }
}
