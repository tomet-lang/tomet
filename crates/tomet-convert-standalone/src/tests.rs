use std::path::Path;

use tomet_config::PrinterConfig;
use tomet_expr::MemoryResolver;
use tomet_vault::Vault;

use super::*;

const PATH: &str = "docs/a.tmt";

/// A vault whose settings define the macro `gh` and the variable `topic`.
fn vault() -> Vault {
    let mut config = PrinterConfig::default();
    config.macros.insert(
        "gh".to_string(),
        Value::String("https://github.com/x/issues/$(1)".to_string()),
    );
    let note = tomet_ast_parser::parse_file("::macro(note: [ Note: $(1) ])\n").expect("parses");
    config
        .macros
        .extend(tomet_declaration::document_macros(&note));
    config.vars.push((
        "topic".to_string(),
        tomet_declaration::VarDecl {
            ty: tomet_type::Type::bare(tomet_type::Kind::String),
            default: Some(Value::String("general".to_string())),
            about: None,
        },
    ));
    Vault::from_sources(config, ".", &[])
}

fn convert(source: &str, options: Options) -> Converted {
    let doc = tomet_ast_parser::parse_file(source).expect("parses");
    let resolver = MemoryResolver::new()
        .with_source(PATH, source)
        .unwrap()
        .with_source("docs/b.tmt", "::meta(title: B)\n\nBody of B.\n")
        .unwrap();
    make_standalone(&vault(), &doc, Path::new(PATH), &resolver, &options)
}

fn printed(converted: &Converted) -> String {
    tomet_printer::document_to_tm(&converted.document)
}

fn treatments(converted: &Converted) -> Vec<Treatment> {
    converted
        .changes
        .iter()
        .map(|change| change.treatment.clone())
        .collect()
}

#[test]
fn a_macro_the_vault_defines_is_written_into_the_file() {
    let converted = convert("See $macro.gh(7).\n", Options::default());
    let text = printed(&converted);
    assert!(text.contains("::macro"), "{text}");
    assert!(text.contains("https://github.com/x/issues/$(1)"), "{text}");
    assert!(
        text.contains("$macro.gh(7)"),
        "the call stays a call: {text}"
    );
    assert!(text.contains("::profile(standalone)"), "{text}");
    assert_eq!(treatments(&converted), vec![Treatment::Carried]);
    assert!(converted.remaining.is_empty(), "{:?}", converted.remaining);
}

#[test]
fn a_block_macro_the_vault_defines_is_written_into_the_file_as_blocks() {
    let converted = convert("$macro.note(\"x\")\n", Options::default());
    let text = printed(&converted);
    assert!(text.contains("::macro"), "{text}");
    assert!(text.contains("Note: $(1)"), "{text}");
    assert!(
        text.contains("$macro.note(\"x\")"),
        "the call stays a call: {text}"
    );
    assert_eq!(treatments(&converted), vec![Treatment::Carried]);
    assert!(converted.remaining.is_empty(), "{:?}", converted.remaining);
}

#[test]
fn a_variable_the_vault_declares_is_written_into_the_file() {
    let converted = convert("About $(var.topic).\n", Options::default());
    let text = printed(&converted);
    assert!(text.contains("::var"), "{text}");
    assert!(text.contains("topic"), "{text}");
    assert!(text.contains("general"), "{text}");
    assert_eq!(treatments(&converted), vec![Treatment::Carried]);
    assert!(converted.remaining.is_empty(), "{:?}", converted.remaining);
}

#[test]
fn a_definition_used_twice_is_written_once() {
    let converted = convert("$macro.gh(1) and $macro.gh(2)\n", Options::default());
    assert_eq!(printed(&converted).matches("issues/$(1)").count(), 1);
}

#[test]
fn where_the_file_sits_is_frozen_to_the_value_it_has() {
    let converted = convert(
        "In $(self::path), named $(self::filename).\n",
        Options::default(),
    );
    let text = printed(&converted);
    assert!(text.contains("In docs/a.tmt, named a.tmt."), "{text}");
    assert!(!text.contains("$("), "{text}");
    assert_eq!(
        treatments(&converted),
        vec![Treatment::Frozen, Treatment::Frozen]
    );
    assert!(converted.remaining.is_empty(), "{:?}", converted.remaining);
}

#[test]
fn an_expression_with_one_part_frozen_is_frozen_whole_and_nothing_is_carried_for_it() {
    let converted = convert(
        "$macro.gh(1) at $(self::path), $macro.gh(self::path)\n",
        Options::default(),
    );
    let text = printed(&converted);
    // The first call is carried; the one with `self.path` in it is a value.
    assert_eq!(text.matches("issues/$(1)").count(), 1, "{text}");
    assert!(
        text.contains("https://github.com/x/issues/docs/a.tmt"),
        "{text}"
    );
}

#[test]
fn another_documents_content_is_taken_in() {
    let converted = convert("$include(tmt(\"docs/b.tmt\"))\n", Options::default());
    let text = printed(&converted);
    assert!(text.contains("Body of B."), "{text}");
    assert!(!text.contains("include"), "{text}");
    assert_eq!(
        treatments(&converted),
        vec![Treatment::Frozen, Treatment::Frozen]
    );
    assert!(converted.remaining.is_empty(), "{:?}", converted.remaining);
}

