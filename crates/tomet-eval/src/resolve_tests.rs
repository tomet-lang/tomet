use std::collections::HashMap;

use tomet_ast::{Block, File, Inline, Tuple, TupleEntry, Value};
use tomet_expr::{DocRef, EvalContext, EvalValue, MemoryResolver, NoResolver, Resolver};
use tomet_scalar::Scalar;

use crate::{resolve_expressions, resolve_expressions_where};
use tomet_transform::{Unresolved, UnresolvedKind};

const NOTE: &str = "::meta(
  title: Note
  tags: (a, b)
)

=[ Heading ]#(top)

Body.

@card(n: 1){ extra: yes }#(c1)
";

fn vault() -> MemoryResolver {
    MemoryResolver::new()
        .with_source("here.tmt", NOTE)
        .unwrap()
        .with_source("docs/a.tmt", "::meta(title: A)\n")
        .unwrap()
        .with_source("docs/b.tmt", "::meta(title: B)\n")
        .unwrap()
}

/// The macro `name` defined by `entry`, e.g. `card: [ ... ]` or `generated_by: @comment[ ... ]`.
fn macro_entry(entry: &str) -> (String, Value) {
    let src = format!("::macro({entry})\n");
    let file = tomet_ast_parser::parse_file(&src).expect("the macro parses");
    let Some(tuple) = &file.header[0].data.args else {
        panic!("expected `(name: template)` entries");
    };
    let item = tuple.nth(0).expect("macro entry");
    (
        item.name().expect("named entry").to_string(),
        item.value().clone(),
    )
}

/// The block macro `name` whose body is `body`, as `::macro(name: [ body ])` defines it.
fn block_macro(name: &str, body: &str) -> Value {
    macro_entry(&format!("{name}: [{body}]")).1
}

/// Resolves `src` as the document `here.tmt` of the small vault above.
fn run_with(src: &str, vars: &EvalValue) -> (File, Vec<Unresolved>) {
    let resolver = vault();
    run_in(src, &resolver, vars)
}

