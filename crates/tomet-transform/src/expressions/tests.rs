use std::collections::HashMap;

use tomet_ast::{Block, ElementValue, Inline, Sigil, Value};
use tomet_compute::eval::{
    DocRef, EvalContext, EvalValue, MemoryResolver, NoResolver, Phase, Resolver, SystemGenerate,
};

use super::*;

const NOTE: &str = "@meta{
  title: Note
  tags: list(a, b)
}

=[ Heading ]#(top)

Body.

@card(n: 1){ extra: yes }#(c1)
";

fn vault() -> MemoryResolver {
    MemoryResolver::new()
        .with_source("here.tmt", NOTE)
        .unwrap()
        .with_source("docs/a.tmt", "@meta{ title: A }\n")
        .unwrap()
        .with_source("docs/b.tmt", "@meta{ title: B }\n")
        .unwrap()
}

/// Resolves `src` as the document `here.tmt` of the small vault above.
fn run_with(src: &str, phase: Phase, vars: &EvalValue) -> (Document, Vec<Unresolved>) {
    let resolver = vault();
    run_in(src, &resolver, phase, vars)
}

fn run_in(
    src: &str,
    resolver: &dyn Resolver,
    phase: Phase,
    vars: &EvalValue,
) -> (Document, Vec<Unresolved>) {
    let mut macros = HashMap::new();
    macros.insert("gh".to_string(), "https://x/issues/${1}".to_string());
    let cx = EvalContext {
        phase,
        resolver,
        generate: &SystemGenerate,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        vars,
    };
    let mut doc = tomet_parser::parse_document(src).expect("source parses");
    let unresolved = resolve_expressions(&mut doc, &cx);
    (doc, unresolved)
}

fn run(src: &str) -> (Document, Vec<Unresolved>) {
    run_with(src, Phase::Output, &EvalValue::Null)
}

/// Every text node in the document, in order, joined with `|` between blocks.
fn text_of(doc: &Document) -> String {
    fn inlines(out: &mut String, items: &[Inline]) {
        for inline in items {
            match inline {
                Inline::Text(t) => out.push_str(&t.value),
                Inline::Raw(t) => out.push_str(&t.value),
                Inline::Element(el) => {
                    out.push_str(&format!(
                        "<{}>",
                        el.sigil.name().map(|n| n.to_string()).unwrap_or_default()
                    ));
                    if let Some(content) = &el.content {
                        blocks(out, content);
                    }
                }
                _ => {}
            }
        }
    }
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
                        if let Some(content) = &item.element.content {
                            blocks(out, content);
                        }
                    }
                    out.push(']');
                }
                Block::Element(el) => {
                    out.push_str(&format!(
                        "<{}>",
                        el.sigil.name().map(|n| n.to_string()).unwrap_or_default()
                    ));
                }
            }
        }
    }
    let mut out = String::new();
    blocks(&mut out, &doc.blocks);
    out
}

fn reasons(unresolved: &[Unresolved]) -> Vec<&str> {
    unresolved.iter().map(|u| u.reason.as_str()).collect()
}

#[test]
fn a_scalar_in_a_paragraph_becomes_text() {
    let (doc, unresolved) = run("Sum is ${add(1, 2)} and ${\"x\"} and ${true}.\n");
    assert_eq!(text_of(&doc), "Sum is 3 and x and true.");
    assert!(unresolved.is_empty());
}

#[test]
fn a_scalar_on_its_own_line_becomes_a_paragraph() {
    let (doc, unresolved) = run("${add(1, 2)}\n");
    assert_eq!(text_of(&doc), "3");
    assert!(unresolved.is_empty());
}

#[test]
fn null_is_empty_text() {
    let (doc, _) = run("a${null}b\n");
    assert_eq!(text_of(&doc), "ab");
}

#[test]
fn a_heading_title_is_a_paragraph_position() {
    let (doc, _) = run("=[ Title ${add(1, 1)} ]\n\nBody\n");
    assert_eq!(text_of(&doc), "Title 2{Body}");
}

#[test]
fn a_fragment_of_one_paragraph_is_spliced_into_a_paragraph() {
    let (doc, unresolved) = run("See ${#top.link()} now.\n");
    assert_eq!(text_of(&doc), "See <link> now.");
    assert!(unresolved.is_empty(), "{unresolved:?}");
}

#[test]
fn a_fragment_on_its_own_line_is_its_blocks() {
    let (doc, unresolved) = run("${#c1.link()}\n");
    assert_eq!(text_of(&doc), "<link>");
    assert!(unresolved.is_empty());
}

