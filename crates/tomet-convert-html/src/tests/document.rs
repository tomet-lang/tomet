use super::*;

#[test]
fn a_nested_map_survives_as_an_exact_copy() {
    // It used to render as `data-m=""` -- key kept, contents gone.
    let doc = parse_document("@deck.x(a: 1, m: { k: v })[ 本文 ]\n").unwrap();
    let html = render_body(&doc);
    assert!(html.contains(r#"data-a="1""#), "got {html}");
    assert!(
        !html.contains(r#"data-m="""#),
        "the empty attr is gone: {html}"
    );
    assert!(html.contains("data-tomet-data="), "got {html}");
    assert!(html.contains("&quot;m&quot;"), "got {html}");
}

#[test]
fn flat_args_need_no_exact_copy() {
    let doc = parse_document("@deck.x(a: 1, tags: list(p, q))[ 本文 ]\n").unwrap();
    let html = render_body(&doc);
    assert!(html.contains(r#"data-tags="p, q""#), "got {html}");
    assert!(!html.contains("data-tomet-data="), "got {html}");
}

#[test]
fn render_page_defaults_to_japanese_lang() {
    let doc = parse_document("=[ One ]\n").unwrap();
    let page = render_page(&doc, "Title");
    assert!(
        page.contains("<html lang=\"ja\">"),
        "expected default lang=\"ja\", got: {page}"
    );
}

#[test]
fn render_page_with_honors_lang_override() {
    let doc = parse_document("=[ One ]\n").unwrap();
    let page = render_page_with(
        &doc,
        "Title",
        &RenderOptions {
            lang: Some("en".to_string()),
            ..RenderOptions::default()
        },
    );
    assert!(
        page.contains("<html lang=\"en\">"),
        "expected lang=\"en\", got: {page}"
    );
}
