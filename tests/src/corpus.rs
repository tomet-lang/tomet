//! Corpus-level tests: what the shared fixtures parse to, in both of the
//! language's two independent grammar implementations.
//!
//! `tomet-parser` is the source of truth; `tree-sitter-tomet`'s
//! `grammar.js` is a hand-maintained approximation used for editor syntax
//! highlighting. The `known error cases` tests below are how the two are
//! noticed drifting apart -- a grammar change in `tomet-parser` does not
//! automatically show up in the tree-sitter grammar.
//!
//! Moved here from `tomet-syntax-parser` and `tree-sitter-tomet`, which
//! were reaching up to the repo root for these fixtures.

use std::path::Path;
use tomet_tests::{
    KNOWN_UNPARSEABLE, corpus, is_known_unparseable, read_fixture, ts_error_texts, ts_parse,
};

#[test]
fn every_fixture_parses_except_the_known_exceptions() {
    let mut unexpected_failures = Vec::new();
    let mut unexpected_successes = Vec::new();

    for rel in corpus() {
        let src = read_fixture(&rel);
        let parsed = tomet_parser::parse_document(&src);
        match (parsed, is_known_unparseable(&rel)) {
            (Err(e), false) => unexpected_failures.push(format!("{}: {e}", rel.display())),
            (Ok(_), true) => unexpected_successes.push(rel.display().to_string()),
            _ => {}
        }
    }

    assert!(
        unexpected_failures.is_empty(),
        "fixtures failed to parse:\n  {}",
        unexpected_failures.join("\n  ")
    );
    assert!(
        unexpected_successes.is_empty(),
        "these fixtures are listed in KNOWN_UNPARSEABLE but now parse cleanly.\n\
         That is good news -- remove them from the list in `src/lib.rs`:\n  {}",
        unexpected_successes.join("\n  ")
    );
}

#[test]
fn corpus_is_not_empty() {
    // Guards against the corpus walk silently finding nothing, which
    // would make every test above vacuously pass.
    assert!(
        !corpus().is_empty(),
        "expected .tmt fixtures under tests/fixtures/"
    );
    for name in KNOWN_UNPARSEABLE {
        assert!(
            corpus().iter().any(|rel| rel == Path::new(name)),
            "KNOWN_UNPARSEABLE lists {name:?}, which is not in the corpus"
        );
    }
}

#[test]
fn tree_sitter_parses_every_fixture_to_a_document_root() {
    for rel in corpus() {
        let src = read_fixture(&rel);
        let tree = ts_parse(&src);
        assert_eq!(
            tree.root_node().kind(),
            "document",
            "{} failed to produce a document root node",
            rel.display()
        );
    }
}

#[test]
fn tree_sitter_readme_fixture_has_only_known_error_cases() {
    let src = read_fixture(Path::new("readme.tmt"));
    let tree = ts_parse(&src);
    let errors = ts_error_texts(&src, &tree);
    // Every error node's text contains (or exactly is) one of these
    // markers, each tied to one documented case in `tree-sitter-tomet`'s
    // module doc comment: the `のうち.../のルール` snippets are the
    // stray-`]`-in-prose and the pre-existing `[]`-inside-`[...]` parser
    // bug, both from this file's self-referential grammar-explanation
    // prose, and `"key":` is the embedded-JSON quoted-key case
    // (`@meta(format:json){ { "key": "value" } }`).
    let known_markers = ["のうち必要なものを付ける", "のルール", "\"key\":", ""];
    for text in &errors {
        assert!(
            known_markers.iter().any(|marker| text.contains(marker)),
            "unexpected error node text: {text:?}"
        );
    }
}

#[test]
fn tree_sitter_image_meta_fixture_parses_cleanly() {
    // Used to have one remaining error wrapping `tags: [a, b]` -- the
    // `key: [seq]`-map-entry-value misparse `tree-sitter-tomet`'s module
    // doc comment used to document as a known gap, fixed by excluding a
    // leading space/tab from `_value_scalar`'s character class.
    let src = read_fixture(Path::new("examples/image.meta.tmt"));
    let tree = ts_parse(&src);
    assert!(!tree.root_node().has_error());
}

#[test]
fn tree_sitter_cheatsheet_fixture_has_only_known_error_cases() {
    // The embedded-JSON/TOML case, the triple-backtick/unterminated
    // code-span case, and the `#memo+++...+++` stray-character
    // case all used to need markers here too, but no longer error at all
    // now that they live inside this file's ``` fenced code block, which
    // the grammar's `fenced_code_block` rule consumes as one opaque text
    // run rather than parsing its contents as markup.
    //
    // `----[💫]----` is the one leftover: `[` has more competing token
    // definitions than `(` does (`heading`'s own content-opening `[`,
    // `area_group`'s, and `punctuation`'s all coexist unshared, unlike
    // `-`, which only ever meant "start a list" or "plain punctuation")
    // -- covered by the `"]"` marker below.
    let src = read_fixture(Path::new("cheatsheet.tmt"));
    let tree = ts_parse(&src);
    let errors = ts_error_texts(&src, &tree);
    let known_markers = ["@config(", "や", "]"];
    for text in &errors {
        assert!(
            known_markers.iter().any(|marker| text.contains(marker)),
            "unexpected error node text: {text:?}"
        );
    }
}