fn run_in(src: &str, resolver: &dyn Resolver, vars: &EvalValue) -> (File, Vec<Unresolved>) {
    let mut macros = HashMap::new();
    macros.insert(
        "gh".to_string(),
        Value::Scalar(Scalar::Text("https://x/issues/$(1)".to_string())),
    );
    macros.insert("note".to_string(), block_macro("note", "Note: $(1)"));
    macros.insert(
        "card".to_string(),
        block_macro("card", "\n@card(title: $(1))[ $(2) ]\n"),
    );
    macros.insert(
        "pair".to_string(),
        block_macro("pair", "\none $(1)\n\ntwo $(1)\n"),
    );
    let (name, tmpl) = macro_entry(
        "generated_by: @comment[ Generated from $(1). Edit that, then `tomet export .`. ]",
    );
    macros.insert(name, tmpl);
    let (name, tmpl) = macro_entry("el_card: @card(title: $(1))[ $(2) ]");
    macros.insert(name, tmpl);
    let cx = EvalContext {
        resolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        var: vars,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let mut doc = tomet_ast_parser::parse_file(src).expect("source parses");
    let unresolved = resolve_expressions(&mut doc, &cx);
    (doc, unresolved)
}

fn run(src: &str) -> (File, Vec<Unresolved>) {
    run_with(src, &EvalValue::Scalar(Scalar::None))
}

/// Every text node in the document, in order, joined with `|` between blocks.
fn text_of(doc: &File) -> String {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn inlines(out: &mut String, items: &[Inline]) {
        for inline in items {
            match inline {
                Inline::Text(t) => out.push_str(&t.value),
                Inline::Raw(t) => out.push_str(&t.value),
                Inline::Element(el) => {
                    out.push_str(&format!("<{}>", el.name));
                    if let Some(content) = &el.content {
                        blocks(out, content);
                    }
                }
                Inline::SoftBreak(_)
                | Inline::LineBreak(_)
                | Inline::Interp(_)
                | Inline::Hole(_)
                | Inline::TypeSeat(_) => {}
            }
        }
    }
    #[deny(clippy::wildcard_enum_match_arm)]
    fn blocks(out: &mut String, items: &[Block]) {
        for (i, block) in items.iter().enumerate() {
            if i > 0 {
                out.push('|');
            }
            match block {
                Block::Paragraph(p) => inlines(out, &p.content),
                Block::Section(s) => {
                    inlines(out, &s.title);
                    out.push('{');
                    blocks(out, &s.blocks);
                    out.push('}');
                }
                Block::List(list) => {
                    out.push('[');
                    for (j, item) in list.items.iter().enumerate() {
                        if j > 0 {
                            out.push(',');
                        }
                        blocks(out, &item.content);
                    }
                    out.push(']');
                }
                Block::Element(el) => {
                    out.push_str(&format!("<{}>", el.name));
                }
                Block::Interp(interp) => out.push_str(&interp.to_string()),
                Block::Hole(_) | Block::TypeSeat(_) => {}
            }
        }
    }
    let mut out = String::new();
    blocks(&mut out, &doc.body);
    out
}

fn reasons(unresolved: &[Unresolved]) -> Vec<&str> {
    unresolved.iter().map(|u| u.reason.as_str()).collect()
}

#[test]
fn a_scalar_in_a_paragraph_becomes_text() {
    let (doc, unresolved) = run("Sum is $add(1, 2) and {$ \"x\" } and {$ true }.\n");
    assert_eq!(text_of(&doc), "Sum is 3 and x and true.");
    assert!(unresolved.is_empty());
}

#[test]
fn a_scalar_on_its_own_line_becomes_a_paragraph() {
    let (doc, unresolved) = run("$add(1, 2)\n");
    assert_eq!(text_of(&doc), "3");
    assert!(unresolved.is_empty());
}

#[test]
fn none_is_empty_text() {
    let (doc, _) = run("a{$ none }b\n");
    assert_eq!(text_of(&doc), "ab");
}

#[test]
fn a_heading_title_is_a_paragraph_position() {
    let (doc, _) = run("=[ Title $add(1, 1) ]\n\nBody\n");
    assert_eq!(text_of(&doc), "Title 2{Body}");
}

#[test]
fn a_fragment_of_one_paragraph_is_spliced_into_a_paragraph() {
    let (doc, unresolved) = run("See {$ #top.link() } now.\n");
    assert_eq!(text_of(&doc), "See <link> now.");
    assert!(unresolved.is_empty(), "{unresolved:?}");
}

#[test]
fn a_fragment_on_its_own_line_is_its_blocks() {
    let (doc, unresolved) = run("{$ #c1.link() }\n");
    assert_eq!(text_of(&doc), "<link>");
    assert!(unresolved.is_empty());
}

#[test]
fn a_list_item_that_is_one_expression_becomes_an_item_per_element() {
    let (doc, unresolved) =
        run("- first\n- {$ addr(dir:docs)::docs.map(d => d.link()) }\n- last\n");
    assert_eq!(text_of(&doc), "[first,<link>,<link>,last]");
    assert!(unresolved.is_empty(), "{unresolved:?}");
}

#[test]
fn a_list_of_scalars_becomes_items_of_text() {
    let (doc, _) = run("- {$ addr(dir:docs)::docs.map(d => d::path) }\n");
    assert_eq!(text_of(&doc), "[docs/a.tmt,docs/b.tmt]");
}

#[test]
fn a_list_item_with_more_than_the_expression_keeps_it_inline() {
    let (doc, _) = run("- n is $add(1, 1)\n");
    assert_eq!(text_of(&doc), "[n is 2]");
}

#[test]
fn an_empty_result_removes_the_item() {
    let (doc, unresolved) = run("- a\n- {$ addr(dir:docs)::docs.filter(d => false) }\n- b\n");
    assert_eq!(text_of(&doc), "[a,b]");
    assert!(unresolved.is_empty());
}

#[test]
fn a_list_in_the_text_is_a_mismatch_that_says_to_join_it() {
    let (doc, unresolved) = run("Tags: $(#c1.prop)\n");
    // A map, not a list, but equally not text.
    assert_eq!(text_of(&doc), "Tags: $(#c1.prop)");
    assert_eq!(unresolved.len(), 1);

    let (doc, unresolved) = run("Docs: {$ addr(dir:docs)::docs }\n");
    assert_eq!(text_of(&doc), "Docs: {$ addr(dir:docs)::docs }");
    assert!(reasons(&unresolved)[0].contains(".join"), "{unresolved:?}");
}

#[test]
fn a_document_in_the_text_says_to_use_include_or_link() {
    let (doc, unresolved) = run("Look: $(self)\n");
    assert_eq!(text_of(&doc), "Look: $(self)");
    let reason = reasons(&unresolved)[0];
    assert!(
        reason.contains("include") && reason.contains(".link()"),
        "{reason}"
    );
}

#[test]
fn a_datetime_in_the_text_says_to_format_it() {
    let when = tomet_expr::parse_datetime("2026-11-11T09:30:00+09:00").expect("a datetime");
    let vars = EvalValue::Tuple(
        Tuple::try_from_entries([TupleEntry::named("when", EvalValue::DateTime(when))]).unwrap(),
    );
    let (_, unresolved) = run_with("When: $(var.when)\n", &vars);
    assert!(
        reasons(&unresolved)[0].contains(".format"),
        "{unresolved:?}"
    );
}

#[test]
fn a_type_in_the_text_has_no_place_there() {
    let (doc, unresolved) = run("Kind: {$ list(string) }\n");
    assert_eq!(text_of(&doc), "Kind: $list(string)");
    assert!(
        reasons(&unresolved)[0].contains("a type cannot be placed in the text"),
        "{unresolved:?}"
    );
}

#[test]
fn raw_placement_alone_on_line_gives_block_and_inline_is_type_mismatch() {
    // 1. Block position (alone on a line): resolves to Block::Element(raw)
    let (doc, unresolved) = run("{$ raw(\"fn main() {}\", lang: \"rust\") }\n");
    assert!(unresolved.is_empty(), "unresolved: {unresolved:?}");
    assert_eq!(doc.body.len(), 1);
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name.to_string(), "raw");
            assert_eq!(el.placement, tomet_ast::Placement::Block);
        }
        other => panic!("expected Block::Element, got {other:?}"),
    }

    // 2. Inline position (inside a paragraph): fragment of more than one paragraph is rejected
    let (doc, unresolved) = run("Look: {$ raw(\"fn main() {}\", lang: \"rust\") }\n");
    assert_eq!(unresolved.len(), 1);
    let reason = reasons(&unresolved)[0];
    assert!(
        reason.contains("cannot go inside a paragraph"),
        "expected mismatch reason, got: {reason}"
    );
    assert_eq!(
        text_of(&doc),
        "Look: $raw(\"fn main() {}\", lang: \"rust\")"
    );
}

