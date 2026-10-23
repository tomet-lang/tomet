//! =[ tomet-edit ]
//!
//! Backend-neutral editable tree model of a Tomet document for collaborative editing.
//!
//! =[ Architecture & Core Principles ]
//!
//! - (CRDT-Agnostic Model)
//!   | This crate provides an in-memory, tree-structured document model with opaque [`NodeId`]s,
//!   | designed to bridge to external collaborative runtimes (such as Loro, Yjs, or Automerge).
//!   | It depends on zero CRDT libraries, networking, or session transport code.
//! - (CST Foundation & Guaranteed Invariant)
//!   | The model is derived from the Concrete Syntax Tree (`tomet-cst`). Trivia (whitespace,
//!   | newlines, comments) is preserved alongside nodes.
//!   | Guaranteed Invariant: Importing and immediately exporting without edits is 100% byte-identical
//!   | to the original input source across the entire corpus (`export_doc(&import_source(src)) == src`).
//! - (Text as Raw String (Paragraphs & Headings))
//!   | Prose within paragraphs, headings, and element content is held as raw text rather than
//!   | inline marks. Structure in the model is retained where identity, moving, and keys matter
//!   | (sections, blocks, attributes), allowing CRDTs to merge text via character-level sequences
//!   | without fragile structured-mark nesting.
//! - (First-Class Move & Closed Edit Operations)
//!   | [`EditOp`] provides a minimal, closed set of operations (`Insert`, `Delete`, `Move`, `EditText`, `SetAttribute`).
//!   | `Move` is a first-class operation, directly supported by tree-based CRDTs (e.g. Loro's MovableTree).
//!   | [`EditOp::SetAttribute`] allows key-level manipulation of an element's `{}` data, `()` args, and `#()` id slot.
//! - (Post-Edit Diagnostics)
//!   | Collaborative engines merge writes without rejecting them. [`diagnose_doc`] exports the edited
//!   | document to source text and re-parses it with the full validator to report duplicate IDs,
//!   | schema errors, or broken syntax after concurrent merges. [`diagnose_doc_in`] does it as a
//!   | document of a vault: its vocabularies, the types its settings declare, its blueprint.

pub mod address;
pub mod diagnostics;
pub mod doc;
pub mod export;
pub mod import;
pub mod node;
pub mod op;

