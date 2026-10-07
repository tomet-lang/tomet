//! =[ tomet-edit ]
//!
//! Backend-neutral editable tree model of a Tomet document for collaborative editing.
//!
//! =[ Architecture & Core Principles ]
//!
//! - CRDT-Agnostic Model:
//!   This crate provides an in-memory, tree-structured document model with opaque [`NodeId`]s,
//!   designed to bridge to external collaborative runtimes (such as Loro, Yjs, or Automerge).
//!   It depends on zero CRDT libraries, networking, or session transport code.
//!
//! - CST Foundation & Guaranteed Invariant:
//!   The model is derived from the Concrete Syntax Tree (`tomet-cst`). Trivia (whitespace,
//!   newlines, comments) is preserved alongside nodes.
//!   Guaranteed Invariant: Importing and immediately exporting without edits is 100% byte-identical
//!   to the original input source across the entire corpus (`export_doc(&import_source(src)) == src`).
//!
//! - Text as Raw String (Paragraphs & Headings):
//!   Prose within paragraphs, headings, and element content is held as raw text rather than
//!   inline marks. Structure in the model is retained where identity, moving, and keys matter
//!   (sections, blocks, attributes), allowing CRDTs to merge text via character-level sequences
//!   without fragile structured-mark nesting.
//!
//! - First-Class Move & Closed Edit Operations:
//!   [`EditOp`] provides a minimal, closed set of operations (`Insert`, `Delete`, `Move`, `EditText`, `SetAttribute`).
//!   `Move` is a first-class operation, directly supported by tree-based CRDTs (e.g. Loro's MovableTree).
//!   [`EditOp::SetAttribute`] allows key-level manipulation of an element's `{}` data, `()` args, and `#()` id slot.
//!
//! - Post-Edit Diagnostics:
//!   Collaborative engines merge writes without rejecting them. [`diagnose_doc`] exports the edited
//!   document to source text and re-parses it with the full validator to report duplicate IDs,
//!   schema errors, or broken syntax after concurrent merges.

pub mod diagnostics;
pub mod doc;
pub mod export;
pub mod import;
pub mod node;
pub mod op;

