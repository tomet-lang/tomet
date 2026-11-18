//! Documents, addresses and directories.

use super::*;

#[test]
fn an_id_is_found_wherever_the_node_sits() {
    // In a section's title, in `(args)` and in a hole's content: places a search over the blocks
    // and the `{value}` entries alone would miss.
    let resolver = MemoryResolver::new()
        .with_source(
            "here.tmt",
            "=[ T @a()#(in_title) ]\n\n@b(icon: @c()#(in_args))\n\n<?(h)>[@d()#(in_hole)]\n",
        )
        .unwrap();
    for (id, name) in [("in_title", "a"), ("in_args", "c"), ("in_hole", "d")] {
        assert_eq!(
            in_resolver(&resolver, &format!("#{id}::name")).unwrap(),
            text(name),
            "#{id}"
        );
    }
}

#[test]
fn a_leading_hash_picks_an_element_of_the_current_document() {
    assert_eq!(in_vault("#c1.n").unwrap(), int(1));
    assert_eq!(in_vault("#c1::id").unwrap(), text("c1"));
    assert_eq!(in_vault("#c1::name").unwrap(), text("card"));
}

#[test]
fn the_leading_hash_is_self_hash() {
    assert_eq!(in_vault("self#c1.n").unwrap(), in_vault("#c1.n").unwrap());
    assert_eq!(
        in_vault("#c1 == self#c1").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
}

#[test]
fn a_dot_reads_the_args_and_the_value_of_a_node_and_item_walks_a_list() {
    assert_eq!(in_vault("#c1.extra").unwrap(), text("yes"));
    assert_eq!(in_vault("#c1.tags.item(2)").unwrap(), text("z"));
    assert_eq!(in_vault("#c1.tags.len()").unwrap(), int(3));
}

/// `.prop` is a retired level between a node and its data; reading it says where to write what it
/// asked for.
#[test]
fn prop_is_retired_and_says_where_its_replacement_is() {
    for src in ["#c1.prop.n", "#c1.prop", "#c1.prop(\"n\")", "#intro.prop.n"] {
        let error = in_vault(src).unwrap_err();
        assert!(
            matches!(&error, EvalError::Retired { written, .. } if written == ".prop"),
            "{src}: {error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("`.prop` is retired")
                && message.contains("`.key`")
                && message.contains("`::`"),
            "{message}"
        );
    }
}

/// `@user(id: "u123", name: "Alice")`: the author's keys are the author's, and the node's own `id` and
/// `name` are a double colon away.
#[test]
fn a_key_called_id_or_name_is_the_authors_and_the_nodes_own_is_reached_with_a_double_colon() {
    let resolver = MemoryResolver::new()
        .with_source(
            "here.tmt",
            "@user(id: \"u123\", name: \"Alice\"){ tag: vip, level: 3 }#(u1)::rule(allow: (card))\n\n@user(age: 3)#(u2)\n",
        )
        .unwrap();
    let eval = |src: &str| in_resolver(&resolver, src);
    assert_eq!(eval("#u1.id").unwrap(), text("u123"));
    assert_eq!(eval("#u1.name").unwrap(), text("Alice"));
    assert_eq!(eval("#u1::id").unwrap(), text("u1"));
    assert_eq!(eval("#u1::name").unwrap(), text("user"));
    // The `{...}` is the node's `meta`, and its data is read with a dot.
    assert_eq!(eval("#u1::meta.tag").unwrap(), text("vip"));
    assert_eq!(eval("#u1::meta.level").unwrap(), int(3));
    assert!(matches!(
        eval("#u1::meta").unwrap(),
        EvalValue::Tuple(tuple) if tuple.len() == 2
    ));
    assert_eq!(eval("#u2::meta").unwrap(), EvalValue::Tuple(Tuple::new()));
    // A connect stacked after the node hangs off it by its name.
    assert_eq!(eval("#u1::rule.allow.item(0)").unwrap(), text("card"));
    assert!(matches!(
        eval("#u1::connects.len()").unwrap(),
        EvalValue::Scalar(Scalar::Int(1))
    ));
    assert_eq!(eval("#u1::doc::path").unwrap(), text("here.tmt"));
    // A node without that connect, or that key, is a mistake and says which.
    assert_eq!(
        eval("#u2::rule"),
        Err(EvalError::UnknownMember {
            on: "element",
            member: "rule".to_string()
        })
    );
    let no_key = eval("#u2.id").unwrap_err().to_string();
    assert!(
        no_key.contains("no data key `id`") && no_key.contains("`::id`"),
        "{no_key}"
    );
    // The dot is not for a map's node attributes, and the colons are not for a map's keys.
    let not_a_node = eval("#u1::meta::tag").unwrap_err().to_string();
    assert!(
        not_a_node.contains("not a node") && not_a_node.contains(".tag"),
        "{not_a_node}"
    );
    // A method is called with a dot.
    let not_a_method = eval("#u1::link()").unwrap_err().to_string();
    assert!(not_a_method.contains("not a method"), "{not_a_method}");
}

/// A document and a directory hold no data of their own; what they have hangs off them.
#[test]
fn the_members_a_document_and_a_directory_had_with_a_dot_are_retired_to_the_double_colon() {
    for (src, member) in [
        ("self.path", ".path"),
        ("self.meta", ".meta"),
        ("tmt(\"docs/a.tmt\").kind", ".kind"),
        ("tmt(\"docs/a.tmt\").filename", ".filename"),
        ("vault.docs", ".docs"),
        ("addr(dir:docs).children", ".children"),
    ] {
        let error = in_vault(src).unwrap_err();
        assert!(
            matches!(&error, EvalError::Retired { written, .. } if written == member),
            "{src}: {error:?}"
        );
        assert!(
            error.to_string().contains(&format!("`::{}`", &member[1..])),
            "{error}"
        );
    }
    assert_eq!(
        in_vault("self::id").unwrap(),
        EvalValue::Scalar(Scalar::None)
    );
    assert_eq!(
        in_vault("tmt(\"docs/a.tmt\")::doc::path").unwrap(),
        text("docs/a.tmt")
    );
    assert_eq!(in_vault("self::doc::meta.title").unwrap(), text("Here"));
}

#[test]
fn a_heading_is_found_by_its_id_and_is_a_section() {
    assert_eq!(in_vault("#intro::id").unwrap(), text("intro"));
    assert_eq!(in_vault("#intro::name").unwrap(), text("section"));
    // The element that follows the heading is under it.
    assert!(matches!(in_vault("#intro::content").unwrap(), EvalValue::Fragment(b) if b.len() == 2));
}

#[test]
fn a_document_on_the_left_picks_an_element_of_that_document() {
    assert_eq!(in_vault("tmt(\"docs/a.tmt\")#top.x").unwrap(), int(7));
    assert_eq!(in_vault("ref(\"a\")#top.x").unwrap(), int(7));
    // A leading `./` names the same document.
    assert_eq!(in_vault("tmt(\"./docs/a.tmt\")#top.x").unwrap(), int(7));
}

#[test]
fn a_missing_document_or_id_is_reported_with_what_was_asked_for() {
    let err = in_vault("tmt(\"nope.tmt\")#x").unwrap_err();
    assert!(
        matches!(&err, EvalError::Unresolved(m) if m.contains("nope.tmt")),
        "{err}"
    );
    let err = in_vault("#missing").unwrap_err();
    assert!(
        matches!(&err, EvalError::Unresolved(m) if m.contains("`missing`") && m.contains("here.tmt")),
        "{err}"
    );
    assert!(matches!(
        in_vault("ref(\"zzz\")"),
        Err(EvalError::Unresolved(_))
    ));
}

#[test]
fn a_hash_needs_a_document_on_its_left() {
    assert!(matches!(
        in_vault("1#x"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn document_members() {
    assert_eq!(in_vault("self::path").unwrap(), text("here.tmt"));
    assert_eq!(in_vault("self::filename").unwrap(), text("here.tmt"));
    assert_eq!(
        in_vault("tmt(\"docs/sub/b.tmt\")::filename").unwrap(),
        text("b.tmt")
    );
    assert_eq!(
        in_vault("tmt(\"./docs/a.tmt\")::path").unwrap(),
        text("docs/a.tmt")
    );
    assert_eq!(in_vault("self::meta.title").unwrap(), text("Here"));
    assert_eq!(in_vault("self::kind").unwrap(), text("note"));
    assert_eq!(
        in_vault("tmt(\"docs/a.tmt\")::kind").unwrap(),
        EvalValue::Scalar(Scalar::None)
    );
    assert_eq!(
        in_vault("self::source").unwrap(),
        text(HERE.strip_suffix('\n').unwrap())
    );
    assert_eq!(
        in_vault("tmt(\"docs/a.tmt\")::source").unwrap(),
        text(DOC_A.strip_suffix('\n').unwrap())
    );
    assert_eq!(
        in_vault("self.source"),
        Err(EvalError::Retired {
            written: ".source".to_string(),
            instead: "`.` reads the data keys of a node, and `::` reaches what hangs off it; write `::source`".to_string(),
        })
    );
    assert_eq!(
        in_vault("self.nope"),
        Err(EvalError::UnknownMember {
            on: "document",
            member: "nope".to_string()
        })
    );
}

#[test]
fn document_source_edge_cases_and_trailing_line_breaks() {
    let resolver = MemoryResolver::new()
        .with_source("lf.tmt", "one\ntwo\n")
        .unwrap()
        .with_source("crlf.tmt", "one\r\ntwo\r\n")
        .unwrap()
        .with_source("none.tmt", "one\ntwo")
        .unwrap()
        .with_source("double_lf.tmt", "one\ntwo\n\n")
        .unwrap()
        .with_source("double_crlf.tmt", "one\r\ntwo\r\n\r\n")
        .unwrap()
        .with_source("empty.tmt", "")
        .unwrap();

    assert_eq!(
        in_resolver(&resolver, "tmt(\"lf.tmt\")::source").unwrap(),
        text("one\ntwo")
    );
    assert_eq!(
        in_resolver(&resolver, "tmt(\"crlf.tmt\")::source").unwrap(),
        text("one\r\ntwo")
    );
    assert_eq!(
        in_resolver(&resolver, "tmt(\"none.tmt\")::source").unwrap(),
        text("one\ntwo")
    );
    assert_eq!(
        in_resolver(&resolver, "tmt(\"double_lf.tmt\")::source").unwrap(),
        text("one\ntwo\n")
    );
    assert_eq!(
        in_resolver(&resolver, "tmt(\"double_crlf.tmt\")::source").unwrap(),
        text("one\r\ntwo\r\n")
    );
    assert_eq!(
        in_resolver(&resolver, "tmt(\"empty.tmt\")::source").unwrap(),
        text("")
    );

    // Document that cannot be loaded/found is an Unresolved error
    assert_eq!(
        in_resolver(&resolver, "tmt(\"missing.tmt\")::source"),
        Err(EvalError::Unresolved(
            "document `missing.tmt`: not found".to_string()
        ))
    );

    // Elements do not have ::source (documents only)
    assert_eq!(
        in_vault("#c1::source"),
        Err(EvalError::UnknownMember {
            on: "element",
            member: "source".to_string(),
        })
    );
}

#[test]
fn the_meta_a_resolver_gives_is_what_is_read() {
    // The in-memory resolver applies no settings, so a field the document
    // lacks is absent; the vault's resolver makes it `None` or `[]`.
    assert!(matches!(
        in_vault("tmt(\"docs/sub/b.tmt\")::meta.tags"),
        Err(EvalError::UnknownMember { on: "tuple", .. })
    ));
}

#[test]
fn root_items_are_numbered_from_zero_and_a_section_is_a_node() {
    assert_eq!(in_vault("#item(0)::name").unwrap(), text("card"));
    assert_eq!(in_vault("self#item(1)::id").unwrap(), text("intro"));
    assert_eq!(in_vault("self.item(1)::name").unwrap(), text("section"));
    assert!(in_vault("#item(99)").is_err());
}

/// `item(n)` counts blocks -- a paragraph is `item(0)` here, so the element
/// after it is `item(1)` -- and not the blank lines between them
/// (`tmtroot/spec/syntax/addressable.tmt`, 位置による番地).
#[test]
fn item_on_a_node_walks_its_children() {
    assert!(matches!(
        in_vault("#intro.item(0)").unwrap(),
        EvalValue::Fragment(_)
    ));
    assert_eq!(in_vault("#intro.item(1)::id").unwrap(), text("c1"));
}

#[test]
fn a_node_has_only_its_own_members() {
    assert_eq!(
        in_vault("#c1.nope"),
        Err(EvalError::UnknownMember {
            on: "element",
            member: "nope".to_string()
        })
    );
    assert_eq!(in_vault("#c1.n").unwrap(), int(1));
}

#[test]
fn a_directory_lists_the_documents_under_it() {
    assert_eq!(
        in_vault("vault::docs.map(d => d::path)").unwrap(),
        seq(vec![
            text("docs/a.tmt"),
            text("docs/sub/b.tmt"),
            text("here.tmt")
        ])
    );
    assert_eq!(in_vault("addr(dir:docs)::docs.len()").unwrap(), int(2));
    assert_eq!(
        in_vault("addr(dir:./docs/sub)::docs.len()").unwrap(),
        int(1)
    );
}

#[test]
fn children_are_the_documents_directly_in_the_directory() {
    // `docs` goes down into `docs/sub`; `children` does not.
    assert_eq!(
        in_vault("addr(dir:docs)::children.map(d => d::path)").unwrap(),
        seq(vec![text("docs/a.tmt")])
    );
    assert_eq!(
        in_vault("vault::children.map(d => d::path)").unwrap(),
        seq(vec![text("here.tmt")])
    );
    assert_eq!(
        in_vault("addr(dir:docs/sub)::children.len()").unwrap(),
        int(1)
    );
}

#[test]
fn addr_reads_a_link_style_address() {
    assert_eq!(in_vault("addr(tmt:docs/a.tmt#top.x)").unwrap(), int(7));
    assert_eq!(
        in_vault("addr(tmt:docs/a.tmt)").unwrap(),
        EvalValue::Document(DocRef("docs/a.tmt".to_string()))
    );
    assert_eq!(in_vault("addr(#c1).n").unwrap(), int(1));
    assert_eq!(in_vault("addr(ref:a#top.x)").unwrap(), int(7));
    assert_eq!(in_vault("addr(\"tmt:docs/a.tmt#top.x\")").unwrap(), int(7));
    // The steps after `#` go as far as an expression after `#` goes.
    assert_eq!(in_vault("addr(#c1.tags.item(1))").unwrap(), text("y"));
}

#[test]
fn addr_rejects_what_is_not_an_address() {
    assert!(matches!(
        in_vault("addr(docs/a.tmt)"),
        Err(EvalError::BadAddress(_))
    ));
    assert!(matches!(
        in_vault("addr(wat:x)"),
        Err(EvalError::BadAddress(_))
    ));
    assert!(matches!(
        in_vault("addr(https://example.com/x)"),
        Err(EvalError::NotYetSupported(_))
    ));
    assert!(in_vault("addr(dir:docs#x)").is_err());
}

#[test]
fn a_string_known_only_at_run_time_is_read_with_dot_addr() {
    assert_eq!(in_vault("\"tmt:docs/a.tmt#top.x\".addr()").unwrap(), int(7));
    assert_eq!(in_vault("\"#c1.n\".addr()").unwrap(), int(1));
    assert!(matches!(
        in_vault("\"not an address\".addr()"),
        Err(EvalError::BadAddress(_))
    ));
    assert!(matches!(
        in_vault("1.addr()"),
        Err(EvalError::UnknownMember { .. })
    ));
}

#[test]
fn an_address_inside_a_lambda_still_resolves_against_the_right_document() {
    // `#top` inside `addr(...)` is resolved against `docs/a.tmt`, not `self`,
    // even though the lambda runs while `self` is `here.tmt`.
    assert_eq!(
        in_vault("vault::docs.filter(d => d::path == \"docs/a.tmt\").map(d => d#top.x)").unwrap(),
        seq(vec![int(7)])
    );
}