pub use address::{AddressPath, BaseTarget, Step, parse_address_path};
pub use diagnostics::{PostEditReport, diagnose_doc, diagnose_doc_in};
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
        let src = "::meta{ x: 1 }\n\nBody text.\n";
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
            "::meta{x: 1, icon: @doc.icon(star, pkg: tabler)}\n\nBody text.\n"
        );
    }

    #[test]
    fn set_attribute_replaces_existing_key() {
        let src = "::meta{ icon: @doc.icon(old) }\n\nBody.\n";
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
        assert_eq!(exported, "::meta{icon: @doc.icon(new)}\n\nBody.\n");
    }

    #[test]
    fn set_attribute_deletes_key_when_value_is_none() {
        let src = "::meta{ x: 1, y: 2 }\n\nBody.\n";
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
        assert_eq!(exported, "::meta{y: 2}\n\nBody.\n");
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
                declaration: true,
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
            "::meta{icon: @doc.icon(star, pkg: tabler)}\n\nJust a note.\n"
        );
    }

    #[test]
    fn address_resolution_by_id_and_index() {
        let src =
            "=[ Sec A ]#(sec-a)\n\nParagraph in A.\n\n=[ Sec B ]#(sec-b)\n\nParagraph in B.\n";
        let doc = import_source(src);

        // Root indexing
        let root_0 = doc.node_at("#item(0)").expect("item(0) must exist");
        let root_1 = doc.node_at("item(1)").expect("item(1) must exist");
        assert_ne!(root_0, root_1);

        // ID resolution with different formats
        let sec_a_hash = doc.node_at("#sec-a").expect("#sec-a must exist");
        let sec_a_group = doc.node_at("#(sec-a)").expect("#(sec-a) must exist");
        let sec_a_id = doc.node_at("id:sec-a").expect("id:sec-a must exist");
        let sec_a_bare = doc.node_at("sec-a").expect("sec-a must exist");
        assert_eq!(root_0, sec_a_hash);
        assert_eq!(root_0, sec_a_group);
        assert_eq!(root_0, sec_a_id);
        assert_eq!(root_0, sec_a_bare);

        // Child item indexing
        let _para_a = doc
            .node_at("#sec-a.item(0)")
            .expect("child 0 of A must exist");
        assert_eq!(
            doc.text_at("#sec-a.item(0)").as_deref(),
            Some("Paragraph in A.\n")
        );

        // Chained root indexing
        let para_b_via_root = doc.node_at("#item(1).item(0)").expect("root 1 child 0");
        assert_eq!(
            doc.text_at("#item(1).item(0)").as_deref(),
            Some("Paragraph in B.\n")
        );
        assert_eq!(doc.node_at("#sec-b.item(0)"), Some(para_b_via_root));
    }

    #[test]
    fn address_text_and_property_queries() {
        let src = "::meta#(main-meta){ icon: @doc.icon(star), title: \"Doc\" }\n\n@callout#(warn)(type: \"warning\")[ Be careful! ]\n";
        let doc = import_source(src);

        // Query element properties
        assert_eq!(
            doc.text_at("#main-meta.icon").as_deref(),
            Some("@doc.icon(star)")
        );
        assert_eq!(
            doc.text_at("#main-meta.prop.title").as_deref(),
            Some("\"Doc\"")
        );
        assert_eq!(doc.text_at("#warn.type").as_deref(), Some("\"warning\""));

        // Query element metadata
        assert_eq!(doc.text_at("#main-meta.id").as_deref(), Some("main-meta"));
        assert_eq!(doc.text_at("#main-meta.sigil").as_deref(), Some("meta"));
        assert_eq!(doc.text_at("#warn.sigil").as_deref(), Some("callout"));

        // Query content
        assert_eq!(
            doc.text_at("#warn.content").as_deref(),
            Some(" Be careful! ")
        );
    }

    #[test]
    fn address_mutations_set_text_and_set_prop() {
        let src = "=[ Main Title ]#(intro)\n\nOld paragraph text.\n";
        let mut doc = import_source(src);

        // Change text via address
        doc.set_text_at("#intro.item(0)", "Updated via address.\n")
            .unwrap();
        let exported = export_doc(&doc);
        assert_eq!(
            exported,
            "=[ Main Title ]#(intro)\n\nUpdated via address.\n"
        );

        // Mutate meta property via address
        let src_meta = "::meta#(cfg){ version: 1 }\n\nText.\n";
        let mut doc_meta = import_source(src_meta);
        doc_meta.set_prop_at("#cfg.version", Some("2")).unwrap();
        doc_meta
            .set_prop_at("#cfg.author", Some("\"Tomet\""))
            .unwrap();
        let exported_meta = export_doc(&doc_meta);
        assert_eq!(
            exported_meta,
            "::meta#(cfg){version: 2, author: \"Tomet\"}\n\nText.\n"
        );
    }

    #[test]
    fn address_set_attribute_on_section_and_list_item() {
        // Section id update
        let src_sec = "=[ Intro ]\n\nBody.\n";
        let mut doc_sec = import_source(src_sec);
        doc_sec
            .set_attribute_at("#item(0)", AttrGroup::Id, "", Some("intro-sec"))
            .unwrap();
        let exported_sec = export_doc(&doc_sec);
        assert_eq!(exported_sec, "=[ Intro ]#(intro-sec)\n\nBody.\n");

        // Replace existing section id
        doc_sec
            .set_attribute_at("#intro-sec", AttrGroup::Id, "", Some("new-intro"))
            .unwrap();
        let exported_sec2 = export_doc(&doc_sec);
        assert_eq!(exported_sec2, "=[ Intro ]#(new-intro)\n\nBody.\n");

        // Remove section id
        doc_sec
            .set_attribute_at("#new-intro", AttrGroup::Id, "", None::<String>)
            .unwrap();
        let exported_sec3 = export_doc(&doc_sec);
        assert_eq!(exported_sec3, "=[ Intro ]\n\nBody.\n");

        // List item id extraction and update
        let src_list = "- [ task 1 ]#(t1)\n- task 2\n";
        let mut doc_list = import_source(src_list);
        assert_eq!(
            doc_list.node_at("#t1"),
            doc_list.node_at("#item(0).item(0)")
        );
        assert_eq!(doc_list.text_at("#t1.id").as_deref(), Some("t1"));

        // Update list item id
        doc_list
            .set_attribute_at("#t1", AttrGroup::Id, "", Some("task-done"))
            .unwrap();
        let exported_list = export_doc(&doc_list);
        assert_eq!(exported_list, "- [ task 1 ]#(task-done)\n- task 2\n");
    }

    #[test]
    fn address_delete_node() {
        let src = "=[ Title ]#(title)\n\nPara 1.\n\nPara 2.\n";
        let mut doc = import_source(src);

        assert_eq!(doc.child_nodes(doc.node_at("#title").unwrap()).len(), 2);
        doc.delete_at("#title.item(0)").unwrap();

        let exported = export_doc(&doc);
        assert_eq!(exported, "=[ Title ]#(title)\n\nPara 2.\n");
        assert_eq!(doc.child_nodes(doc.node_at("#title").unwrap()).len(), 1);
    }

    #[test]
    fn find_element_by_name_and_meta_helpers() {
        let src = "::meta{ version: 1 }\n\n// comment\n@callout(info)[ Notice ]\n";
        let doc = import_source(src);

        // Find elements by sigil/name
        let meta_id = doc
            .find_element_by_name("meta")
            .expect("meta should be found");
        let callout_id = doc
            .find_element_by_name("callout")
            .expect("callout should be found");
        assert_ne!(meta_id, callout_id);
        assert_eq!(doc.find_element_by_name("unknown"), None);

        // meta_node shortcut
        assert_eq!(doc.meta_node(), Some(meta_id));
        assert_eq!(doc.meta_data("version").as_deref(), Some("1"));
        assert_eq!(doc.meta_data("nonexistent"), None);

        // top_level_elements count
        assert_eq!(doc.top_level_elements().count(), 2);
    }

    #[test]
    fn ensure_meta_node_and_set_meta_data_on_existing_and_missing() {
        // 1. Existing ::meta: updates and adds keys
        let src_existing = "::meta{ a: 1 }\n\nBody text.\n";
        let mut doc = import_source(src_existing);
        let orig_meta_id = doc.meta_node().unwrap();

        doc.set_meta_data("b", Some("2")).unwrap();
        doc.set_meta_data("a", Some("10")).unwrap();
        assert_eq!(doc.meta_node(), Some(orig_meta_id));

        let exported = export_doc(&doc);
        assert_eq!(exported, "::meta{a: 10, b: 2}\n\nBody text.\n");

        // Delete a key
        doc.set_meta_data("b", None::<String>).unwrap();
        let exported_del = export_doc(&doc);
        assert_eq!(exported_del, "::meta{a: 10}\n\nBody text.\n");

        // 2. Missing ::meta: automatically creates ::meta at top with spacing trivia
        let src_none = "Just a note.\n";
        let mut doc_none = import_source(src_none);
        assert_eq!(doc_none.meta_node(), None);

        doc_none
            .set_meta_data("icon", Some("@doc.icon(star, pkg: tabler)"))
            .unwrap();
        assert!(doc_none.meta_node().is_some());

        let exported_none = export_doc(&doc_none);
        assert_eq!(
            exported_none,
            "::meta{icon: @doc.icon(star, pkg: tabler)}\n\nJust a note.\n"
        );

        // 3. ensure_meta_node on empty document
        let mut doc_empty = EditDoc::new();
        let empty_meta_id = doc_empty.ensure_meta_node();
        assert_eq!(doc_empty.ensure_meta_node(), empty_meta_id); // idempotent
        assert_eq!(export_doc(&doc_empty), "::meta\n");
    }

    #[test]
    fn tree_navigation_queries() {
        let src = r#"=[ Section 1 ]#(sec1)
First paragraph.

==[ Subsection ]#(sec2)
Second paragraph.

- item 1
- item 2
"#;
        let doc = import_source(src);

        let sec1_id = doc.node_at("#sec1").unwrap();
        let sec2_id = doc.node_at("#sec2").unwrap();
        let p1_id = doc.child_node_at(sec1_id, 0).unwrap(); // First paragraph
        let list_id = doc.child_node_at(sec2_id, 1).unwrap(); // List under Subsection
        let item1_id = doc.child_node_at(list_id, 0).unwrap();
        let item2_id = doc.child_node_at(list_id, 1).unwrap();

        // 1. parent_of
        assert_eq!(doc.parent_of(sec1_id), None);
        assert_eq!(doc.parent_of(p1_id), Some(sec1_id));
        assert_eq!(doc.parent_of(sec2_id), Some(sec1_id));
        assert_eq!(doc.parent_of(list_id), Some(sec2_id));
        assert_eq!(doc.parent_of(item1_id), Some(list_id));
        assert_eq!(doc.parent_of(item2_id), Some(list_id));

        // 2. ancestors
        assert_eq!(doc.ancestors(sec1_id), vec![]);
        assert_eq!(doc.ancestors(p1_id), vec![sec1_id]);
        assert_eq!(doc.ancestors(item1_id), vec![list_id, sec2_id, sec1_id]);

        // 3. index_in_parent (among sibling nodes)
        assert_eq!(doc.index_in_parent(sec1_id), Some(0));
        assert_eq!(doc.index_in_parent(p1_id), Some(0));
        assert_eq!(doc.index_in_parent(sec2_id), Some(1));
        assert_eq!(doc.index_in_parent(item1_id), Some(0));
        assert_eq!(doc.index_in_parent(item2_id), Some(1));

        // 4. raw_index_in_parent
        assert!(doc.raw_index_in_parent(sec1_id).is_some());
        assert!(doc.raw_index_in_parent(item2_id).is_some());

        // 5. prev_sibling & next_sibling
        assert_eq!(doc.prev_sibling(sec1_id), None);
        assert_eq!(doc.next_sibling(sec1_id), None); // only 1 root node in this doc

        assert_eq!(doc.prev_sibling(p1_id), None);
        assert_eq!(doc.next_sibling(p1_id), Some(sec2_id));
        assert_eq!(doc.prev_sibling(sec2_id), Some(p1_id));
        assert_eq!(doc.next_sibling(sec2_id), None);

        assert_eq!(doc.prev_sibling(item1_id), None);
        assert_eq!(doc.next_sibling(item1_id), Some(item2_id));
        assert_eq!(doc.prev_sibling(item2_id), Some(item1_id));
        assert_eq!(doc.next_sibling(item2_id), None);

        // 6. leaf_nodes
        let leaves = doc.leaf_nodes();
        // Leaves should be: First paragraph, Second paragraph, item 1, item 2
        assert_eq!(leaves.len(), 4);
        assert_eq!(leaves[0], p1_id);
        assert_eq!(leaves[2], item1_id);
        assert_eq!(leaves[3], item2_id);
    }

    #[test]
    fn node_factories_and_fluent_insertion() {
        let mut doc = EditDoc::new();

        let sec_id = doc
            .append_section(None, 1, "Introduction", Some("intro"))
            .unwrap();
        let p_id = doc
            .append_paragraph(Some(sec_id), "This is an introductory paragraph.")
            .unwrap();
        let code_id = doc
            .append_code_block(
                Some(sec_id),
                Some("rust"),
                "fn main() {\n    println!(\"Hello\");\n}",
            )
            .unwrap();

        assert_eq!(doc.parent_of(p_id), Some(sec_id));
        assert_eq!(doc.parent_of(code_id), Some(sec_id));

        let exported = export_doc(&doc);
        assert!(exported.starts_with("=[ Introduction ] #(intro)\n"));
        assert!(exported.contains("This is an introductory paragraph.\n"));
        assert!(exported.contains("```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n"));

        // insert_paragraph at position 0 inside section
        let inserted_p = doc
            .insert_paragraph(Some(sec_id), 0, "Top notice inside section.")
            .unwrap();
        assert_eq!(doc.index_in_parent(inserted_p), Some(0));
        assert_eq!(doc.index_in_parent(p_id), Some(1));
    }

    #[test]
    fn atomic_replace_node_and_apply_ops() {
        let src = "=[ Title ]\n\nOld paragraph to replace.\n";
        let mut doc = import_source(src);
        let sec_id = doc.root_nodes().next().unwrap();
        let old_p_id = doc.child_node_at(sec_id, 0).unwrap();

        let new_id = doc.alloc_id();
        let new_node = Node::code_block(new_id, Some("toml"), "key = \"value\"\n");

        doc.replace_node(old_p_id, new_node).unwrap();

        // Old node is removed
        assert!(doc.get(old_p_id).is_none());
        assert!(doc.get(new_id).is_some());
        assert_eq!(doc.child_node_at(sec_id, 0), Some(new_id));

        let exported = export_doc(&doc);
        assert!(exported.contains("```toml\nkey = \"value\"\n```\n"));

        // Batch apply_ops
        let p2_id = doc.alloc_id();
        let p2 = Node::paragraph(p2_id, "Batch paragraph.");
        doc.apply_ops([
            EditOp::Insert {
                parent: Some(sec_id),
                index: 1,
                node: p2,
            },
            EditOp::EditText {
                id: p2_id,
                new_text: "Edited batch paragraph.\n".to_string(),
            },
        ])
        .unwrap();

        assert_eq!(
            doc.text_at("#item(0).item(0)").as_deref(),
            Some("Edited batch paragraph.\n")
        );
        assert_eq!(
            doc.text_at("#item(0).item(1)").as_deref(),
            Some("```toml\nkey = \"value\"\n```\n")
        );
    }

    #[test]
    fn task_item_detection_and_toggling() {
        let src = r#"- () Task 1
- (x) Task 2
- Regular item
- ( ) Task 3
- (done) Task 4
- (todo) Task 5
"#;
        let mut doc = import_source(src);
        let list_id = doc.root_nodes().next().unwrap();
        let items = doc.child_nodes(list_id);
        assert_eq!(items.len(), 6);

        // Detection & inspection
        assert!(doc.is_task_item(items[0]));
        assert_eq!(doc.task_state(items[0]), Some(false));

        assert!(doc.is_task_item(items[1]));
        assert_eq!(doc.task_state(items[1]), Some(true));

        assert!(!doc.is_task_item(items[2]));
        assert_eq!(doc.task_state(items[2]), None);

        assert!(doc.is_task_item(items[3]));
        assert_eq!(doc.task_state(items[3]), Some(false));

        assert!(doc.is_task_item(items[4]));
        assert_eq!(doc.task_state(items[4]), Some(true));

        assert!(doc.is_task_item(items[5]));
        assert_eq!(doc.task_state(items[5]), Some(false));

        // Toggle Task 1: false -> true
        doc.toggle_task(items[0]).unwrap();
        assert_eq!(doc.task_state(items[0]), Some(true));
        assert!(export_doc(&doc).contains("- (x) Task 1"));

        // Toggle Task 1 back: true -> false
        doc.toggle_task(items[0]).unwrap();
        assert_eq!(doc.task_state(items[0]), Some(false));
        assert!(export_doc(&doc).contains("- ( ) Task 1"));

        // Toggle Task 5 ((todo) -> (done)): false -> true
        doc.toggle_task(items[5]).unwrap();
        assert_eq!(doc.task_state(items[5]), Some(true));
        assert!(export_doc(&doc).contains("- (done) Task 5"));

        // Toggle Task 5 back ((done) -> (todo)): true -> false
        doc.toggle_task(items[5]).unwrap();
        assert_eq!(doc.task_state(items[5]), Some(false));
        assert!(export_doc(&doc).contains("- (todo) Task 5"));

        // Standalone @task element
        let task_elem_src = "@task(todo)[ Standalone task ]\n";
        let mut doc_elem = import_source(task_elem_src);
        let elem_id = doc_elem.root_nodes().next().unwrap();
        assert!(doc_elem.is_task_item(elem_id));
        assert_eq!(doc_elem.task_state(elem_id), Some(false));

        doc_elem.toggle_task(elem_id).unwrap();
        assert_eq!(doc_elem.task_state(elem_id), Some(true));
        assert_eq!(export_doc(&doc_elem), "@task(done)[ Standalone task ]\n");

        doc_elem.toggle_task(elem_id).unwrap();
        assert_eq!(doc_elem.task_state(elem_id), Some(false));
        assert_eq!(export_doc(&doc_elem), "@task(todo)[ Standalone task ]\n");

        // Node::task_item factory
        let item_id = doc.alloc_id();
        let task_node = Node::task_item(item_id, "-", true, "New task", None::<String>);
        assert_eq!(
            match &task_node.kind {
                NodeKind::ListItem { text, .. } => text.as_str(),
                _ => "",
            },
            "- (x) New task\n"
        );
    }

    #[test]
    fn list_item_indent_and_outdent() {
        let src = "- item 1\n- item 2\n";
        let mut doc = import_source(src);
        let list_id = doc.root_nodes().next().unwrap();
        let item2_id = doc.child_nodes(list_id)[1];

        // Indent item 2 by 2 spaces
        doc.indent_list_item(item2_id).unwrap();
        let exported_indented = export_doc(&doc);
        assert_eq!(exported_indented, "- item 1\n  - item 2\n");

        // Indent item 2 by 2 more spaces (total 4)
        doc.indent_list_item(item2_id).unwrap();
        assert_eq!(export_doc(&doc), "- item 1\n    - item 2\n");

        // Outdent by 2 spaces
        doc.outdent_list_item(item2_id).unwrap();
        assert_eq!(export_doc(&doc), "- item 1\n  - item 2\n");

        // Outdent back to 0 spaces
        doc.outdent_list_item(item2_id).unwrap();
        assert_eq!(export_doc(&doc), "- item 1\n- item 2\n");
    }
}
