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
/// A backtick fence (the current way to carry a verbatim raw body, nested
/// in `[content]` or at the document's own top level) removes that whole
/// class of bug by construction. It is delimited by a line, not by
/// matched brackets, so no character inside the body -- full-width brace,
/// unquoted `}`, stray apostrophe -- can end it early or make the
/// formatter disagree with the parser about where verbatim content
/// begins.
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

        let before = tomet_ast_parser::parse_file(&src)
            .unwrap_or_else(|e| panic!("{key} failed to parse: {e}"));
        let formatted = tomet_formatter::format_source(&src);
        let after = tomet_ast_parser::parse_file(&formatted)
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
fn a_paragraph_separator_does_not_change_the_parsed_document() {
    // `paragraph.separator` rewrites the text between two paragraphs: a blank line and a `;` that
    // opens the next line are two spellings of one separator.
    use tomet_config::{FormatConfig, PrinterConfig};
    use tomet_style::ParagraphSeparator;

    let mut changed_by = [0usize; 2];
    for (rel, src) in parseable_corpus() {
        let before = tomet_ast_parser::parse_file(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        for (i, separator) in [ParagraphSeparator::Blank, ParagraphSeparator::Semicolon]
            .into_iter()
            .enumerate()
        {
            let config = PrinterConfig {
                format: FormatConfig {
                    paragraph_separator: Some(separator),
                    ..Default::default()
                },
                ..Default::default()
            };
            let once = tomet_formatter::format_source_with_config(&src, &config);
            let after = tomet_ast_parser::parse_file(&once).unwrap_or_else(|e| {
                panic!("{} failed to parse after {separator:?}: {e}", rel.display())
            });
            assert!(
                before == after,
                "{separator:?} changed the parsed document for {}",
                rel.display()
            );
            assert_eq!(
                tomet_formatter::format_source_with_config(&once, &config),
                once,
                "{separator:?} is not idempotent for {}",
                rel.display()
            );
            if once != tomet_formatter::format_source_with_config(&src, &PrinterConfig::default()) {
                changed_by[i] += 1;
            }
        }
    }
    // Both directions must really rewrite something in the corpus, or the check above is vacuous.
    assert!(
        changed_by[0] > 0,
        "no fixture has a `;` to turn into a blank line"
    );
    assert!(
        changed_by[1] > 0,
        "no fixture has a blank line to turn into a `;`"
    );
}

#[test]
fn aligning_values_does_not_change_the_parsed_document() {
    // `map.align_values` only moves the spaces after a `key:`; a key the parser reads as a string
    // (`見出し: 1`) has its spacing for its value, and must be left alone.
    use tomet_config::{FormatConfig, PrinterConfig};

    let config = PrinterConfig {
        format: FormatConfig {
            map_align_values: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut changed = 0;
    for (rel, src) in parseable_corpus() {
        let before = tomet_ast_parser::parse_file(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        let once = tomet_formatter::format_source_with_config(&src, &config);
        let after = tomet_ast_parser::parse_file(&once)
            .unwrap_or_else(|e| panic!("{} failed to parse after aligning: {e}", rel.display()));
        assert!(
            before == after,
            "aligning changed the parsed document for {}",
            rel.display()
        );
        assert_eq!(
            tomet_formatter::format_source_with_config(&once, &config),
            once,
            "aligning is not idempotent for {}",
            rel.display()
        );
        if once != tomet_formatter::format_source_with_config(&src, &PrinterConfig::default()) {
            changed += 1;
        }
    }
    assert!(changed > 0, "no fixture has a multi-line group to align");
}

#[test]
fn heading_blank_lines_do_not_change_the_parsed_document() {
    use tomet_config::{FormatConfig, PrinterConfig};

    for before_opt in [Some(true), Some(false)] {
        for after_opt in [Some(true), Some(false)] {
            let config = PrinterConfig {
                format: FormatConfig {
                    heading_blank_before: before_opt,
                    heading_blank_after: after_opt,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut changed = 0;
            for (rel, src) in parseable_corpus() {
                let before = tomet_ast_parser::parse_file(&src)
                    .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
                let once = tomet_formatter::format_source_with_config(&src, &config);
                let after = tomet_ast_parser::parse_file(&once).unwrap_or_else(|e| {
                    panic!(
                        "{} failed to parse after heading blank formatting ({before_opt:?}, {after_opt:?}): {e}",
                        rel.display()
                    )
                });
                assert!(
                    before == after,
                    "heading blank formatting ({before_opt:?}, {after_opt:?}) changed parsed document for {}",
                    rel.display()
                );
                assert_eq!(
                    tomet_formatter::format_source_with_config(&once, &config),
                    once,
                    "heading blank formatting ({before_opt:?}, {after_opt:?}) is not idempotent for {}",
                    rel.display()
                );
                if once != tomet_formatter::format_source_with_config(&src, &PrinterConfig::default()) {
                    changed += 1;
                }
            }
            assert!(
                changed > 0,
                "no fixture was rewritten by heading blank formatting ({before_opt:?}, {after_opt:?})"
            );
        }
    }
}

#[test]
fn declaration_blank_lines_do_not_change_the_parsed_document() {
    use tomet_config::{FormatConfig, PrinterConfig};

    for between_opt in [Some(true), Some(false)] {
        for after_opt in [Some(true), Some(false)] {
            let config = PrinterConfig {
                format: FormatConfig {
                    declaration_blank_between: between_opt,
                    declaration_blank_after: after_opt,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut changed = 0;
            for (rel, src) in parseable_corpus() {
                let before = tomet_ast_parser::parse_file(&src)
                    .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
                let once = tomet_formatter::format_source_with_config(&src, &config);
                let after = tomet_ast_parser::parse_file(&once).unwrap_or_else(|e| {
                    panic!(
                        "{} failed to parse after declaration blank formatting ({between_opt:?}, {after_opt:?}): {e}",
                        rel.display()
                    )
                });
                assert!(
                    before == after,
                    "declaration blank formatting ({between_opt:?}, {after_opt:?}) changed parsed document for {}",
                    rel.display()
                );
                assert_eq!(
                    tomet_formatter::format_source_with_config(&once, &config),
                    once,
                    "declaration blank formatting ({between_opt:?}, {after_opt:?}) is not idempotent for {}",
                    rel.display()
                );
                if once != tomet_formatter::format_source_with_config(&src, &PrinterConfig::default()) {
                    changed += 1;
                }
            }
            assert!(
                changed > 0,
                "no fixture was rewritten by declaration blank formatting ({between_opt:?}, {after_opt:?})"
            );
        }
    }
}

#[test]
fn laying_out_content_does_not_change_the_parsed_document() {
    // A layout only moves white space inside and around the brackets of content that already runs
    // over several lines, and what it writes is what it would write again.
    use tomet_config::PrinterConfig;
    use tomet_style::ContentLayout;

    let mut changed = 0;
    for layout in ContentLayout::ALL {
        let mut config = PrinterConfig::default();
        config.format.callout_content_style = Some(layout);
        config.format.list_multiline_style_content = Some(layout);
        for (rel, src) in parseable_corpus() {
            let before = tomet_ast_parser::parse_file(&src)
                .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
            let once = tomet_formatter::format_source_with_config(&src, &config);
            let after = tomet_ast_parser::parse_file(&once).unwrap_or_else(|e| {
                panic!("{} failed to parse after {layout:?}: {e}", rel.display())
            });
            assert!(
                before == after,
                "{layout:?} changed the parsed document for {}",
                rel.display()
            );
            assert_eq!(
                tomet_formatter::format_source_with_config(&once, &config),
                once,
                "{layout:?} is not idempotent for {}",
                rel.display()
            );
            if once != tomet_formatter::format_source_with_config(&src, &PrinterConfig::default()) {
                changed += 1;
            }
        }
    }
    assert!(changed > 0, "no fixture has multi-line content to lay out");
}

#[test]
fn spacing_a_heading_and_a_link_does_not_change_the_parsed_document() {
    // Only the spaces inside a heading's brackets and after the colon of a link's `target` move, and
    // what is written is what would be written again.
    use tomet_config::PrinterConfig;

    let mut changed = 0;
    for on in [true, false] {
        let mut config = PrinterConfig::default();
        config.format.heading_brackets = Some(if on {
            tomet_style::BracketSpacing::Padded
        } else {
            tomet_style::BracketSpacing::Tight
        });
        config.format.link_target_colon = Some(if on {
            tomet_style::ColonSpacing::Tight
        } else {
            tomet_style::ColonSpacing::Spaced
        });
        for (rel, src) in parseable_corpus() {
            let before = tomet_ast_parser::parse_file(&src)
                .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
            let once = tomet_formatter::format_source_with_config(&src, &config);
            let after = tomet_ast_parser::parse_file(&once).unwrap_or_else(|e| {
                panic!(
                    "{} failed to parse after spacing ({on}): {e}",
                    rel.display()
                )
            });
            assert!(
                before == after,
                "spacing ({on}) changed the parsed document for {}",
                rel.display()
            );
            assert_eq!(
                tomet_formatter::format_source_with_config(&once, &config),
                once,
                "spacing ({on}) is not idempotent for {}",
                rel.display()
            );
            if once != tomet_formatter::format_source_with_config(&src, &PrinterConfig::default()) {
                changed += 1;
            }
        }
    }
    assert!(changed > 0, "no fixture has a heading or a link to space");
}

#[test]
fn laying_out_meta_does_not_change_the_parsed_document() {
    // Only where the fields of `::meta` break and the spaces after their colons move, and what is
    // written is what would be written again.
    use tomet_config::PrinterConfig;
    use tomet_style::GroupLayout;

    let mut changed = 0;
    for layout in [GroupLayout::OneLine, GroupLayout::OnePerLine] {
        let mut config = PrinterConfig::default();
        config.meta.field_layout = Some(layout);
        config.format.map_align_values = true;
        for (rel, src) in parseable_corpus() {
            let before = tomet_ast_parser::parse_file(&src)
                .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
            let once = tomet_formatter::format_source_with_config(&src, &config);
            let after = tomet_ast_parser::parse_file(&once).unwrap_or_else(|e| {
                panic!("{} failed to parse after {layout:?}: {e}", rel.display())
            });
            assert!(
                before == after,
                "{layout:?} changed the parsed document for {}",
                rel.display()
            );
            assert_eq!(
                tomet_formatter::format_source_with_config(&once, &config),
                once,
                "{layout:?} is not idempotent for {}",
                rel.display()
            );
            if once != tomet_formatter::format_source_with_config(&src, &PrinterConfig::default()) {
                changed += 1;
            }
        }
    }
    assert!(changed > 0, "no fixture has a ::meta to lay out");
}

#[test]
fn printed_source_parses() {
    // parse -> print -> parse. This asserts only that `tomet-printer`
    // emits syntax `tomet-parser` accepts, not that the round trip is an
    // identity: the printer is a *styling* serializer and applies
    // `PrinterConfig` defaults, so e.g. a bare `::meta(...)` may come back
    // multi-line (`meta.always_newline`) -- a different AST shape for the
    // same meaning. What the printer actually emits is pinned by
    // the snapshot test instead.
    for (rel, src) in parseable_corpus() {
        let doc = tomet_ast_parser::parse_file(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        let printed = tomet_printer::document_to_tm(&doc);
        tomet_ast_parser::parse_file(&printed).unwrap_or_else(|e| {
            panic!(
                "{} failed to reparse after printing: {e}\n--- printed ---\n{printed}",
                rel.display()
            )
        });
    }
}

#[test]
fn config_fixtures_load() {
    // The fixtures are local to this package.
    let cfg = tomet_load_config::load_config_from_file(&fixtures_dir().join("test.config.tmt"))
        .expect("failed to load test.config.tmt");
    assert_eq!(
        cfg.meta.field_layout,
        Some(tomet_style::GroupLayout::OnePerLine)
    );
    assert!(cfg.meta.fields.get("aliases").unwrap().always_newline);
    assert_eq!(
        cfg.meta.fields.get("created").unwrap().format.as_deref(),
        Some("rfc3339")
    );
    assert_eq!(
        cfg.meta.fields.get("created").unwrap().offset.as_deref(),
        Some("+09:00")
    );
    assert_eq!(
        cfg.format.link_target_colon,
        Some(tomet_style::ColonSpacing::Tight)
    );
    assert_eq!(
        cfg.format.callout_content_style,
        Some(tomet_style::ContentLayout::Block)
    );
    assert_eq!(
        cfg.format.list_multiline_style_content,
        Some(tomet_style::ContentLayout::Box)
    );
    assert_eq!(
        cfg.workspace.ignore,
        vec!["00-09 System/01 Apps/obsidian".to_string()]
    );

    let default_cfg =
        tomet_load_config::load_config_from_file(&fixtures_dir().join("default.config.tmt"))
            .expect("failed to load default.config.tmt");
    assert_eq!(
        default_cfg.format.callout_content_style,
        Some(tomet_style::ContentLayout::Block)
    );
    assert_eq!(
        default_cfg.format.list_multiline_style_content,
        Some(tomet_style::ContentLayout::Box)
    );
}

#[test]
fn parses_the_repo_spec_examples() {
    // Kept as its own named test (moved from `tomet-syntax-parser`) so a
    // failure points at the two hand-picked documents that exercise the
    // widest slice of the grammar, rather than at the whole corpus.
    tomet_ast_parser::parse_file(&read_fixture(Path::new("readme.tmt"))).unwrap();
    tomet_ast_parser::parse_file(&read_fixture(Path::new("examples/image.meta.tmt"))).unwrap();
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
        let cst = tomet_cst_parser::parse_cst(&src);
        assert_eq!(
            cst.text().to_string(),
            src,
            "CST lost bytes for {}",
            rel.display()
        );
    }
}

#[test]
fn tomet_edit_is_lossless_across_the_corpus() {
    // Unedited import followed by export must be byte-identical across the corpus.
    for rel in corpus() {
        let src = read_fixture(&rel);
        let doc = tomet_edit::import_source(&src);
        let exported = tomet_edit::export_doc(&doc);
        assert_eq!(
            exported,
            src,
            "tomet-edit export lost bytes for {}",
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
        let doc = tomet_ast_parser::parse_file(&src).expect("parseable corpus parses");
        let mut expected = Vec::new();
        ast_section_levels(&doc.body, &mut expected);

        let cst = tomet_cst_parser::parse_cst(&src);
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

/// A hole printed by the printer reads back as the same hole, as a box.
///
/// The corpus above holds no `<?...>` yet, so these are written here.
#[test]
fn a_hole_survives_print_and_parse() {
    for src in [
        "adr-<?int(n)>-<?(slug)>.md",
        "<?()>",
        "<?(n)>",
        "<?>",
        "<?int>",
        "<?(id)>[adr-<?int(n)>-<?(slug)>]",
        "<?int(n, \"x\"){min: 1}>",
        "<?alt(doc, ([adr-<?int(n)>], [rfc-<?int(n)>]))>",
        "@link(url: <?(u)>)[<?(text)>]",
        "<?tandr.heading(h)>",
        "@meta(){ type: <?(kind)> }",
        "--out <PATH> and Vec<T>",
    ] {
        let before = tomet_ast_parser::parse_file(src)
            .unwrap_or_else(|e| panic!("`{src}` failed to parse: {e}"));
        let printed = tomet_printer::document_to_tm(&before);
        let after = tomet_ast_parser::parse_file(&printed).unwrap_or_else(|e| {
            panic!("`{src}` printed as `{printed}`, which fails to parse: {e}")
        });
        assert_eq!(
            before, after,
            "`{src}` printed as `{printed}` and read back differently"
        );
        if src.contains("<?") {
            assert!(
                printed.contains("<?"),
                "`{src}` printed as `{printed}`, which has lost its hole"
            );
        }
    }
}

/// The printer writes text that is clean as it is -- no trailing blanks, no run of blank lines,
/// one final newline -- so it needs no pass of `tomet-formatter` after it. Checked on the
/// documents of the corpus and on what comes back from Pandoc, whose AST has the shapes a
/// parser does not give (a list item that is a block element, an empty paragraph).
#[test]
fn the_printer_writes_text_the_formatter_leaves_as_it_is() {
    for (rel, src) in parseable_corpus() {
        let doc = tomet_ast_parser::parse_file(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        let from_pandoc = tomet_pandoc::from_pandoc(&tomet_pandoc::to_pandoc(&doc));
        for (what, doc) in [
            ("the document", &doc),
            ("its Pandoc round trip", &from_pandoc),
        ] {
            let printed = tomet_printer::document_to_tm(doc);
            assert_eq!(
                tomet_formatter::format_source(&printed),
                printed,
                "the printer's text for {what} of {} is not clean",
                rel.display()
            );
        }
    }
}
