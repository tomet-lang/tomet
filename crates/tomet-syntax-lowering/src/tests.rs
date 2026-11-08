//! Lowering refuses a nexus -- `@name`, `::name`, `:name` -- that has no primary group, `(args)`
//! or `[content]`, at the offset where the nexus begins; the tree it reads is the CST's, whole.

use crate::{Error, lower_data_file, lower_declaration, lower_file, lower_value};
use tomet_ast::{Block, Value};
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
        ("@name:rule(a)\n", 0),
        ("@name:{a: 1}\n", 0),
        ("@name:(a: 1)\n", 0),
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
    // A nameless `:(...)` would otherwise be read as the declaration's own args.
    for src in [
        "::name:(a: 1)\n",
        "::name:{a: 1}\n",
        "::name(a):{b: 1}\n",
        "::name():rule(a)\n",
    ] {
        let e = refusal(file(src), src);
        assert!(
            e.message.contains("takes no `:connect`"),
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
        ("::meta:(a: 1)\n", "takes no `:connect`"),
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
        ("k: @name:rule(a)\n", 3),
        ("k: @name:{a: 1}\n", 3),
        ("a: [ x @name{} ]\n", 7),
        ("k: list(@name{})\n", 8),
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
        Value::Map(vec![("k".into(), Value::String("@name".into()))])
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
        ("@el(a):rule{x: 1}\n", 6),
        ("@el(a):rule{}\n", 6),
        ("-#(id) :rule{}\n", 7),
        ("=#(id) :rule{}\n", 7),
        // A bracket-less body claims the connect all the same.
        ("- item :rule{a: 1}\n", 7),
        ("= H :rule{a: 1}\n", 4),
    ] {
        let e = refusal(file(src), src);
        assert!(
            e.message.contains("`:rule` needs a primary group"),
            "{src:?}: {}",
            e.message
        );
        assert_eq!(e.offset, at, "{src:?}");
    }
}

#[test]
fn a_nameless_connect_cannot_hold_content() {
    let e = refusal(file("@el(a):[x]\n"), "@el(a):[x]");
    assert!(e.message.contains("nameless"), "{}", e.message);
    assert_eq!(e.offset, 7);
}

#[test]
fn what_has_a_primary_group_lowers() {
    for src in [
        "@name()\n",
        "@name[ x ]\n",
        "@name\n| a\n",
        "@name(a)[x]{k: 1}#(id):rule(b)\n",
        "@el(a):{b: 1}\n",
        "@el(a):(b: 1)\n",
        "ping @alice\n",
        "- item :{a: 1}\n",
        "= heading {a: 1}\n",
        "- #(id)\n",
        "::kind(x)\n",
    ] {
        file(src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    }
    let doc = file("@name\n| a\n").unwrap();
    assert!(matches!(&doc.body[0], Block::Element(el) if el.content.is_some()));
}
