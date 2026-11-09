use super::*;
use tomet_expr::Reach;

fn findings(source: &str) -> Vec<Dirt> {
    let doc = tomet_ast_parser::parse_file(source).expect("parses");
    standalone_findings(&doc)
        .into_iter()
        .map(|finding| finding.dirt)
        .collect()
}

fn messages(source: &str) -> Vec<String> {
    let doc = tomet_ast_parser::parse_file(source).expect("parses");
    validate_profile(&doc)
        .into_iter()
        .map(|diagnostic| diagnostic.to_string())
        .collect()
}

#[test]
fn a_document_that_needs_nothing_from_outside_has_no_findings() {
    let source = "\
::profile(standalone)
::macro(gh: \"https://x/${1}\")
::var(topic: { type: string, default: \"a\" })

See ${macro.gh(1)} on ${var.topic}, a ${self.kind} and ${add(1, 2)}.
";
    assert_eq!(findings(source), vec![]);
    assert_eq!(messages(source), Vec::<String>::new());
}

#[test]
fn a_document_that_does_not_promise_standalone_is_not_reported() {
    let source = "a ${vault.docs}\n";
    assert_eq!(findings(source).len(), 1);
    assert_eq!(messages(source), Vec::<String>::new());
    assert_eq!(
        messages("::profile(oneline)\n\na ${vault.docs}\n"),
        Vec::<String>::new()
    );
}

#[test]
fn what_the_expression_takes_is_named_in_the_report() {
    let source = "::profile(standalone)\n\na ${macro.gh(1)} b\n";
    assert_eq!(
        findings(source),
        vec![Dirt::Expression(Reach::Macro("gh".into()))]
    );
    assert_eq!(
        messages(source),
        vec![
            "`::profile(standalone)` is declared, but the file needs the macro `gh`, which `::macro` does not define"
                .to_string()
        ]
    );
}

#[test]
fn an_expression_is_found_wherever_it_is_written() {
    let positions = [
        ("running text", "a ${self.path} b\n"),
        ("a line of its own", "${self.path}\n"),
        ("a heading", "=[ T ${self.path} ]\n"),
        ("a list item", "- ${self.path} x\n"),
        ("an element's content", "@note[ ${self.path} ]\n"),
        ("an element's value", "@note(){ k: ${self.path} }\n"),
        ("an element's argument", "@note(k: ${self.path})\n"),
        ("a declaration's data", "::meta(k: ${self.path})\n"),
        ("a section's value", "=[ T ]{ k: ${self.path} }\n"),
        ("a connect", "@note[ x ]::rule(k: ${self.path})\n"),
    ];
    for (place, body) in positions {
        let source = format!("::profile(standalone)\n\n{body}");
        let found = findings(&source);
        assert!(
            found.contains(&Dirt::Expression(Reach::Placement("path"))),
            "{place}: {body:?} gave {found:?}"
        );
    }
}

#[test]
fn an_element_no_vocabulary_is_in_scope_for_is_taken_from_outside() {
    let source = "::profile(standalone)\n::kind(writ)\n\n@frobnicate[ x ]\n";
    assert_eq!(
        findings(source),
        vec![Dirt::Vocabulary {
            name: "frobnicate".into()
        }]
    );
    // What is built in needs no vocabulary.
    assert_eq!(findings("@callout[ x ] and @std.quote[ y ]\n"), vec![]);
}

#[test]
fn a_term_is_taken_from_outside_unless_the_file_is_the_dictionary_that_defines_it() {
    assert_eq!(
        findings("a @term(\"word\") b\n"),
        vec![Dirt::Term {
            target: "word".into()
        }]
    );
    let own = "::dictionary(d)\n\n;@term(\"word\")[ W ]#(w)\n\na @term(\"word\") b @term(d#w)\n";
    assert_eq!(findings(own), vec![]);
    // A definition, a `@term` block of the body with a name, is not a mention.
    assert_eq!(findings("::dictionary(d)\n\n;@term(\"w\")[ W ]\n"), vec![]);
    // A word the file's own dictionary lacks is still outside.
    assert_eq!(
        findings("::dictionary(d)\n\n;@term(\"w\")[ W ]\n\nsee @term(\"other\")\n").len(),
        1
    );
    // A `@term` block in a document that is no dictionary is a mention.
    assert_eq!(findings("::kind(note)\n\n;@term(\"w\")[ W ]\n").len(), 1);
}

#[test]
fn findings_come_in_the_order_they_are_written() {
    let source = "a @term(\"w\") b ${vault.docs} c ${self.path}\n";
    assert_eq!(
        findings(source),
        vec![
            Dirt::Term { target: "w".into() },
            Dirt::Expression(Reach::Vault),
            Dirt::Expression(Reach::Placement("path")),
        ]
    );
}

#[test]
fn a_term_in_a_list_item_is_found_too() {
    assert_eq!(
        findings("-@term(\"word\") in a list item\n"),
        vec![Dirt::Term {
            target: "word".into()
        }]
    );
    let own = "::dictionary(d)\n\n;@term(\"word\")[ W ]#(w)\n\n-@term(d#w) mentioned\n";
    assert_eq!(findings(own), vec![]);
}
