//! Invariants that span more than one crate: parser + formatter, and
//! parser + printer.
//!
//! Moved here from `tomet-formatter` and `tomet-config`, which
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
/// Currently empty. `examples/bookmark.tmt` was the sole entry: it uses
/// full-width braces (`｛ ｝`, U+FF5B/U+FF5D) where the grammar only
/// accepts ASCII `{ }`, so its `(content:raw)` never took effect and the
/// following `[ ` was not treated as verbatim content -- the
/// whitespace-hygiene pass then stripped a space and changed the content.
///
/// The `+++` fence removes that whole class of bug by construction. A
/// fence is delimited by a line, not by matched brackets, so no character
/// inside the body -- full-width brace, unquoted `}`, stray apostrophe --
/// can end it early or make the formatter disagree with the parser about
/// where verbatim content begins.
const KNOWN_FORMAT_CHANGES_DOCUMENT: &[&str] = &[];

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
    // `tomet-config`; the fixtures are local to this package now.
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

// ---------------------------------------------------------------------
// CST
// ---------------------------------------------------------------------

#[test]
fn cst_is_lossless_across_the_corpus() {
    // Runs over every fixture including the ones the parser rejects: the
    // CST parser never fails, and keeping every byte on malformed input is
    // what an editor relies on.
    for rel in corpus() {
        let src = read_fixture(&rel);
        let cst = tomet_parser::parse_cst(&src);
        assert_eq!(
            cst.text().to_string(),
            src,
            "CST lost bytes for {}",
            rel.display()
        );
    }
}

/// Levels of every section in `blocks`, in document order.
fn ast_section_levels(blocks: &[tomet_ast::Block], out: &mut Vec<usize>) {
    for block in blocks {
        if let tomet_ast::Block::Section(s) = block {
            out.push(s.level);
            ast_section_levels(&s.blocks, out);
        }
    }
}

#[test]
fn cst_sections_match_the_ast_across_the_corpus() {
    // The CST and the AST are two parsers over one grammar. If a section
    // form is added to one and not the other, the levels they report for
    // the same source disagree.
    for (rel, src) in parseable_corpus() {
        let doc = tomet_parser::parse_document(&src).expect("parseable corpus parses");
        let mut expected = Vec::new();
        ast_section_levels(&doc.blocks, &mut expected);

        let cst = tomet_parser::parse_cst(&src);
        let actual: Vec<usize> = cst
            .descendants()
            .filter(|n| n.kind() == tomet_cst::SyntaxKind::SECTION_HEADING)
            .map(|n| {
                n.children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .take_while(|t| t.kind() == tomet_cst::SyntaxKind::EQUAL)
                    .count()
            })
            .collect();
        assert_eq!(
            actual,
            expected,
            "CST and AST disagree on sections for {}",
            rel.display()
        );
    }
}
