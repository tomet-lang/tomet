//! =[ tree-sitter-tomet ]
//!
//! Rust binding for the Tomet tree-sitter grammar (`grammar.js` at
//! the crate root; `src/parser.c` etc. are generated from it via
//! `npx tree-sitter-cli@0.26.12 generate`, then compiled by the crate's build script).
//!
//! This grammar exists for editor syntax highlighting (Zed and other
//! tree-sitter-based editors), not as a source of truth -- that's
//! `tomet-parser`'s job. It's a **reasonable approximation of the
//! real grammar, not a byte-for-byte match**; several things a
//! context-free grammar can't cheaply express are simplified away or
//! left as known-narrow parse-error cases:
//!
//! -| **No flanking-delimiter whitespace rules.** `*em*`/`**strong**`/
//!  | `==mark==` don't reject e.g. `* not em *` the way the real parser
//!  | does; `_`/`__` also skip the "must follow a non-alphanumeric
//!  | character" check.
//! -| **No lazy paragraph continuation.** A paragraph ends at a blank
//!  | line or EOF only, not at the next line merely *looking* like a new
//!  | block. `tomet-parser`'s own lazy-continuation check also ends a
//!  | paragraph early when the next line is an element trigger (`@name`)
//!  | -- e.g. `::meta(...)`
//!  | immediately followed by `::format(...)` with no blank line between
//!  | them becomes two separate top-level elements in the real parser.
//!  | This grammar doesn't simulate that either, so it still merges such
//!  | adjacent elements into one `paragraph` node.
//! -| **Fenced code blocks only recognize exactly 3 backticks, not 3-or-
//!  | more with the closer tracking the opener's length.** Same
//!  | "known-narrow" simplification as `thematic_break`'s fixed dash-run
//!  | handling. The real parser
//!  | accepts any run of 3+ and
//!  | requires the closer to have at least as many backticks as the
//!  | opener; matching that here would need a stateful external scanner
//!  | to remember the opening run's length across the body, which this
//!  | grammar doesn't have (see `fenced_code_block`'s own comment in
//!  | `grammar.js`).
//! -| **`(args)`/`[content]`/`{value}` aren't capped at one each, in any
//!  | order.** The real grammar allows each group at most once, in any
//!  | order; here they're just `repeat(choice(...))`, so e.g. two
//!  | `(args)` groups back to back would (harmlessly) both parse.
//! -| **The nexus rule is applied, but not everywhere it is checked.** A
//!  | nexus (`@name`, `::name`) needs a primary group -- `(args)`,
//!  | `[content]` or a `|` run -- and `{value}`, `#(id)` and a `::connect`
//!  | only add to one (`_nexus_groups` in `grammar.js`). So `::meta(a: 1)` and
//!  | `@name(){ a: 1 }` are clean and `::meta{ a: 1 }`, `@name{}` and
//!  | `@name#(id)` are `ERROR`s, as they are in the real parser. What the
//!  | grammar cannot see is position: a lone `@name` is prose inline and an
//!  | error as a block, so it is accepted everywhere (`me@example.com`
//!  | must stay clean), and after it a `::rule(...)` or `::{...}` reads as
//!  | punctuation and text rather than as the error the real parser
//!  | reports. A `|` run on the line *after* its element
//!  | (`@table` then `|[ a ][ b ]`) is not a primary group here either; see
//!  | `syntax/pipe-table.tmt` in `KNOWN_TS_ERRORS`.
//! -| **`em`/`strong` don't nest in each other.** `*`/`**`/`_`/`__` all
//!  | share a delimiter character, and letting them nest is the classic
//!  | Markdown emphasis/strong ambiguity that needs real flanking-rule
//!  | lookahead (or an external scanner) to resolve properly.
//! -| **`//`/`/* */` comments aren't understood inside a `(...)`/`{...}`
//!  | group's value content at all.** `tomet-parser` does support
//!  | them there, e.g. `::config(...)`'s several-line
//!  | body in `tests/fixtures/cheatsheet.tmt` uses `///`/`//`-prefixed lines as a
//!  | doc-comment convention -- but `map`/`children`/`_entry_sep` have no
//!  | rule admitting a bare comment line at all, so a comment anywhere in
//!  | a group's body derails the rest of that group into an `ERROR`. Not
//!  | investigated further here -- a real fix means teaching `map`'s
//!  | entry-separator logic about comments specifically, not just the
//!  | `map`/`children` vs. their own trailing gap ambiguity below.
//! -| **Stray/unmatched `]`** (e.g. literal `[content]` written as prose,
//!  | not as a real `content_group`) has no fallback token and produces a
//!  | small `ERROR` -- unlike `(`/`[`/`{`/`-`, which all fall back to a
//!  | `punctuation` node when nothing opens with them.
//! -| **A `<...>` box is approximated.** A name tag (`<PATH>`) is one `label`
//!  | token and a hole (`<?int(n)>[...]`) is a `hole` node with `hole_open`,
//!  | the groups, `hole_close` and the content. The real parser needs the
//!  | head to be tight (`<?int (n)>` is text there), but `extras` lets
//!  | white space into it here. A box in a value position (`url: <?(u)>`)
//!  | is left to `scalar`, so it is not a `hole` there. A shape that is no
//!  | box (`a < b`, `x<=1`, `<->`, `</x>`, `<x/>`) is `text` and
//!  | `punctuation`, never an `ERROR`.
//! -| **A type seat (`<$uuid>`, `<$list(string, length: 6)>`) is a `type_seat`
//!  | node: `type_seat_open` (the `<$` and the name), the arguments as an
//!  | expression's, and `type_seat_close`.** It stands wherever a hole does and
//!  | in a value position as well (`::id(<$uuid(default: auto)>)`,
//!  | `key: <$string>`, `@x(<$int>)`). Whether a `<$x` is a seat is only known
//!  | at its close, so `type_seat_open` is an external token that `scanner.c`
//!  | emits once the `>` (or the `(args)` and then the `>`) is there, and a
//!  | `<$x >`, `<$5>`, `<$x{a: 1}>` or `\<$x>` is `text` and `punctuation`, as
//!  | in the real parser. A seat takes no content, so in `<$x>[t]` the `[t]`
//!  | is prose, stray `]` and all. A `(` that never closes (`<$x(a>`) is no
//!  | seat either, but `$x(` then reads as an interpolation call that never
//!  | closes, which is an `ERROR` here as it is a parse error there.
//! -| **Unterminated `/* ... */` doesn't produce an `ERROR`.** The real
//!  | parser hard-errors on a missing
//!  | `*/` -- silently swallowing the rest of the document is a much
//!  | worse failure mode for a comment than for e.g. a code span. Here an
//!  | unmatched `/*` just fails to lex as `block_comment` and falls back
//!  | to ordinary `text`/`punctuation` tokens instead.
//! -| `thematic_break`'s dash run only ever consumes exactly 3 `-`,
//!  | even when the source has more (`-----` -> a 3-dash `thematic_break`
//!  | plus the leftover 2 dashes falling back to `punctuation` inside a
//!  | `paragraph`) -- discovered while adding `titled_thematic_break`
//!  | (`---[ Title ]---`, which shares its dash-run token with the plain
//!  | break), but present before that addition too. Not an `ERROR`, just a
//!  | shape mismatch: the real parser
//!  | correctly accepts any run of 3+. See
//!  | `more_than_three_dashes_are_only_partially_consumed_by_thematic_break`
//!  | below for the pinned exact shape. The same goes for the closing run
//!  | of the `|` form of a titled break (`---` / `| Title` / `---`).
//! -| **Two `|`-form titled breaks that share a `---` pair up differently.**
//!  | In `---`/`| a`/`---`/`| b`/`---`, both "(a) is the title, then a
//!  | paragraph `| b`" and "a plain break, a paragraph `| a`, then (b) is
//!  | the title" are valid readings, and the grammar picks the second;
//!  | `tomet-parser` reads left to right and takes the first (a `---` that
//!  | closes a break never opens the next one). Not an `ERROR`. Pinned by
//!  | `consecutive_pipe_form_breaks_pair_up_the_other_way_round`.
//! -| **A bare call or sequence occupying the *whole* `(args)`/`{value}`
//!  | doesn't parse, unlike the same value under a named key (`a: (b, c)`,
//!  | `a: list(string)`, which work fine).** `@id(list(a, b))` and
//!  | `::meta((a, b))` both land in an `ERROR` even with `$.call` as an
//!  | alternative to `args_group`/`value_group`'s shared `value` rule -- that
//!  | changes nothing: whatever conflicts an
//!  | identifier-then-`(` at that exact position with, it isn't decided by
//!  | `value`'s own alternatives. Not root-caused. `tomet-parser` accepts
//!  | both (`lower_single_value_from_children` reads a lone call or `(...)`
//!  | before its scalar fallback); `$.sequence` is reached through
//!  | `_entry_value` only, so a named key is the form this grammar covers.
//!
//! `scanner.c` (see its own module doc) makes a bare, colon-less,
//! identifier-shaped scalar directly inside `(...)`/`{...}` (e.g. a bare
//! tag like `@foo(yaml)`, or `{required}`) parse as a clean `scalar`
//! instead of an `ERROR`. It also owns the head of a type seat, tried before
//! `scalar` wherever a value stands.
//!
//! `_value_scalar`'s regex (see its own comment in `grammar.js`) keeps a
//! `key: (...)` map entry (e.g. `tags: (a, b)`) from being GLR-merged
//! with a second, spurious top-level reading of the same text, which
//! would wrap the whole map in an `ERROR` -- not just when that entry is
//! last in the map with no trailing comma, but for an earlier, non-last
//! `key: (...)` entry too. The regex excludes space/tab from its
//! character class: otherwise it could match a lone leading space
//! (between `:` and the value) as a complete one-character token in its
//! own right, and tree-sitter's lexer would take that real, if useless,
//! token match over skipping the space as `extras` first -- the same
//! class of bug `SCALAR`/`LIST_MARKER_GAP` exist to route around.
//!
//! `conflicts: [$.map, $.children]` (both declared with no `prec.right`
//! of their own) resolves another ambiguity: a bare newline right before
//! a `map`/`children`-holding group's `}`/`)`, with nothing else after
//! it, is genuinely ambiguous with only one token of lookahead ("is this
//! newline the start of another entry, or the group's own trailing
//! gap?"), and always guessing "another entry" is a dead end whenever
//! nothing valid actually follows. GLR tries both and keeps whichever one
//! doesn't dead-end. `-` vs `punctuation` is handled the same class of
//! way: `punctuation`'s `-` (see its own comment in `grammar.js`) is the
//! exact same shared token as the list markers', not a second, competing
//! definition that would always win tree-sitter's lexer-level tie
//! regardless of what (if anything) validly followed.
//!
//! Verified against real content: `cargo test` in this crate parses
//! `tests/fixtures/readme.tmt` and `tests/fixtures/examples/image.meta.tmt` and checks that
//! the only `ERROR`/`MISSING` nodes are the known, narrow cases above --
//! not that there are none. `tests/fixtures/readme.tmt` also has one
//! pre-existing case (`###[ [] のルール ]`, a `[]` immediately inside a
//! `[...]` content-span) that the *real* hand-written parser mishandles
//! too (confirmed separately against `tomet-parser`), so this
//! grammar erroring on it as well isn't a regression.