#[test]
fn an_expression_with_no_value_in_the_vault_is_left_as_it_is() {
    let converted = convert(
        "{$ tmt(\"docs/gone.tmt\")::meta.title } and $macro.nowhere(1)\n",
        Options::default(),
    );
    let text = printed(&converted);
    assert!(
        text.contains("{$ tmt(\"docs/gone.tmt\")::meta.title }"),
        "{text}"
    );
    assert!(text.contains("$macro.nowhere(1)"), "{text}");
    let left = converted
        .changes
        .iter()
        .filter(|change| matches!(change.treatment, Treatment::Left { .. }))
        .count();
    assert_eq!(left, 2);
    assert_eq!(converted.remaining.len(), 2);
    // Still promised, so that `tomet check` goes on reporting what is left.
    assert!(text.contains("::profile(standalone)"), "{text}");
}

#[test]
fn a_meta_field_is_left_unless_asked_to_freeze() {
    let source = "::meta(title: Hello)\n\n$(self::meta.title)\n";
    let left = convert(source, Options::default());
    assert!(matches!(&treatments(&left)[..], [Treatment::Left { .. }]));
    assert_eq!(left.remaining.len(), 1);
    assert!(printed(&left).contains("$(self::meta.title)"));

    let frozen = convert(
        source,
        Options {
            freeze_fields: true,
        },
    );
    assert_eq!(treatments(&frozen), vec![Treatment::Frozen]);
    assert!(frozen.remaining.is_empty(), "{:?}", frozen.remaining);
    assert!(printed(&frozen).contains("Hello"));
}

#[test]
fn what_a_file_cannot_hold_yet_is_left_and_reported() {
    let converted = convert("@frobnicate[ x ] and @term(\"word\")\n", Options::default());
    assert_eq!(converted.remaining.len(), 2);
    assert!(
        treatments(&converted)
            .iter()
            .all(|t| matches!(t, Treatment::Left { .. }))
    );
}

#[test]
fn standalone_is_added_to_a_profile_that_is_already_declared() {
    let converted = convert("::profile(oneline)\n\n$(self::path)\n", Options::default());
    let text = printed(&converted);
    assert!(text.contains("::profile(oneline, standalone)"), "{text}");
    assert_eq!(text.matches("::profile").count(), 1, "{text}");
}

#[test]
fn a_file_that_is_already_standalone_is_not_changed() {
    let source = "::profile(standalone)\n::macro(gh: \"https://x/$(1)\")\n\n$macro.gh(1)\n";
    let converted = convert(source, Options::default());
    assert!(converted.changes.is_empty());
    assert!(converted.remaining.is_empty());
    assert_eq!(
        printed(&converted),
        printed(&Converted {
            document: tomet_ast_parser::parse_file(source).unwrap(),
            changes: vec![],
            remaining: vec![],
        })
    );
}

#[test]
fn the_result_has_nothing_left_for_the_validator_but_what_was_reported_left() {
    let converted = convert(
        "$macro.gh(1) $(self::path) $(var.topic) $include(tmt(\"docs/b.tmt\"))\n\n=[ T $(self::filename) ]\n",
        Options::default(),
    );
    assert!(converted.remaining.is_empty(), "{:?}", converted.remaining);
    assert!(
        treatments(&converted)
            .iter()
            .all(|t| !matches!(t, Treatment::Left { .. }))
    );
}

/// The reason for the conversion: the file taken out of its vault gives what it gave inside it.
#[test]
fn outside_its_vault_the_converted_file_renders_as_the_original_did_inside() {
    // One paragraph under the heading: the printer writes two paragraphs of a section with no
    // blank line between, and reading that back gives one (see the task file).
    let source = "\
::meta(title: Hello)

$include(tmt(\"docs/b.tmt\"))

=[ $(self::filename) ]

See $macro.gh(7) about $(var.topic) in $(self::path).
";
    let inside = {
        let vault = vault();
        let mut doc = tomet_ast_parser::parse_file(source).unwrap();
        let resolver = MemoryResolver::new()
            .with_source(PATH, source)
            .unwrap()
            .with_source("docs/b.tmt", "::meta(title: B)\n\nBody of B.\n")
            .unwrap();
        let prepared = vault.prepare(&mut doc, Path::new(PATH), &resolver);
        assert!(prepared.unresolved.is_empty(), "{:?}", prepared.unresolved);
        tomet_markdown::to_markdown(&doc)
    };

    let converted = convert(source, Options::default());
    assert!(converted.remaining.is_empty(), "{:?}", converted.remaining);
    let taken_out = printed(&converted);

    // An empty vault, no other file, and the file at a different place.
    let outside = {
        let vault = Vault::from_sources(PrinterConfig::default(), ".", &[]);
        let mut doc = tomet_ast_parser::parse_file(&taken_out).expect("the printed file parses");
        let resolver = MemoryResolver::new();
        let prepared = vault.prepare(&mut doc, Path::new("elsewhere/renamed.tmt"), &resolver);
        assert!(prepared.unresolved.is_empty(), "{:?}", prepared.unresolved);
        tomet_markdown::to_markdown(&doc)
    };
    assert_eq!(outside, inside);
}
