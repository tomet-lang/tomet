//! Lowering refuses a nexus -- `@name`, a `::name` declaration or connect -- that has no primary group, `(args)`
//! or `[content]`, at the offset where the nexus begins; the tree it reads is the CST's, whole.

use crate::{Error, lower_data_file, lower_declaration, lower_file, lower_value};
use tomet_ast::{Block, Tuple, Value};
use tomet_cst_parser::{parse_cst, parse_cst_value};

fn file(src: &str) -> Result<tomet_ast::File, Error> {
    lower_file(&parse_cst(src))
}

fn value(src: &str) -> Result<Value, Error> {
    lower_value(&parse_cst_value(src))
}

fn refusal(result: Result<impl std::fmt::Debug, Error>, src: &str) -> Error {
    match result {
        Ok(ok) => panic!("{src:?} should be refused, got {ok:?}"),
        Err(e) => e,
    }
}

#[test]
fn a_block_nexus_without_a_primary_group_is_refused_where_it_begins() {
    for (src, at) in [
        ("@name\n", 0),
        ("@name{}\n", 0),
        ("@name#(id)\n", 0),
        ("@name{}#(id)\n", 0),
        ("@name::rule(a)\n", 0),
        ("@name::{a: 1}\n", 0),
        ("@name::(a: 1)\n", 0),
        ("a\n\n@name{}\n", 3),
        ("::name\n", 0),
        ("::name{}\n", 0),
    ] {
        let e = refusal(file(src), src);
        assert!(
            e.message.contains("needs a primary group"),
            "{src:?}: {}",
            e.message
        );
        assert_eq!(e.offset, at, "{src:?}");
    }
}

#[test]
fn a_declaration_refuses_a_connect_before_it_reads_any_group() {
    // A nameless `::(...)` would otherwise be read as the declaration's own args.
    for src in [
        "::name::(a: 1)\n",
        "::name::{a: 1}\n",
        "::name(a)::{b: 1}\n",
        "::name()::rule(a)\n",
    ] {
        let e = refusal(file(src), src);
        assert!(
            e.message.contains("takes no `::connect`"),
            "{src:?}: {}",
            e.message
        );
        assert_eq!(e.offset, 0, "{src:?}");
    }
}

#[test]
fn lower_declaration_holds_to_the_same_rule() {
    for (src, message) in [
        ("::meta{ a: 1 }\n", "needs a primary group"),
        ("::meta\n", "needs a primary group"),
        ("::meta:(a: 1)\n", "takes no `::connect`"),
    ] {
        let root = parse_cst(src);
        let node = root.children().next().expect("a declaration");
        let e = lower_declaration(&node).expect_err(src);
        assert!(e.message.contains(message), "{src:?}: {}", e.message);
    }
    let root = parse_cst("::vocabulary(ns){ a: 1 }\n");
    let decl = lower_declaration(&root.children().next().unwrap()).unwrap();
    assert!(decl.data.args.is_some() && decl.data.value.is_some());
}

#[test]
fn a_nexus_inside_a_value_is_held_to_it_too() {
    for (src, at) in [
        ("k: @name{}\n", 3),
        ("k: @name#(id)\n", 3),
        ("k: @name::rule(a)\n", 3),
        ("k: @name::{a: 1}\n", 3),
        ("a: [ x @name{} ]\n", 7),
        ("k: (@name{})\n", 4),
    ] {
        let e = refusal(value(src), src);
        assert!(
            e.message.contains("needs a primary group"),
            "{src:?}: {}",
            e.message
        );
        assert_eq!(e.offset, at, "{src:?}");
    }
    // Bare, in a value, it is a string.
    assert_eq!(
        value("k: @name\n").unwrap(),
        Value::Tuple(Tuple::from_named(vec![(
            "k".into(),
            Value::String("@name".into())
        )]))
    );
    assert!(value("k: @name()\n").is_ok());
    assert!(value("k: @name[ x ]\n").is_ok());
}

#[test]
fn a_data_file_body_is_held_to_it_too() {
    let root = parse_cst_value("k: @name{}\n");
    let e = lower_data_file(&root, "k: @name{}\n").expect_err("a nexus without a primary group");
    assert!(e.message.contains("needs a primary group"), "{}", e.message);
    assert_eq!((e.line, e.column, e.offset), (1, 4, 3));
}

#[test]
fn a_named_connect_without_a_primary_group_is_refused_at_the_colon() {
    for (src, at) in [
        ("@el(a)::rule{x: 1}\n", 6),
        ("@el(a)::rule{}\n", 6),
        ("-#(id) ::rule{}\n", 7),
        ("=#(id) ::rule{}\n", 7),
        // A bracket-less body claims the connect all the same.
        ("- item ::rule{a: 1}\n", 7),
        ("= H ::rule{a: 1}\n", 4),
    ] {
        let e = refusal(file(src), src);
        assert!(
            e.message.contains("`::rule` needs a primary group"),
            "{src:?}: {}",
            e.message
        );
        assert_eq!(e.offset, at, "{src:?}");
    }
}