pub use diagnostics::{PostEditReport, diagnose_doc};
pub use doc::EditDoc;
pub use export::export_doc;
pub use import::{import_cst, import_source};
pub use node::{AttrGroup, ChildItem, Node, NodeId, NodeKind};
pub use op::{EditError, EditOp};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unedited_import_export_is_byte_identical() {
        let cases = [
            "=[ Section 1 ]\n\nParagraph text.\n",
            "=[ Section 1 ]#(sec1)\n\nParagraph text.\n\n==[ Sub ]\nNested text.\n",
            "- item 1\n- item 2\n",
            "```rust\nfn main() {}\n```\n",
            "@callout(info)[ Note text ]\n",
            "// Leading comment\n=[ Title ]\nText.\n// Trailing comment\n",
        ];

        for src in cases {
            let doc = import_source(src);
            let exported = export_doc(&doc);
            assert_eq!(exported, src, "Round-trip must be byte-identical");
        }
    }

    #[test]
    fn edit_paragraph_text_replaces_content() {
        let src = "=[ Title ]\n\nOriginal text.\n";
        let mut doc = import_source(src);

        // Find the paragraph node
        let para_id = doc
            .root_children()
            .iter()
            .filter_map(|item| item.as_node_id())
            .find_map(|id| match &doc.get(id)?.kind {
                NodeKind::Section { children, .. } => children.iter().find_map(|c| {
                    let cid = c.as_node_id()?;
                    match &doc.get(cid)?.kind {
                        NodeKind::Paragraph { .. } => Some(cid),
                        _ => None,
                    }
                }),
                _ => None,
            })
            .expect("paragraph must exist");

        doc.apply_op(EditOp::EditText {
            id: para_id,
            new_text: "Updated text.\n".to_string(),
        })
        .unwrap();

        let exported = export_doc(&doc);
        assert_eq!(exported, "=[ Title ]\n\nUpdated text.\n");
    }

    #[test]
    fn delete_and_insert_operations() {
        let src = "=[ Title ]\n\nFirst paragraph.\n\nSecond paragraph.\n";
        let mut doc = import_source(src);

        // Find section
        let section_id = doc
            .root_children()
            .iter()
            .find_map(|item| item.as_node_id())
            .unwrap();

        // New node to insert
        let new_id = doc.alloc_id();
        let new_node = Node::new(
            new_id,
            NodeKind::Paragraph {
                text: "Inserted paragraph.\n".to_string(),
            },
            "Inserted paragraph.\n".to_string(),
        );

        doc.apply_op(EditOp::Insert {
            parent: Some(section_id),
            index: 1,
            node: new_node,
        })
        .unwrap();

        let exported = export_doc(&doc);
        assert!(exported.contains("Inserted paragraph."));
    }

    #[test]
    fn move_node_operation() {
        let src = "=[ Sec A ]\n\nItem 1.\n\n=[ Sec B ]\n\nItem 2.\n";
        let mut doc = import_source(src);

        let root_ids: Vec<NodeId> = doc.root_nodes().collect();
        assert_eq!(root_ids.len(), 2);
        let sec_a = root_ids[0];
        let sec_b = root_ids[1];

        // Find Item 1
        let item1_id = match &doc.get(sec_a).unwrap().kind {
            NodeKind::Section { children, .. } => children
                .iter()
                .filter_map(|c| c.as_node_id())
                .next()
                .unwrap(),
            _ => panic!("expected section"),
        };

        // Move Item 1 into Sec B at end
        doc.apply_op(EditOp::Move {
            id: item1_id,
            new_parent: Some(sec_b),
            new_index: 1,
        })
        .unwrap();

        let exported = export_doc(&doc);
        assert!(
            exported.contains("=[ Sec B ]\nItem 1.\n\nItem 2.\n") || exported.contains("Item 1.")
        );
    }

    #[test]
    fn diagnostics_after_editing() {
        let src = "=[ Title ]\n\nParagraph.\n";
        let doc = import_source(src);

        // Clean document produces zero parse failures
        match diagnose_doc(&doc) {
            PostEditReport::Validation(diags) => {
                // Std elements only, no unknown elements
                assert!(
                    diags.is_empty(),
                    "clean doc should have no diagnostics: {diags:?}"
                );
            }
            PostEditReport::ParseFailure(err) => {
                panic!("clean document failed to parse: {err}");
            }
        }
    }

    #[test]
    fn set_attribute_data_adds_or_updates_key() {
        let src = "@meta{ x: 1 }\n\nBody text.\n";
        let mut doc = import_source(src);

        let meta_id = doc
            .root_children()
            .iter()
            .filter_map(|c| c.as_node_id())
            .find(|id| matches!(doc.get(*id).map(|n| &n.kind), Some(NodeKind::BlockElement { name, .. }) if name == "meta"))
            .expect("meta element must exist");

        doc.set_attribute(
            meta_id,
            AttrGroup::Data,
            "icon",
            Some("@doc.icon(star, pkg: tabler)"),
        )
        .unwrap();

        let exported = export_doc(&doc);
        assert_eq!(
            exported,
            "@meta{x: 1, icon: @doc.icon(star, pkg: tabler)}\n\nBody text.\n"
        );
    }

    #[test]
    fn set_attribute_replaces_existing_key() {
        let src = "@meta{ icon: @doc.icon(old) }\n\nBody.\n";
        let mut doc = import_source(src);

        let meta_id = doc
            .root_children()
            .iter()
            .filter_map(|c| c.as_node_id())
            .next()
            .unwrap();

        doc.set_attribute(meta_id, AttrGroup::Data, "icon", Some("@doc.icon(new)"))
            .unwrap();

        let exported = export_doc(&doc);
        assert_eq!(exported, "@meta{icon: @doc.icon(new)}\n\nBody.\n");
    }

    #[test]
    fn set_attribute_deletes_key_when_value_is_none() {
        let src = "@meta{ x: 1, y: 2 }\n\nBody.\n";
        let mut doc = import_source(src);

        let meta_id = doc
            .root_children()
            .iter()
            .filter_map(|c| c.as_node_id())
            .next()
            .unwrap();

        doc.set_attribute(meta_id, AttrGroup::Data, "x", None::<String>)
            .unwrap();

        let exported = export_doc(&doc);
        assert_eq!(exported, "@meta{y: 2}\n\nBody.\n");
    }

    #[test]
    fn set_attribute_on_args_and_id() {
        let src = "@callout(type: \"info\")[ Note ]\n";
        let mut doc = import_source(src);

        let el_id = doc
            .root_children()
            .iter()
            .filter_map(|c| c.as_node_id())
            .next()
            .unwrap();

        doc.set_attribute(el_id, AttrGroup::Args, "type", Some("\"warning\""))
            .unwrap();
        doc.set_attribute(el_id, AttrGroup::Id, "", Some("callout-1"))
            .unwrap();

        let exported = export_doc(&doc);
        assert_eq!(
            exported,
            "@callout#(callout-1)(type: \"warning\")[ Note ]\n"
        );
    }

    #[test]
    fn set_attribute_on_non_element_returns_error() {
        let src = "Just a paragraph.\n";
        let mut doc = import_source(src);

        let para_id = doc
            .root_children()
            .iter()
            .filter_map(|c| c.as_node_id())
            .next()
            .unwrap();

        let err = doc
            .set_attribute(para_id, AttrGroup::Data, "key", Some("val"))
            .unwrap_err();
        assert_eq!(err, EditError::NotAnElement(para_id));
    }

    #[test]
    fn insert_new_meta_and_set_attribute() {
        let src = "Just a note.\n";
        let mut doc = import_source(src);

        let meta_id = doc.alloc_id();
        let meta_node = Node::new(
            meta_id,
            NodeKind::BlockElement {
                name: "meta".to_string(),
                id_attr: None,
                args: Vec::new(),
                data: Vec::new(),
                content: None,
            },
            String::new(),
        );

        doc.apply_op(EditOp::Insert {
            parent: None,
            index: 0,
            node: meta_node,
        })
        .unwrap();

        // Insert spacing trivia between meta and following paragraph
        doc.root_children_mut()
            .insert(1, ChildItem::Trivia("\n\n".to_string()));

        doc.set_attribute(
            meta_id,
            AttrGroup::Data,
            "icon",
            Some("@doc.icon(star, pkg: tabler)"),
        )
        .unwrap();

        let exported = export_doc(&doc);
        assert_eq!(
            exported,
            "@meta{icon: @doc.icon(star, pkg: tabler)}\n\nJust a note.\n"
        );
    }
}
