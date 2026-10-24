//! The cotomet corpus (`fixtures/cotomet/*.ctmt`), read by the two independent
//! implementations of the language: `cotomet-syntax` (the source of truth) and
//! `tree-sitter-cotomet` (a hand-maintained approximation for editor
//! highlighting). Same arrangement as `corpus.rs` has for `.tmt`.
//!
//! A fixture is named for what the parser must say of it: `ok-*` is a cotomet
//! file, `err-*` is not. The grammar must read every `ok-*` without an
//! `ERROR`. Of the `err-*` it may read some without one -- it is looser than
//! the parser on purpose -- but only those in `GRAMMAR_ACCEPTS`, so a fixture
//! that starts or stops being accepted shows up as a failure and not as a
//! quietly changed picture of what is covered.

use std::path::{Path, PathBuf};

/// `err-*` fixtures the grammar reads without an error, and why.
const GRAMMAR_ACCEPTS: &[(&str, &str)] = &[
    (
        "err-bare-value.ctmt",
        "a file is `key: value` entries; the grammar reads a stray value as a value",
    ),
    (
        "err-declaration-after-entry.ctmt",
        "the parser wants declarations before the first entry; the grammar has no notion of before",
    ),
    (
        "err-duplicate-key.ctmt",
        "a repeated key is a meaning, not a shape",
    ),
    (
        "err-integer-overflow.ctmt",
        "a number is typed by the highlight query, which does not know i64",
    ),
    (
        "err-lone-scalar.ctmt",
        "a file is `key: value` entries; the grammar reads a stray value as a value",
    ),
];

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/cotomet");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to read {dir:?}: {e}"))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "ctmt"))
        .collect();
    files.sort();
    files
}

fn name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

fn ts_has_error(src: &str) -> bool {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_cotomet::LANGUAGE.into())
        .expect("failed to load the cotomet grammar");
    parser
        .parse(src, None)
        .expect("parse returned None")
        .root_node()
        .has_error()
}

#[test]
fn every_fixture_is_named_for_what_the_parser_says_of_it() {
    let mut wrong = Vec::new();
    for path in fixtures() {
        let src = std::fs::read_to_string(&path).unwrap();
        let parsed = cotomet_syntax::parse_file(&src);
        match (name(&path).starts_with("ok-"), parsed) {
            (true, Err(e)) => wrong.push(format!("{}: should parse: {e}", name(&path))),
            (false, Ok(_)) => wrong.push(format!("{}: should be rejected", name(&path))),
            _ => {}
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_grammar_reads_every_cotomet_file_without_an_error() {
    let mut errors = Vec::new();
    for path in fixtures() {
        if name(&path).starts_with("ok-") && ts_has_error(&std::fs::read_to_string(&path).unwrap())
        {
            errors.push(name(&path));
        }
    }
    assert!(
        errors.is_empty(),
        "tree-sitter-cotomet errors on what cotomet-syntax accepts: {errors:?}"
    );
}

#[test]
fn the_grammar_rejects_what_the_parser_rejects_except_the_known_looseness() {
    let mut accepted: Vec<String> = fixtures()
        .iter()
        .filter(|p| name(p).starts_with("err-"))
        .filter(|p| !ts_has_error(&std::fs::read_to_string(p).unwrap()))
        .map(|p| name(p))
        .collect();
    accepted.sort();
    let mut known: Vec<String> = GRAMMAR_ACCEPTS.iter().map(|(n, _)| n.to_string()).collect();
    known.sort();
    assert_eq!(
        accepted, known,
        "the grammar's looseness changed; update GRAMMAR_ACCEPTS (and say why) or fix the grammar"
    );
}