#[test]
fn an_expression_that_fails_is_written_back_as_its_own_source() {
    let (doc, unresolved) = run("Hi $(nope) there $gh(1).\n");
    assert_eq!(text_of(&doc), "Hi $(nope) there $gh(1).");
    assert_eq!(unresolved.len(), 2);
    assert_eq!(unresolved[0].source, "nope");
    assert!(unresolved[0].reason.contains("unknown name"));
    assert_eq!(unresolved[0].kind, UnresolvedKind::Expression);
    assert_eq!(
        unresolved[0].message(),
        format!("unresolved {{$ nope }} -- {}", unresolved[0].reason)
    );
    assert!(unresolved[1].reason.contains("unknown function"));
}

#[test]
fn a_macro_is_called_through_its_namespace() {
    let (doc, unresolved) = run("Issue $macro.gh(12).\n");
    assert_eq!(text_of(&doc), "Issue https://x/issues/12.");
    assert!(unresolved.is_empty());
}

#[test]
fn macro_table_cases() {
    let mut macros = HashMap::new();
    macros.insert(
        "gh".to_string(),
        Value::Scalar(Scalar::Text("https://x/$(1)".to_string())),
    );
    macros.insert(
        "two".to_string(),
        Value::Scalar(Scalar::Text("$(1)-$(2)".to_string())),
    );
    macros.insert(
        "named".to_string(),
        Value::Scalar(Scalar::Text("hi $(who)".to_string())),
    );
    macros.insert(
        "none".to_string(),
        Value::Scalar(Scalar::Text("const".to_string())),
    );
    let resolver = vault();
    let cx = EvalContext {
        resolver: &resolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        var: &EvalValue::Scalar(Scalar::None),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let test_run = |expr: &str| {
        let src = format!("Test {}.\n", expr);
        let mut doc = tomet_ast_parser::parse_file(&src).unwrap();
        let unresolved = resolve_expressions(&mut doc, &cx);
        (text_of(&doc), unresolved)
    };

    assert!(test_run("$macro.gh(12)").1.is_empty());
    assert!(test_run("$macro.gh(\"a/b\")").1.is_empty());
    assert!(test_run("$macro.named(who: \"w\")").1.is_empty());

    let unres = test_run("$macro.gh(tomet)").1;
    assert_eq!(unres.len(), 1);
    assert!(unres[0].reason.contains("unknown name `tomet`"));

    let unres = test_run("$macro.gh(1, 2)").1;
    assert_eq!(unres.len(), 1);
    assert_eq!(unres[0].reason, "`macro.gh` expects 1 argument(s), got 2");

    let unres = test_run("$macro.gh()").1;
    assert_eq!(unres.len(), 1);
    assert_eq!(unres[0].reason, "`macro.gh` expects 1 argument(s), got 0");

    let unres = test_run("$macro.two(1)").1;
    assert_eq!(unres.len(), 1);
    assert_eq!(unres[0].reason, "`macro.two` expects 2 argument(s), got 1");

    let unres = test_run("$macro.none(1)").1;
    assert_eq!(unres.len(), 1);
    assert_eq!(unres[0].reason, "`macro.none` expects 0 argument(s), got 1");

    let unres = test_run("$macro.named()").1;
    assert_eq!(unres.len(), 1);
    assert_eq!(unres[0].reason, "macro.named expects named argument `who`");

    let unres = test_run("$macro.nosuch(1)").1;
    assert_eq!(unres.len(), 1);
    assert_eq!(unres[0].reason, "unknown macro `macro.nosuch`");

    let unres = test_run("$(macro.gh)").1;
    assert_eq!(unres.len(), 1);
    assert_eq!(
        unres[0].reason,
        "the macro `gh` has to be called: `macro.gh(...)`"
    );
}

#[test]
fn a_value_in_an_element_argument_keeps_its_type() {
    let (doc, unresolved) = run("@card(n: $add(1, 2), label: {$ \"x\" })\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let Block::Element(el) = &doc.body[0] else {
        panic!("an element block");
    };
    assert_eq!(
        el.data.args,
        Some(Tuple::from_named([
            ("n".into(), Value::Scalar(Scalar::Int(3))),
            ("label".into(), Value::Scalar(Scalar::Text("x".into())))
        ]))
    );
}

#[test]
fn a_value_in_a_value_group_keeps_its_type_and_a_list_stays_a_list() {
    let (doc, unresolved) =
        run("::meta(n: $add(1, 1), names: {$ addr(dir:docs)::docs.map(d => d::path) })\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let declaration = &doc.header[0];
    let Some(tuple) = &declaration.data.args else {
        panic!("the entries of `::meta(...)`");
    };
    assert_eq!(
        tuple,
        &Tuple::from_named([
            ("n".into(), Value::Scalar(Scalar::Int(2))),
            (
                "names".into(),
                Value::Tuple(Tuple::from_unnamed([
                    Value::Scalar(Scalar::Text("docs/a.tmt".into())),
                    Value::Scalar(Scalar::Text("docs/b.tmt".into()))
                ]))
            ),
        ])
    );
}

#[test]
fn a_document_cannot_be_stored_as_data_and_the_expression_stays() {
    let (doc, unresolved) = run("@card(doc: $(self))\n");
    assert!(reasons(&unresolved)[0].contains("cannot be stored as data"));
    let Block::Element(el) = &doc.body[0] else {
        panic!("an element block");
    };
    // Still the expression, so the author sees what did not resolve.
    let Some(tuple) = &el.data.args else {
        panic!("args");
    };
    assert!(matches!(tuple.get("doc"), Some(Value::Interp(_))));
}

#[test]
fn expressions_inside_an_elements_content_are_resolved_too() {
    let (doc, _) = run("@callout[ value is $add(2, 3) ]\n");
    let Block::Element(el) = &doc.body[0] else {
        panic!("an element block");
    };
    let content = el.content.as_ref().unwrap();
    assert_eq!(
        text_of(&File {
            body: content.clone(),
            ..doc.clone()
        }),
        "value is 5"
    );
}

#[test]
fn a_reference_is_answered_from_the_document_the_resolver_holds() {
    // `#c1` is looked up through the resolver, which holds `here.tmt` as it
    // was written -- not through the document this pass is rewriting.
    let (doc, unresolved) = run("n=$(#c1.n) extra=$(#c1.extra)\n");
    assert_eq!(text_of(&doc), "n=1 extra=yes");
    assert!(unresolved.is_empty());
}

#[test]
fn a_document_reads_var() {
    let vars = EvalValue::Tuple(
        Tuple::try_from_entries([TupleEntry::named(
            "title",
            EvalValue::Scalar(Scalar::Text("Plan".into())),
        )])
        .unwrap(),
    );
    let (doc, unresolved) = run_with(
        "::meta(title: $(var.title))\n\nPlan for $(var.title)\n",
        &vars,
    );
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let declaration = &doc.header[0];
    let Some(tuple) = &declaration.data.args else {
        panic!("the entries of `::meta(...)`");
    };
    assert_eq!(
        tuple.get("title"),
        Some(&Value::Scalar(Scalar::Text("Plan".into())))
    );
    assert_eq!(text_of(&doc), "Plan for Plan");
}

/// A variable that has no value yet is not a mistake in a blueprint, which is
/// filled in later; `awaits_input` is what tells it from a mistake.
#[test]
fn only_a_variable_that_has_no_value_yet_awaits_input() {
    let resolver = vault();
    let macros = HashMap::new();
    let vars = EvalValue::Tuple(Tuple::new());
    let cx = EvalContext {
        resolver: &resolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        var: &vars,
        var_unset: &["title".to_string()],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let mut doc = tomet_ast_parser::parse_file("$(var.title) $(nope) $(var.other)\n").unwrap();
    let unresolved = resolve_expressions(&mut doc, &cx);
    let awaiting: Vec<(&str, bool)> = unresolved
        .iter()
        .map(|u| (u.source.as_str(), u.awaits_input))
        .collect();
    assert_eq!(
        awaiting,
        [("var.title", true), ("nope", false), ("var.other", false)]
    );
}

#[test]
fn with_no_vault_a_reference_to_another_document_fails_and_stays() {
    let mut doc = tomet_ast_parser::parse_file("see {$ tmt(\"x.tmt\")#a::id }\n").unwrap();
    let macros = HashMap::new();
    let vars = EvalValue::Scalar(Scalar::None);
    let cx = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef("here.tmt".into()),
        macros: &macros,
        var: &vars,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let unresolved = resolve_expressions(&mut doc, &cx);
    assert_eq!(unresolved.len(), 1);
    assert_eq!(text_of(&doc), "see {$ tmt(\"x.tmt\")#a::id }");
}

#[test]
fn a_macro_in_an_element_argument_is_expanded_and_stays_text() {
    // What `expand_document_macros` did, for the one binding that called it.
    let (doc, unresolved) = run("- @link($macro.gh(7))\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let Block::List(list) = &doc.body[0] else {
        panic!("a list");
    };
    let content = &list.items[0].content;
    let [Block::Paragraph(p)] = content.as_slice() else {
        panic!("a paragraph");
    };
    let Inline::Element(link) = &p.content[0] else {
        panic!("a link");
    };
    assert_eq!(
        link.data.args,
        Some(Tuple::from_unnamed([Value::Scalar(Scalar::Text(
            "https://x/issues/7".into()
        ))]))
    );
}

// ---- unique ids after splicing ----------------------------------------------

#[test]
fn an_id_used_twice_after_an_include_keeps_the_first_and_reports_the_later() {
    // `here.tmt` has `#(c1)` and includes a copy of itself's `c1` element.
    let (doc, unresolved) = run("@card(n: 5)#(c1)\n\n$include(tmt(\"here.tmt\")#c1)\n");
    assert_eq!(unresolved.len(), 1, "{unresolved:?}");
    assert_eq!(unresolved[0].source, "#(c1)");
    assert!(
        unresolved[0].reason.contains("already used"),
        "{unresolved:?}"
    );
    // It is not an expression, and does not read as one.
    assert_eq!(unresolved[0].kind, UnresolvedKind::DroppedId);
    assert_eq!(unresolved[0].message(), unresolved[0].reason);
    assert!(!unresolved[0].message().contains("$("), "{unresolved:?}");

    let ids: Vec<Option<String>> = doc
        .body
        .iter()
        .filter_map(|b| match b {
            Block::Element(el) => Some(el.id.as_ref().map(|i| i.0.clone())),
            _ => None,
        })
        .collect();
    // The original keeps its id; the included copy has none.
    assert_eq!(ids, [Some("c1".to_string()), None]);
}

#[test]
fn an_id_used_again_in_args_or_in_a_holes_content_is_reported_too() {
    // Every node that can carry an id is claimed, in document order: an element in `(args)` and
    // one in a hole's content count, which a walk over the blocks alone would miss.
    let (_, unresolved) = run("@a()#(x)\n\n@b(icon: @c()#(x))\n\n<?(h)>[@d()#(x)]\n");
    assert_eq!(unresolved.len(), 2, "{unresolved:?}");
    assert!(
        unresolved
            .iter()
            .all(|u| u.kind == UnresolvedKind::DroppedId && u.source == "#(x)"),
        "{unresolved:?}"
    );
}

#[test]
fn ids_that_are_each_used_once_are_left_alone() {
    let (doc, unresolved) = run("@card(n: 1)#(a)\n\n@card(n: 2)#(b)\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let ids: Vec<String> = doc
        .body
        .iter()
        .filter_map(|b| match b {
            Block::Element(el) => el.id.as_ref().map(|i| i.0.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(ids, ["a", "b"]);
}

#[test]
fn a_section_id_is_claimed_too() {
    let (doc, unresolved) = run("=[ One ]#(s)\n\n=[ Two ]#(s)\n");
    assert_eq!(unresolved.len(), 1, "{unresolved:?}");
    let ids: Vec<Option<String>> = doc
        .body
        .iter()
        .filter_map(|b| match b {
            Block::Section(s) => Some(s.id.as_ref().map(|i| i.0.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(ids, [Some("s".to_string()), None]);
}

#[test]
fn an_include_lands_one_heading_below_the_section_it_sits_in() {
    let (doc, unresolved) = run("=[ Host ]\n\n$include(tmt(\"here.tmt\")#top)\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let Block::Section(host) = &doc.body[0] else {
        panic!("a section");
    };
    let included: Vec<usize> = host
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Section(s) => Some(s.level),
            _ => None,
        })
        .collect();
    // `top` is a level-1 heading in `here.tmt`; under a level-1 host it is 2.
    assert_eq!(included, [2]);
}

/// Only the picked `{$ ... }` are resolved; the rest stay interpolations (not text) and are not reported.
#[test]
fn only_the_selected_expressions_are_resolved() {
    let src = "a $(self::path) b $macro.gh(1) c\n\n$(self::filename)\n\n- $(self::path)\n\n@card(n: $(self::path)){ k: $(self::path) }\n";
    let resolver = vault();
    let macros = HashMap::new();
    let cx = EvalContext {
        resolver: &resolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        var: &EvalValue::Scalar(Scalar::None),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let mut doc = tomet_ast_parser::parse_file(src).expect("source parses");
    // Pick every expression that mentions `path`, by the span it is written at.
    let mut picked = Vec::new();
    tomet_tree::for_each_interpolation(&doc, |interp| {
        if interp.expr.to_string().contains("path") {
            picked.push(interp.span);
        }
    });
    assert_eq!(picked.len(), 4);
    let unresolved = resolve_expressions_where(&mut doc, &cx, &|span| {
        picked.iter().any(|p| p.exact_eq(&span))
    });
    assert_eq!(unresolved, vec![], "the unpicked are not reported");

    let mut left = Vec::new();
    tomet_tree::for_each_interpolation(&doc, |interp| left.push(interp.expr.to_string()));
    assert_eq!(left, vec!["macro.gh(1)", "self::filename"]);
}

#[test]
fn written_number_in_metadata_interpolates_as_written() {
    let resolver = MemoryResolver::new()
        .with_source(
            "doc.tmt",
            "::meta(rev: 1.10)\n\nVersion is $(self::meta.rev).\n",
        )
        .unwrap();
    let cx = EvalContext {
        resolver: &resolver,
        self_doc: DocRef("doc.tmt".to_string()),
        macros: &HashMap::new(),
        var: &EvalValue::Scalar(Scalar::None),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let mut doc =
        tomet_ast_parser::parse_file("::meta(rev: 1.10)\n\nVersion is $(self::meta.rev).\n")
            .unwrap();
    let unresolved = resolve_expressions(&mut doc, &cx);
    assert!(unresolved.is_empty());
    assert_eq!(text_of(&doc), "Version is 1.10.");
}

// ---- block macros ----

#[test]
fn a_block_macro_on_its_own_line_is_its_blocks() {
    let (doc, unresolved) = run("$macro.card(\"Title\", \"words\")\n");
    assert_eq!(text_of(&doc), "<card>");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let Block::Element(card) = &doc.body[0] else {
        panic!("expected the card, got {:?}", doc.body[0]);
    };
    assert_eq!(
        card.data.args,
        Some(Tuple::from_named([(
            "title".to_string(),
            Value::Scalar(Scalar::Text("Title".to_string()))
        )]))
    );
}

#[test]
fn a_block_macro_of_one_paragraph_is_spliced_into_a_paragraph() {
    let (doc, unresolved) = run("before $macro.note(\"x\") after\n");
    assert_eq!(text_of(&doc), "before Note: x after");
    assert!(unresolved.is_empty(), "{unresolved:?}");
}

#[test]
fn a_block_macro_of_several_blocks_has_no_place_in_a_paragraph() {
    let (doc, unresolved) = run("before $macro.pair(\"x\") after\n");
    // Written back as it was, and reported: nothing is lost and nothing is fatal.
    assert_eq!(unresolved.len(), 1, "{unresolved:?}");
    assert_eq!(text_of(&doc), "before $macro.pair(\"x\") after");
    let (doc, unresolved) = run("$macro.pair(\"x\")\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert_eq!(text_of(&doc), "one x|two x");
}

#[test]
fn a_block_macro_called_wrongly_is_reported_and_kept() {
    let (doc, unresolved) = run("$macro.card(\"only one\")\n");
    assert_eq!(unresolved.len(), 1, "{unresolved:?}");
    assert_eq!(text_of(&doc), "$macro.card(\"only one\")");
}

// ---- element macros ----

#[test]
fn an_element_macro_on_its_own_line_expands_to_a_block_element() {
    let (doc, unresolved) = run("$macro.generated_by(self::path)\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert_eq!(text_of(&doc), "<comment>");
    let Block::Element(comment) = &doc.body[0] else {
        panic!("expected comment element, got {:?}", doc.body[0]);
    };
    assert_eq!(comment.name.to_string(), "comment");
    assert_eq!(comment.placement, tomet_ast::Placement::Block);

    let (doc, unresolved) = run("$macro.el_card(\"Title\", \"words\")\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert_eq!(text_of(&doc), "<card>");
    let Block::Element(card) = &doc.body[0] else {
        panic!("expected card element, got {:?}", doc.body[0]);
    };
    assert_eq!(card.placement, tomet_ast::Placement::Block);
    assert_eq!(
        card.data.args,
        Some(Tuple::from_named([(
            "title".to_string(),
            Value::Scalar(Scalar::Text("Title".to_string()))
        )]))
    );
}

#[test]
fn an_element_macro_in_a_paragraph_is_rejected_and_in_value_position_becomes_blocks() {
    let (doc, unresolved) = run("before $macro.generated_by(self::path) after\n");
    assert_eq!(unresolved.len(), 1, "{unresolved:?}");
    assert_eq!(
        text_of(&doc),
        "before $macro.generated_by(self::path) after"
    );

    let (doc, unresolved) = run("@box(item: $macro.el_card(\"T\", \"B\"))\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let Block::Element(el) = &doc.body[0] else {
        panic!("expected box element");
    };
    let Some(Value::Blocks(blocks)) = el.data.args.as_ref().and_then(|a| a.get("item")) else {
        panic!("expected Value::Blocks, got {:?}", el.data.args);
    };
    assert!(matches!(&blocks[..], [Block::Element(c)] if c.name.to_string() == "card"));
}

#[test]
fn a_seat_that_arrives_by_an_expression_is_a_stop_and_not_a_seat_in_the_document() {
    let resolver = MemoryResolver::new()
        .with_source("other.tmt", "=[ Top ]#(top)\n\n<$string>\n")
        .unwrap();
    let mut doc = tomet_ast_parser::parse_file("::blueprint(note)\n\n$include(tmt(\"other.tmt\")#top)\n").unwrap();
    let macros = HashMap::new();
    let cx = EvalContext {
        resolver: &resolver,
        self_doc: DocRef("notes/plan.tmt".into()),
        macros: &macros,
        var: &EvalValue::Scalar(Scalar::None),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let source = tomet_transform::FixedSource::at("2026-11-11T09:30:00+09:00").unwrap();
    let found = crate::instantiate_blueprint(&mut doc, &cx, &source).unwrap_err();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].seat, "<$string>");
    assert_eq!(found[0].reason, tomet_transform::SeatReason::Left);
}

fn instantiate_with_eval(src: &str, vars: EvalValue) -> (File, tomet_transform::Instantiated) {
    let mut doc = tomet_ast_parser::parse_file(src).unwrap();
    let macros = HashMap::new();
    let cx = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef("notes/plan.tmt".into()),
        macros: &macros,
        var: &vars,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let source = tomet_transform::FixedSource::at("2026-11-11T09:30:00+09:00").unwrap();
    let report = crate::instantiate_blueprint(&mut doc, &cx, &source).unwrap();
    (doc, report)
}

#[test]
fn instantiates_blueprint_to_kind_and_resolves_expressions() {
    let src = "::blueprint(daily-note)\n::meta(\n  file: $(self::filename)\n  title: $(var.title)\n)\n\n=[ Plan for $(var.title) ]#(plan)\n";
    let vars = EvalValue::Tuple(
        Tuple::try_from_entries([TupleEntry::named(
            "title".to_string(),
            EvalValue::Scalar(Scalar::Text("Sprint 42".into())),
        )])
        .unwrap(),
    );
    let (doc, report) = instantiate_with_eval(src, vars);
    assert!(report.changed);
    assert!(report.unresolved.is_empty(), "{:?}", report.unresolved);

    let mut top_level = Vec::new();
    tomet_tree::for_each_declaration(&doc, |declaration| top_level.push(declaration.clone()));

    let kind = top_level
        .iter()
        .find(|d| {
            tomet_semantics::classify_declaration_lenient(d)
                == tomet_semantics::ElementKind::Declaration(tomet_declaration::DeclKind::Kind)
        })
        .expect("expected a ::kind declaration");
    assert_eq!(
        kind.data.args,
        Some(Tuple::from_unnamed([Value::Scalar(Scalar::Text(
            "daily-note".into()
        ))]))
    );

    let meta = top_level
        .iter()
        .find(|d| {
            tomet_semantics::classify_declaration_lenient(d)
                == tomet_semantics::ElementKind::Declaration(tomet_declaration::DeclKind::Meta)
        })
        .expect("expected a ::meta declaration");
    let Some(tuple) = &meta.data.args else {
        panic!("expected meta tuple");
    };
    let get = |key: &str| tuple.get(key).unwrap().clone();
    assert_eq!(get("file"), Value::Scalar(Scalar::Text("plan.tmt".into())));
    assert_eq!(
        get("title"),
        Value::Scalar(Scalar::Text("Sprint 42".into()))
    );
}

#[test]
fn an_expression_with_no_value_is_left_and_reported() {
    let (doc, report) = instantiate_with_eval(
        "::blueprint(note)\n::meta(title: $(var.title))\n",
        EvalValue::Tuple(Tuple::new()),
    );
    assert_eq!(report.unresolved.len(), 1);
    assert!(
        report.unresolved[0].reason.contains("title"),
        "{:?}",
        report.unresolved
    );
    assert!(format!("{doc:?}").contains("Interp"));
}
