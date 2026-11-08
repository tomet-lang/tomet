use tomet_ast::File;
use tomet_cst::{SyntaxKind, SyntaxNode, TextRange};
use tomet_span::Span;

/// Walks every node in `doc` and returns `(id, span)` for each
/// anchor ID found, in document order.
pub fn collect_ids(doc: &File) -> Vec<(String, Span)> {
    tomet_address::collect_anchors(doc)
}

/// Scans `root` for every `#(id)` slot and returns the exact [`TextRange`]
/// of each id's value, in document order.
///
/// Mirrors [`collect_ids`]. No owner-kind allowlist or `@link` exclusion
/// is needed the way the old `MAP_ENTRY`-scanning version needed both:
/// an `ID_GROUP` node can only ever appear where `parse_groups`/
/// `parse_connect` put one, so there is nothing else it could be mistaken
/// for, and no attribute-key collision with `@link`'s own `id` argument
/// to guard against (that argument is not this slot).
pub fn collect_ids_cst(root: &SyntaxNode) -> Vec<(String, TextRange)> {
    use SyntaxKind as K;

    root.descendants()
        .filter(|node| node.kind() == K::ID_GROUP)
        .filter_map(|group| {
            let value = group
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| {
                    !t.kind().is_trivia() && !matches!(t.kind(), K::HASH | K::L_PAREN | K::R_PAREN)
                })?;
            let text = value.text().trim_matches('"').to_string();
            Some((text, value.text_range()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_doc(src: &str) -> File {
        tomet_ast_parser::parse_file(src).unwrap()
    }

    #[test]
    fn collects_ids_in_section_data() {
        let doc = parse_doc("=[ Sec ]{ k: @note[x]#(in-sec-data) }\n\nBody text.\n");
        let ids = collect_ids(&doc);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].0, "in-sec-data");
    }

    #[test]
    fn detects_duplicate_ids_including_section_data() {
        let doc = parse_doc("=[ Sec ]{ k: @note[x]#(dup) }\n\n@other()#(dup)\n");
        let ids = collect_ids(&doc);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0].0, "dup");
        assert_eq!(ids[1].0, "dup");
    }
}
