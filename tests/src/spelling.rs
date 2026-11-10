//! What the writers spell, held against what the parser reads.
//!
//! A key or a string value is written once by the printer (`tomet-printer`), once by cotomet's
//! serde writer and once by `tomet-edit`, and read back by one parser. These tests take text the
//! parser reads, write it with each writer and read the result again: a writer that leaves
//! quotes off where the parser needs them (or the reverse) shows up as a different document.
//!
//! The keys are every run of up to [`MAX_PARTS`] pieces of [`PARTS`], so a rule the parser has
//! about where a `.`, a digit or a keyword may stand is met by construction and not by a list
//! someone thought of.

use std::collections::BTreeMap;
use tomet_ast::File;

/// Pieces a key is built from: the characters a name has a rule about, a letter outside ASCII, and
/// the words that are values on their own.
const PARTS: &[&str] = &["a", "_", "-", ".", "1", "t", "é", "true", "null"];

/// Longest run of [`PARTS`] tried as a key.
const MAX_PARTS: usize = 4;

/// Strings a value may hold, each one a way a bare scalar could be read as something else: a
/// number, a keyword, a date, an expression, a delimiter, or a character the text cannot carry raw.
const SCALARS: &[&str] = &[
    "",
    " ",
    "a",
    " a",
    "a ",
    "a b",
    "-",
    "-a",
    "&a",
    "*a",
    "!a",
    "%a",
    "@a",
    "`a",
    "|a",
    ">a",
    "?a",
    "~",
    "~a",
    "true",
    "false",
    "null",
    "yes",
    "1",
    "-1",
    "+1",
    ".5",
    "5.",
    "1.5",
    "1e3",
    "1_000",
    "0x1",
    "NaN",
    "inf",
    "2024-01-02",
    "2024-01-02T03:04:05Z",
    "a$b",
    "a\nb",
    "a\tb",
    "a\"b",
    "a\\b",
    "a: b",
    "a:b",
    "a, b",
    "a,b",
    "a # b",
    "a#b",
    "// a",
    "a //b",
    "(a)",
    "a)b",
    "a(b",
    "[a]",
    "{a}",
    "<a>",
    "'a'",
    "=a",
    "::a",
    "#a",
    ";a",
    "@a(b)",
    "日本語",
    "é",
    "$5",
    "2024-01-02 03:04",
    "2024-01-02T03:04:05+09:00",
];

/// Holds `through` over [`SCALARS`]: each string reads back.
fn assert_scalars(what: &str, through: impl Fn(&str) -> Result<(), String>) {
    let bad: Vec<String> = SCALARS
        .iter()
        .filter_map(|s| through(s).err().map(|why| format!("string {s:?}: {why}")))
        .collect();
    assert_none(what, &bad);
}

/// Pieces of the longer runs: only what a token boundary can turn on, so a run can be long.
const DEEP_PARTS: &[&str] = &["a", "e", "_", "-", ".", "1"];

/// Longest run of [`DEEP_PARTS`] tried as a key.
const DEEP_MAX_PARTS: usize = 6;

/// Every run of one up to `max` pieces of `parts`, and the empty key.
fn runs(parts: &[&str], max: usize) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut layer = vec![String::new()];
    for _ in 0..max {
        layer = layer
            .iter()
            .flat_map(|head| parts.iter().map(move |part| format!("{head}{part}")))
            .collect();
        all.extend(layer.iter().cloned());
    }
    all
}

/// Every key tried: the short runs of [`PARTS`] and the long runs of [`DEEP_PARTS`].
fn keys() -> Vec<String> {
    let mut all = runs(PARTS, MAX_PARTS);
    all.extend(runs(DEEP_PARTS, DEEP_MAX_PARTS));
    all.sort();
    all.dedup();
    all
}

