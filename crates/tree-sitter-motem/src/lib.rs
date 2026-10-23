//! =[ tree-sitter-motem ]
//!
//! Rust binding for the motem tree-sitter grammar (`grammar.js` at the crate
//! root; `src/parser.c` etc. are generated from it by
//! `npx tree-sitter-cli@0.26.12 generate`, then compiled by `build.rs`).
//!
//! This grammar exists for editor syntax highlighting of a standalone motem
//! file (`.mtm`), not as a source of truth -- that is `motem-syntax`'s job. It
//! is an approximation, and `tests/src/motem.rs` in the `tomet-tests` package
//! runs a corpus through both to catch the two drifting apart. What it
//! simplifies:
//!
//! -| **A `:` ends a key only at the start of an entry.** The parser reads
//!  | `note: see: this` as the text `see: this`, and so does the grammar; but
//!  | in a file or a `{...}` an entry that starts `x: y` is a pair, and a bare
//!  | scalar there holds a `:` only when text runs straight on after it
//!  | (`12:30`). Entries need no separator in the grammar, where the parser
//!  | wants a newline or a `,` (it reads the rest of the line as one scalar).
//! -| **A bare scalar is not typed here.** `null`, `true`, `false` and a
//!  | number are the highlight query's business, decided from the node's
//!  | text, so the tree has one `scalar` node for all of them.
//! -| **Call arguments are values only.** `list(a: 1)` is an error here as it
//!  | is for the parser, but `list(a: b)` reads as the scalar `a: b`, which
//!  | the parser also does.
//! -| **A top-level bare value is accepted.** A `.mtm` file is `key: value`
//!  | entries only (`motem_syntax::parse_file`); the grammar reads a stray
//!  | value as the value it is.
//! -| **No block comments.** motem has `//` only; `/* */` is a tomet
//!  | document construct.

use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_motem() -> *const ();
}

/// The tree-sitter language function for motem. Convert to a
/// `tree_sitter::Language` via `.into()` to build a `tree_sitter::Parser`.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_motem) };

/// The grammar's `node-types.json`.
pub const NODE_TYPES: &str = include_str!("node-types.json");

