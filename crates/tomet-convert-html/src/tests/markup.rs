use super::*;

#[test]
fn escapes_paragraph_text() {
    let doc = parse_document("a < b & c\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "<p>a &lt; b &amp; c</p>\n");
}

#[test]
fn renders_thematic_break_as_hr() {
    let doc = parse_document("---\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "<hr>\n");
}

#[test]
fn renders_titled_thematic_break() {
    let doc = parse_document("---[ Title ]---\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<div class=\"tm-hr-titled\"><hr><span>Title</span><hr></div>\n"
    );
}

#[test]
fn renders_emphasis_strong_and_mark() {
    let doc = parse_document("a *em* b **strong** c @mark[mark]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<p>a <em>em</em> b <strong>strong</strong> c <mark>mark</mark></p>\n"
    );
}

#[test]
fn renders_ruby() {
    let doc = parse_document("a @ruby[漢字](rt:\"かんじ\") b\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "<p>a <ruby>漢字<rt>かんじ</rt></ruby> b</p>\n");
}

#[test]
fn renders_table_element_to_html() {
    let src = "@table()[\n[ title ][  sdfasdf   ][    fasdf    ][ sdffdsf ]\n[ title ][ sdfddfasdf ][ fasddfdfdff ][ sdffdsf ]\n[ title ][  sdfasdf   ][   fasdf     ][ sdffdsf ]\n]{}\n";
    let doc = parse_document(src).unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<table class=\"tm-element tm-table\">\n\
<thead>\n\
<tr>\n\
<th>title</th>\n\
<th>sdfasdf</th>\n\
<th>fasdf</th>\n\
<th>sdffdsf</th>\n\
</tr>\n\
</thead>\n\
<tbody>\n\
<tr>\n\
<td>title</td>\n\
<td>sdfddfasdf</td>\n\
<td>fasddfdfdff</td>\n\
<td>sdffdsf</td>\n\
</tr>\n\
<tr>\n\
<td>title</td>\n\
<td>sdfasdf</td>\n\
<td>fasdf</td>\n\
<td>sdffdsf</td>\n\
</tr>\n\
</tbody>\n\
</table>\n"
    );
}

#[test]
fn test_renders_inline_footnote() {
    let src = "Prose with @footnote[a short note] here.\n";
    let doc = parse_document(src).unwrap();
    let html = render_body(&doc);

    assert!(
        html.contains(
            "<sup><a href=\"#fn-1\" id=\"fnref-1-1\" class=\"footnote-ref\">[1]</a></sup>"
        ),
        "expected footnote link in body: {html}"
    );
    assert!(
        html.contains("<section role=\"doc-endnotes\" class=\"footnotes\">"),
        "expected footnotes section: {html}"
    );
    assert!(
            html.contains("<li id=\"fn-1\">\n<p>a short note <a href=\"#fnref-1-1\" role=\"doc-backlink\" class=\"footnote-backref\">↩</a></p>\n</li>"),
            "expected footnote item with backlink: {html}"
        );
}

#[test]
fn test_renders_separated_footnote_with_multiple_references() {
    let src = "Prose A^(note1) and Prose B^footnote(note1).\n\n@footnote(note1)[Shared footnote explanation.]\n";
    let doc = parse_document(src).unwrap();
    let html = render_body(&doc);

    assert!(
        html.contains(
            "Prose A<sup><a href=\"#fn-1\" id=\"fnref-1-1\" class=\"footnote-ref\">[1]</a></sup>"
        ),
        "expected first footnote ref: {html}"
    );
    assert!(
        html.contains(
            "Prose B<sup><a href=\"#fn-1\" id=\"fnref-1-2\" class=\"footnote-ref\">[1]</a></sup>"
        ),
        "expected second footnote ref: {html}"
    );
    assert!(
            html.contains("<li id=\"fn-1\">\n<p>Shared footnote explanation. <span class=\"footnote-backrefs\"><a href=\"#fnref-1-1\" role=\"doc-backlink\" class=\"footnote-backref\">^1</a> <a href=\"#fnref-1-2\" role=\"doc-backlink\" class=\"footnote-backref\">^2</a></span></p>\n</li>"),
            "expected shared footnote item with multiple backlinks: {html}"
        );
}