use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_tomet() -> *const ();
}

/// The tree-sitter language function for Tomet. Convert to a
/// `tree_sitter::Language` via `.into()` to build a `tree_sitter::Parser`.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_tomet) };

/// The grammar's `node-types.json`, e.g. for tooling that wants to
/// inspect the node type set without parsing anything.
pub const NODE_TYPES: &str = include_str!("node-types.json");

/// The grammar's `highlights.scm` query string for syntax highlighting.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../queries/highlights.scm");

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::{Node, Parser};

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&LANGUAGE.into())
            .expect("failed to load the Tomet grammar");
        parser.parse(src, None).expect("parse returned None")
    }

    /// Collects the source text of every `ERROR`/`MISSING` node in the
    /// tree, so a fixture-level assertion can compare against exactly
    /// the known, documented cases rather than just counting them.

    #[test]
    fn parses_a_section() {
        let tree = parse("=[ Hello ]{ id:header1 }\n");
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parses_headingless_sections() {
        for src in [
            "=\n",
            "==\n",
            "===\n",
            "=   \n",
            "={ id: sec }\n",
            "=(args)\n",
            "=(id: sec){ id: sec }\n",
            "==[ Title ]==\n",
        ] {
            let tree = parse(src);
            let root = tree.root_node();
            assert!(!root.has_error(), "failed to parse {src:?}");
            assert_eq!(
                root.named_child(0).unwrap().kind(),
                "section",
                "expected section node for {src:?}"
            );
        }
    }

    #[test]
    fn parses_section_with_decorative_trailing_equals() {
        let tree = parse("==[ Title ]==\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        let sec = root.named_child(0).unwrap();
        assert_eq!(sec.kind(), "section");
        assert_eq!(
            sec.child_by_field_name("marker").unwrap().kind(),
            "section_marker"
        );
        assert_eq!(
            sec.child_by_field_name("close_marker").unwrap().kind(),
            "section_marker"
        );
    }

    #[test]
    fn parses_a_titled_thematic_break() {
        let tree = parse("---[ Title ]---\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        assert_eq!(root.named_child(0).unwrap().kind(), "titled_thematic_break");
    }

    /// The kinds of the root's named children.
    fn block_kinds(src: &str) -> Vec<&'static str> {
        let tree = parse(src);
        let root = tree.root_node();
        assert!(!root.has_error(), "{src:?}");
        (0..root.named_child_count() as u32)
            .map(|i| root.named_child(i).unwrap().kind())
            .collect()
    }

    #[test]
    fn parses_a_titled_thematic_break_written_with_pipe_lines() {
        // `|` lines are the title; the closing run stands for the `]---`.
        for src in ["---\n| Title\n---\n", "---\n| one\n| two\n---\n"] {
            assert_eq!(block_kinds(src), ["titled_thematic_break"], "{src:?}");
        }
        let tree = parse("---\n| one\n| two\n---\n");
        let hr = tree.root_node().named_child(0).unwrap();
        assert_eq!(
            hr.child_by_field_name("content").unwrap().kind(),
            "marked_content"
        );
        assert_eq!(
            hr.child_by_field_name("close").unwrap().kind(),
            "thematic_break_marker"
        );
    }

    #[test]
    fn consecutive_pipe_form_breaks_pair_up_the_other_way_round() {
        // `tomet-parser` closes the first break with the second `---` (titled, paragraph, plain);
        // both readings are valid here and this grammar takes the other one. See the module doc.
        assert_eq!(
            block_kinds("---\n| a\n---\n| b\n---\n"),
            ["thematic_break", "paragraph", "titled_thematic_break"]
        );
    }

    #[test]
    fn a_plain_break_followed_by_a_pipe_line_without_a_closing_run_stays_plain() {
        // No closing `---`, so there is no titled break; the plain break is still one.
        let tree = parse("---\n| text\n");
        assert_eq!(
            tree.root_node().named_child(0).unwrap().kind(),
            "thematic_break"
        );
    }

    #[test]
    fn a_semicolon_at_the_start_of_a_line_breaks_the_block() {
        let tree = parse("a\n;b\nc\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        let kinds: Vec<_> = (0..root.named_child_count() as u32)
            .map(|i| root.named_child(i).unwrap().kind())
            .collect();
        assert_eq!(kinds, ["paragraph", "semi_break", "paragraph"]);
        // In the middle of a line, and escaped, it is text.
        for src in ["a;b\n", "a\n\\;b\n"] {
            let tree = parse(src);
            let root = tree.root_node();
            assert!(!root.has_error(), "{src:?}");
            assert_eq!(root.named_child_count(), 1, "{src:?}");
        }
    }

    #[test]
    fn two_backticks_at_the_start_of_a_line_make_a_line_raw() {
        let tree = parse("a\n`` b *c*\n`` d\n\ne `f` h\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        let kinds: Vec<_> = (0..root.named_child_count() as u32)
            .map(|i| root.named_child(i).unwrap().kind())
            .collect();
        assert_eq!(kinds, ["paragraph", "line_raw", "paragraph"]);
        // Three backticks are a fence.
        let tree = parse("```\nx\n```\n");
        assert_eq!(
            tree.root_node().named_child(0).unwrap().kind(),
            "fenced_code_block"
        );
    }

    #[test]
    fn parses_a_fenced_code_block() {
        let tree = parse("```rust\nfn main() {}\n```\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        assert_eq!(root.named_child(0).unwrap().kind(), "fenced_code_block");
    }

    #[test]
    fn fenced_code_block_body_can_contain_short_backtick_runs() {
        let tree = parse("```\nsee `foo` and ``bar``\n```\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        assert_eq!(root.named_child(0).unwrap().kind(), "fenced_code_block");
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
        // `thematic_break`'s `-{3,}` token only ever consumes exactly 3
        // dashes here, even though the real parser happily accepts any
        // run of 3+. The leftover dashes fall back to `punctuation`
        // inside a `paragraph`, same as any other unmatched `-` -- not an
        // `ERROR`, just a shape mismatch with the real grammar.
        //
        // This is not an `ERROR`, though `unordered_list_item`'s `"-"` is
        // its own standalone lexer token (rather than fused with its
        // trailing whitespace into one atomic `token(seq("-", /[ \t]/))`)
        // so an optional marker can sit after it: a bare `-` with
        // nothing valid after it must not win tree-sitter's lexer tie
        // against `punctuation`'s own `-` unconditionally, dead-ending
        // instead of falling back. `punctuation`'s `-` is a bare string
        // literal alternative (see its own comment), not folded into a
        // character-class regex -- an identical string literal anywhere in
        // the grammar is one shared token, so this `-` and
        // `unordered_list_item`'s marker `-` are not two different token
        // definitions competing for the same text, but one shared token
        // whose eventual reduction (list item vs bare `punctuation`)
        // ordinary LALR lookahead can decide once it's clear no valid
        // marker/gap follows.
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

    /// Kinds of every `map_entry`'s `key` field, in source order.
    fn map_entry_key_kinds(node: Node) -> Vec<&'static str> {
        let mut kinds = Vec::new();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if n.kind() == "map_entry"
                && let Some(key) = n.child_by_field_name("key")
            {
                kinds.push(key.kind());
            }
            let mut cursor = n.walk();
            let children: Vec<_> = n.children(&mut cursor).collect();
            stack.extend(children.into_iter().rev());
        }
        kinds
    }

    #[test]
    fn a_quoted_string_before_a_colon_is_a_map_key() {
        // `tomet-parser` reads `"a/b": 1` as a keyed entry wherever it reads
        // `a: 1`: in `{value}`, in `(args)`, nested, and mixed with bare keys.
        for (src, keys) in [
            ("::meta(\"a/b\": 1)\n", vec!["string"]),
            (
                "::meta(\n  \"a/b\": 1\n  c: 2\n)\n",
                vec!["string", "identifier"],
            ),
            (
                "::meta(\n  c: 2\n  \"a/b\": 1\n)\n",
                vec!["identifier", "string"],
            ),
            ("@x(\"a/b\": 1, c: 2)\n", vec!["string", "identifier"]),
            ("@x(){ k: { \"c d\": 3 } }\n", vec!["identifier", "string"]),
        ] {
            let tree = parse(src);
            let root = tree.root_node();
            assert!(!root.has_error(), "expected no errors for {src:?}");
            assert_eq!(map_entry_key_kinds(root), keys, "for {src:?}");
        }
    }

    #[test]
    fn a_quoted_string_without_a_colon_is_still_a_value() {
        for src in ["@x(\"a/b\")\n", "@x(){ \"a/b\" }\n"] {
            let tree = parse(src);
            let root = tree.root_node();
            assert!(!root.has_error(), "expected no errors for {src:?}");
            let keys = map_entry_key_kinds(root);
            assert!(
                !keys.contains(&"string"),
                "a quoted value must not become a key in {src:?}: {keys:?}"
            );
        }
    }

    #[test]
    fn parses_list_value_markers() {
        // `- (T)`/`- (?)`/etc, with a space before the marker (the real
        // fixture shape -- see `tests/fixtures/cheatsheet.tmt`), each produce a
        // `marker: (list_marker)` field/node distinct from the rest of
        // the item's content, and a trailing `{attrs}` group still works
        // alongside it.
        for (src, marker_text) in [
            ("- (T) task\n", "(T)"),
            ("- (?) task\n", "(?)"),
            ("- ( ) task\n", "( )"),
            ("-(x) task\n", "(x)"),
            ("- (x) task {tag: dev}\n", "(x)"),
        ] {
            let tree = parse(src);
            let root = tree.root_node();
            assert!(!root.has_error(), "expected no errors for {src:?}");
            let item = root
                .named_child(0)
                .unwrap()
                .named_child(0)
                .unwrap()
                .named_child(0)
                .unwrap();
            assert_eq!(item.kind(), "unordered_list_item");
            let marker = item.child_by_field_name("marker").unwrap();
            assert_eq!(marker.kind(), "list_marker");
            assert_eq!(&src[marker.byte_range()], marker_text);
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
            "@caution[ be careful ]\n",
            "@input(name:email, type:email){status:required}\n",
            "@(url:https://example.com)[Wiki]\n",
            "@embed(file:assets/pic.png)[alt text]\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}"
            );
        }
    }

    #[test]
    fn parses_colon_connect_syntax() {
        for src in [
            "=[ Title ]::{ id: intro, tag: main }\n",
            "@task[ Task A ]::{ id: taskA, priority: high }\n",
            "@id(taskA)::{ priority: high, tag: dev }\n",
            // A bare positional `(...)` occupying the whole `(args)`
            // value -- as opposed to `a: (...)` under a named key,
            // which already works (see `parses_sequence_syntax_in_values`)
            // -- doesn't reach `sequence` in this grammar yet
            // (`args_group`'s own `value` rule, unlike `_entry_value`,
            // still trips over it). Genuine `tomet-parser` syntax, not
            // covered here: named-key form exercises the same
            // sequence + colon-connect interaction without hitting
            // that gap.
            "@id(ids: (taskA, taskB))::{ tag: house }\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}"
            );
        }
    }

    #[test]
    fn parses_id_group_on_elements_sections_and_connects() {
        for src in [
            "@memo(a: 1)[ x ]#(myid)\n",
            "@memo#(myid)(a: 1)\n",
            "=[ Title ]#(intro)\n",
            "@task(a: 1)::rule(allow: (card))#(myid)\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}"
            );
        }
    }

    #[test]
    fn id_group_is_a_real_node_not_fallen_through_text() {
        // Same `has_error()`-is-weak reasoning as the connect test right
        // below: confirm an `id_group` node genuinely exists, not just
        // that the line parsed without error.
        let tree = parse("@memo(a: 1)#(myid)\n");
        let element = tree
            .root_node()
            .named_child(0)
            .unwrap()
            .named_child(0)
            .unwrap()
            .named_child(0)
            .unwrap();
        let has_id_group = (0..element.named_child_count() as u32)
            .any(|i| element.named_child(i).unwrap().kind() == "id_group");
        assert!(has_id_group, "expected an id_group child");
    }

    /// `has_error()` alone is a weak check here (arbitrary prose parses
    /// error-free too -- a `::name(...)` that fell through to plain text
    /// would still pass it), so this also confirms a `connect` node
    /// genuinely exists as a *child of the element*, not just that the
    /// line parsed somehow. Regression coverage: a `:` right after a
    /// *closed* group (`(a: 1)`/`[ a ]`) must not lose the lexer's tie
    /// against plain `text` and never reach `_element_group`/`connect` at
    /// all -- see `text`/`_element_group`/`connect`'s own comments in
    /// `grammar.js`.
    #[test]
    fn named_connect_reaches_the_grammar_after_a_closed_group() {
        for src in ["@x(a: 1)::as(y)\n", "@x[ a ]::as(y)\n", "@x()::as(y)\n"] {
            let tree = parse(src);
            let root = tree.root_node();
            assert!(!root.has_error(), "expected no errors for {src:?}");
            let element = root
                .named_child(0)
                .unwrap()
                .named_child(0)
                .unwrap()
                .named_child(0)
                .unwrap();
            assert_eq!(element.kind(), "inline_element", "for {src:?}");
            let has_connect = (0..element.named_child_count() as u32)
                .any(|i| element.named_child(i).unwrap().kind() == "connect");
            assert!(has_connect, "expected a connect child for {src:?}");
        }
    }

    /// `@x()::as(y)::rule(z)` must produce *two* `connect` nodes as
    /// siblings, not one nested inside the other -- matching the real
    /// parser's `parse_groups(cur, &mut connect_el, false)`, which stops
    /// a connect's own repeat before it can swallow a sibling connect.
    #[test]
    fn stacked_connects_are_flat_siblings_not_nested() {
        let tree = parse("@x(a: 1)::as(y)::rule(z)\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        let element = root
            .named_child(0)
            .unwrap()
            .named_child(0)
            .unwrap()
            .named_child(0)
            .unwrap();
        let connects: Vec<Node> = (0..element.named_child_count() as u32)
            .map(|i| element.named_child(i).unwrap())
            .filter(|n| n.kind() == "connect")
            .collect();
        assert_eq!(connects.len(), 2, "expected two sibling connects");
        for connect in &connects {
            let nested = (0..connect.named_child_count() as u32)
                .any(|i| connect.named_child(i).unwrap().kind() == "connect");
            assert!(!nested, "a connect must not itself contain a connect");
        }
    }

    #[test]
    fn section_supports_named_connect() {
        let tree = parse("=[ h ]::rule(allow: (card))\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        let section = root.named_child(0).unwrap();
        assert_eq!(section.kind(), "section");
        let has_connect = (0..section.named_child_count() as u32)
            .any(|i| section.named_child(i).unwrap().kind() == "connect");
        assert!(has_connect);
    }

    /// `list(...)`/`enum(...)` -- an immediately-resolved call literal,
    /// mirroring `tomet_ast::Value::Call`. Also regression coverage: giving
    /// the call's callee token a precedence boost / reusing `_element_name`
    /// (this grammar's `word` token) would break `url:https://example.com`
    /// (a scheme URI, real and common in `.tmt` fixtures) -- either lets a
    /// short, high-priority identifier match preempt `_value_scalar`'s otherwise-longer match
    /// for the same text. See `call`'s own comment in `grammar.js`.
    #[test]
    fn parses_sequence_syntax_in_values() {
        for src in [
            "@x(a: ())\n",
            "@x(a: (card))\n",
            "@x(a: (card, ns.mycard))\n",
            "@x(a: (b, (c)))\n",
            "@x(a: ((b), (c, d), ()))\n",
            "@x(a: ({ k: 1 }, \"s\", 2))\n",
            "::workspace(ignore: (\"a\", \"b\"), unswept: (\"c\"))\n",
        ] {
            let tree = parse(src);
            let root = tree.root_node();
            assert!(
                !root.has_error(),
                "expected no errors for {src:?}: {}",
                root.to_sexp()
            );
            assert!(
                tree_contains_kind(root, "sequence"),
                "expected a sequence node for {src:?}"
            );
            assert!(
                !tree_contains_kind(root, "call"),
                "a bare `(...)` is not a call for {src:?}"
            );
        }
    }

    #[test]
    fn parses_call_syntax_in_values() {
        for src in [
            "@x(a: list(card))\n",
            "@x(a: list(card, ns.mycard))\n",
            "@x(a: list(b, list(c)))\n",
            "@x(a: string(b))\n",
        ] {
            let tree = parse(src);
            let root = tree.root_node();
            assert!(!root.has_error(), "expected no errors for {src:?}");
            assert!(
                tree_contains_kind(root, "call"),
                "expected a call node for {src:?}"
            );
        }
    }

    #[test]
    fn scheme_uri_values_still_parse_as_one_scalar_alongside_call_syntax() {
        let tree = parse("@(url:https://example.com)[Wiki]\n");
        let root = tree.root_node();
        assert!(!root.has_error());
        assert!(!tree_contains_kind(root, "ERROR"));
    }

    fn tree_contains_kind(node: Node, kind: &str) -> bool {
        if node.kind() == kind {
            return true;
        }
        (0..node.named_child_count() as u32)
            .any(|i| tree_contains_kind(node.named_child(i).unwrap(), kind))
    }

    /// `::` goes from a node to what hangs off it, and a dot reads inside the node: both
    /// chain, and a call may follow either.
    #[test]
    fn parses_interpolation_connect() {
        for src in [
            "$(self::path)\n",
            "$(u1::id)\n",
            "$(self::doc::meta)\n",
            "$(self::meta.tags)\n",
            "{$ self::meta.tags.join(\", \") }\n",
            "$(#u1::meta.tag)\n",
            "{$ tmt(\"a.tmt\")::meta.title }\n",
            "{$ vault::docs.filter(d => d::meta.draft == false).map(d => d.link()) }\n",
            "{$ addr(dir:docs)::docs.item(0) }\n",
            "{$ a :: b }\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}: {}",
                tree.root_node().to_sexp()
            );
            assert!(find_kind(tree.root_node(), "interp_connect").is_some());
        }
        // A named argument is still `name: value`.
        let tree = parse("$f(a: b::c)\n");
        assert!(!tree.root_node().has_error());
        assert!(find_kind(tree.root_node(), "interp_named_arg").is_some());
    }

    #[test]
    fn parses_interpolation_path_and_call() {
        for src in [
            "$(id)\n",
            "$(a.b.c)\n",
            "$sum(a, mul(b, c))\n",
            "{$ 1.5 }\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}"
            );
        }
        let tree = parse("$sum(a, b)\n");
        assert!(find_kind(tree.root_node(), "interpolation").is_some());
        assert!(find_kind(tree.root_node(), "interp_dollar_call").is_some());
    }

    /// `{$ ... }` is one `interpolation` wherever a value is, not a `$` scalar
    /// followed by a stray `{...}` group -- which is how a value position read
    /// it before, and with a call inside it also left an error node.
    #[test]
    fn interpolation_in_a_value_position_is_one_interpolation() {
        for (src, expected) in [
            ("::meta(id: $f())\n", 1),
            ("::meta(id: $(f))\n", 1),
            ("::meta($f())\n", 1),
            ("@x($(a.b))\n", 1),
            ("@x(tags: list($(a), b, $c(1)))\n", 2),
            ("::meta(\n  id: $macro.stamp()\n  by: $(var.author)\n)\n", 2),
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}: {}",
                tree.root_node().to_sexp()
            );
            assert_eq!(
                count_kind(tree.root_node(), "interpolation"),
                expected,
                "{src:?}: {}",
                tree.root_node().to_sexp()
            );
        }
    }

    fn count_kind(node: Node, kind: &str) -> usize {
        usize::from(node.kind() == kind)
            + (0..node.named_child_count() as u32)
                .map(|i| count_kind(node.named_child(i).unwrap(), kind))
                .sum::<usize>()
    }

    #[test]
    fn bare_non_colon_scalar_parses_cleanly() {
        // `scalar`/`map_entry`'s key are equal-length, identifier-shaped
        // matches at this position, and precedence alone can't
        // disambiguate without knowing whether a `:` follows. The external
        // scanner supplies that one token of lookahead (see `scanner.c`'s
        // module doc for the mechanism `$._bare_word_no_colon`).
        for src in [
            "@foo(yaml)\n",
            "@foo(json)\n",
            "@foo(toml)\n",
            "@input(){required}\n",
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
        let src = "@input(){hello world}\n";
        let tree = parse(src);
        assert!(!tree.root_node().has_error());
        let scalar = find_kind(tree.root_node(), "scalar").expect("expected a scalar node");
        assert_eq!(scalar.utf8_text(src.as_bytes()).unwrap(), "hello world");
    }

    #[test]
    fn map_entry_key_still_wins_when_a_colon_follows() {
        // The other half of the same regression guard: an identifier-
        // shaped bare word immediately followed by `:` must still become
        // a `map_entry` key, not the new bare-scalar token.
        let src = "::meta(format:json)\n";
        let tree = parse(src);
        assert!(!tree.root_node().has_error());
        let entry = find_kind(tree.root_node(), "map_entry").expect("expected a map_entry node");
        assert_eq!(entry.utf8_text(src.as_bytes()).unwrap(), "format:json");
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

    /// A nexus (`@name`, `::name`) needs a primary group: `(args)`,
    /// `[content]` or a `|` run. `{value}`, `#(id)` and a `::connect` only add
    /// to one. The real parser rejects the second list; the grammar shows the
    /// same by erroring on it, so the highlighter does not paint a
    /// `@name{ ... }` as if it were an element.
    ///
    /// Known gap: `@name::rule(a)`, `@name::{ a: 1 }` and `@name::(a)` are errors
    /// in the real parser, but here the lone `@name` is a complete (prose)
    /// element and the `:` after it is punctuation, so they read without an
    /// error node. Only a group the element would have taken (`{`, `#(`) can
    /// be refused. The same goes for `- item ::rule{}` and `= H ::rule{}`: the
    /// `::rule` is text and the `{}` the item's or heading's own attrs.
    #[test]
    fn a_nexus_needs_a_primary_group() {
        for src in [
            "@name()\n",
            "@name[ x ]\n",
            "@name(){ a: 1 }\n",
            "@name()#(id)\n",
            "@name{ a: 1 }(b: 1)\n",
            "@name#(id)(a)\n",
            "@name()#(id)::rule(a)\n",
            "@name()::{ a: 1 }\n",
            "@name(a)::(b)\n",
            "-@name(a)\n",
            "text @name() more\n",
            "@quote| x\n",
            "::meta(a: 1)\n",
            "::meta()\n",
            "::meta[ x ]\n",
            "::vocabulary(ns){ version: \"1.0.0\" }\n",
            "::dictionary(n)\n\n;@term(\"a\")\n| x\n",
            "- item ::rule(a)\n",
            "- item ::{ a: 1 }\n",
            "- item ::( a )\n",
            "= H ::rule(a)\n",
            "= H ::{ a: 1 }\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}: {}",
                tree.root_node().to_sexp()
            );
        }
        for src in [
            "@name{}\n",
            "@name{ a: 1 }\n",
            "@name#(id)\n",
            "@name{}#(id)\n",
            "@name()::rule{}\n",
            "-@name{}\n",
            "text @name{} more\n",
            "::name{}\n",
            "::name{ a: 1 }\n",
            "::name#(id)\n",
        ] {
            let tree = parse(src);
            assert!(
                tree.root_node().has_error(),
                "expected an error for {src:?}: {}",
                tree.root_node().to_sexp()
            );
        }
    }

    /// An `@name` with no group at all is text, not a nexus error: the real
    /// parser keeps `me@example.com` and a lone `@mention` as prose. (At the
    /// start of a line the real parser calls a bare `@name` an error; the
    /// grammar cannot tell position, so it accepts it everywhere -- see
    /// `_nexus_groups` in `grammar.js`.)
    #[test]
    fn a_bare_at_name_is_not_an_error() {
        for src in ["me@example.com\n", "ping @someone about it\n", "a @hr b\n"] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}: {}",
                tree.root_node().to_sexp()
            );
        }
    }

    #[test]
    fn parses_a_group_separated_from_its_element_by_whitespace() {
        // Regression test: `token(prec(1, "["))`/`"("`/`"{"` on the
        // group-opening literals in `grammar.js` -- without it, any
        // whitespace between an element and its first group caused the
        // element to parse with zero groups and the group's own text to
        // fall out as unrelated paragraph content.
        let tree = parse("@links() {\n  (1)[ note ]\n  (anotation1)[ note2 ]\n}\n");
        let root = tree.root_node();
        let element = root.child(0).unwrap().child(0).unwrap();
        assert_eq!(element.kind(), "element");
        let inline_element = element
            .child(0)
            .expect("expected element to wrap an inline_element");
        assert_eq!(inline_element.kind(), "inline_element");
        let value_group = inline_element
            .named_child(2)
            .expect("expected a value_group as the element's third named child");
        assert_eq!(value_group.kind(), "value_group");
        let children = value_group
            .named_child(0)
            .expect("expected a children node");
        assert_eq!(children.kind(), "children");
        assert_eq!(children.named_child_count(), 2);
    }

    #[test]
    fn highlights_query_is_valid() {
        let query_src = include_str!("../queries/highlights.scm");
        tree_sitter::Query::new(&LANGUAGE.into(), query_src)
            .expect("queries/highlights.scm should be a valid query against this grammar");
    }

    #[test]
    fn highlights_captures_element_brackets_as_tag() {
        use tree_sitter::StreamingIterator;
        let src = "@link[content](args)\n";
        let tree = parse(src);
        let query_src = include_str!("../queries/highlights.scm");
        let query = tree_sitter::Query::new(&LANGUAGE.into(), query_src).unwrap();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());

        let mut captured: Vec<(String, String)> = Vec::new();
        while let Some(m) = matches.next() {
            for c in m.captures {
                let name = query.capture_names()[c.index as usize].to_string();
                let text = &src[c.node.byte_range()];
                captured.push((name, text.to_string()));
            }
        }

        // Verify that "[" and "]" and "(" and ")" inside the element are captured as "tag"
        assert!(captured.contains(&("tag".into(), "[".into())));
        assert!(captured.contains(&("tag".into(), "]".into())));
        assert!(captured.contains(&("tag".into(), "(".into())));
        assert!(captured.contains(&("tag".into(), ")".into())));

        // Plain brackets outside element are punctuation.bracket, not tag
        let plain_src = "plain [brackets] and (parens)\n";
        let plain_tree = parse(plain_src);
        let mut plain_matches =
            cursor.matches(&query, plain_tree.root_node(), plain_src.as_bytes());
        let mut plain_captured: Vec<(String, String)> = Vec::new();
        while let Some(m) = plain_matches.next() {
            for c in m.captures {
                let name = query.capture_names()[c.index as usize].to_string();
                let text = &plain_src[c.node.byte_range()];
                plain_captured.push((name, text.to_string()));
            }
        }
        assert!(!plain_captured.contains(&("tag".into(), "[".into())));
        assert!(!plain_captured.contains(&("tag".into(), "(".into())));
    }

    #[test]
    fn highlights_captures_call_and_brackets_as_function() {
        use tree_sitter::StreamingIterator;
        let src = "::meta(tags: list(1, 2))\n";
        let tree = parse(src);
        let query_src = include_str!("../queries/highlights.scm");
        let query = tree_sitter::Query::new(&LANGUAGE.into(), query_src).unwrap();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());

        let mut captured: Vec<(String, String)> = Vec::new();
        while let Some(m) = matches.next() {
            for c in m.captures {
                let name = query.capture_names()[c.index as usize].to_string();
                let text = &src[c.node.byte_range()];
                captured.push((name, text.to_string()));
            }
        }

        // Verify that "list", "(", and ")" inside the call are captured as "function"
        assert!(captured.contains(&("function".into(), "list".into())));
        assert!(captured.contains(&("function".into(), "(".into())));
        assert!(captured.contains(&("function".into(), ")".into())));
    }

    #[test]
    fn highlights_captures_named_connect_as_punctuation() {
        use tree_sitter::StreamingIterator;
        let src = "@doc()::rule(allow)\n";
        let tree = parse(src);
        let query_src = include_str!("../queries/highlights.scm");
        let query = tree_sitter::Query::new(&LANGUAGE.into(), query_src).unwrap();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());

        let mut captured: Vec<(String, String)> = Vec::new();
        while let Some(m) = matches.next() {
            for c in m.captures {
                let name = query.capture_names()[c.index as usize].to_string();
                let text = &src[c.node.byte_range()];
                captured.push((name, text.to_string()));
            }
        }

        // Verify that "::", "rule", "(", and ")" in ::rule() are captured as "punctuation.special"
        assert!(captured.contains(&("punctuation.special".into(), "::".into())));
        assert!(captured.contains(&("punctuation.special".into(), "rule".into())));
        assert!(captured.contains(&("punctuation.special".into(), "(".into())));
        assert!(captured.contains(&("punctuation.special".into(), ")".into())));
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
    fn injections_query_is_valid() {
        let query_src = include_str!("../queries/injections.scm");
        tree_sitter::Query::new(&LANGUAGE.into(), query_src)
            .expect("queries/injections.scm should be a valid query against this grammar");
    }

    #[test]
    fn folds_query_is_valid() {
        let query_src = include_str!("../queries/folds.scm");
        tree_sitter::Query::new(&LANGUAGE.into(), query_src)
            .expect("queries/folds.scm should be a valid query against this grammar");
    }

    #[test]
    fn parses_single_quoted_strings() {
        let src = "::settings(title: 'my title') { path: 'foo/bar' }\n";
        let tree = parse(src);
        assert!(!tree.root_node().has_error());
        assert!(find_kind(tree.root_node(), "string").is_some());
    }

    #[test]
    fn parses_colon_less_braced_maps() {
        let src = "::settings() {\n  elements {\n    bookmark {\n      title: { type: string }\n    }\n  }\n}\n";
        let tree = parse(src);
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn map_entry_value_is_optional_for_null_shorthand() {
        // `key:` with nothing after it is a legal map entry with no
        // value. Were `_entry_value` *mandatory*, this
        // shape would not just mismatch locally like the grammar's other
        // known-narrow cases -- it would derail the entire enclosing
        // block into an `ERROR` that swallowed everything after it too.
        let src = "::meta(\nid: doc-1\nflags:\nrating:\nreviewed:\ntype: j.daily\n)\n";
        let tree = parse(src);
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn map_entry_call_value_parses_cleanly_last_or_not() {
        // `_value_scalar`'s regex must not match a lone leading space as
        // a bogus one-character `scalar`, which would win out over
        // skipping that space as `extras` first -- see that rule's own
        // comment in `grammar.js`. Covers both the shape found first
        // (a `list(...)`-valued entry last in the map, no trailing comma)
        // and the broader one (an earlier, non-last `list(...)`-valued
        // entry, still followed by more entries).
        for src in [
            "::meta(\naliases: list()\n)\n",
            "::meta(\naliases: list()\nflags: x\n)\n",
            "::meta(\naliases: list(a, b)\nflags: x\n)\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}"
            );
        }
    }

    #[test]
    fn map_entry_value_scalar_keeps_multi_word_and_colon_content() {
        // Guards against an overly-broad fix for the above: excluding a
        // leading space/tab from `_value_scalar` must not exclude *all*
        // whitespace, or multi-word bare values would break, and the tail
        // must still allow colons (URLs, timestamps).
        let src = "::meta(\na: hello world\nb: 12:34\nc: https://example.com/x\n)\n";
        let tree = parse(src);
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn probe2() {
        let src = "@ns.thing(\n  a: [\n    para1.\n\n    para2.\n  ]\n  b: [ one para. ]\n)\n";
        let tree = parse(src);
        eprintln!(
            "error={} sexp={}",
            tree.root_node().has_error(),
            tree.root_node().to_sexp()
        );
    }

    fn nodes_of_kind<'t>(tree: &'t tree_sitter::Tree, kind: &str) -> Vec<Node<'t>> {
        let mut out = Vec::new();
        let mut cursor = tree.walk();
        'walk: loop {
            if cursor.node().kind() == kind {
                out.push(cursor.node());
            }
            if cursor.goto_first_child() {
                continue;
            }
            loop {
                if cursor.goto_next_sibling() {
                    continue 'walk;
                }
                if !cursor.goto_parent() {
                    break 'walk;
                }
            }
        }
        out
    }

    #[test]
    fn a_double_colon_line_is_a_document_declaration() {
        let src = "::kind(config)\n::meta(id: x)\n\n=[ T ]\n";
        let tree = parse(src);
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
        let decls = nodes_of_kind(&tree, "document_element");
        assert_eq!(decls.len(), 2);
        let name = decls[0].child_by_field_name("name").unwrap();
        assert_eq!(name.utf8_text(src.as_bytes()).unwrap(), "kind");
        assert_eq!(
            decls[0].utf8_text(src.as_bytes()).unwrap(),
            "::kind(config)"
        );
    }

    #[test]
    fn a_declaration_takes_groups_and_an_id_but_no_connect() {
        let tree = parse("::x(a)[ b ]{ c: 1 }#(i)\n");
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
        assert_eq!(nodes_of_kind(&tree, "document_element").len(), 1);
        assert_eq!(nodes_of_kind(&tree, "id_group").len(), 1);
        // The real parser refuses a connect on a declaration. Here a `::name(...)` after one
        // reads as a declaration of its own, never as a `connect`.
        let tree = parse("::x(a)::rule(b)\n");
        assert_eq!(nodes_of_kind(&tree, "document_element").len(), 2);
        assert_eq!(nodes_of_kind(&tree, "connect").len(), 0);
    }

    #[test]
    fn a_double_colon_that_does_not_open_a_line_is_not_one() {
        for src in [
            "std::vec is a type\n",
            "text ::kind(x) in the middle\n",
            "Rust's mem::swap(a, b) swaps\n",
        ] {
            let tree = parse(src);
            assert!(
                nodes_of_kind(&tree, "document_element").is_empty(),
                "{src:?}: {}",
                tree.root_node().to_sexp()
            );
        }
    }

    #[test]
    fn the_next_declaration_is_not_a_connect_of_the_one_before() {
        let tree = parse("::kind(config)\n::meta(id: x)\n");
        assert_eq!(nodes_of_kind(&tree, "document_element").len(), 2);
        assert!(nodes_of_kind(&tree, "connect").is_empty());
    }

    #[test]
    fn highlights_colour_a_declaration_like_an_element() {
        use tree_sitter::StreamingIterator;
        let src = "::kind(config)\n";
        let tree = parse(src);
        let query = tree_sitter::Query::new(&LANGUAGE.into(), HIGHLIGHTS_QUERY).unwrap();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());
        let mut captured: Vec<(String, String)> = Vec::new();
        while let Some(m) = matches.next() {
            for c in m.captures {
                captured.push((
                    query.capture_names()[c.index as usize].to_string(),
                    src[c.node.byte_range()].to_string(),
                ));
            }
        }
        for want in ["::", "kind", "(", ")"] {
            assert!(
                captured.contains(&("tag".to_string(), want.to_string())),
                "{want:?} in {captured:?}"
            );
        }
    }

    #[test]
    fn a_name_tag_and_a_hole_are_boxes() {
        let tree = parse("--out <PATH> と <ファイル> and Vec<T>\n");
        assert!(!tree.root_node().has_error());
        assert_eq!(nodes_of_kind(&tree, "label").len(), 3);

        let tree = parse("adr-<?int(n)>-<?(slug)>.md and <?(id)>[sub <?>] end\n");
        assert!(!tree.root_node().has_error());
        assert_eq!(nodes_of_kind(&tree, "hole").len(), 4);
        // The content is outside the box and holds a box of its own.
        let outer = nodes_of_kind(&tree, "hole")
            .into_iter()
            .find(|h| {
                h.child((h.child_count() - 1) as u32)
                    .is_some_and(|c| c.kind() == "content_group")
            })
            .expect("the hole with a content group");
        assert_eq!(nodes_of_kind(&tree, "content_group").len(), 1);
        let inside = nodes_of_kind(&tree, "hole")
            .iter()
            .filter(|h| h.start_byte() >= outer.start_byte() && h.end_byte() <= outer.end_byte())
            .count();
        assert_eq!(inside, 2, "the hole and the one in its content");
    }

    #[test]
    fn text_that_is_no_box_is_neither_a_label_nor_a_hole() {
        for src in [
            "a < b\n",
            "x<=1\n",
            "<->\n",
            "<>\n",
            "<3>\n",
            "<a b>\n",
            "<!doctype html>\n",
            "</x>\n",
            "<x/>\n",
            "a<b と c>d\n",
        ] {
            let tree = parse(src);
            assert!(!tree.root_node().has_error(), "`{src}` has an error");
            assert!(
                nodes_of_kind(&tree, "label").is_empty(),
                "`{src}` has a label"
            );
            assert!(
                nodes_of_kind(&tree, "hole").is_empty(),
                "`{src}` has a hole"
            );
        }
    }

    #[test]
    fn highlights_colour_a_box() {
        use tree_sitter::StreamingIterator;
        let src = "see <PATH> and <?int(n)>[x]\n";
        let tree = parse(src);
        let query = tree_sitter::Query::new(&LANGUAGE.into(), HIGHLIGHTS_QUERY).unwrap();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());
        let mut captured: Vec<(String, String)> = Vec::new();
        while let Some(m) = matches.next() {
            for c in m.captures {
                captured.push((
                    query.capture_names()[c.index as usize].to_string(),
                    src[c.node.byte_range()].to_string(),
                ));
            }
        }
        for (capture, text) in [
            ("string.special", "<PATH>"),
            ("keyword", "<?int"),
            ("keyword", ">"),
        ] {
            assert!(
                captured.contains(&(capture.to_string(), text.to_string())),
                "{capture} on {text:?} in {captured:?}"
            );
        }
    }

    /// The source text of every `type_seat` node, in order.
    fn seat_texts(src: &str) -> Vec<String> {
        let tree = parse(src);
        nodes_of_kind(&tree, "type_seat")
            .into_iter()
            .map(|n| src[n.byte_range()].to_string())
            .collect()
    }

    #[test]
    fn a_type_seat_is_a_head_arguments_and_a_close() {
        let src = "<$uuid> and <$list(string, length: 6)>\n";
        let tree = parse(src);
        assert!(!tree.root_node().has_error());
        assert_eq!(seat_texts(src), ["<$uuid>", "<$list(string, length: 6)>"]);
        let text_of = |kind: &str| -> Vec<&str> {
            nodes_of_kind(&tree, kind)
                .into_iter()
                .map(|n| &src[n.byte_range()])
                .collect()
        };
        // The head is the `<$` and the name; the `>` is its own node.
        assert_eq!(text_of("type_seat_open"), ["<$uuid", "<$list"]);
        assert_eq!(text_of("type_seat_close"), [">", ">"]);
        // The arguments are an expression's: a type, and a named argument.
        assert_eq!(text_of("interp_named_arg"), ["length: 6"]);
        assert_eq!(text_of("number"), ["6"]);
        // Not a hole, though it takes a hole's `>`.
        assert!(nodes_of_kind(&tree, "hole").is_empty());
        assert!(nodes_of_kind(&tree, "hole_close").is_empty());
    }

    #[test]
    fn a_type_seat_stands_where_a_hole_does_and_in_a_value() {
        for (src, expected) in [
            // A block, in running text and in a heading.
            ("<$uuid>\n", 1),
            ("id <$uuid> and <$date(default: auto)> x\n", 2),
            ("=[ title <$string> ]\n", 1),
            ("- item <$int>\n-. item <$int(min: 0)>\n", 2),
            ("*em <$int>* **strong <$int>** ~~gone <$int>~~\n", 3),
            ("@a[ <$int> ]\n@b[ x\n<$int>\ny ]\n", 2),
            ("<?(h)>[ <$int> ]\n", 1),
            // A value: the whole `(args)`, a pair, a sequence, a `{value}`.
            ("::id(<$uuid(default: auto)>)\n", 1),
            ("@x(<$int>)\n", 1),
            ("@x(key: <$string>)\n", 1),
            ("@x(k: (<$string>, <$int>))\n", 2),
            ("@x(k: list(<$int>))\n", 1),
            ("@x(){ k: <$string> }\n", 1),
            ("@x(){ k: <$string>, l: <$int(default: 0)> }\n", 2),
            ("::meta(\n  a: <$string>\n  b: <$list(string)>\n)\n", 2),
            ("@x(k: <$string> )\n", 1),
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "an error in {src:?}: {}",
                tree.root_node().to_sexp()
            );
            assert_eq!(seat_texts(src).len(), expected, "{src:?}");
        }
        // In a value, it is a seat and not the scalar it would otherwise read as.
        let tree = parse("@x(k: <$string>, l: <$uuid(default: auto)>)\n");
        assert!(nodes_of_kind(&tree, "scalar").is_empty());
    }

    #[test]
    fn a_type_seat_closes_at_the_first_closer_outside_parentheses_and_strings() {
        for src in [
            "<$list(list(string), length: 6)>",
            // A `>` in the arguments closes nothing, nested or in a string.
            "<$x(a > b)>",
            "<$x((a > b), c)>",
            r#"<$nanoid(prefix: ">")>"#,
            r#"<$nanoid(prefix: ")>")>"#,
            r#"<$x(")")>"#,
            // The arguments may run over lines.
            "<$list(\n  string,\n  length: 6\n)>",
            "<$list(\n)>",
        ] {
            let text = format!("{src}\n");
            let tree = parse(&text);
            assert!(!tree.root_node().has_error(), "an error in {src:?}");
            assert_eq!(seat_texts(&text), [src], "{src:?}");
        }
        // The first `>` outside them ends it: what follows is text.
        for (src, seats) in [
            ("<$x>y>\n", vec!["<$x>"]),
            ("<$x(a)> > <$y>\n", vec!["<$x(a)>", "<$y>"]),
        ] {
            assert!(!parse(src).root_node().has_error(), "{src:?}");
            assert_eq!(seat_texts(src), seats, "{src:?}");
        }
    }

    #[test]
    fn text_that_is_no_type_seat_is_neither_a_seat_nor_an_error() {
        for src in [
            // No name, or no name the parser accepts.
            "<$>\n",
            "<$ x>\n",
            "<$5>\n",
            "<$-x>\n",
            "<$é>\n",
            "<$x.y>\n",
            "<$x::y>\n",
            "cost <$5> and <$> x\n",
            // Not tight, or not closed.
            "<$x >\n",
            "<$x\n",
            "a <$x b\n",
            // Takes no `{value}` and no second `(args)`.
            "<$x{a: 1}>\n",
            "<$x(a)(b)>\n",
            // Escaped.
            "\\<$uuid> is text\n",
            // Inside a code span.
            "`<$uuid>`\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "an error in {src:?}: {}",
                tree.root_node().to_sexp()
            );
            assert!(seat_texts(src).is_empty(), "a seat in {src:?}");
        }
        // As a value, too: a scalar, as it was before there were seats.
        for src in [
            "@x(k: <$5>, l: <$x >)\n",
            "@x(<$5>)\n",
            "@x(<$>)\n",
            "@x(<$x y>)\n",
            "@x(k: <$x y>)\n",
            "@x(<name>)\n",
            "@x(a < b)\n",
        ] {
            let tree = parse(src);
            assert!(!tree.root_node().has_error(), "an error in {src:?}");
            assert!(seat_texts(src).is_empty(), "a seat in {src:?}");
        }
    }

    #[test]
    fn a_type_seat_takes_no_content() {
        // The `[t]` after it is prose -- and a stray `]`, as anywhere in prose (see the module doc).
        let tree = parse("<$x>[t]\n");
        assert_eq!(nodes_of_kind(&tree, "type_seat").len(), 1);
        assert!(nodes_of_kind(&tree, "content_group").is_empty());
        // A hole takes it.
        let tree = parse("<?x>[t]\n");
        assert_eq!(nodes_of_kind(&tree, "content_group").len(), 1);
    }

    #[test]
    fn highlights_colour_a_type_seat() {
        use tree_sitter::StreamingIterator;
        let src = "see <$uuid> and <$list(list(string), length: 6)> and <?int(n)>\n";
        let tree = parse(src);
        let query = tree_sitter::Query::new(&LANGUAGE.into(), HIGHLIGHTS_QUERY).unwrap();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());
        let mut captured: Vec<(String, String)> = Vec::new();
        while let Some(m) = matches.next() {
            for c in m.captures {
                captured.push((
                    query.capture_names()[c.index as usize].to_string(),
                    src[c.node.byte_range()].to_string(),
                ));
            }
        }
        for (capture, text) in [
            ("type", "<$uuid"),
            ("type", "<$list"),
            ("type", ">"),
            ("punctuation.bracket", "("),
            ("punctuation.bracket", ")"),
            ("property", "length"),
            ("function", "list"),
            // A hole keeps its own.
            ("keyword", "<?int"),
        ] {
            assert!(
                captured.contains(&(capture.to_string(), text.to_string())),
                "{capture} on {text:?} in {captured:?}"
            );
        }
    }

    #[test]
    fn parses_general_backslash_escapes_and_single_backtick_code_spans() {
        for (src, expected_escapes) in [
            ("\\*not em\\*\n", vec!["\\*", "\\*"]),
            ("\\**not strong\\**\n", vec!["\\**", "\\**"]),
            ("use \\`code\\` and `real`\n", vec!["\\`", "\\`"]),
            (
                "text \\// not comment and \\/* not block */\n",
                vec!["\\//", "\\/"],
            ),
            (
                "\\= not section and \\--- not hr and \\- not list\n",
                vec!["\\=", "\\---", "\\-"],
            ),
            ("\\::decl(x) and \\@elem(x)\n", vec!["\\::", "\\@"]),
            ("\\${interp} and \\<?hole>\n", vec!["\\$", "\\<"]),
            ("@note[a \\[ b \\] c]\n", vec!["\\[", "\\]"]),
            (
                "literal \\\\ and \\\\*em* and C:\\Users\\name\n",
                vec!["\\\\", "\\\\"],
            ),
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "expected no errors for {src:?}: {}",
                tree.root_node().to_sexp()
            );
            let actual_escapes: Vec<&str> = nodes_of_kind(&tree, "escape")
                .into_iter()
                .map(|n| n.utf8_text(src.as_bytes()).unwrap())
                .collect();
            assert_eq!(actual_escapes, expected_escapes, "for {src:?}");
        }

        // Empty `` in mid-line is not a code_span; single-backtick `code` is.
        let tree = parse("mid `` empty and `code` span\n");
        assert!(!tree.root_node().has_error());
        let spans: Vec<&str> = nodes_of_kind(&tree, "code_span")
            .into_iter()
            .map(|n| {
                n.utf8_text("mid `` empty and `code` span\n".as_bytes())
                    .unwrap()
            })
            .collect();
        assert_eq!(spans, ["`code`"]);
    }
}