/// The grammar's `highlights.scm` query string for syntax highlighting.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../queries/highlights.scm");

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&LANGUAGE.into())
            .expect("failed to load the motem grammar");
        parser.parse(src, None).expect("parse returned None")
    }

    /// The tree as an S-expression, without the positions.
    fn sexp(src: &str) -> String {
        parse(src).root_node().to_sexp()
    }

    /// Every `(capture name, text)` the highlight query makes over `src`.
    fn highlights(src: &str) -> Vec<(String, String)> {
        let tree = parse(src);
        let query = Query::new(&LANGUAGE.into(), HIGHLIGHTS_QUERY).expect("query compiles");
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());
        let mut captured = Vec::new();
        while let Some(m) = matches.next() {
            for c in m.captures {
                let name = query.capture_names()[c.index as usize].to_string();
                captured.push((name, src[c.node.byte_range()].to_string()));
            }
        }
        captured
    }

    #[test]
    fn a_file_of_entries_has_no_error() {
        for src in [
            "",
            "// only a comment\n",
            "a: 1\nb: two words\nc: true, d: null\n",
            "\"tmtroot/a.tmt\": \"../a.tmt\"\n",
            "m: { x: 1, y: { z: 2 } }\n",
            "m: {\n  x: 1\n  y: 2\n}\n",
            "l: list(1, 2, 3)\n",
            "l: list(\n  a,\n  b\n)\n",
            "n: list()\nname: string(abc)\n",
            "url: https://example.com/a//b\n",
            "path: ../vocab/x.tmt\nabs: /usr/bin\n",
            "k:\n  v\n",
            "a: 1 // note\n// header\nb: 2\n",
            "s: \"multi\nline \\\" escaped\"\n",
            "e: enum(x)\n",
            "a: 1\r\nb: 2\r\n",
            "host: localhost:8080\nt: 12:30\nratio: 1:2:3 and more\n",
        ] {
            let tree = parse(src);
            assert!(
                !tree.root_node().has_error(),
                "{src:?} -> {}",
                tree.root_node().to_sexp()
            );
        }
    }

    #[test]
    fn a_key_a_value_and_a_callee_are_told_apart_by_what_follows() {
        assert_eq!(
            sexp("a: 1\n"),
            "(source_file (pair key: (key) value: (scalar (word))))"
        );
        assert_eq!(
            sexp("\"k\": v\n"),
            "(source_file (pair key: (quoted_key) value: (scalar (word))))"
        );
        assert_eq!(
            sexp("a: hello world\n"),
            "(source_file (pair key: (key) value: (scalar (word) (word))))"
        );
        assert_eq!(
            sexp("l: list(a)\n"),
            "(source_file (pair key: (key) value: (call function: (name) (scalar (word)))))"
        );
        // a word with nothing after it is a value, not a key
        assert_eq!(sexp("x\n"), "(source_file (scalar (word)))");
    }

    #[test]
    fn a_scheme_colon_does_not_make_a_key() {
        assert_eq!(
            sexp("u: https://example.com\n"),
            "(source_file (pair key: (key) value: (scalar (word))))"
        );
    }

    #[test]
    fn a_colon_that_runs_on_belongs_to_the_scalar() {
        assert_eq!(
            sexp("host: localhost:8080\n"),
            "(source_file (pair key: (key) value: (scalar (word))))"
        );
        // and a `: ` still ends the key
        assert_eq!(
            sexp("a: b\n"),
            "(source_file (pair key: (key) value: (scalar (word))))"
        );
    }

    #[test]
    fn a_colon_and_space_inside_a_value_is_text() {
        assert_eq!(
            sexp("note: see: this one\n"),
            "(source_file (pair key: (key) value: (scalar (word) (word) (word))))"
        );
        assert_eq!(
            sexp("sep: 1 b: 2\n"),
            "(source_file (pair key: (key) value: (scalar (word) (word) (word))))"
        );
    }

    #[test]
    fn a_comment_ends_a_scalar() {
        assert_eq!(
            sexp("a: hello world // note\n"),
            "(source_file (pair key: (key) value: (scalar (word) (word))) (comment))"
        );
    }

    #[test]
    fn highlights_type_a_scalar_from_its_text() {
        let captured = highlights("a: null\nb: true\nc: -12.5e3\nd: 007\ne: two words\nf: \"s\"\n");
        for (name, text) in [
            ("constant.builtin", "null"),
            ("constant.builtin", "true"),
            ("number", "-12.5e3"),
            // not a JSON number, so a string
            ("string", "007"),
            ("string", "two words"),
            ("string", "\"s\""),
            ("property", "a"),
            ("punctuation.delimiter", ":"),
        ] {
            assert!(
                captured.contains(&(name.into(), text.into())),
                "{name} {text:?} in {captured:?}"
            );
        }
        // a scalar is exactly one of the three
        for text in ["null", "true", "-12.5e3", "007", "two words"] {
            let n = captured
                .iter()
                .filter(|(name, t)| {
                    t == text && matches!(name.as_str(), "constant.builtin" | "number" | "string")
                })
                .count();
            assert_eq!(n, 1, "{text:?} in {captured:?}");
        }
    }

    #[test]
    fn highlights_name_the_resolved_constructors_as_builtin() {
        let captured = highlights("l: list(1)\nx: custom(1)\n");
        assert!(captured.contains(&("function.builtin".into(), "list".into())));
        assert!(captured.contains(&("function".into(), "custom".into())));
        assert!(!captured.contains(&("function".into(), "list".into())));
    }

    #[test]
    fn highlights_a_quoted_key_as_a_property_and_a_comment_as_a_comment() {
        let captured = highlights("// header\n\"a/b\": 1\n");
        assert!(captured.contains(&("comment".into(), "// header".into())));
        assert!(captured.contains(&("property".into(), "\"a/b\"".into())));
        // and a key is not also a string
        assert!(!captured.contains(&("string".into(), "\"a/b\"".into())));
    }

    #[test]
    fn the_other_queries_compile() {
        for src in [
            include_str!("../queries/brackets.scm"),
            include_str!("../queries/folds.scm"),
            include_str!("../queries/indents.scm"),
        ] {
            Query::new(&LANGUAGE.into(), src).expect("query compiles");
        }
    }
}