/// `s` as a quoted string literal, which the parser reads as exactly `s`. A carriage return goes in
/// as it is: the parser has no escape for it.
fn quoted(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Fails with the first `LIMIT` of `bad`, so a broken rule reads as a list and not as one case.
fn assert_none(what: &str, bad: &[String]) {
    const LIMIT: usize = 40;
    assert!(
        bad.is_empty(),
        "{what}: {} case(s) did not read back as written, the first {}:\n  {}",
        bad.len(),
        LIMIT.min(bad.len()),
        bad.iter()
            .take(LIMIT)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// Parses `src`, prints it, parses the print; `Err` says which of the three failed or differed.
fn through_printer(src: &str) -> Result<(), String> {
    let before: File = tomet_ast_parser::parse_file(src).map_err(|e| format!("source: {e}"))?;
    let printed = tomet_printer::document_to_tm(&before);
    let after = tomet_ast_parser::parse_file(&printed)
        .map_err(|e| format!("printed {printed:?} does not parse: {e}"))?;
    if before == after {
        Ok(())
    } else {
        Err(format!("printed {printed:?} reads as another document"))
    }
}

#[test]
fn the_enumeration_covers_what_it_claims() {
    // A change to the pieces or the length shows here, not as a quietly smaller net.
    assert_eq!(runs(PARTS, MAX_PARTS).len(), 1 + 9 + 81 + 729 + 6561);
    assert_eq!(
        runs(DEEP_PARTS, DEEP_MAX_PARTS).len(),
        1 + 6 + 36 + 216 + 1296 + 7776 + 46656
    );
    assert!(keys().len() > 60_000);
}

#[test]
fn a_key_is_bare_exactly_when_the_parser_reads_it_bare() {
    // Both ways: a bare key the parser reads as another would corrupt the document, and a quoted
    // one it would read bare is quoting for nothing. Keys with a non-ASCII letter are always
    // quoted whatever the parser takes of them; see the crate doc.
    let (mut wrong, mut needless) = (Vec::new(), Vec::new());
    for key in keys().into_iter().filter(|k| k.is_ascii()) {
        let quoted_src = format!("::x({}: 1)\n", quoted(&key));
        let Ok(expected) = tomet_ast_parser::parse_file(&quoted_src) else {
            continue;
        };
        let reads_bare = matches!(
            tomet_ast_parser::parse_file(&format!("::x({key}: 1)\n")),
            Ok(file) if file == expected
        );
        match (tomet_spelling::is_bare_key(&key), reads_bare) {
            (true, false) => wrong.push(format!("bare {key:?} is read as another document")),
            (false, true) => needless.push(format!("quoted {key:?} is read bare")),
            _ => {}
        }
    }
    assert_none("is_bare_key, written bare", &wrong);
    assert_none("is_bare_key, quoted for nothing", &needless);
}

#[test]
fn the_printer_writes_every_key_so_it_reads_back() {
    let mut bad = Vec::new();
    for key in keys() {
        let src = format!("::x({}: 1)\n", quoted(&key));
        // A quoted key the parser itself refuses is not the printer's to write.
        if tomet_ast_parser::parse_file(&src).is_err() {
            continue;
        }
        if let Err(why) = through_printer(&src) {
            bad.push(format!("key {key:?}: {why}"));
        }
    }
    assert_none("tomet-printer, map key", &bad);
}

#[test]
fn the_printer_writes_every_string_so_it_reads_back() {
    assert_scalars("tomet-printer, string value", |s| {
        through_printer(&format!("::x(v: {})\n", quoted(s)))
    });
}

#[test]
fn cotomet_writes_every_key_so_it_reads_back() {
    let mut bad = Vec::new();
    for key in keys() {
        let src = format!("{}: \"x\"\n", quoted(&key));
        let Ok(map) = cotomet::from_str::<BTreeMap<String, String>>(&src) else {
            continue;
        };
        let written = match cotomet::to_string(&map) {
            Ok(text) => text,
            Err(e) => {
                bad.push(format!("key {key:?}: not written: {e}"));
                continue;
            }
        };
        match cotomet::from_str::<BTreeMap<String, String>>(&written) {
            Ok(again) if again == map => {}
            Ok(_) => bad.push(format!(
                "key {key:?}: written {written:?} reads as another map"
            )),
            Err(e) => bad.push(format!(
                "key {key:?}: written {written:?} does not parse: {e}"
            )),
        }
    }
    assert_none("cotomet, map key", &bad);
}

#[test]
fn cotomet_writes_every_string_so_it_reads_back() {
    assert_scalars("cotomet, string value", |s| {
        let map = BTreeMap::from([("v".to_string(), s.to_string())]);
        let written = cotomet::to_string(&map).map_err(|e| format!("not written: {e}"))?;
        match cotomet::from_str::<BTreeMap<String, String>>(&written) {
            Ok(again) if again == map => Ok(()),
            Ok(_) => Err(format!("written {written:?} reads as another value")),
            Err(e) => Err(format!("written {written:?} does not parse: {e}")),
        }
    });
}
