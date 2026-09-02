//! Shared harness for the workspace's cross-crate tests.
//!
//! This crate exists because the tests it hosts span more than one layer
//! of the pipeline and therefore belong to none of the individual crates.
//! Before it existed, four crates reached outside their own directory for
//! the shared corpus -- three via `include_str!("../../../fixtures/...")`
//! and one via a `.parent().unwrap()` walk up to the repo root, both
//! flagged as a fragility in `crates/README.dirs.tmt`.
//!
//! Layer-local tests stay in their own crate. Only tests that need the
//! shared corpus, or that cross a crate boundary, live here.
//!
//! Three test targets sit on top of this module:
//! - `corpus` -- every fixture parses (or is a known exception), and the
//!   tree-sitter grammar produces only its documented error cases.
//! - `roundtrip` -- invariants that span parser + printer + formatter.
//! - `snapshot` -- `.tmt` converted to each output format, compared
//!   against committed reference files.

use std::path::{Path, PathBuf};

/// Root of this package (`<repo>/tests`).
pub fn package_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixtures_dir() -> PathBuf {
    package_dir().join("fixtures")
}

pub fn ref_dir() -> PathBuf {
    package_dir().join("ref")
}

/// Where actual output is written when it does not match the reference,
/// so the two can be diffed by hand. Gitignored.
pub fn store_dir() -> PathBuf {
    package_dir().join("store")
}

/// Every `.tmt` file in the corpus, sorted, as paths relative to
/// `fixtures/` (e.g. `examples/image.meta.tmt`).
pub fn corpus() -> Vec<PathBuf> {
    let root = fixtures_dir();
    let mut out = Vec::new();
    collect(&root, &mut out);
    out.sort();
    out.iter()
        .map(|p| {
            p.strip_prefix(&root)
                .expect("corpus path is under fixtures/")
                .to_path_buf()
        })
        .collect()
}

fn collect(dir: &Path, acc: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => panic!("failed to read corpus directory {dir:?}: {e}"),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, acc);
        } else if path.extension().is_some_and(|ext| ext == "tmt") {
            acc.push(path);
        }
    }
}

pub fn read_fixture(rel: &Path) -> String {
    let path = fixtures_dir().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"))
}

/// Corpus entries that `tomet-parser` is currently expected to reject.
///
/// This is an allow-list, not a skip-list: `corpus.rs` asserts it matches
/// reality exactly, so a fixture that starts or stops parsing shows up as
/// a failure rather than silently changing which files get covered.
///
/// Currently empty. `cheatsheet.tmt` used to be the sole entry: it is a
/// frozen copy of `docs/guide/cheatsheet.tmt` and exercised constructs the
/// parser did not accept. The sigil rework fixed the last of them -- the
/// `(format:...)` brace scanner that ended a body early at an unquoted
/// `}` -- so it now parses, and the entry is gone rather than kept as a
/// permanent carve-out.
pub const KNOWN_UNPARSEABLE: &[&str] = &[];

pub fn is_known_unparseable(rel: &Path) -> bool {
    let key = rel.to_string_lossy().replace('\\', "/");
    KNOWN_UNPARSEABLE.contains(&key.as_str())
}

/// Corpus entries that parse, as `(relative path, source)` pairs.
pub fn parseable_corpus() -> Vec<(PathBuf, String)> {
    corpus()
        .into_iter()
        .filter(|rel| !is_known_unparseable(rel))
        .map(|rel| {
            let src = read_fixture(&rel);
            (rel, src)
        })
        .collect()
}

// ---------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------

fn update_mode() -> bool {
    std::env::var_os("TOMET_UPDATE_REF").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Compares `actual` against the committed reference at `ref/<name>`.
///
/// With `TOMET_UPDATE_REF=1` the reference is rewritten instead of
/// compared. On mismatch the actual output is written to `store/<name>`
/// so the two files can be diffed directly.
pub fn assert_snapshot(name: &str, actual: &str) {
    let ref_path = ref_dir().join(name);

    if update_mode() {
        write_out(&ref_path, actual);
        return;
    }

    let expected = match std::fs::read_to_string(&ref_path) {
        Ok(text) => text,
        Err(_) => {
            let stored = store_dir().join(name);
            write_out(&stored, actual);
            panic!(
                "no reference output for {name}\n\
                 actual output written to {stored:?}\n\
                 if this is a new snapshot, create it with:\n\
                 \x20   TOMET_UPDATE_REF=1 cargo test -p tomet-tests"
            );
        }
    };

    if expected != actual {
        let stored = store_dir().join(name);
        write_out(&stored, actual);
        panic!(
            "snapshot mismatch for {name}\n\
             {}\n\
             reference: {ref_path:?}\n\
             actual:    {stored:?}\n\
             if the new output is correct, accept it with:\n\
             \x20   TOMET_UPDATE_REF=1 cargo test -p tomet-tests",
            first_difference(&expected, actual)
        );
    }
}

fn write_out(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("failed to create {parent:?}: {e}"));
    }
    std::fs::write(path, contents).unwrap_or_else(|e| panic!("failed to write {path:?}: {e}"));
}

/// A one-line summary of where two outputs first diverge. Enough to see
/// what changed without opening both files; the full text is on disk.
fn first_difference(expected: &str, actual: &str) -> String {
    let mut expected_lines = expected.lines();
    let mut actual_lines = actual.lines();
    let mut line_no = 1;
    loop {
        match (expected_lines.next(), actual_lines.next()) {
            (None, None) => {
                return "outputs differ only in trailing newlines".to_string();
            }
            (Some(e), Some(a)) if e == a => line_no += 1,
            (e, a) => {
                return format!(
                    "first difference at line {line_no}:\n  expected: {}\n  actual:   {}",
                    e.map_or("<end of file>".to_string(), |l| format!("{l:?}")),
                    a.map_or("<end of file>".to_string(), |l| format!("{l:?}")),
                );
            }
        }
    }
}

/// Turns `examples/image.meta.tmt` into `examples/image.meta.<ext>` for
/// naming the reference file of a converted fixture.
pub fn snapshot_name(rel: &Path, ext: &str) -> String {
    let stem = rel.with_extension("");
    format!("{}.{ext}", stem.to_string_lossy().replace('\\', "/"))
}

// ---------------------------------------------------------------------
// tree-sitter
// ---------------------------------------------------------------------

/// Parses with the tree-sitter grammar -- the second, hand-maintained
/// approximation of the language, kept in sync with `tomet-parser` by the
/// `corpus` target's drift tests.
pub fn ts_parse(src: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_tomet::LANGUAGE.into())
        .expect("failed to load the Tomet grammar");
    parser.parse(src, None).expect("parse returned None")
}

/// Collects the source text of every `ERROR`/`MISSING` node, so a
/// fixture-level assertion can compare against exactly the known,
/// documented cases rather than just counting them.
pub fn ts_error_texts(src: &str, tree: &tree_sitter::Tree) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            out.insert(
                node.utf8_text(src.as_bytes())
                    .unwrap_or_default()
                    .to_string(),
            );
        }
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
