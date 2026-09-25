use super::*;

/// Pulls every `data-tmt-start`/`data-tmt-end` pair out of rendered HTML,
/// in document order, as `(start, end)` byte offsets.
fn extract_spans(html: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut rest = html;
    while let Some(idx) = rest.find("data-tmt-start=\"") {
        rest = &rest["data-tmt-start=\"".len() + idx..];
        let (start_str, after) = rest.split_once('"').unwrap();
        let start: usize = start_str.parse().unwrap();
        let after = after.strip_prefix(" data-tmt-end=\"").unwrap();
        let (end_str, after) = after.split_once('"').unwrap();
        let end: usize = end_str.parse().unwrap();
        spans.push((start, end));
        rest = after;
    }
    spans
}

#[test]
fn spans_are_absent_unless_opted_in() {
    let doc = parse_document("hello world\n").unwrap();
    let html = render_body(&doc);
    assert!(!html.contains("data-tmt-start"));
}

#[test]
fn a_paragraph_is_tagged_with_its_own_span() {
    let src = "hello world\n";
    let doc = parse_document(src).unwrap();
    let options = RenderOptions {
        emit_source_spans: true,
        ..RenderOptions::default()
    };
    let html = render_body_with(&doc, &options);
    let spans = extract_spans(&html);
    assert_eq!(spans.len(), 1);
    assert_eq!(&src[spans[0].0..spans[0].1], "hello world");
}

#[test]
fn a_heading_and_the_paragraph_after_it_each_get_their_own_span() {
    let src = "=[ Title ]\n\nbody text\n";
    let doc = parse_document(src).unwrap();
    let options = RenderOptions {
        emit_source_spans: true,
        ..RenderOptions::default()
    };
    let html = render_body_with(&doc, &options);
    let spans = extract_spans(&html);
    assert_eq!(
        spans.len(),
        2,
        "expected one span for the heading and one for the paragraph, got {html:?}"
    );
    // Each span reaches through its own trailing newline, up to (not
    // including) the blank line that separates it from the next block.
    assert_eq!(&src[spans[0].0..spans[0].1], "=[ Title ]\n");
    // The final block's span stops at its own content -- there's no
    // following block for it to reach a separating blank line toward.
    assert_eq!(&src[spans[1].0..spans[1].1], "body text");
}

#[test]
fn list_items_are_tagged_individually_and_the_list_itself_is_not() {
    let src = "- one\n- two\n";
    let doc = parse_document(src).unwrap();
    let options = RenderOptions {
        emit_source_spans: true,
        ..RenderOptions::default()
    };
    let html = render_body_with(&doc, &options);
    assert!(
        !html.starts_with("<ul data-tmt-start"),
        "the list wrapper itself must not carry a span: {html:?}"
    );
    let spans = extract_spans(&html);
    assert_eq!(spans.len(), 2);
    // Each item's span covers its whole source line, marker included --
    // exactly the raw text an editor would want to seed a textarea with.
    assert_eq!(&src[spans[0].0..spans[0].1], "- one\n");
    assert_eq!(&src[spans[1].0..spans[1].1], "- two\n");
}

#[test]
fn a_non_list_non_heading_block_is_tagged_as_one_opaque_leaf() {
    let src = "@quote[ said something ]\n";
    let doc = parse_document(src).unwrap();
    let options = RenderOptions {
        emit_source_spans: true,
        ..RenderOptions::default()
    };
    let html = render_body_with(&doc, &options);
    let spans = extract_spans(&html);
    assert_eq!(spans.len(), 1);
    assert_eq!(&src[spans[0].0..spans[0].1], src.trim_end());
}
