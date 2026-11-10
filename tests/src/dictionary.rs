//! The dictionary of the ways tomet is written (`tomet-style`), held against the parser, the printer
//! and the formatter.
//!
//! An entry says that its spellings read back as the same `File`, and the printer and the formatter
//! both write them from it. The parser decides what is the same document, so these tests read the
//! parser's tree and the writers' text, and the dictionary has to agree.

use tomet_style::ContentLayout;

fn file(src: &str) -> tomet_ast::File {
    tomet_ast_parser::parse_file(src).unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

// ---- aligning the values of `key: value` lines ---------------------------------------------------

/// The key lists the alignment is tried on: short ones, one wider than the limit, wide letters, and
/// the punctuation a bare key may hold.
const KEY_LISTS: &[&[&str]] = &[
    &["id", "title", "kind"],
    &["a", "bb", "ccc", "dddd", "eeeee"],
    &["id", "a_field_that_is_far_too_long", "kind"],
    &["\"見出し\"", "id", "tags"],
    &["a-b", "a.b", "c"],
    &["only"],
];

/// How many spaces follow the `:` on each `key: value` line of `text`.
fn pads(text: &str) -> Vec<usize> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            // The colon after the key: a quoted key may hold one.
            let after = if let Some(rest) = line.strip_prefix('"') {
                let end = rest.find('"')?;
                &rest[end + 2..]
            } else {
                &line[line.find(':')? + 1..]
            };
            after
                .starts_with(' ')
                .then(|| after.len() - after.trim_start().len())
        })
        .collect()
}

fn aligned_config(max_key_width: usize) -> tomet_config::PrinterConfig {
    let mut config = tomet_config::PrinterConfig::default();
    config.format.map_align_values = true;
    config.format.map_max_key_width = Some(max_key_width);
    config.meta.field_layout = Some(tomet_style::GroupLayout::OnePerLine);
    config
}

#[test]
fn the_printer_and_the_formatter_pad_the_same_keys_alike() {
    // The printer aligns the fields of `::meta`, the formatter the `(...)` of everything else: two
    // procedures, one rule, so the same keys must be padded the same.
    for max in [4, 8, 20] {
        let config = aligned_config(max);
        for keys in KEY_LISTS {
            let body: String = keys.iter().map(|k| format!("  {k}: x\n")).collect();
            let printed = tomet_printer::document_to_tm_with_config(
                &file(&format!("::meta(\n{body})\n")),
                &config,
            );
            let formatted =
                tomet_formatter::format_source_with_config(&format!("::x(\n{body})\n"), &config);
            assert_eq!(
                pads(&printed),
                pads(&formatted),
                "keys {keys:?}, max {max}:\n{printed}\n{formatted}"
            );
            assert_eq!(pads(&formatted).len(), keys.len(), "{formatted}");
        }
    }
    // That both agree is no proof that either aligned: the first list is padded to its widest key.
    let config = aligned_config(20);
    let body = "  a: x\n  bb: x\n  ccc: x\n";
    let formatted =
        tomet_formatter::format_source_with_config(&format!("::x(\n{body})\n"), &config);
    assert_eq!(pads(&formatted), [3, 2, 1], "{formatted}");
}

// ---- setting paragraphs off from each other ------------------------------------------------------

#[test]
fn both_paragraph_separators_read_back_as_the_same_file() {
    use tomet_style::ParagraphSeparator;
    // The text the dictionary gives for each separator, between the line end of one paragraph and the
    // start of the next, at the top of a document and inside an element's content.
    let top = |sep: ParagraphSeparator| format!("a\n{}b\n", sep.after_line_end(""));
    let nested = |sep: ParagraphSeparator| format!("@x[\n  a\n{}b\n]\n", sep.after_line_end("  "));
    for make in [top, nested] {
        assert_eq!(
            file(&make(ParagraphSeparator::Blank)),
            file(&make(ParagraphSeparator::Semicolon)),
            "{:?} and {:?}",
            make(ParagraphSeparator::Blank),
            make(ParagraphSeparator::Semicolon)
        );
    }
}

// ---- laying out the content of an element or an item over several lines ---------------------------

#[test]
fn every_content_layout_reads_back_as_the_same_file() {
    let lines = ["one", "two"];
    for layout in ContentLayout::ALL {
        // After a callout the brackets stand on the next line; in a list item the content is padded
        // to the width of the marker.
        let callout = format!("@callout\n{}\n", layout.render(&lines, "  ", ""));
        assert_eq!(
            file(&callout),
            file("@callout[ one\ntwo ]\n"),
            "{layout:?}: {callout:?}"
        );
        let item = format!("- {}\n", layout.render(&lines, "    ", "  "));
        assert_eq!(
            file(&item),
            file("- [ one\ntwo ]\n"),
            "{layout:?}: {item:?}"
        );
    }
}

