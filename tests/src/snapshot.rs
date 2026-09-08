//! End-to-end conversion snapshots: every parseable fixture rendered
//! through each output format, compared against a committed reference.
//!
//! This is the coverage that did not exist anywhere before: each convert
//! crate tested its own units, but nothing checked `.tmt` all the way to
//! HTML/CommonMark/Typst as one pipeline.
//!
//! References are generated from current behavior, so they lock in
//! today's output including any bugs. They are a regression net, not a
//! statement that the output is correct -- when one changes, read the
//! diff and decide, then accept it with:
//!
//! ```text
//! TOMET_UPDATE_REF=1 cargo test -p tomet-tests
//! ```

use tomet_tests::{assert_snapshot, parseable_corpus, snapshot_name};

#[test]
fn html_output_matches_reference() {
    for (rel, src) in parseable_corpus() {
        let doc = tomet_parser::parse_document(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        // `render_body` rather than `render_page`: the page wrapper is
        // boilerplate, and pinning it would make every fixture's
        // reference churn on an unrelated template tweak.
        assert_snapshot(&snapshot_name(&rel, "html"), &tomet_html::render_body(&doc));
    }
}

#[test]
fn markdown_output_matches_reference() {
    for (rel, src) in parseable_corpus() {
        let doc = tomet_parser::parse_document(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        assert_snapshot(
            &snapshot_name(&rel, "md"),
            &tomet_markdown::to_markdown(&doc),
        );
    }
}

#[test]
fn typst_output_matches_reference() {
    for (rel, src) in parseable_corpus() {
        let doc = tomet_parser::parse_document(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        assert_snapshot(&snapshot_name(&rel, "typ"), &tomet_typst::to_typst(&doc));
    }
}

#[test]
fn printed_source_matches_reference() {
    // The printer rebuilds `.tmt` source from the AST. `roundtrip.rs`
    // checks that the result reparses to the same document; this pins
    // what it actually looks like, which reparsing alone does not.
    for (rel, src) in parseable_corpus() {
        let doc = tomet_parser::parse_document(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        assert_snapshot(
            &snapshot_name(&rel, "printed.tmt"),
            &tomet_printer::document_to_tm(&doc),
        );
    }
}

#[test]
fn markdown_import_export_is_stable() {
    // CommonMark -> Document -> CommonMark. Lossy in both directions by
    // design, so this does not assert a round trip -- it pins where the
    // losses currently land.
    for (rel, src) in parseable_corpus() {
        let doc = tomet_parser::parse_document(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        let exported = tomet_markdown::to_markdown(&doc);
        let reimported = tomet_markdown::from_markdown(&exported);
        assert_snapshot(
            &snapshot_name(&rel, "md.reimported.md"),
            &tomet_markdown::to_markdown(&reimported),
        );
    }
}

#[test]
fn pandoc_output_matches_reference() {
    // `.tmt` -> Pandoc's AST, JSON-encoded. This is what
    // `pandoc -f json` reads, so the reference is the wire format
    // itself -- a change here is a change to what Pandoc will accept.
    for (rel, src) in parseable_corpus() {
        let doc = tomet_parser::parse_document(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        let json = serde_json::to_string_pretty(&tomet_pandoc::to_pandoc(&doc))
            .expect("the Pandoc AST serializes");
        assert_snapshot(&snapshot_name(&rel, "pandoc.json"), &json);
    }
}

/// A sigil is a shape, not a name, and the Pandoc bridge must not turn
/// one into the other.
///
/// `Sigil::Bare` used to travel in the `tomet-<name>` class like every
/// named element, because `ElementKind::Bare::as_str()` is `"bare"`, and
/// came back as an element *named* `bare`. No vocabulary declares that
/// name, so the round-tripped document failed validation -- a conversion
/// producing something the language rejects. It travels under a reserved
/// key now.
#[test]
fn a_bare_entry_does_not_come_back_as_an_element_named_bare() {
    let src = "@deck.card{ title: t, (a)[ x ] }\n";
    let doc = tomet_parser::parse_document(src).unwrap();
    let back = tomet_pandoc::from_pandoc(&tomet_pandoc::to_pandoc(&doc));

    let mut names = Vec::new();
    tomet_tree::for_each_element(&back, |el| {
        if let Some(name) = el.sigil.name() {
            names.push(name.name.clone());
        }
    });
    assert!(
        !names.iter().any(|n| n == "bare"),
        "a sigil came back as a name: {names:?}"
    );
}

#[test]
fn pandoc_round_trip_is_stable() {
    // `.tmt` -> Pandoc -> `.tmt`. Lossy by design: Pandoc's `Attr` is
    // flat, so unless the exact copy was needed there is nothing left to
    // say which pair came from `(args)` rather than `{value}`, and
    // Pandoc metadata has no numeric type. This does not assert a round
    // trip -- it pins where the losses land, so a change in them shows up
    // as a diff rather than as a surprise.
    for (rel, src) in parseable_corpus() {
        let doc = tomet_parser::parse_document(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", rel.display()));
        let back = tomet_pandoc::from_pandoc(&tomet_pandoc::to_pandoc(&doc));
        assert_snapshot(
            &snapshot_name(&rel, "pandoc.roundtrip.tmt"),
            &tomet_printer::document_to_tm(&back),
        );
    }
}
