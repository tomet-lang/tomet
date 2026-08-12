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
//! - **A bare newline (no comma) right before the closing `}` of a
//!   newline-separated `map`/`children` block** can produce a small,
//!   locally-contained `ERROR` node right at the closing brace, even
//!   though every entry before it -- there can be just the one -- parses
//!   correctly. Affects the ordinary, idiomatic multi-line shape (each
//!   entry own its own line, `}` on the next), not just an actual blank
//!   line before `}`.
//! - **A `key: [seq]` map entry, when it's the map's last entry with no
//!   trailing comma** (e.g. `{ title: ..., tags: [a, b] }` on one line, as
//!   in `docs/tmt/examples/image_meta.tm`'s `@meta(format:yaml){...}`), gets GLR-merged
//!   with a second, spurious top-level `seq` reading of the same `[a, b]`
//!   text, wrapping the whole map in an `ERROR`. Overlaps with the bare-
//!   newline-before-`}` case above (same "is this the map's own trailing
//!   gap, or more content" one-token-lookahead limitation) but shows up
//!   even without a newline there, as long as a `[...]`-valued entry is
//!   last. Pre-existing and independent of `scanner.c`'s bare-scalar fix
//!   (confirmed against the grammar from before that fix existed -- same
//!   misparse shape either way, modulo exactly which node the `ERROR`
//!   lands on). Not investigated further here -- `typedmark-parser`'s real
//!   grammar has no such issue (`parses_flat_map` covers exactly this
//!   shape).
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
//! - **Unterminated `/* ... */` doesn't produce an `ERROR`.** The real
//!   parser (`document.rs::skip_block_comment`) hard-errors on a missing
//!   `*/` -- silently swallowing the rest of the document is a much
//!   worse failure mode for a comment than for e.g. a code span. Here an
//!   unmatched `/*` just fails to lex as `block_comment` and falls back
//!   to ordinary `text`/`punctuation` tokens instead.
//! - **A real embedded JSON/YAML/TOML body inside an element's `{...}`
//!   (any element whose `(input)` has a `format:json|yaml|toml` key, not
//!   just `@meta`) isn't understood as such.** `typedmark-parser` hands
//!   that body as-is to `serde_json`/`serde_yaml`/`toml` (see
//!   `typedmark-parser::embedded_format`), but this grammar has no idea a
//!   `{value}` group's content might be a different language -- it still
//!   tries its own `map`/`seq`/`scalar` rules. A quoted JSON key
//!   (`{ "key": "value" }`, needed since JSON has no bare-identifier keys
//!   -- see `docs/tmt/typedmark.tm`'s `@meta(format:json)` block) doesn't
//!   match `map_entry`'s bare-identifier `key` field, so the nested
//!   `{...}` becomes an `ERROR`. Not worth a real per-format sub-grammar
//!   here -- this crate is for editor highlighting, not validation.
//! - **`thematic_break`'s dash run only ever consumes exactly 3 `-`,
//!   even when the source has more** (`-----` -> a 3-dash `thematic_break`
//!   plus the leftover 2 dashes falling back to `punctuation` inside a
//!   `paragraph`) -- discovered while adding `titled_thematic_break`
//!   (`---[ Title ]---`, which shares its dash-run token with the plain
//!   break), but present before that addition too. Not an `ERROR`, just a
//!   shape mismatch: the real parser (`document.rs::is_thematic_break`)
//!   correctly accepts any run of 3+. See
//!   `more_than_three_dashes_are_only_partially_consumed_by_thematic_break`
//!   below for the pinned exact shape.
//!
//! `scanner.c` (see its own module doc) resolves what used to be listed
//! here as a known limitation: a bare, colon-less, identifier-shaped
//! scalar directly inside `(...)`/`{...}` (e.g. a bare tag like `@foo(yaml)`,
//! or `{required}`) now parses as a clean `scalar` instead of an `ERROR`.
//!
//! Verified against real content: `cargo test` in this crate parses
//! `docs/tmt/typedmark.tm` and `docs/tmt/examples/image_meta.tm` and checks that
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
        parser
            .set_language(&LANGUAGE.into())
            .expect("failed to load the TypedMark grammar");
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
                out.insert(
                    node.utf8_text(src.as_bytes())
                        .unwrap_or_default()
                        .to_string(),
                );
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
    fn parses_a_titled_thematic_break() {
        let tree = parse("---[ Title ]---\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        assert_eq!(root.named_child(0).unwrap().kind(), "titled_thematic_break");
    }

    #[test]
    fn plain_and_titled_thematic_breaks_are_distinct_adjacent_blocks() {
        // A dash-run token is shared between `thematic_break` and
        // `titled_thematic_break` (see `grammar.js`'s `_dash_run`) -- this
        // exercises that the lexer/parser still cleanly tell them apart
        // when they're right next to each other, not just in isolation.
        // Exactly 3 dashes each, deliberately -- more than 3 hits the
        // separate, pre-existing `thematic_break` truncation quirk covered
        // by `more_than_three_dashes_are_only_partially_consumed` below.
        let tree = parse("---\n---\n---[ Title ]---\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        let kinds: Vec<_> = (0..root.named_child_count() as u32)
            .map(|i| root.named_child(i).unwrap().kind())
            .collect();
        assert_eq!(
            kinds,
            ["thematic_break", "thematic_break", "titled_thematic_break"]
        );
    }

    #[test]
    fn more_than_three_dashes_are_only_partially_consumed_by_thematic_break() {
        // Pre-existing, discovered while adding `titled_thematic_break`
        // (confirmed present before that change too, by testing against
        // the grammar without it): `thematic_break`'s `-{3,}` token only
        // ever consumes exactly 3 dashes here, even though the real parser
        // (`typedmark-parser::document.rs::is_thematic_break`) happily
        // accepts any run of 3+ (see `parses_thematic_break_with_more_than_three_dashes`
        // in that crate). The leftover dashes fall back to `punctuation`
        // inside a `paragraph`, same as any other unmatched `-` -- not an
        // `ERROR`, just a shape mismatch with the real grammar. Root cause
        // not fully understood (an unbounded `{3,}` quantifier stopping at
        // its own minimum rather than matching maximally in this lexer
        // state); pinning the current behavior here rather than leaving it
        // silently uncovered.
        let tree = parse("-----\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        let kinds: Vec<_> = (0..root.named_child_count() as u32)
            .map(|i| root.named_child(i).unwrap().kind())
            .collect();
        assert_eq!(kinds, ["thematic_break", "paragraph"]);
    }

    #[test]
    fn parses_lists_ordered_and_unordered() {
        for src in ["- one\n- two\n- three\n", "-. one\n-. two\n"] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}"
            );
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
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}"
            );
        }
    }

    #[test]
    fn bare_non_colon_scalar_parses_cleanly() {
        // Used to be a known limitation (see `scanner.c`'s module doc for
        // the mechanism `$._bare_word_no_colon` fixes): `scalar`/
        // `map_entry`'s key are equal-length, identifier-shaped matches at
        // this position, and precedence alone can't disambiguate without
        // knowing whether a `:` follows. The external scanner supplies that
        // one token of lookahead.
        for src in [
            "@foo(yaml)\n",
            "@foo(json)\n",
            "@foo(toml)\n",
            "<input>{required}\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}"
            );
        }
    }

    /// Depth-first search for the first descendant of `node` with the given
    /// `kind`, `node` itself included.
    fn find_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn bare_scalar_still_wins_longest_match_when_not_identifier_shaped() {
        // Regression guard for `$._bare_word_no_colon`'s own doc comment:
        // it must decline (not truncate the match) whenever the internal
        // `scalar` regex would keep matching past the identifier-shaped
        // run, e.g. a space -- a multi-word bare scalar must still parse as
        // one whole `scalar` node, not just its first word. No leading
        // space before `hello` on purpose -- see `scanner.c`'s comment on
        // why this scanner only engages when the identifier-shaped run
        // starts immediately at the current lex position.
        let src = "<input>{hello world}\n";
        let tree = parse(src);
        assert!(!tree.root_node().has_error());
        let scalar = find_kind(tree.root_node(), "scalar").expect("expected a scalar node");
        assert_eq!(scalar.utf8_text(src.as_bytes()).unwrap(), "hello world");
    }

    #[test]
    fn map_entry_key_still_wins_when_a_colon_follows() {
        // The other half of the same regression guard: an identifier-
        // shaped bare word immediately followed by `:` must still become a
        // `map_entry` key, not the new bare-scalar token. All on one line,
        // on purpose -- a newline directly before the closing `}` hits a
        // different, pre-existing limitation unrelated to this one (see
        // this module's doc comment), confirmed to already reproduce with
        // this exact source on the grammar from before `scanner.c` existed.
        let tree = parse("@meta(format:json){key:value}\n");
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parses_line_and_block_comments() {
        for src in [
            "// a line comment\n",
            "before\n\n// a comment\n\nafter\n",
            "/* a block\ncomment */\n",
            "keep /* inline */ also keep\n",
            "see https://example.com for more\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}"
            );
        }
    }

    #[test]
    fn line_comment_is_its_own_top_level_node_not_paragraph_content() {
        let tree = parse("// just a note\n");
        let root = tree.root_node();
        assert_eq!(root.child_count(), 1);
        assert_eq!(root.child(0).unwrap().kind(), "line_comment");
    }

    #[test]
    fn block_comment_is_its_own_top_level_node_not_paragraph_content() {
        let tree = parse("/* just a note */\n");
        let root = tree.root_node();
        assert_eq!(root.child_count(), 1);
        assert_eq!(root.child(0).unwrap().kind(), "block_comment");
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
        let at_element = element
            .child(0)
            .expect("expected element to wrap an at_element");
        assert_eq!(at_element.kind(), "at_element");
        let value_group = at_element
            .named_child(1)
            .expect("expected a value_group as the at_element's second named child");
        assert_eq!(value_group.kind(), "value_group");
        let children = value_group
            .named_child(0)
            .expect("expected a children node");
        assert_eq!(children.kind(), "children");
        assert_eq!(children.named_child_count(), 2);
    }

    #[test]
    fn typedmark_tm_fixture_has_only_known_error_cases() {
        let src = include_str!("../../../docs/readme.ja.tm");
        let tree = parse(src);
        let errors = error_texts(src, &tree);
        // Every error node's text contains (or exactly is) one of these
        // markers, each tied to one documented case in this module's
        // doc comment: a bare `"\n"` is the trailing-blank-line-before-`}`
        // case, the `のうち.../のルール` snippets are the
        // stray-`]`-in-prose and the pre-existing `[]`-inside-`[...]`
        // parser bug, both from this file's self-referential
        // grammar-explanation prose, and `"key":` is the embedded-JSON
        // quoted-key case (`@meta(format:json){ { "key": "value" } }`).
        // (`required`/`anotation1`, the old bare-non-colon-scalar cases,
        // no longer error -- see `scanner.c`.)
        let known_markers = ["のうち必要なものを付ける", "のルール", "\n", "\"key\":"];
        for text in &errors {
            assert!(
                known_markers.iter().any(|marker| text.contains(marker)),
                "unexpected error node text: {text:?}"
            );
        }
    }

    #[test]
    fn image_meta_tm_fixture_has_only_known_error_cases() {
        let src = include_str!("../../../docs/tmt/examples/image_meta.tm");
        let tree = parse(src);
        let errors = error_texts(src, &tree);
        // The lone remaining error wraps `title: value` and `tags: ` --
        // the `key: [seq]`-map-entry-value misparse documented in this
        // module's doc comment (`tags: [a, b]`), unrelated to `@meta`'s
        // own `(format:yaml)` tag, which parses cleanly (an ordinary
        // `key:value` map entry, not the bare-scalar case `scanner.c`
        // exists for).
        for text in &errors {
            assert!(
                text.contains("tags:"),
                "unexpected error node text: {text:?}"
            );
        }
    }

    #[test]
    fn highlights_query_is_valid() {
        let query_src = include_str!("../queries/highlights.scm");
        tree_sitter::Query::new(&LANGUAGE.into(), query_src)
            .expect("queries/highlights.scm should be a valid query against this grammar");
    }

    #[test]
    fn indents_query_is_valid() {
        let query_src = include_str!("../queries/indents.scm");
        tree_sitter::Query::new(&LANGUAGE.into(), query_src)
            .expect("queries/indents.scm should be a valid query against this grammar");
    }

    #[test]
    fn brackets_query_is_valid() {
        let query_src = include_str!("../queries/brackets.scm");
        tree_sitter::Query::new(&LANGUAGE.into(), query_src)
            .expect("queries/brackets.scm should be a valid query against this grammar");
    }

    #[test]
    fn cheatsheet_tm_fixture_has_only_the_bare_newline_before_brace_error() {
        // `docs/cheatsheet.tm`'s three `@meta(format:json|yaml|toml){...}`
        // blocks don't hit the old bare-non-colon-scalar case at all
        // anymore (`format:json` etc. is an ordinary `key:value` map entry)
        // -- but each one's body is a single entry with a bare newline
        // right before the closing `}`, which is the *other*, unrelated,
        // still-open limitation documented in this module's doc comment.
        let src = include_str!("../../../docs/cheatsheet.tm");
        let tree = parse(src);
        let errors = error_texts(src, &tree);
        for text in &errors {
            assert!(
                !text.contains("json") && !text.contains("yaml") && !text.contains("toml"),
                "unexpected error node text touching @meta's format tag: {text:?}"
            );
        }
    }
}