#[test]
fn the_formatter_lays_content_out_as_the_printer_does() {
    use tomet_style::ContentLayout;
    // Content that already runs over several lines, in a callout and in a list item, however it is
    // laid out now: the formatter's text must be what the printer writes for the same document.
    let sources = [
        "@callout(info)[ one\ntwo ]\n",
        "@callout(info)[\n  one\n  two\n]\n",
        "@callout(info)\n[ one\n  two ]\n",
        "- [ one\ntwo ]\n",
        "-. [ one\n     two\n     ]\n",
    ];
    for layout in ContentLayout::ALL {
        let mut config = tomet_config::PrinterConfig::default();
        config.format.callout_content_style = Some(layout);
        config.format.list_multiline_style_content = Some(layout);
        for src in sources {
            let printed = tomet_printer::document_to_tm_with_config(&file(src), &config);
            let formatted = tomet_formatter::format_source_with_config(src, &config);
            assert_eq!(formatted, printed, "{layout:?}: {src:?}");
            // And what it wrote is stable.
            assert_eq!(
                tomet_formatter::format_source_with_config(&formatted, &config),
                formatted,
                "{layout:?}: {src:?}"
            );
        }
    }
}

// ---- the spaces inside a heading's brackets and after the colon of a link's target -----------------

#[test]
fn the_formatter_spaces_a_heading_as_the_printer_does() {
    let sources = ["=[ Title ]\n", "==[Title]\n", "=[  Title  ](a: 1)\n"];
    for padded in [true, false] {
        let mut config = tomet_config::PrinterConfig::default();
        config.format.heading_brackets = Some(if padded {
            tomet_style::BracketSpacing::Padded
        } else {
            tomet_style::BracketSpacing::Tight
        });
        for src in sources {
            let printed = tomet_printer::document_to_tm_with_config(&file(src), &config);
            let formatted = tomet_formatter::format_source_with_config(src, &config);
            assert_eq!(formatted, printed, "padded {padded}: {src:?}");
            assert_eq!(
                tomet_formatter::format_source_with_config(&formatted, &config),
                formatted,
                "padded {padded}: {src:?}"
            );
        }
    }
}

#[test]
fn the_formatter_spaces_a_link_target_as_the_printer_does() {
    let sources = [
        "@link(target: \"https://x.y\")[T]\n",
        "@link(target:\"https://x.y\")[T]\n",
        "see @link(target:   \"https://x.y\")[T] here\n",
    ];
    for tight in [true, false] {
        let mut config = tomet_config::PrinterConfig::default();
        config.format.link_target_colon = Some(if tight {
            tomet_style::ColonSpacing::Tight
        } else {
            tomet_style::ColonSpacing::Spaced
        });
        for src in sources {
            let printed = tomet_printer::document_to_tm_with_config(&file(src), &config);
            let formatted = tomet_formatter::format_source_with_config(src, &config);
            assert_eq!(formatted, printed, "tight {tight}: {src:?}");
            assert_eq!(
                tomet_formatter::format_source_with_config(&formatted, &config),
                formatted,
                "tight {tight}: {src:?}"
            );
        }
    }
}

// ---- breaking the fields of `::meta` over lines ----------------------------------------------------

#[test]
fn the_formatter_lays_the_fields_of_meta_out_as_the_printer_does() {
    use tomet_style::GroupLayout;
    // Written one line or one to a line, aligned or not: the formatter's text is the printer's.
    let sources = [
        "::meta(a: 1, b: 2)\n",
        "::meta(title: Hello, id: x)\n",
        "::meta(only: 1)\n",
        "::meta(a: 1,   b: 2)\n",
        "::meta(\n  a: 1\n  b: 2\n)\n",
        "::meta(\n  title:  Hello\n  id: x\n)\n",
    ];
    for layout in [GroupLayout::OneLine, GroupLayout::OnePerLine] {
        for align in [false, true] {
            let mut config = tomet_config::PrinterConfig::default();
            config.meta.field_layout = Some(layout);
            config.format.map_align_values = align;
            for src in sources {
                let printed = tomet_printer::document_to_tm_with_config(&file(src), &config);
                let formatted = tomet_formatter::format_source_with_config(src, &config);
                assert_eq!(formatted, printed, "{layout:?}, align {align}: {src:?}");
                assert_eq!(
                    tomet_formatter::format_source_with_config(&formatted, &config),
                    formatted,
                    "{layout:?}, align {align}: {src:?}"
                );
            }
        }
    }
}