#[test]
fn a_list_item_that_is_one_expression_becomes_an_item_per_element() {
    let (doc, unresolved) = run("- first\n- ${addr(dir:docs).docs.map(d => d.link())}\n- last\n");
    assert_eq!(text_of(&doc), "[first,<link>,<link>,last]");
    assert!(unresolved.is_empty(), "{unresolved:?}");
}

#[test]
fn a_list_of_scalars_becomes_items_of_text() {
    let (doc, _) = run("- ${addr(dir:docs).docs.map(d => d.path)}\n");
    assert_eq!(text_of(&doc), "[docs/a.tmt,docs/b.tmt]");
}

#[test]
fn a_list_item_with_more_than_the_expression_keeps_it_inline() {
    let (doc, _) = run("- n is ${add(1, 1)}\n");
    assert_eq!(text_of(&doc), "[n is 2]");
}

#[test]
fn an_empty_result_removes_the_item() {
    let (doc, unresolved) = run("- a\n- ${addr(dir:docs).docs.filter(d => false)}\n- b\n");
    assert_eq!(text_of(&doc), "[a,b]");
    assert!(unresolved.is_empty());
}

#[test]
fn a_list_in_the_text_is_a_mismatch_that_says_to_join_it() {
    let (doc, unresolved) = run("Tags: ${#c1.prop}\n");
    // A map, not a list, but equally not text.
    assert_eq!(text_of(&doc), "Tags: ${#c1.prop}");
    assert_eq!(unresolved.len(), 1);

    let (doc, unresolved) = run("Docs: ${addr(dir:docs).docs}\n");
    assert_eq!(text_of(&doc), "Docs: ${addr(dir:docs).docs}");
    assert!(reasons(&unresolved)[0].contains(".join"), "{unresolved:?}");
}

#[test]
fn a_document_in_the_text_says_to_use_include_or_link() {
    let (doc, unresolved) = run("Look: ${self}\n");
    assert_eq!(text_of(&doc), "Look: ${self}");
    let reason = reasons(&unresolved)[0];
    assert!(
        reason.contains("include") && reason.contains(".link()"),
        "{reason}"
    );
}

#[test]
fn a_datetime_in_the_text_says_to_format_it() {
    let (_, unresolved) = run_with("When: ${generate.now()}\n", Phase::Create, &EvalValue::Null);
    assert!(
        reasons(&unresolved)[0].contains(".format"),
        "{unresolved:?}"
    );
}

#[test]
fn an_expression_that_fails_is_written_back_as_its_own_source() {
    let (doc, unresolved) = run("Hi ${nope} there ${gh(1)}.\n");
    assert_eq!(text_of(&doc), "Hi ${nope} there ${gh(1)}.");
    assert_eq!(unresolved.len(), 2);
    assert_eq!(unresolved[0].source, "nope");
    assert!(unresolved[0].reason.contains("unknown name"));
    assert!(unresolved[1].reason.contains("unknown function"));
}

#[test]
fn a_macro_is_called_through_its_namespace() {
    let (doc, unresolved) = run("Issue ${macro.gh(12)}.\n");
    assert_eq!(text_of(&doc), "Issue https://x/issues/12.");
    assert!(unresolved.is_empty());
}

#[test]
fn a_value_in_an_element_argument_keeps_its_type() {
    let (doc, unresolved) = run("@card(n: ${add(1, 2)}, label: ${\"x\"})\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let Block::Element(el) = &doc.blocks[0] else {
        panic!("an element block");
    };
    assert_eq!(
        el.args,
        Some(Value::Map(vec![
            ("n".into(), Value::Int(3)),
            ("label".into(), Value::String("x".into()))
        ]))
    );
}

#[test]
fn a_value_in_a_value_group_keeps_its_type_and_a_list_stays_a_list() {
    let (doc, unresolved) =
        run("@meta{ n: ${add(1, 1)}, names: ${addr(dir:docs).docs.map(d => d.path)} }\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let Block::Element(el) = &doc.blocks[0] else {
        panic!("an element block");
    };
    let Some(ElementValue::Group(entries)) = &el.value else {
        panic!("a group");
    };
    assert_eq!(
        entries,
        &vec![
            tomet_ast::Entry::Pair("n".into(), Value::Int(2)),
            tomet_ast::Entry::Pair(
                "names".into(),
                Value::Seq(vec![
                    Value::String("docs/a.tmt".into()),
                    Value::String("docs/b.tmt".into())
                ])
            ),
        ]
    );
}

