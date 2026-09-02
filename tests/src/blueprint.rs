use std::collections::HashMap;
use std::fs;
use std::path::Path;

use tomet_ast::Value;
use tomet_config::PrinterConfig;
use tomet_validator::validate_against_blueprint;
use tomet_workspace::{create_file_from_template, find_template, list_templates};

#[test]
fn test_blueprint_end_to_end_lifecycle() {
    let temp_dir =
        std::env::temp_dir().join(format!("tm_test_cross_blueprint_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let _ = fs::create_dir_all(&temp_dir);

    // 1. Create a blueprint file
    let tmpl_dir = temp_dir.join("templates");
    fs::create_dir_all(&tmpl_dir).unwrap();

    let blueprint_src = r#"@blueprint(daily-note){
  description: "Daily Standup & Plan"
  vars: {
    author: { type: string, default: "Anonymous" }
  }
}
@meta{
  id: ${uuid()}
  date: ${date("YYYY-MM-DD")}
  author: ${vars.author}
  title: ${title}
}

#[ Goals for ${date("YYYY-MM-DD")} ] {id: goals}
- ( ) First goal

#[ Review ] {id: review}
"#;

    let blueprint_file = tmpl_dir.join("daily-note.tmt");
    fs::write(&blueprint_file, blueprint_src).unwrap();

    // 2. Discover template
    let config = PrinterConfig::default();
    let found = find_template(&temp_dir, "daily-note", &config);
    assert_eq!(found, Some(blueprint_file.clone()));

    let templates = list_templates(&temp_dir, &config);
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].0, "daily-note");

    // 3. Instantiate document via workspace
    let mut vars = HashMap::new();
    vars.insert(
        "vars".to_string(),
        Value::Map(vec![("author".to_string(), Value::String("Alice".into()))]),
    );

    let doc_rel_path = Path::new("journal/2026-09-01.tmt");
    let doc_abs_path =
        create_file_from_template(&temp_dir, "daily-note", doc_rel_path, &vars, &config, false)
            .expect("creation should succeed");

    assert!(doc_abs_path.is_file());
    let doc_content = fs::read_to_string(&doc_abs_path).unwrap();

    // Verify converted #kind(daily-note)
    assert!(doc_content.contains("#kind(daily-note)"));
    assert!(!doc_content.contains("#blueprint"));
    assert!(doc_content.contains("author: Alice"));
    assert!(doc_content.contains("2026-09-01"));

    // 4. Validate generated document against blueprint
    let blueprint_ast = tomet_parser::parse_document(blueprint_src).unwrap();
    let doc_ast = tomet_parser::parse_document(&doc_content).unwrap();

    let errors = validate_against_blueprint(&doc_ast, &blueprint_ast);
    assert!(
        errors.is_empty(),
        "newly generated document must satisfy blueprint: {errors:?}"
    );

    // 5. Test validation failure when a required section is removed
    let corrupted_src = doc_content
        .replace("#[ Review ]", "#[ Freeform Notes ]")
        .replace("{id: review}", "{id: notes}")
        .replace("Review", "Freeform Notes");
    let corrupted_ast = tomet_parser::parse_document(&corrupted_src).unwrap();
    let errors_on_corrupt = validate_against_blueprint(&corrupted_ast, &blueprint_ast);
    assert_eq!(errors_on_corrupt.len(), 1);
    assert!(
        errors_on_corrupt[0]
            .to_string()
            .contains("missing required section `Review` (id: review)")
    );

    // 6. Test converters (HTML, Markdown, Typst ignore @blueprint)
    let html_out = tomet_html::render_body(&blueprint_ast);
    assert!(!html_out.contains("blueprint"));
    let md_out = tomet_markdown::to_markdown(&blueprint_ast);
    assert!(!md_out.contains("blueprint"));
    let typst_out = tomet_typst::to_typst(&blueprint_ast);
    assert!(!typst_out.contains("blueprint"));
}
