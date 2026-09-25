use super::*;

#[test]
fn renders_heading_with_id_and_cssclass() {
    let doc = parse_document("=[ Hello ]{ id:header1, cssclass:card }\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "<h1 id=\"header1\" class=\"card\">Hello</h1>\n");
}

#[test]
fn nested_inline_heading_falls_back_to_generic_rendering() {
    // `@heading(2)[...]` outside block-top-level position (nested
    // inside a blockquote's content here) never renders as a real
    // An element nested inside another element's content falls to
    // `render_element`'s generic rendering, keyed on its kind name.
    // (This used to be demonstrated with a nested `@heading`; headings
    // are block-shaped now, so an inline one is not expressible.)
    let doc = parse_document("@quote[ @deck.badge(2)[Nested] ]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<blockquote><span class=\"tm-element tm-deck.badge\" data-value=\"2\">Nested</span></blockquote>\n"
    );
}

#[test]
fn default_options_leave_headings_unnumbered() {
    let doc = parse_document("=[ One ]\n").unwrap();
    assert_eq!(
        render_body_with(&doc, &RenderOptions::default()),
        render_body(&doc),
    );
    assert_eq!(render_body(&doc), "<h1>One</h1>\n");
}

#[test]
fn auto_slug_headings_generates_ids_from_text() {
    let doc = parse_document("=[ Hello World ]\n").unwrap();
    let body = render_body_with(
        &doc,
        &RenderOptions {
            auto_slug_headings: true,
            ..RenderOptions::default()
        },
    );
    assert_eq!(body, "<h1 id=\"hello-world\">Hello World</h1>\n");
}

#[test]
fn auto_slug_headings_keeps_non_ascii_letters() {
    let doc = parse_document("=[ 見出し テスト ]\n").unwrap();
    let body = render_body_with(
        &doc,
        &RenderOptions {
            auto_slug_headings: true,
            ..RenderOptions::default()
        },
    );
    assert_eq!(body, "<h1 id=\"見出し-テスト\">見出し テスト</h1>\n");
}

#[test]
fn auto_slug_headings_disambiguates_duplicates() {
    let doc = parse_document("=[ Intro ]\n==[ Intro ]\n==[ Intro ]\n").unwrap();
    let body = render_body_with(
        &doc,
        &RenderOptions {
            auto_slug_headings: true,
            ..RenderOptions::default()
        },
    );
    assert_eq!(
        body,
        "<h1 id=\"intro\">Intro</h1>\n\
             <h2 id=\"intro-2\">Intro</h2>\n\
             <h2 id=\"intro-3\">Intro</h2>\n"
    );
}

#[test]
fn auto_slug_headings_never_overrides_an_explicit_id() {
    let doc = parse_document("=[ Hello World ]{ id:custom }\n").unwrap();
    let body = render_body_with(
        &doc,
        &RenderOptions {
            auto_slug_headings: true,
            ..RenderOptions::default()
        },
    );
    assert_eq!(body, "<h1 id=\"custom\">Hello World</h1>\n");
}

#[test]
fn numbers_headings_by_nesting_level() {
    let doc = parse_document(
        "=[ One ]\n==[ One One ]\n==[ One Two ]\n=[ Two ]\n==[ Two One ]\n===[ Two One One ]\n",
    )
    .unwrap();
    let body = render_body_with(
        &doc,
        &RenderOptions {
            number_headings: true,
            ..RenderOptions::default()
        },
    );
    assert_eq!(
        body,
        "<h1><span class=\"tm-heading-number\">1</span>One</h1>\n\
             <h2><span class=\"tm-heading-number\">1.1</span>One One</h2>\n\
             <h2><span class=\"tm-heading-number\">1.2</span>One Two</h2>\n\
             <h1><span class=\"tm-heading-number\">2</span>Two</h1>\n\
             <h2><span class=\"tm-heading-number\">2.1</span>Two One</h2>\n\
             <h3><span class=\"tm-heading-number\">2.1.1</span>Two One One</h3>\n"
    );
}

fn outline_of(src: &str, options: &RenderOptions) -> Vec<HeadingInfo> {
    let doc = parse_document(src).unwrap();
    render_body_with_outline(&doc, options).1
}

#[test]
fn the_outline_reports_level_text_and_order() {
    let outline = outline_of(
        "=[ First ]\n\n==[ Nested ]\n\n=[ Second ]\n",
        &RenderOptions::default(),
    );

    let seen: Vec<(u8, &str)> = outline.iter().map(|h| (h.level, h.text.as_str())).collect();
    assert_eq!(seen, vec![(1, "First"), (2, "Nested"), (1, "Second")]);
    assert!(outline.iter().all(|h| h.number.is_none()));
    assert!(outline.iter().all(|h| h.id.is_none()));
}

#[test]
fn the_outline_carries_the_id_the_heading_was_rendered_with() {
    let explicit = outline_of("=[ Hello ]{ id:header1 }\n", &RenderOptions::default());
    assert_eq!(explicit[0].id.as_deref(), Some("header1"));

    let generated = outline_of(
        "=[ Hello World ]\n",
        &RenderOptions {
            auto_slug_headings: true,
            ..RenderOptions::default()
        },
    );
    assert_eq!(generated[0].id.as_deref(), Some("hello-world"));
}

#[test]
fn duplicate_slugs_are_reported_as_rendered() {
    let outline = outline_of(
        "=[ Same ]\n\n=[ Same ]\n",
        &RenderOptions {
            auto_slug_headings: true,
            ..RenderOptions::default()
        },
    );
    let ids: Vec<&str> = outline.iter().filter_map(|h| h.id.as_deref()).collect();
    assert_eq!(ids, vec!["same", "same-2"]);
}

#[test]
fn numbering_is_reported_separately_from_the_text() {
    let outline = outline_of(
        "=[ One ]\n\n==[ One A ]\n\n==[ One B ]\n\n=[ Two ]\n",
        &RenderOptions {
            number_headings: true,
            ..RenderOptions::default()
        },
    );

    let numbers: Vec<&str> = outline.iter().filter_map(|h| h.number.as_deref()).collect();
    assert_eq!(numbers, vec!["1", "1.1", "1.2", "2"]);
    // The label lives in `number`; `text` stays the author's words.
    assert_eq!(outline[1].text, "One A");
}

#[test]
fn inline_markup_is_flattened_in_the_outline_text() {
    let outline = outline_of("=[ **bold** and plain ]\n", &RenderOptions::default());
    assert_eq!(outline[0].text, "bold and plain");
}

#[test]
fn a_document_without_headings_has_an_empty_outline() {
    assert!(outline_of("just a paragraph\n", &RenderOptions::default()).is_empty());
}

#[test]
fn the_outline_does_not_change_the_html() {
    // render_body_with is now a thin wrapper; guard against it drifting.
    let doc = parse_document("=[ One ]\n\n==[ Two ]\n\ntext\n").unwrap();
    let options = RenderOptions {
        number_headings: true,
        auto_slug_headings: true,
        ..RenderOptions::default()
    };
    let (html, _) = render_body_with_outline(&doc, &options);
    assert_eq!(html, render_body_with(&doc, &options));
}

#[test]
fn only_block_level_headings_reach_the_outline() {
    // A heading nested in another element renders generically, so it is
    // not a heading in the output and must not appear in the outline.
    let outline = outline_of(
        "@quote[ @deck.badge(2)[Nested] ]\n",
        &RenderOptions::default(),
    );
    assert!(outline.is_empty());
}
