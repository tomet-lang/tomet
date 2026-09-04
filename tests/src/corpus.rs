//! Corpus-level tests: what the shared fixtures parse to, in both of the
//! language's two independent grammar implementations.
//!
//! `tomet-parser` is the source of truth; `tree-sitter-tomet`'s
//! `grammar.js` is a hand-maintained approximation used for editor syntax
//! highlighting. A grammar change in `tomet-parser` does not show up in
//! the tree-sitter grammar on its own, so
//! `tree_sitter_has_no_unexpected_errors_anywhere_in_the_corpus` sweeps
//! **every** fixture through both and holds the difference against
//! `KNOWN_TS_ERRORS`.
//!
//! It used to read two hard-coded filenames instead, which meant most of
//! the corpus was never shown to the grammar at all, and one of the two
//! carried an empty marker -- `text.contains("")` is always true, so that
//! check could not fail. Widening the sweep immediately turned up five
//! fixtures that had been drifting unseen, and one entry that had been
//! recording errors the grammar stopped making.
//!
//! Moved here from `tomet-syntax-parser` and `tree-sitter-tomet`, which
//! were reaching up to the repo root for these fixtures.

use std::path::Path;
use tomet_tests::{
    ANY_ERROR, KNOWN_TS_ERRORS, KNOWN_UNPARSEABLE, MISSING_NODE, corpus, is_known_unparseable, known_ts_errors,
    read_fixture, ts_error_texts, ts_parse,
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
fn tree_sitter_has_no_unexpected_errors_anywhere_in_the_corpus() {
    let mut unexpected = Vec::new();
    let mut clean_but_listed = Vec::new();

    for rel in corpus() {
        let src = read_fixture(&rel);
        let tree = ts_parse(&src);
        let errors: Vec<String> = ts_error_texts(&src, &tree)
            .into_iter()
            .map(|text| {
                if text.is_empty() {
                    MISSING_NODE.to_string()
                } else {
                    text
                }
            })
            .collect();

        match known_ts_errors(&rel) {
            None => {
                for text in &errors {
                    unexpected.push(format!("{}: {text:?}", rel.display()));
                }
            }
            Some(markers) => {
                if errors.is_empty() {
                    clean_but_listed.push(rel.display().to_string());
                }
                if markers.contains(&ANY_ERROR) {
                    continue;
                }
                for text in &errors {
                    if !markers.iter().any(|marker| text.contains(marker)) {
                        unexpected.push(format!("{}: {text:?}", rel.display()));
                    }
                }
            }
        }
    }

    assert!(
        unexpected.is_empty(),
        "the tree-sitter grammar errors on syntax `tomet-parser` accepts.\n\
         `grammar.js` is hand-maintained and does not follow the parser on\n\
         its own -- update it, or record the case in `KNOWN_TS_ERRORS`:\n  {}",
        unexpected.join("\n  ")
    );
    assert!(
        clean_but_listed.is_empty(),
        "these fixtures are listed in KNOWN_TS_ERRORS but the grammar now\n\
         parses them cleanly. That is good news -- remove them:\n  {}",
        clean_but_listed.join("\n  ")
    );
}

/// A marker of `""` matches every string, which would make the sweep above
/// pass no matter what the grammar did. One was in the list until the
/// sweep was written, and it had made the readme check vacuous.
#[test]
fn known_ts_error_markers_are_not_vacuous() {
    for (name, markers) in KNOWN_TS_ERRORS {
        assert!(
            !markers.is_empty(),
            "{name} has an empty marker list; drop the entry instead"
        );
        for marker in *markers {
            assert!(
                !marker.is_empty(),
                "{name} has an empty marker, which matches anything"
            );
        }
        assert!(
            corpus().iter().any(|rel| rel == Path::new(name)),
            "KNOWN_TS_ERRORS lists {name:?}, which is not in the corpus"
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