#[test]
fn a_nameless_connect_cannot_hold_content() {
    let e = refusal(file("@el(a)::[x]\n"), "@el(a)::[x]");
    assert!(e.message.contains("nameless"), "{}", e.message);
    assert_eq!(e.offset, 8);
}

#[test]
fn what_has_a_primary_group_lowers() {
    for src in [
        "@name()\n",
        "@name[ x ]\n",
        "@name\n| a\n",
        "@name(a)[x]{k: 1}#(id)::rule(b)\n",
        "@el(a)::{b: 1}\n",
        "@el(a)::(b: 1)\n",
        "ping @alice\n",
        "- item ::{a: 1}\n",
        "= heading {a: 1}\n",
        "- #(id)\n",
        "::kind(x)\n",
    ] {
        file(src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    }
    let doc = file("@name\n| a\n").unwrap();
    assert!(matches!(&doc.body[0], Block::Element(el) if el.content.is_some()));
}

#[test]
fn fenced_code_block_line_markers_lower_identically_to_legacy() {
    use tomet_ast::Inline;

    let marked = "```(rust)\n`` fn main() {\n``     println!();\n`` }\n```\n";
    let legacy = "```(rust)\nfn main() {\n    println!();\n}\n```\n";

    let doc_marked = file(marked).expect("parse marked");
    let doc_legacy = file(legacy).expect("parse legacy");

    // Both spellings produce the exact same text content
    let extract_raw = |doc: &tomet_ast::File| -> String {
        match &doc.body[0] {
            Block::Element(el) => match &el.content.as_ref().unwrap()[0] {
                Block::Paragraph(p) => match &p.content[0] {
                    Inline::Raw(r) => r.value.clone(),
                    other => panic!("expected raw inline, got {other:?}"),
                },
                other => panic!("expected paragraph, got {other:?}"),
            },
            other => panic!("expected element, got {other:?}"),
        }
    };

    assert_eq!(extract_raw(&doc_marked), extract_raw(&doc_legacy));
    assert_eq!(extract_raw(&doc_marked), "fn main() {\n    println!();\n}");
}

#[test]
fn fenced_code_block_line_marker_rules() {
    use tomet_ast::Inline;

    let extract_raw = |src: &str| -> String {
        let doc = file(src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
        match &doc.body[0] {
            Block::Element(el) => match &el.content.as_ref().unwrap()[0] {
                Block::Paragraph(p) => match &p.content[0] {
                    Inline::Raw(r) => r.value.clone(),
                    other => panic!("expected raw inline, got {other:?}"),
                },
                other => panic!("expected paragraph, got {other:?}"),
            },
            other => panic!("expected element, got {other:?}"),
        }
    };

    // 1. Bare `` line is an empty line, and truly empty line is also allowed
    let src1 = "```\n`` first\n``\n\n`` last\n```\n";
    assert_eq!(extract_raw(src1), "first\n\n\nlast");

    // 2. Exactly one space is stripped as separator; second space is content
    let src2 = "```\n``  two spaces\n``   three spaces\n```\n";
    assert_eq!(extract_raw(src2), " two spaces\n  three spaces");

    // 3. Content starting with `` is written `` ``
    let src3 = "```\n`` `` code\n```\n";
    assert_eq!(extract_raw(src3), "`` code");

    // 4. Marked line holding ``` does not close fence
    let src4 = "```\n`` ```\n`` code\n```\n";
    assert_eq!(extract_raw(src4), "```\ncode");

    // 5. In a | run
    let src5 = "@memo\n| ```\n| `` hello\n| ```\n";
    let doc5 = file(src5).expect("parse in pipe run");
    match &doc5.body[0] {
        Block::Element(memo) => match &memo.content.as_ref().unwrap()[0] {
            Block::Element(raw) => match &raw.content.as_ref().unwrap()[0] {
                Block::Paragraph(p) => match &p.content[0] {
                    Inline::Raw(r) => assert_eq!(r.value, "hello"),
                    other => panic!("expected raw inline, got {other:?}"),
                },
                other => panic!("expected paragraph, got {other:?}"),
            },
            other => panic!("expected raw block, got {other:?}"),
        },
        other => panic!("expected memo, got {other:?}"),
    }
}

#[test]
fn an_unnamed_entry_written_twice_is_a_clash_at_its_position() {
    for (src, message) in [
        (
            "@memo(1)::(2)\n",
            "duplicate entry `0`: first written at 1:7",
        ),
        (
            "@memo(1)::(1)\n",
            "duplicate entry `0`: first written at 1:7",
        ),
        (
            "@memo(1, 2)::(3)\n",
            "duplicate entry `0`: first written at 1:7",
        ),
        (
            "@memo(a: (1))::(a: (2))\n",
            "duplicate entry `a.0`: first written at 1:11",
        ),
    ] {
        let e = refusal(file(src), src);
        assert_eq!(e.message, message, "{src:?}");
    }
}

#[test]
fn unnamed_entries_merge_when_both_are_tuples_or_only_one_side_has_them() {
    let doc = file("@memo({x: 1}, k: 0)::({y: 2}, 3)\n").unwrap();
    let Block::Element(el) = &doc.body[0] else {
        panic!()
    };
    let args = el.data.args.as_ref().unwrap();
    assert_eq!(args.len(), 3);
    let Some(Value::Tuple(first)) = args.nth_unnamed(0) else {
        panic!("{args:?}")
    };
    assert_eq!(first.get("x"), Some(&Value::Int(1)));
    assert_eq!(first.get("y"), Some(&Value::Int(2)));
    assert_eq!(args.get("k"), Some(&Value::Int(0)));
    assert_eq!(args.nth_unnamed(1), Some(&Value::Int(3)));
    // The first writer's entries stay as written; what only the second has comes after.
    assert_eq!(args.nth(1).and_then(|e| e.name()), Some("k"));
}

#[test]
fn merge_named_entries_disjoint_and_recursive() {
    let doc = file("@memo(a: {x: 1})::(b: 2)::(a: {y: 3})\n").unwrap();
    let Block::Element(el) = &doc.body[0] else {
        panic!()
    };
    let args = el.data.args.as_ref().unwrap();
    assert_eq!(args.len(), 2);
    let a_val = args.get("a").unwrap();
    match a_val {
        Value::Tuple(t) => {
            assert_eq!(t.get("x"), Some(&Value::Int(1)));
            assert_eq!(t.get("y"), Some(&Value::Int(3)));
        }
        other => panic!("expected tuple, got {other:?}"),
    }
    assert_eq!(args.get("b"), Some(&Value::Int(2)));
}

#[test]
fn a_group_that_holds_an_element_merges_its_pairs_and_keeps_every_entry() {
    let doc = file("@memo(){ a: 1, @x(1) }::{ b: 2, @y(2) }\n").unwrap();
    let Block::Element(el) = &doc.body[0] else {
        panic!()
    };
    let group = el.data.value.as_ref().unwrap();
    let written: Vec<String> = group
        .0
        .iter()
        .map(|entry| match entry {
            tomet_ast::Entry::Pair(key, _) => key.clone(),
            tomet_ast::Entry::Element(child) => format!("@{}", child.name.name),
            tomet_ast::Entry::Hole(_) => "?".to_string(),
        })
        .collect();
    assert_eq!(written, ["a", "@x", "b", "@y"]);

    let src = "@memo(){ a: 1, @x(1) }::{ a: 2 }\n";
    let e = refusal(file(src), src);
    assert_eq!(e.message, "duplicate key `a`: first written at 1:10");
}

#[test]
fn a_key_written_twice_is_a_clash_that_names_the_first_place() {
    for (src, message) in [
        (
            "@memo(a: 1)::(a: 1)\n",
            "duplicate key `a`: first written at 1:7",
        ),
        (
            "@memo(a: 1)::(a: 2)\n",
            "duplicate key `a`: first written at 1:7",
        ),
        (
            "@memo(a: (1, 2))::(a: 1)\n",
            "duplicate key `a`: first written at 1:7",
        ),
        (
            "@memo(a: list(x))::(a: list(x))\n",
            "duplicate key `a`: first written at 1:7",
        ),
        (
            "@memo(){ a: 1 }::{ a: 2 }\n",
            "duplicate key `a`: first written at 1:10",
        ),
        (
            "@memo(){ a: 1 }::meta(a: 2)\n",
            "duplicate key `a`: first written at 1:10",
        ),
        (
            "@memo(a: {x: 1})\n::(b: 2)\n::(a: {x: 3})\n",
            "duplicate key `a.x`: first written at 1:11",
        ),
        (
            "@memo(a: 1)::(b: 2)::(b: 3)\n",
            "duplicate key `b`: first written at 1:15",
        ),
    ] {
        let e = refusal(file(src), src);
        assert_eq!(e.message, message, "{src:?}");
    }
    let src = "@memo(a: 1)\n::(b: 2)\n::(b: 3)\n";
    let e = refusal(file(src), src).located_in(src);
    assert_eq!(
        (e.line, e.column),
        (3, 4),
        "the error is at the second writer"
    );
}
