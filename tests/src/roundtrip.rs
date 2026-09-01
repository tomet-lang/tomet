//! Invariants that span more than one crate: parser + formatter, and
//! parser + printer.
//!
//! Moved here from `tomet-formatter` and `tomet-workspace-config`, which
//! were reaching up to the repo root for the shared corpus.

use std::path::Path;
use tomet_tests::{corpus, fixtures_dir, parseable_corpus, read_fixture};

#[test]
fn formatting_is_idempotent_across_the_corpus() {
    // Runs over every fixture including the ones the parser rejects:
    // `format_source` is a whitespace-hygiene pass that does not require
    // a successful parse, and staying stable on malformed input is part
    // of what it promises.
    for rel in corpus() {
        let src = read_fixture(&rel);
        let once = tomet_formatter::format_source(&src);
        let twice = tomet_formatter::format_source(&once);
        assert_eq!(
            once,
            twice,
            "formatting is not idempotent for {}",
            rel.display()
        );
    }
}

/// Fixtures where `format_source` is currently known to change the parsed
/// document, violating `tomet-formatter`'s stated invariant.
///
/// `examples/bookmark.tmt` uses full-width braces (`｛ ｝`, U+FF5B/U+FF5D)
/// where the grammar only accepts ASCII `{ }`, so its `(content:raw)`
/// never takes effect and the following `[ ` (trailing space, newline)
/// is not treated as verbatim content. The whitespace-hygiene pass then
/// strips that space, which changes the content. Minimal repro:
///
/// ```text
/// <x>(content:raw)｛ id:1 ｝
/// [
///   a
/// ]
/// ```
///
/// With ASCII braces the same input is preserved correctly. Whether the
/// fix belongs in the formatter (hold the invariant even for input that
/// did not parse the way it looks) or in the fixture (a full-width brace
/// typo) is an open question -- see `.agents/tasks/root-test-crate.md`.
const KNOWN_FORMAT_CHANGES_DOCUMENT: &[&str] = &["examples/bookmark.tmt"];

#[test]
fn formatting_does_not_change_the_parsed_document() {
    // `tomet-formatter`'s core guarantee: the whitespace-hygiene pass
    // rewrites bytes but never the meaning.
    let mut changed = Vec::new();
    let mut unexpectedly_clean = Vec::new();

    for (rel, src) in parseable_corpus() {
        let key = rel.to_string_lossy().replace('\\', "/");
        let known = KNOWN_FORMAT_CHANGES_DOCUMENT.contains(&key.as_str());

        let before = tomet_parser::parse_document(&src)
            .unwrap_or_else(|e| panic!("{key} failed to parse: {e}"));
        let formatted = tomet_formatter::format_source(&src);
        let after = tomet_parser::parse_document(&formatted)
            .unwrap_or_else(|e| panic!("{key} failed to parse after formatting: {e}"));

        match (before == after, known) {
            (false, false) => changed.push(key),
            (true, true) => unexpectedly_clean.push(key),
            _ => {}
        }
    }

    assert!(
        changed.is_empty(),
        "formatting changed the parsed document for:\n  {}",
        changed.join("\n  ")
    );
    assert!(
        unexpectedly_clean.is_empty(),
        "these are listed in KNOWN_FORMAT_CHANGES_DOCUMENT but now survive \
         formatting unchanged.\nThat is good news -- remove them from the list:\n  {}",
        unexpectedly_clean.join("\n  ")
    );
}

#[test]
fn printed_source_parses() {
    // parse -> print -> parse. This asserts only that `tomet-printer`
    // emits syntax `tomet-parser` accepts, not that the round trip is an
    // identity: the printer is a *styling* serializer and applies
    // `PrinterConfig` defaults, so e.g. a bare `@meta{...}` comes back
    // as `@meta(format:yaml){...}` -- a different AST for the same
    // meaning. What the printer actually emits is pinned by
    // `snapshot.rs`'s `printed_source_matches_reference` instead.
    for (rel, src) in parseable_corpus() {
        let doc = tomet_parser::parse_document(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        let printed = tomet_printer::document_to_tm(&doc);
        tomet_parser::parse_document(&printed).unwrap_or_else(|e| {
            panic!(
                "{} failed to reparse after printing: {e}\n--- printed ---\n{printed}",
                rel.display()
            )
        });
    }
}

#[test]
fn config_fixtures_load() {
    // Was a `.parent().unwrap()` walk up to the repo root inside
    // `tomet-workspace-config`; the fixtures are local to this package now.
    let cfg = tomet_config::load_config_from_file(&fixtures_dir().join("test.config.tmt"))
        .expect("failed to load test.config.tmt");
    assert_eq!(cfg.meta_format.as_deref(), Some("yaml"));
    assert!(cfg.meta_always_newline);
    assert!(cfg.meta_fields.get("aliases").unwrap().always_newline);
    assert_eq!(
        cfg.meta_fields.get("created").unwrap().format.as_deref(),
        Some("rfc3339")
    );
    assert_eq!(
        cfg.meta_fields.get("created").unwrap().offset.as_deref(),
        Some("+09:00")
    );
    assert!(cfg.link_no_space);
    assert_eq!(cfg.callout_content_style.as_deref(), Some("block"));
    assert_eq!(cfg.list_multiline_style_content.as_deref(), Some("box"));
    assert_eq!(
        cfg.ignore_files,
        vec!["00-09 System/01 Apps/obsidian".to_string()]
    );

    let default_cfg =
        tomet_config::load_config_from_file(&fixtures_dir().join("default.config.tmt"))
            .expect("failed to load default.config.tmt");
    assert_eq!(default_cfg.callout_content_style.as_deref(), Some("block"));
    assert_eq!(
        default_cfg.list_multiline_style_content.as_deref(),
        Some("box")
    );
}

#[test]
fn parses_the_repo_spec_examples() {
    // Kept as its own named test (moved from `tomet-syntax-parser`) so a
    // failure points at the two hand-picked documents that exercise the
    // widest slice of the grammar, rather than at the whole corpus.
    tomet_parser::parse_document(&read_fixture(Path::new("readme.tmt"))).unwrap();
    tomet_parser::parse_document(&read_fixture(Path::new("examples/image.meta.tmt"))).unwrap();
}