#[test]
fn a_document_cannot_be_stored_as_data_and_the_expression_stays() {
    let (doc, unresolved) = run("@card(doc: ${self})\n");
    assert!(reasons(&unresolved)[0].contains("cannot be stored as data"));
    let Block::Element(el) = &doc.blocks[0] else {
        panic!("an element block");
    };
    // Still the expression, so the author sees what did not resolve.
    let Some(Value::Map(entries)) = &el.args else {
        panic!("args");
    };
    assert!(matches!(&entries[0].1, Value::Element(e) if e.sigil == Sigil::Dollar));
}

#[test]
fn expressions_inside_an_elements_content_are_resolved_too() {
    let (doc, _) = run("@callout[ value is ${add(2, 3)} ]\n");
    let Block::Element(el) = &doc.blocks[0] else {
        panic!("an element block");
    };
    let content = el.content.as_ref().unwrap();
    assert_eq!(
        text_of(&Document {
            blocks: content.clone(),
            ..doc.clone()
        }),
        "value is 5"
    );
}

#[test]
fn a_reference_is_answered_from_the_document_the_resolver_holds() {
    // `#c1` is looked up through the resolver, which holds `here.tmt` as it
    // was written -- not through the document this pass is rewriting.
    let (doc, unresolved) = run("n=${#c1.prop.n} extra=${#c1.prop.extra}\n");
    assert_eq!(text_of(&doc), "n=1 extra=yes");
    assert!(unresolved.is_empty());
}

#[test]
fn creating_a_document_reads_vars_and_generates() {
    let vars = EvalValue::Map(vec![("title".into(), EvalValue::String("Plan".into()))]);
    let (doc, unresolved) = run_with(
        "@meta{ id: ${generate.uuid()}, title: ${vars.title} }\n\nPlan for ${vars.title}\n",
        Phase::Create,
        &vars,
    );
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let Block::Element(el) = &doc.blocks[0] else {
        panic!("meta");
    };
    let Some(ElementValue::Group(entries)) = &el.value else {
        panic!("a group");
    };
    let tomet_ast::Entry::Pair(_, Value::String(id)) = &entries[0] else {
        panic!("a string id, got {entries:?}");
    };
    assert_eq!(id.len(), 36, "a UUID: {id}");
    assert_eq!(
        entries[1],
        tomet_ast::Entry::Pair("title".into(), Value::String("Plan".into()))
    );
    assert_eq!(
        text_of(&Document {
            blocks: doc.blocks[1..].to_vec(),
            ..doc.clone()
        }),
        "Plan for Plan"
    );
}

#[test]
fn creation_only_names_are_refused_when_rendering() {
    let (doc, unresolved) = run("@meta{ id: ${generate.uuid()} }\n");
    assert!(reasons(&unresolved)[0].contains("when a document is being created"));
    let Block::Element(el) = &doc.blocks[0] else {
        panic!("meta");
    };
    // The expression is still there.
    let Some(ElementValue::Group(entries)) = &el.value else {
        panic!("a group");
    };
    assert!(matches!(
        &entries[0],
        tomet_ast::Entry::Pair(_, Value::Element(_))
    ));
}

#[test]
fn with_no_vault_a_reference_to_another_document_fails_and_stays() {
    let mut doc = tomet_parser::parse_document("see ${tmt(\"x.tmt\")#a.id}\n").unwrap();
    let macros = HashMap::new();
    let vars = EvalValue::Null;
    let cx = EvalContext {
        phase: Phase::Output,
        resolver: &NoResolver,
        generate: &SystemGenerate,
        self_doc: DocRef("here.tmt".into()),
        macros: &macros,
        vars: &vars,
    };
    let unresolved = resolve_expressions(&mut doc, &cx);
    assert_eq!(unresolved.len(), 1);
    assert_eq!(text_of(&doc), "see ${tmt(\"x.tmt\")#a.id}");
}

#[test]
fn a_macro_in_an_element_argument_is_expanded_and_stays_text() {
    // What `expand_document_macros` did, for the one binding that called it.
    let (doc, unresolved) = run("- @link(${macro.gh(7)})\n");
    assert!(unresolved.is_empty(), "{unresolved:?}");
    let Block::List(list) = &doc.blocks[0] else {
        panic!("a list");
    };
    let content = list.items[0].element.content.as_ref().unwrap();
    let [Block::Paragraph(p)] = content.as_slice() else {
        panic!("a paragraph");
    };
    let Inline::Element(link) = &p.content[0] else {
        panic!("a link");
    };
    assert_eq!(link.args, Some(Value::String("https://x/issues/7".into())));
}
