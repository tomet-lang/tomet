//! AST node accessors, extension traits, builders, and recursive tree traversal for Tomet.
//!
//! Provides:
//! - (ValueExt) Inspection and accessor methods on [`tomet_ast::Value`].
//! - (ElementExt) Builders and name helpers on [`tomet_ast::Element`].
//! - (DataExt) Property access on [`tomet_ast::Data`], shared by every node that carries `(args)` and `{value}`.
//! - (DocumentExt) Block-level manipulation helpers on [`tomet_ast::Document`].
//! - (walk) One definition of what sits inside what ([`children`]), the one recursion over it ([`walk`]), and the walkers built on that.

pub mod document;
pub mod element;
pub mod value;
pub mod walk;

pub use document::*;
pub use element::*;
pub use value::*;
pub use walk::*;

// Free-function convenience wrappers
pub fn element_get_attr<'a>(el: &'a tomet_ast::Element, key: &str) -> Option<&'a tomet_ast::Value> {
    el.data.get_attr(key)
}

pub fn element_get_attr_mut<'a>(
    el: &'a mut tomet_ast::Element,
    key: &str,
) -> Option<&'a mut tomet_ast::Value> {
    el.data.get_attr_mut(key)
}

pub fn element_has_prop_key(el: &tomet_ast::Element, key: &str) -> bool {
    el.data.has_prop_key(key)
}

pub fn element_set_prop(el: &mut tomet_ast::Element, key: &str, new_val: tomet_ast::Value) {
    el.data.set_prop(key, new_val);
}

pub fn element_rename_prop_key(el: &mut tomet_ast::Element, old_key: &str, new_key: &str) -> bool {
    el.data.rename_prop_key(old_key, new_key)
}

pub fn element_replace_prop_value(
    el: &mut tomet_ast::Element,
    target_key: &str,
    new_val: tomet_ast::Value,
) -> bool {
    el.data.replace_prop_value(target_key, new_val)
}

pub fn element_remove_prop(el: &mut tomet_ast::Element, key: &str) -> Option<tomet_ast::Value> {
    el.data.remove_prop(key)
}

pub fn element_transform_prop<F>(el: &mut tomet_ast::Element, key: &str, f: F) -> bool
where
    F: FnOnce(tomet_ast::Value) -> tomet_ast::Value,
{
    el.data.transform_prop(key, f)
}

pub fn element_attrs_view(el: &tomet_ast::Element) -> Option<tomet_ast::Value> {
    el.data.attrs_view()
}

pub fn element_attrs_mut(el: &mut tomet_ast::Element) -> Option<&mut tomet_ast::Value> {
    el.data.attrs_mut()
}

pub fn insert_block_at(doc: &mut tomet_ast::Document, index: usize, block: tomet_ast::Block) {
    doc.insert_block(index, block);
}

pub fn remove_block_at(doc: &mut tomet_ast::Document, index: usize) -> Option<tomet_ast::Block> {
    doc.remove_block(index)
}

pub fn replace_block_at(
    doc: &mut tomet_ast::Document,
    index: usize,
    new_block: tomet_ast::Block,
) -> Option<tomet_ast::Block> {
    doc.replace_block(index, new_block)
}

pub fn find_block_index<F>(doc: &tomet_ast::Document, predicate: F) -> Option<usize>
where
    F: FnMut(&tomet_ast::Block) -> bool,
{
    doc.find_block_index(predicate)
}

pub fn find_element_block_index<F>(doc: &tomet_ast::Document, predicate: F) -> Option<usize>
where
    F: FnMut(&tomet_ast::Element) -> bool,
{
    doc.find_element_block_index(predicate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{Block, Inline, Name, Span, Text, Value};

    fn parse(src: &str) -> tomet_ast::Document {
        tomet_parser::parse_document(src).expect("valid Tomet source")
    }

    #[test]
    fn test_value_extension_traits() {
        let val = Value::Map(vec![("id".into(), Value::Int(123))]);
        assert_eq!(val.get("id").and_then(|v| v.as_i64()), Some(123));
        assert!(!val.is_empty());
    }

    #[test]
    fn test_element_extension_traits() {
        let mut el = element_new(Name::bare("card"));
        el.data.set_prop("tag", Value::String("urgent".into()));
        assert_eq!(
            el.data.get_attr("tag").and_then(|v| v.as_str()),
            Some("urgent")
        );
        assert!(el.data.has_prop_key("tag"));

        el.data.rename_prop_key("tag", "priority");
        assert_eq!(
            el.data.get_attr("priority").and_then(|v| v.as_str()),
            Some("urgent")
        );
        assert!(!el.data.has_prop_key("tag"));

        let removed = el.data.remove_prop("priority");
        assert_eq!(removed, Some(Value::String("urgent".into())));
        assert_eq!(el.data.get_attr("priority"), None);
    }

    #[test]
    fn test_document_extension_traits_and_walk() {
        let mut doc = parse("#[ Heading ]\n\n- item\n");
        assert_eq!(doc.blocks.len(), 2);

        let third = Block::Paragraph(tomet_ast::Paragraph {
            content: vec![Inline::Text(Text {
                value: "Third".into(),
                span: Span::dummy(),
            })],
            span: Span::dummy(),
        });
        doc.insert_block(1, third);
        assert_eq!(doc.blocks.len(), 3);

        // The bare list item (`- item`) is a `ListItem`, not an `Element`,
        // so only `for_each_list_item` visits it.
        let mut elements = 0;
        for_each_element(&doc, |_el| elements += 1);
        assert_eq!(elements, 0);
        let mut items = 0;
        for_each_list_item(&doc, |_item| items += 1);
        assert_eq!(items, 1);
    }
}
