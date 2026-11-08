//! In-memory anchor (`#(id)`) resolution and lookup across a [`File`].
//!
//! Searches for matching [`tomet_ast::Id`] definitions anywhere in a parsed AST using
//! [`tomet_tree`]. Pure in-memory (no I/O).

use std::ops::ControlFlow;
use tomet_ast::File;
use tomet_span::Span;
use tomet_tree::Visit;

/// Returns whether an anchor with the given ID exists anywhere in `file`.
pub fn has_anchor(file: &File, anchor: &str) -> bool {
    find_anchor(file, anchor).is_some()
}

/// Finds the [`Span`] of the node defining the given anchor ID in `file`, if any.
pub fn find_anchor(file: &File, anchor: &str) -> Option<Span> {
    match tomet_tree::walk(file, &mut |node, _| {
        if let (Some(id), Some(span)) = (node.id(), node.span())
            && id.0 == anchor
        {
            return Visit::Break(span);
        }
        Visit::Descend
    }) {
        ControlFlow::Break(span) => Some(span),
        ControlFlow::Continue(()) => None,
    }
}

/// Collects all anchor IDs defined in `file` along with their [`Span`]s, in document order.
pub fn collect_anchors(file: &File) -> Vec<(String, Span)> {
    let mut anchors = Vec::new();
    tomet_tree::for_each_node(file, |node| {
        if let (Some(id), Some(span)) = (node.id(), node.span()) {
            anchors.push((id.0.clone(), span));
        }
    });
    anchors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_doc(src: &str) -> File {
        tomet_ast_parser::parse_file(src).unwrap()
    }

    #[test]
    fn finds_declaration_anchor_in_header() {
        let doc = parse_doc("::meta(id: x)#(doc-meta)\n\nSome body text.\n");
        assert!(has_anchor(&doc, "doc-meta"));
        assert!(find_anchor(&doc, "doc-meta").is_some());
    }

    #[test]
    fn finds_section_heading_anchor() {
        let doc = parse_doc("=[ Introduction ] #(intro)\n\nSome body text.\n");
        assert!(has_anchor(&doc, "intro"));
        assert!(!has_anchor(&doc, "outro"));
        assert!(find_anchor(&doc, "intro").is_some());
    }

    #[test]
    fn finds_element_anchor() {
        let doc = parse_doc("@card() #(my-card)\n\nParagraph with @badge() #(my-badge) inside.\n");
        assert!(has_anchor(&doc, "my-card"));
        assert!(has_anchor(&doc, "my-badge"));
        assert!(!has_anchor(&doc, "other"));
    }

    #[test]
    fn finds_list_anchor() {
        let doc = parse_doc("- item 1 #(my-list)\n- item 2\n");
        assert!(has_anchor(&doc, "my-list"));
    }

    #[test]
    fn collects_all_anchors_in_document_order() {
        let doc = parse_doc(
            "=[ Sec ] #(sec)\n\n@card() #(c1)\n\nText with @tag() #(t1) in it.\n\n- list item #(l1)\n",
        );
        let ids: Vec<String> = collect_anchors(&doc)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids, vec!["sec", "c1", "t1", "l1"]);
    }

    #[test]
    fn finds_and_collects_anchor_in_section_data() {
        let doc = parse_doc("=[ Sec ]{ k: @note[x]#(in-sec-data) }\n\nBody text.\n");
        assert!(has_anchor(&doc, "in-sec-data"));
        let span = find_anchor(&doc, "in-sec-data");
        assert!(span.is_some());
        let anchors = collect_anchors(&doc);
        assert!(
            anchors
                .iter()
                .any(|(id, s)| id == "in-sec-data" && Some(*s) == span)
        );
    }
}
