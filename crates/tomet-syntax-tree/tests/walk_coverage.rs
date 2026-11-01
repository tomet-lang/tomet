//! What each walker of `tomet-tree` visits, position by position.
//!
//! One document holds a sentinel (`@s_...`) in every position an element can sit
//! in. Each walker reports the sentinels it reaches, in order, and the lists
//! below are exact: a walker that starts to miss a position, or to reach a new
//! one, fails here. Where two walkers differ, the comment says which is meant.

use tomet_ast::{Block, Document, Entry, Inline, Value};
use tomet_tree::*;

const SRC: &str = "\
::meta{ k: @s_decl_value() }

::kind(@s_decl_args())

text @s_para_inline[x] more

@s_block()

<?(h)>[@s_hole_block_content()]

text <?(h2)>[@s_hole_inline_content()]

@s_host(a: @s_args(), b: [@s_args_blocks()], c: list(@s_seq())){ d: @s_pair(), @s_entry(id){} }[ @s_content()[ @s_deep() ] ]:rule(allow: list(@s_connect()))

=[ title @s_title() ]{ k: @s_sec_data() }:rule(allow: list(@s_sec_connect()))

@s_sec_block()

- item @s_item()
  - sub @s_sub()
";

fn doc() -> Document {
    tomet_parser::parse_document(SRC).expect("the walk document parses")
}

fn sentinel(el: &tomet_ast::Element) -> Option<String> {
    let name = el.name.name.clone();
    name.starts_with("s_").then_some(name)
}

fn names(f: impl FnOnce(&mut Vec<String>)) -> Vec<String> {
    let mut v = Vec::new();
    f(&mut v);
    v
}

/// The positions an element sits in, in the order the element walkers meet them. Not reached on
/// purpose: a `:connect` (metadata about its owner, not a part of the document: see the doc comment
/// of `for_each_descendant`). Not reached, and nowhere said to be on purpose: the `{value}` of a
/// section (`s_sec_data`). Everything else is.
const ELEMENT_WALK: [&str; 18] = [
    "s_decl_value",
    "s_decl_args",
    "s_para_inline",
    "s_block",
    "s_hole_block_content",
    "s_hole_inline_content",
    "s_host",
    "s_args",
    "s_args_blocks",
    "s_seq",
    "s_content",
    "s_deep",
    "s_pair",
    "s_entry",
    "s_title",
    "s_sec_block",
    "s_item",
    "s_sub",
];

#[test]
fn the_element_walkers_reach_every_position_but_connects_and_a_sections_value() {
    let expected: Vec<&str> = ELEMENT_WALK.to_vec();
    let doc = doc();
    assert_eq!(
        names(|v| for_each_element(&doc, |el| v.extend(sentinel(el)))),
        expected
    );
    let mut d = doc.clone();
    assert_eq!(
        names(|v| for_each_element_mut(&mut d, |el| v.extend(sentinel(el)))),
        expected,
        "the mutable walker visits what the immutable one does"
    );
}

#[test]
fn for_each_node_reaches_the_connects_and_a_sections_value_too() {
    // It also differs in order: the groups of a node come as `(args)`, `{value}`, `[content]`,
    // where the element walkers have `(args)`, `[content]`, `{value}`.
    let doc = doc();
    let got = names(|v| {
        for_each_node(&doc, |n| match n {
            Node::Block(Block::Element(el))
            | Node::Inline(Inline::Element(el))
            | Node::Entry(Entry::Element(el)) => v.extend(sentinel(el)),
            Node::Value(Value::Element(el)) => v.extend(sentinel(el)),
            _ => {}
        })
    });
    assert_eq!(
        got,
        [
            "s_decl_value",
            "s_decl_args",
            "s_para_inline",
            "s_block",
            "s_hole_block_content",
            "s_hole_inline_content",
            "s_host",
            "s_args",
            "s_args_blocks",
            "s_seq",
            "s_pair",
            "s_entry",
            "s_content",
            "s_deep",
            "s_connect",
            "s_title",
            "s_sec_data",
            "s_sec_connect",
            "s_sec_block",
            "s_item",
            "s_sub",
        ]
    );
}

#[test]
fn for_each_inline_reaches_the_inline_elements_and_holes_the_element_walkers_do() {
    let doc = doc();
    let got = names(|v| {
        for_each_inline(&doc, |i| match i {
            Inline::Element(el) => v.extend(sentinel(el)),
            Inline::Hole(_) => v.push("<hole>".into()),
            _ => {}
        })
    });
    assert_eq!(
        got,
        [
            "s_para_inline",
            "s_hole_block_content",
            "<hole>",
            "s_hole_inline_content",
            "s_args_blocks",
            "s_content",
            "s_deep",
            "s_title",
            "s_item",
            "s_sub",
        ]
    );
}

#[test]
fn descendants_are_the_content_and_the_entries_of_the_value_group_not_the_args() {
    // `s_args`, `s_args_blocks` and `s_seq` sit in `(args)`, which a descendant walk skips on
    // purpose (they describe the element). `s_pair` is the value of a `key: value` entry, and
    // only the elements that are entries themselves count: nothing says that is on purpose.
    let doc = doc();
    let mut got = Vec::new();
    for_each_element(&doc, |el| {
        if el.name.name == "s_host" {
            for direct in [false, true] {
                let w = names(|w| {
                    for_each_descendant(el, direct, |c| w.extend(c.name().map(|n| n.name.clone())))
                });
                got.push((direct, w));
            }
        }
    });
    assert_eq!(
        got,
        [
            (
                false,
                vec!["s_content".to_string(), "s_deep".into(), "s_entry".into()]
            ),
            (true, vec!["s_content".to_string(), "s_entry".into()]),
        ]
    );
}

#[test]
fn the_top_level_walkers_see_only_the_top() {
    let doc = doc();
    assert_eq!(
        names(|v| for_each_top_level_element(&doc, |el| v.extend(sentinel(el)))),
        ["s_para_inline", "s_block", "s_host"]
    );
    assert_eq!(
        names(|v| for_each_top_level_declaration(&doc, |d| v.push(d.name.name.clone()))),
        ["meta", "kind"]
    );
}

#[test]
fn the_other_walkers_count() {
    let doc = doc();
    let (mut sections, mut lists, mut items, mut holes, mut decls) = (0, 0, 0, 0, 0);
    for_each_section(&doc, |_| sections += 1);
    for_each_list(&doc, |_| lists += 1);
    for_each_list_item(&doc, |_| items += 1);
    for_each_hole(&doc, |_| holes += 1);
    for_each_declaration(&doc, |_| decls += 1);
    // The sublist is a list of its own; a hole in running text and one standing as a block.
    assert_eq!((sections, lists, items, holes, decls), (1, 2, 2, 2, 2));
}
