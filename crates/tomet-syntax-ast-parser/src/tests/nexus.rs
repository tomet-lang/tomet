//! A nexus -- `@name`, a `::name` declaration or connect -- stands on a primary group, `(args)` or `[content]`
//! (a `|` run is the latter). `{value}`, `#(id)` and a `::connect` only add to one.
//!
//! Every shape here is one of: refused where the nexus begins, kept as running text, or valid.

use super::*;

/// The message every nexus without a primary group is refused with.
const NEEDS_PRIMARY: &str = "needs a primary group";
/// What a `::` declaration says to any `::connect`.
const NO_CONNECT: &str = "takes no `::connect`";

/// Each source is refused with `message`, at the byte offset beside it.
fn refused(message: &str, cases: &[(&str, usize)]) {
    for (src, at) in cases {
        let err = parse_file(src).expect_err(src);
        assert!(err.message.contains(message), "{src:?}: {}", err.message);
        assert_eq!(err.offset, *at, "{src:?}: {}", err.message);
    }
}

fn ok(src: &str) -> tomet_ast::File {
    parse_file(src).unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

fn element(block: &Block) -> &Element {
    let Block::Element(el) = block else {
        panic!("expected an element block, got {block:?}")
    };
    el
}

// --- a block `@name` ------------------------------------------------------

#[test]
fn a_block_name_alone_or_with_only_secondary_groups_is_refused() {
    refused(
        NEEDS_PRIMARY,
        &[
            ("@name\n", 0),
            ("@name", 0),
            ("@name{}\n", 0),
            ("@name{a: 1}\n", 0),
            ("@name#(id)\n", 0),
            ("@name{}#(id)\n", 0),
            ("@name#(id){a: 1}\n", 0),
            ("@ns.name{}\n", 0),
            // Behind a blank line, and after other blocks: still the name's own offset.
            ("text\n\n@name{}\n", 6),
            ("@name{}\n\n@fine()\n", 0),
            ("@fine()\n\n@name\n", 9),
            // Groups on the lines after it are its groups all the same.
            ("@name\n{a: 1}\n", 0),
            ("@name\n#(id)\n", 0),
            ("@name\n::{a: 1}\n", 0),
        ],
    );
}

#[test]
fn a_connect_is_not_a_primary_group() {
    // A named connect has groups of its own, and a nameless one merges into its owner's: neither
    // is the owner's `(args)` or `[content]`, so the owner has none.
    refused(
        NEEDS_PRIMARY,
        &[
            ("@name::rule(a: 1)\n", 0),
            ("@name::rule[x]\n", 0),
            ("@name::rule(a: 1)::other[x]\n", 0),
            ("@name::{a: 1}\n", 0),
            ("@name::{}\n", 0),
            ("@name::(a: 1)\n", 0),
            ("@name::()\n", 0),
            ("@name{}::(a: 1)\n", 0),
            ("@name #(id) ::{a: 1}\n", 0),
        ],
    );
}

#[test]
fn the_refusal_names_what_cannot_stand_alone() {
    let err = parse_file("@name{a: 1}#(id)\n").unwrap_err();
    assert!(err.message.contains("`@name`"), "{}", err.message);
    assert!(err.message.contains("`{...}`"), "{}", err.message);
    assert!(err.message.contains("`#(id)`"), "{}", err.message);
    let err = parse_file("@name\n").unwrap_err();
    assert!(err.message.contains("`@name()`"), "{}", err.message);
}

// --- an inline `@name` ----------------------------------------------------

#[test]
fn an_inline_name_with_a_secondary_group_is_refused_and_not_text() {
    refused(
        NEEDS_PRIMARY,
        &[
            ("a @name{} b\n", 2),
            ("a @name{k: 1} b\n", 2),
            ("a @name#(id) b\n", 2),
            ("a @name{}#(id) b\n", 2),
            ("a @name::rule(a: 1) b\n", 2),
            ("a @name::{k: 1} b\n", 2),
            ("a @name::() b\n", 2),
            ("a @name::(k: 1) b\n", 2),
            // Wherever running text goes.
            ("*em @name{}*\n", 4),
            ("- a @name{}\n", 4),
            ("=[ @name{} ]\n", 3),
            ("@box[ @name{} ]\n", 6),
            ("@box[\n  a @name#(id) b\n]\n", 10),
            ("@quote\n| @name{}\n", 9),
        ],
    );
}

#[test]
fn a_bare_inline_name_is_running_text() {
    for src in [
        "ping @alice now\n",
        "foo@bar.com\n",
        "mail me @ home\n",
        "@alice\nsaid hello\n",
        "said hello\n@alice\n",
        "@alice and @bob\n",
        "@alice: hello\n",
        // A `::name` with no group after it is not a connect, so the name has nothing.
        "cc @alice:bob\n",
        "user@host:22\n",
        "@alice // note\nsaid\n",
        "*em @alice*\n",
        "- ping @alice\n",
        "= ping @alice\n",
        "@box[ ping @alice ]\n",
    ] {
        let mut found = 0;
        tomet_tree::for_each_element(&ok(src), |el| {
            if el.name != Name::bare("box") && el.name.name != "em" {
                found += 1;
            }
        });
        assert_eq!(found, 0, "{src:?} should hold no element");
    }
    // The text is kept whole.
    let doc = ok("ping @alice now\n");
    let Block::Paragraph(p) = &doc.body[0] else {
        panic!("expected a paragraph, got {:?}", doc.body[0])
    };
    assert_eq!(&p.content, &vec![Inline::Text("ping @alice now".into())]);
}

#[test]
fn a_bare_name_in_a_value_is_a_string() {
    let doc = ok("@el(a: @name)\n");
    assert_eq!(
        element(&doc.body[0]).data.args,
        Some(Value::Map(vec![(
            "a".into(),
            Value::String("@name".into())
        )]))
    );
}

#[test]
fn a_name_with_only_a_secondary_group_in_a_value_is_refused() {
    refused(
        NEEDS_PRIMARY,
        &[
            ("@el(a: @name{})\n", 7),
            ("@el(){ @name{} }\n", 7),
            ("@el(){ k: @name#(id) }\n", 10),
            ("@el(a: @name::rule(b))\n", 7),
        ],
    );
}

// --- the combine notation -------------------------------------------------

#[test]
fn a_combine_item_needs_a_primary_group_too() {
    refused(
        NEEDS_PRIMARY,
        &[
            ("-@name\n", 1),
            ("-@name text\n", 1),
            ("-@name{}\n", 1),
            ("-@name#(id)\n", 1),
            ("-@name::rule(a)\n", 1),
            ("-@name::{k: 1}\n", 1),
            ("-.@name{}\n", 2),
            ("- @name{}\n", 2),
        ],
    );
    // `- @name` with a space is an ordinary item that happens to mention a name.
    let doc = ok("- @name\n");
    assert!(matches!(&doc.body[0], Block::List(_)));
}

#[test]
fn a_combine_item_with_a_primary_group_is_valid() {
    for src in [
        "-@name()\n",
        "-@name(a: 1) text\n",
        "-@name[ x ]\n",
        "-@name()#(id)\n",
        "-@name(){k: 1}\n",
        "-.@name()\n",
    ] {
        let doc = ok(src);
        let Block::List(list) = &doc.body[0] else {
            panic!("{src:?}: expected a list, got {:?}", doc.body[0])
        };
        assert_eq!(list.items[0].name, Some(Name::bare("name")), "{src:?}");
    }
}

// --- a `::name` declaration -------------------------------------------------

#[test]
fn a_declaration_needs_a_primary_group() {
    refused(
        NEEDS_PRIMARY,
        &[
            ("::name\n", 0),
            ("::name{}\n", 0),
            ("::name{a: 1}\n", 0),
            ("::name#(id)\n", 0),
            ("::name{}#(id)\n", 0),
            ("::meta{}\n", 0),
            ("::meta{ a: 1 }\n", 0),
            ("::kind(x)\n::meta{ a: 1 }\n", 10),
        ],
    );
}

#[test]
fn a_declaration_takes_no_connect_of_either_kind() {
    // The nameless ones would merge into the args and pass for them; they are checked for
    // before any group is read.
    refused(
        NO_CONNECT,
        &[
            ("::name:(a: 1)\n", 0),
            ("::name:{a: 1}\n", 0),
            ("::name:{}\n", 0),
            ("::name:()\n", 0),
            ("::name(a)::{b: 1}\n", 0),
            ("::name(a)::(b: 1)\n", 0),
            ("::name(a)::rule(b)\n", 0),
            ("::name:rule(a)\n", 0),
            ("::name[x]::rule[y]\n", 0),
            ("::kind(config)\n:rule(allow:(k))\n", 0),
        ],
    );
}

#[test]
fn a_declaration_with_a_primary_group_is_valid() {
    for src in [
        "::name()\n",
        "::name(a: 1)\n",
        "::name[ x ]\n",
        "::name(a)[ x ]{ c: 1 }#(i)\n",
        "::kind(config)\n",
        "::meta()\n",
        "::meta(id: x, title: T)\n",
        // The ones that keep a `{}` body have a `()` before it.
        "::vocabulary(ns){ a: 1 }\n",
        "::blueprint(x){ a: 1 }\n",
    ] {
        assert_eq!(ok(src).header.len(), 1, "{src:?}");
    }
    let doc = ok("::vocabulary(ns){ a: 1 }\n");
    assert!(doc.header[0].data.args.is_some());
    assert!(doc.header[0].data.value.is_some());
}

#[test]
fn a_data_file_header_is_held_to_the_same_rule() {
    // A cotomet `.ctmt` header is a run of declarations.
    for (src, message, at) in [
        ("::meta\nk: v\n", NEEDS_PRIMARY, 0),
        ("::meta{ a: 1 }\nk: v\n", NEEDS_PRIMARY, 0),
        ("::meta{}\n", NEEDS_PRIMARY, 0),
        ("::kind(x)\n::var{ a: 1 }\nk: v\n", NEEDS_PRIMARY, 10),
        ("::meta:(a: 1)\nk: v\n", NO_CONNECT, 0),
        ("::meta(a: 1)::{b: 2}\nk: v\n", NO_CONNECT, 0),
    ] {
        let err = parse_data_file(src).expect_err(src);
        assert!(err.message.contains(message), "{src:?}: {}", err.message);
        assert_eq!(err.offset, at, "{src:?}");
        assert_eq!((err.line, err.column), {
            let l = tomet_position::LineIndex::new(src).line_col(at);
            (l.line, l.column)
        });
    }
    for src in [
        "::meta()\nk: v\n",
        "::meta(a: 1)\nk: v\n",
        "::kind(x)\n::macro(a: 1)\n::var(b: 2)\nk: v\n",
        "::meta(a: 1)\n",
    ] {
        let file = parse_data_file(src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
        assert!(!file.header.is_empty(), "{src:?}");
    }
}

// --- a named connect --------------------------------------------------------

#[test]
fn a_named_connect_needs_a_primary_group() {
    refused(
        NEEDS_PRIMARY,
        &[
            ("@el(a)::rule{x: 1}\n", 6),
            ("@el(a)::rule{}\n", 6),
            ("@el[x]::rule{x: 1}\n", 6),
            ("@el(a)::ok(b)::rule{}\n", 13),
            // The full form of an item or a heading claims its connect outright.
            ("-#(id) ::rule{}\n", 7),
            ("- {a: 1} ::rule{}\n", 9),
            ("=#(id) ::rule{}\n", 7),
            ("=[ T ]::rule{x: 1}\n", 6),
            ("- [ x ]::rule{x: 1}\n", 7),
        ],
    );
}

#[test]
fn a_named_connect_with_a_primary_group_is_valid() {
    for src in [
        "@el(a)::rule(b)\n",
        "@el(a)::rule[x]\n",
        "@el(a)::rule(b){c: 1}\n",
        "@el(a)::rule(b)::other[x]\n",
        "- {a: 1} ::rule(b)\n",
        "=[ T ]::rule(b)\n",
    ] {
        ok(src);
    }
    let doc = ok("@el(a)::rule[x]::other(b)\n");
    assert_eq!(element(&doc.body[0]).connects.len(), 2);
}

#[test]
fn a_named_connect_on_a_bracket_less_line_is_refused_like_one_on_a_full_form_head() {
    // A bracket-less body claims a trailing connect that is structurally one, so the shorthand
    // and the full form (`-#(id) ::rule{}`) agree: neither downgrades to text.
    refused(
        NEEDS_PRIMARY,
        &[
            ("- item ::rule{a: 1}\n", 7),
            ("- item ::rule{}\n", 7),
            ("= H ::rule{a: 1}\n", 4),
            ("= H ::rule{}\n", 4),
            ("-. item ::rule{a: 1}\n", 8),
            ("- item {a: 1} ::rule{}\n", 14),
            ("- item {a: 1}::rule{b: 2}\n", 13),
            ("- item ::ok(a) ::rule{b: 2}\n", 15),
            // The full forms, for the same answer.
            ("-#(id) ::rule{a: 1}\n", 7),
            ("=#(id) ::rule{a: 1}\n", 7),
        ],
    );
    let err = parse_file("- item ::rule{a: 1}\n").unwrap_err();
    assert!(err.message.contains("`::rule`"), "{}", err.message);
}

#[test]
fn a_named_connect_with_a_primary_group_on_a_bracket_less_line_is_a_connect() {
    for (src, connects) in [
        ("- item ::rule(a: 1)\n", 1),
        ("- item ::rule(a: 1){b: 2}\n", 1),
        ("- item ::rule(a: 1) ::other(b)\n", 2),
        ("= H ::rule(a: 1)\n", 1),
        ("- item ::{a: 1}\n", 0),
        ("= H ::(a)\n", 0),
    ] {
        let doc = ok(src);
        let got = match &doc.body[0] {
            Block::List(l) => l.items[0].connects.len(),
            Block::Section(s) => s.connects.len(),
            other => panic!("{src:?}: {other:?}"),
        };
        assert_eq!(got, connects, "{src:?}");
    }
}

#[test]
fn prose_that_looks_a_little_like_a_connect_stays_prose() {
    // A connect is `:name` and then its group, with no white space between: the `:` of a label
    // (`Note: {x}`), a `:` between words, and a `{...}` that is not a map are all text.
    for src in [
        "- Note: {x}\n",
        "- Note: {a: 1}\n",
        "- note: a:b\n",
        "- foo:bar{not a map}\n",
        "- a ::b{c}\n",
        "= Note: {a: 1}\n",
        "= note: a:b\n",
        "= foo:bar{not a map}\n",
        "- see http://x.y/z{a: 1}\n",
    ] {
        let doc = ok(src);
        let (connects, data) = match &doc.body[0] {
            Block::List(l) => (l.items[0].connects.len(), l.items[0].data.clone()),
            Block::Section(s) => (s.connects.len(), s.data.clone()),
            other => panic!("{src:?}: {other:?}"),
        };
        assert_eq!(connects, 0, "{src:?}");
        assert_eq!(
            data,
            tomet_ast::Data::default(),
            "{src:?}: its text, not its groups"
        );
    }
    let doc = ok("- Note: {a: 1}\n");
    let Block::List(list) = &doc.body[0] else {
        panic!("expected a list")
    };
    assert_eq!(
        list.items[0].content,
        wrap(vec![Inline::Text("Note: {a: 1}".into())]).unwrap()
    );
}

#[test]
fn a_nameless_connect_is_refused_when_it_holds_content() {
    let err = parse_file("@el(a)::[x]\n").unwrap_err();
    assert!(err.message.contains("nameless"), "{}", err.message);
    assert_eq!(err.offset, 8, "at the `[`");
}

#[test]
fn a_named_connect_alone_is_not_a_connect() {
    // With nothing after the name it cannot be told from text (`:smile:`), so it is text.
    for src in [
        "@el(a):rule\n",
        "@el(a) :rule\n",
        ":rule\n",
        "@el(a)\n:rule\n",
    ] {
        let doc = ok(src);
        let mut connects = 0;
        tomet_tree::for_each_element(&doc, |el| connects += el.connects.len());
        assert_eq!(connects, 0, "{src:?}");
    }
}

// --- what stays valid ---------------------------------------------------------

#[test]
fn nameless_connects_keep_working_on_a_valid_owner() {
    let doc = ok("@el(a: 1)::{b: 2}\n");
    let el = element(&doc.body[0]);
    assert!(el.data.args.is_some());
    assert_eq!(
        el.data.value,
        Some(Group::from_map(Value::Map(vec![(
            "b".into(),
            Value::Int(2)
        )])))
    );
    assert!(el.connects.is_empty());

    let doc = ok("@el(a: 1)::(b: 2)\n");
    // The merged-in args come first, then the owner's own.
    let Some(Value::Map(mut args)) = element(&doc.body[0]).data.args.clone() else {
        panic!("expected merged args")
    };
    args.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        args,
        vec![("a".into(), Value::Int(1)), ("b".into(), Value::Int(2))]
    );

    for src in [
        "- item ::{a: 1}\n",
        "- item ::(a)\n",
        "= heading ::{a: 1}\n",
        "= heading ::(a)\n",
        "= heading\n::(a)\n",
        "@el[x]::{a: 1}\n",
        "@el()\n::{a: 1}\n",
    ] {
        ok(src);
    }
}

#[test]
fn a_pipe_run_is_a_primary_group() {
    for src in [
        "@table\n| a | b\n",
        "@table\n| a | b\n| c | d\n",
        "@name| text\n",
        "@name\n  | text\n",
        "@quote\n| a\n",
        // A comment between the name and the run does not detach the run.
        "@table // note\n| text\n",
        "@table /* note */\n| text\n",
    ] {
        let doc = ok(src);
        let el = element(&doc.body[0]);
        assert!(el.content.is_some(), "{src:?}");
    }
    // The run is the name's content, so a name beside text that is not a run is still bare.
    refused(NEEDS_PRIMARY, &[("@table\n\n| a | b\n", 0)]);
}

#[test]
fn forms_that_are_not_nexuses_need_no_primary_group() {
    let doc = ok("- #(id)\n");
    let Block::List(list) = &doc.body[0] else {
        panic!("expected a list")
    };
    assert_eq!(list.items[0].id, Some(tomet_ast::Id("id".into())));

    let doc = ok("- {a: 1}\n");
    let Block::List(list) = &doc.body[0] else {
        panic!("expected a list")
    };
    assert!(list.items[0].data.value.is_some());

    let Block::Section(s) = &ok("= heading {a: 1}\n").body[0] else {
        panic!("expected a section")
    };
    assert!(s.data.value.is_some());

    let Block::Section(s) = &ok("==#(id)\n").body[0] else {
        panic!("expected a section")
    };
    assert_eq!(s.level, 2);
    assert_eq!(s.id, Some(tomet_ast::Id("id".into())));
}

#[test]
fn a_nexus_with_a_primary_group_takes_any_secondary_ones() {
    for src in [
        "@name()\n",
        "@name[]\n",
        "@name(){}\n",
        "@name()#(id)\n",
        "@name(){a: 1}#(id)\n",
        "@name{a: 1}()\n",
        "@name#(id)[ x ]\n",
        "@name(a)[x]{k: 1}#(id)::rule(b)\n",
        "a @name() b\n",
        "a @name[ x ] b\n",
        "@el(a: @name())\n",
    ] {
        ok(src);
    }
}

#[test]
fn a_bare_hr_is_refused_and_its_spellings_are_not() {
    refused(NEEDS_PRIMARY, &[("@hr\n", 0), ("text\n\n@hr\n", 6)]);
    for src in ["@hr()\n", "---\n", "---[ title ]---\n"] {
        let doc = ok(src);
        assert_eq!(element(&doc.body[0]).name, Name::bare("hr"), "{src:?}");
    }
}
