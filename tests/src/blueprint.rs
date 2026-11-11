use std::collections::HashMap;
use std::fs;
use std::path::Path;

use tomet_ast::Value;
use tomet_config::{PrinterConfig, RegistryConfig};
use tomet_transform::FixedSource;
use tomet_validator::{Diagnostic, validate_against_blueprint, validate_document};
use tomet_workspace::{
    SeatsNotFilled, create_file_from_blueprint_with, find_blueprint, list_blueprints,
};

/// The moment `tomet new` is run at, and the id it draws, in these tests.
const NOW: &str = "2026-09-01T09:30:00+09:00";
const UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

fn fixed() -> FixedSource {
    FixedSource::at(NOW).expect("a datetime").with_ids([UUID])
}

#[test]
fn test_blueprint_end_to_end_lifecycle() {
    let temp_dir =
        std::env::temp_dir().join(format!("tm_test_cross_blueprint_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let _ = fs::create_dir_all(&temp_dir);

    // 1. Create a blueprint file. Where it sits is up to the vault --
    //    what makes it a blueprint is being declared, and naming itself.
    let tmpl_dir = temp_dir.join(".tomet/blueprints");
    fs::create_dir_all(&tmpl_dir).unwrap();

    let blueprint_src = r#"::kind(blueprint)
::blueprint(daily-note){
  description: "Daily Standup & Plan"
}
::var(
  author: { type: string, default: "Anonymous" }
  title: { type: string, default: "Daily" }
)
::id(<$uuid(default: auto)>)
::meta(
  author: $(var.author)
  title: $(var.title)
  file: $(self::filename)
  created: <$datetime(default: auto)>
)

=[ Goals for $(var.title) ]#(goals)
- ( ) First goal

=[ Review ]#(review)
Reviewed on <$date(default: auto)>.
"#;

    let blueprint_file = tmpl_dir.join("daily-note.blueprint.tmt");
    fs::write(&blueprint_file, blueprint_src).unwrap();

    // 2. Discover it through the declaration, by the name it gives itself
    let config = PrinterConfig {
        registry: RegistryConfig {
            blueprints: vec![".tomet/blueprints/daily-note.blueprint.tmt".to_string()],
            ..Default::default()
        },
        ..PrinterConfig::default()
    };
    let found = find_blueprint(&temp_dir, "daily-note", &config);
    assert_eq!(found, Some(blueprint_file.clone()));

    let blueprints = list_blueprints(&temp_dir, &config);
    assert_eq!(blueprints.len(), 1);
    assert_eq!(blueprints[0].0, "daily-note");

    // The blueprint is a valid document: its type seats are types.
    let blueprint_ast = tomet_ast_parser::parse_file(blueprint_src).unwrap();
    assert_eq!(validate_document(&blueprint_ast), vec![]);

    // 3. Instantiate document via workspace, from a fixed clock and a fixed id
    let mut vars = HashMap::new();
    vars.insert("author".to_string(), Value::String("Alice".into()));

    let doc_rel_path = Path::new("journal/2026-09-01.tmt");
    let created = create_file_from_blueprint_with(
        &temp_dir,
        "daily-note",
        doc_rel_path,
        &vars,
        &config,
        false,
        &fixed(),
    )
    .expect("creation should succeed");
    let doc_abs_path = created.path;

    assert!(doc_abs_path.is_file());
    let doc_content = fs::read_to_string(&doc_abs_path).unwrap();

    // Verify converted ::kind(daily-note)
    assert!(doc_content.contains("::kind(daily-note)"));
    assert!(!doc_content.contains("::blueprint"));
    assert!(doc_content.contains("author: Alice"));
    assert!(doc_content.contains("title: Daily"));
    assert!(doc_content.contains("2026-09-01"));

    // Every seat is filled from its source, and the document has none left.
    assert!(
        doc_content.contains(&format!("::id({UUID})")),
        "{doc_content}"
    );
    assert!(
        doc_content.contains("created: 2026-09-01T09:30:00+09:00"),
        "{doc_content}"
    );
    assert!(
        doc_content.contains("Reviewed on 2026-09-01."),
        "{doc_content}"
    );
    assert!(!doc_content.contains("<$"), "{doc_content}");

    // 4. Validate the created document against the blueprint
    let doc_ast = tomet_ast_parser::parse_file(&doc_content).unwrap();
    assert_eq!(
        validate_document(&doc_ast),
        vec![],
        "a newly created document is a valid document"
    );

    let errors = validate_against_blueprint(&doc_ast, &blueprint_ast);
    assert!(
        errors.is_empty(),
        "a newly created document must satisfy the blueprint: {errors:?}"
    );

    // 5. Test validation failure when a required section is removed
    let corrupted_src = doc_content
        .replace("=[ Review ]", "=[ Freeform Notes ]")
        .replace("#(review)", "#(notes)")
        .replace("Review", "Freeform Notes");
    let corrupted_ast = tomet_ast_parser::parse_file(&corrupted_src).unwrap();
    let errors_on_corrupt = validate_against_blueprint(&corrupted_ast, &blueprint_ast);
    assert_eq!(errors_on_corrupt.len(), 1);
    assert!(
        errors_on_corrupt[0]
            .to_string()
            .contains("missing required section `Review` (id: review)")
    );

    // 6. Test converters (HTML, Markdown, Typst ignore ::blueprint)
    let html_out = tomet_html::render_body(&blueprint_ast);
    assert!(!html_out.contains("blueprint"));
    let md_out = tomet_markdown::to_markdown(&blueprint_ast);
    assert!(!md_out.contains("blueprint"));
    let typst_out = tomet_typst::to_typst(&blueprint_ast);
    assert!(!typst_out.contains("blueprint"));
}

/// A type seat with no value source stops `tomet new`: every such seat is reported, with where it
/// is, and nothing is written.
#[test]
fn a_blueprint_with_a_seat_nothing_fills_creates_nothing() {
    let temp_dir = std::env::temp_dir().join(format!(
        "tm_test_cross_blueprint_stop_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join(".tomet/blueprints")).unwrap();
    fs::write(
        temp_dir.join(".tomet/blueprints/note.blueprint.tmt"),
        "::kind(blueprint)\n::blueprint(note)\n::meta(\n  title: <$string>\n  uid: <$uuid(default: auto)>\n  rank: <$int(default: \"first\")>\n)\n",
    )
    .unwrap();
    let config = PrinterConfig {
        registry: RegistryConfig {
            blueprints: vec![".tomet/blueprints/note.blueprint.tmt".to_string()],
            ..Default::default()
        },
        ..PrinterConfig::default()
    };

    let err = create_file_from_blueprint_with(
        &temp_dir,
        "note",
        Path::new("notes/a.tmt"),
        &HashMap::new(),
        &config,
        false,
        &fixed(),
    )
    .expect_err("a seat has no value source");
    let stopped = err
        .downcast_ref::<SeatsNotFilled>()
        .expect("the type seats are the cause");
    let lines: Vec<usize> = stopped.problems.iter().map(|p| p.at.line).collect();
    assert_eq!(lines, [4, 6]);
    assert!(!temp_dir.join("notes").exists(), "nothing was written");

    // `tomet check` says the same of the blueprint as `tomet new` does, before anyone runs it.
    let blueprint = tomet_ast_parser::parse_file(
        &fs::read_to_string(temp_dir.join(".tomet/blueprints/note.blueprint.tmt")).unwrap(),
    )
    .unwrap();
    let found = validate_document(&blueprint);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        matches!(&found[0], Diagnostic::InvalidTypeSeat { seat, .. } if seat == "<$int(default: \"first\")>"),
        "{found:?}"
    );
    let _ = fs::remove_dir_all(&temp_dir);
}

/// A type seat belongs to a blueprint: written in an ordinary document it is an error, and the
/// blueprint it was copied from is not.
#[test]
fn a_type_seat_in_an_ordinary_document_is_an_error() {
    let found = validate_document(
        &tomet_ast_parser::parse_file("::kind(note)\n::id(<$uuid(default: auto)>)\n").unwrap(),
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        matches!(&found[0], Diagnostic::TypeSeatOutsideBlueprint { seat, .. } if seat == "<$uuid(default: auto)>"),
        "{found:?}"
    );
}
