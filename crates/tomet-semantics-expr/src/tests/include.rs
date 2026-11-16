//! `include`: a document, a section, headings, cycles and depth.

use super::*;

const GUIDE: &str = "::kind(note)
::meta(title: Guide)

=[ Install ]#(install)

Run it.

==[ Linux ]#(linux)

Use apt.

==[ Mac ]#(mac)

Use brew.

=[ Usage ]#(usage)

See @link(./img/a.png) and @link(../shared/s.tmt#top) and @link(tmt:abs.tmt).

@card(n: 1)#(c1)
";

fn guide_vault() -> MemoryResolver {
    MemoryResolver::new()
        .with_source("docs/guide.tmt", GUIDE)
        .unwrap()
        .with_source("here.tmt", "::meta(title: Here)\n")
        .unwrap()
        .with_source("docs/sub/host.tmt", "Host\n")
        .unwrap()
        .with_source("docs/other.tmt", "Other\n")
        .unwrap()
}

/// Evaluates `src` as written in `host`, under a heading of `level`, with
/// `includes` being included already.
fn include_in(
    host: &str,
    level: usize,
    includes: &[DocRef],
    src: &str,
) -> Result<EvalValue, EvalError> {
    let resolver = guide_vault();
    let macros = HashMap::new();
    let vars = EvalValue::Scalar(Scalar::None);
    let cx = EvalContext {
        resolver: &resolver,
        self_doc: DocRef(host.to_string()),
        macros: &macros,
        var: &vars,
        var_unset: &[],
        includes,
        heading_level: level,
        notes: Default::default(),
    };
    evaluate(&parse(src), &cx)
}

fn fragment(value: EvalValue) -> Vec<tomet_ast::Block> {
    match value {
        EvalValue::Fragment(blocks) => blocks,
        other => panic!("expected a fragment, got {other:?}"),
    }
}

/// `level:title` for each section and `p:text` for each plain paragraph, in
/// document order, nested sections after their parent.
fn outline(blocks: &[tomet_ast::Block]) -> Vec<String> {
    let text = |inlines: &[tomet_ast::Inline]| -> String {
        inlines
            .iter()
            .filter_map(|i| match i {
                tomet_ast::Inline::Text(t) => Some(t.value.as_str()),
                _ => None,
            })
            .collect()
    };
    let mut out = Vec::new();
    for block in blocks {
        match block {
            tomet_ast::Block::Section(s) => {
                out.push(format!("{}:{}", s.level, text(&s.title).trim()));
                out.extend(outline(&s.blocks));
            }
            tomet_ast::Block::Paragraph(p) => {
                let t = text(&p.content);
                if !t.trim().is_empty() && !t.starts_with("See ") {
                    out.push(format!("p:{}", t.trim()));
                }
            }
            tomet_ast::Block::Element(el) => out.push(format!("e:{}", el.name.to_string())),
            _ => {}
        }
    }
    out
}

#[test]
fn a_document_is_included_without_its_own_declarations() {
    let blocks =
        fragment(include_in("here.tmt", 0, &[], "include(tmt(\"docs/guide.tmt\"))").unwrap());
    let all = outline(&blocks);
    assert_eq!(
        all,
        [
            "1:Install",
            "p:Run it.",
            "2:Linux",
            "p:Use apt.",
            "2:Mac",
            "p:Use brew.",
            "1:Usage",
            "e:card"
        ]
    );
    // `::kind` and `::meta` are the document's own business.
    assert!(!all.iter().any(|line| line == "e:kind" || line == "e:meta"));
}

#[test]
fn included_headings_go_one_below_the_heading_they_land_under() {
    let blocks =
        fragment(include_in("here.tmt", 2, &[], "include(tmt(\"docs/guide.tmt\"))").unwrap());
    let all = outline(&blocks);
    assert!(all.contains(&"3:Install".to_string()), "{all:?}");
    assert!(all.contains(&"4:Linux".to_string()), "{all:?}");
    assert!(all.contains(&"3:Usage".to_string()), "{all:?}");
}

#[test]
fn level_sets_the_level_of_the_top_headings_outright() {
    let blocks = fragment(
        include_in(
            "here.tmt",
            3,
            &[],
            "include(tmt(\"docs/guide.tmt\"), level: 2)",
        )
        .unwrap(),
    );
    let all = outline(&blocks);
    assert!(all.contains(&"2:Install".to_string()), "{all:?}");
    assert!(all.contains(&"3:Linux".to_string()), "{all:?}");
}

#[test]
fn an_id_takes_the_whole_section() {
    let blocks = fragment(
        include_in(
            "here.tmt",
            1,
            &[],
            "include(tmt(\"docs/guide.tmt\")#install)",
        )
        .unwrap(),
    );
    assert_eq!(
        outline(&blocks),
        [
            "2:Install",
            "p:Run it.",
            "3:Linux",
            "p:Use apt.",
            "3:Mac",
            "p:Use brew."
        ]
    );
}

#[test]
fn heading_false_takes_the_body_and_puts_the_subsections_directly_below() {
    let blocks = fragment(
        include_in(
            "here.tmt",
            1,
            &[],
            "include(tmt(\"docs/guide.tmt\")#install, heading: false)",
        )
        .unwrap(),
    );
    assert_eq!(
        outline(&blocks),
        ["p:Run it.", "2:Linux", "p:Use apt.", "2:Mac", "p:Use brew."]
    );
}

#[test]
fn an_element_without_a_section_is_just_that_element() {
    let blocks =
        fragment(include_in("here.tmt", 1, &[], "include(tmt(\"docs/guide.tmt\")#c1)").unwrap());
    assert_eq!(outline(&blocks), ["e:card"]);
}

#[test]
fn include_checks_what_it_is_given() {
    let err = include_in(
        "here.tmt",
        0,
        &[],
        "include(tmt(\"docs/guide.tmt\"), heading: false)",
    )
    .unwrap_err();
    assert!(err.to_string().contains("section"), "{err}");
    let err = include_in(
        "here.tmt",
        0,
        &[],
        "include(tmt(\"docs/guide.tmt\")#c1, heading: false)",
    )
    .unwrap_err();
    assert!(err.to_string().contains("section"), "{err}");
    assert!(matches!(
        include_in("here.tmt", 0, &[], "include(1)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
    assert!(
        include_in(
            "here.tmt",
            0,
            &[],
            "include(tmt(\"docs/guide.tmt\"), level: 0)"
        )
        .is_err()
    );
    assert!(
        include_in(
            "here.tmt",
            0,
            &[],
            "include(tmt(\"docs/guide.tmt\"), depth: 2)"
        )
        .is_err()
    );
    assert!(include_in("here.tmt", 0, &[], "include()").is_err());
    assert!(include_in("here.tmt", 0, &[], "include(tmt(\"nope.tmt\"))").is_err());
}

#[test]
fn a_document_already_being_included_is_a_cycle() {
    let chain = [
        DocRef("docs/other.tmt".into()),
        DocRef("docs/guide.tmt".into()),
    ];
    let err = include_in(
        "docs/other.tmt",
        0,
        &chain,
        "include(tmt(\"docs/guide.tmt\"))",
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("cycle"), "{message}");
    assert!(
        message.contains("docs/other.tmt -> docs/guide.tmt -> docs/guide.tmt"),
        "{message}"
    );
}

#[test]
fn includes_nested_too_deep_stop() {
    let chain: Vec<DocRef> = (0..MAX_INCLUDE_DEPTH)
        .map(|i| DocRef(format!("n{i}.tmt")))
        .collect();
    let err = include_in("here.tmt", 0, &chain, "include(tmt(\"docs/guide.tmt\"))").unwrap_err();
    assert!(err.to_string().contains("nested"), "{err}");
}
