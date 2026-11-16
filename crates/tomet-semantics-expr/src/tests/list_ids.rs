//! A list that carries an id.

use super::*;

/// A document whose list has a unique id.
///
/// No syntax writes one yet -- a trailing `#(id)` belongs to the last item --
/// so the id is set on the tree. The list is addressed by the rules the
/// element and the section already follow, which is what these pin down.
fn list_vault() -> MemoryResolver {
    let mut doc = tomet_ast_parser::parse_file("Lead.\n\n- one #(i1)\n- two\n- three\n").unwrap();
    let mut named = 0;
    for block in &mut doc.body {
        if let tomet_ast::Block::List(list) = block {
            list.id = Some(tomet_ast::Id("shopping".to_string()));
            named += 1;
        }
    }
    assert_eq!(named, 1, "the document has one list");
    MemoryResolver::new().with_document("here.tmt", std::sync::Arc::new(doc))
}

fn in_list_vault(src: &str) -> Result<EvalValue, EvalError> {
    let resolver = list_vault();
    let macros = HashMap::new();
    let vars = EvalValue::Scalar(Scalar::None);
    let cx = EvalContext {
        resolver: &resolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        var: &vars,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    evaluate(&parse(src), &cx)
}

#[test]
fn a_list_is_found_by_its_id_like_an_element() {
    assert_eq!(in_list_vault("#shopping::id").unwrap(), text("shopping"));
    assert_eq!(in_list_vault("#shopping::name").unwrap(), text("list"));
    assert_eq!(in_list_vault("#shopping::connects").unwrap(), seq(vec![]));
    // A list holds no data of its own.
    assert!(in_list_vault("#shopping.title").is_err());
    // An item still has its own id, and the list's does not shadow it.
    assert_eq!(in_list_vault("#i1::id").unwrap(), text("i1"));
    assert!(in_list_vault("#nope::id").is_err());
}

#[test]
fn item_on_a_list_counts_its_items() {
    assert_eq!(in_list_vault("#shopping.item(0)::id").unwrap(), text("i1"));
    assert_eq!(
        in_list_vault("#shopping.item(1)::id").unwrap(),
        EvalValue::Scalar(Scalar::None)
    );
    assert!(in_list_vault("#shopping.item(3)").is_err());
}

/// The list is a node wherever a block is counted -- at the root of the
/// document, as the section already is -- so it can be asked for its id.
#[test]
fn a_list_among_the_blocks_is_a_node() {
    assert_eq!(in_list_vault("#item(1)::id").unwrap(), text("shopping"));
    assert!(matches!(
        in_list_vault("#item(0)").unwrap(),
        EvalValue::Fragment(_)
    ));
}

#[test]
fn a_list_with_an_id_can_be_linked_and_included() {
    assert!(matches!(
        in_list_vault("#shopping.link()").unwrap(),
        EvalValue::Fragment(_)
    ));
    let included = match in_list_vault("include(#shopping)").unwrap() {
        EvalValue::Fragment(blocks) => blocks,
        other => panic!("expected a fragment, got {other:?}"),
    };
    assert!(
        matches!(included.as_slice(), [tomet_ast::Block::List(list)] if list.items.len() == 3),
        "{included:?}"
    );
    // `heading:` is about a section's heading, and a list has none.
    assert!(in_list_vault("include(#shopping, heading: false)").is_err());
}
