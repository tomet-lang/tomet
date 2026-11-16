//! Fragments, emoji and unicode.

use super::*;

/// The link element inside a one-paragraph fragment, as its `args` string.
fn link_target(value: &EvalValue) -> String {
    let EvalValue::Fragment(blocks) = value else {
        panic!("expected a fragment, got {value:?}");
    };
    let [tomet_ast::Block::Paragraph(p)] = blocks.as_slice() else {
        panic!("expected one paragraph, got {blocks:?}");
    };
    let [tomet_ast::Inline::Element(el)] = p.content.as_slice() else {
        panic!("expected one inline element, got {:?}", p.content);
    };
    assert_eq!(el.name, tomet_ast::Name::bare("link"));
    match el.data.args.as_ref().and_then(|args| args.nth_unnamed(0)) {
        Some(tomet_ast::Value::Scalar(Scalar::Text(target))) => target.clone(),
        other => panic!("expected a string target, got {other:?}"),
    }
}

#[test]
fn a_document_links_to_itself_as_a_fragment() {
    assert_eq!(
        link_target(&in_vault("tmt(\"docs/a.tmt\").link()").unwrap()),
        "tmt:docs/a.tmt"
    );
    assert_eq!(
        link_target(&in_vault("link(self)").unwrap()),
        "tmt:here.tmt"
    );
}

#[test]
fn an_element_with_an_id_links_to_that_id() {
    assert_eq!(
        link_target(&in_vault("#c1.link()").unwrap()),
        "tmt:here.tmt#c1"
    );
    assert_eq!(
        link_target(&in_vault("#intro.link()").unwrap()),
        "tmt:here.tmt#intro"
    );
}

#[test]
fn an_element_without_an_id_cannot_be_linked() {
    let err = in_vault("self#item(0).link()").unwrap_err();
    assert!(err.to_string().contains("without an id"), "{err}");
}

#[test]
fn link_turns_a_list_of_documents_into_a_list_of_links() {
    let links = in_vault("addr(dir:docs)::docs.map(d => d.link())").unwrap();
    let EvalValue::Tuple(items) = links else {
        panic!("expected a tuple");
    };
    let targets: Vec<String> = items.unnamed().map(link_target).collect();
    assert_eq!(targets, ["tmt:docs/a.tmt", "tmt:docs/sub/b.tmt"]);
}

#[test]
fn link_wants_a_document_or_an_element() {
    assert!(matches!(
        in_vault("link(1)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn a_fragment_is_data_and_a_reference_is_not() {
    let link = in_vault("self.link()").unwrap();
    assert!(link.to_ast().is_some());
    assert_eq!(in_vault("self").unwrap().to_ast(), None);
}

#[test]
fn emoji_looks_a_name_up_and_refuses_one_it_does_not_know() {
    assert_eq!(out("emoji(\"rocket\")").unwrap(), text("🚀"));
    assert_eq!(out("emoji(\":tada:\")").unwrap(), text("🎉"));
    assert_eq!(out("emoji(\"party\")").unwrap(), text("🎉"));
    let err = out("emoji(\"nope\")").unwrap_err();
    assert!(err.to_string().contains("nope"), "{err}");
    assert!(matches!(
        out("emoji(1)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn unicode_takes_a_code_point_as_a_number_or_hex_text() {
    assert_eq!(out("unicode(65)").unwrap(), text("A"));
    assert_eq!(out("unicode(\"U+1F680\")").unwrap(), text("🚀"));
    assert_eq!(out("unicode(\"1F680\")").unwrap(), text("🚀"));
    assert_eq!(out("unicode(\"0x41\")").unwrap(), text("A"));
    assert!(out("unicode(\"zzz\")").is_err());
    assert!(out("unicode(-1)").is_err());
    // A surrogate is not a character.
    assert!(out("unicode(55296)").is_err());
    assert!(matches!(
        out("unicode(true)"),
        Err(EvalError::TypeMismatch { got: "bool", .. })
    ));
}
