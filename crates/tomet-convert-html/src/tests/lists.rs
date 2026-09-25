use super::*;

#[test]
fn renders_list() {
    let doc = parse_document("- one\n- two\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n");
}

#[test]
fn renders_ordered_list_as_ol() {
    let doc = parse_document("-. one\n-. two\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "<ol>\n<li>one</li>\n<li>two</li>\n</ol>\n");
}

#[test]
fn renders_list_with_value_markers_and_attrs() {
    let doc =
        parse_document("- (T) in-progress {tag: dev}\n- (\"?\") question {id: task1}\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<ul>\n<li data-tag=\"dev\"><span class=\"tm-list-marker\" data-marker=\"T\">[T]</span> in-progress</li>\n<li id=\"task1\"><span class=\"tm-list-marker\" data-marker=\"?\">[?]</span> question</li>\n</ul>\n"
    );
}

/// `[...]` after a list marker is the item's content group, the same
/// group `@name[...]` takes -- not a checkbox, and not literal text.
/// Text following the group stays in the item, the way
/// `@x[T] content` keeps both halves in one paragraph.
#[test]
fn bracket_after_a_list_marker_is_the_content_group() {
    let doc = parse_document("- [T] content\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "<ul>\n<li>T content</li>\n</ul>\n");
}

#[test]
fn renders_key_value_list_marker_as_multiple_data_attrs() {
    let doc = parse_document("- (color: red, priority: high) content\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<ul>\n<li><span class=\"tm-list-marker\" data-color=\"red\" data-priority=\"high\"></span> content</li>\n</ul>\n"
    );
}

#[test]
fn renders_list_marker_with_embedded_element() {
    let doc =
        parse_document("- (@em[Important]) content\n- (@link(https://example.com)[Wiki]) docs\n")
            .unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<ul>\n<li><span class=\"tm-list-marker\"><em>Important</em></span> content</li>\n<li><span class=\"tm-list-marker\"><a class=\"tm-url\" href=\"https://example.com\">Wiki</a></span> docs</li>\n</ul>\n"
    );
}
