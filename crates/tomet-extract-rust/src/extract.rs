use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tomet_ast::{
    Block, Document, ElementValue, Inline, Paragraph, Placement, Section, Sigil, Span, Text, Value,
};
use tomet_tree::{ElementExt, element_new};

/// Options controlling extraction of Rust code documentation.
#[derive(Debug, Clone, Default)]
pub struct RustExtractOptions {
    /// Include private items as well as public (`pub`) items.
    pub private: bool,
}

/// In-memory representation of a Rust crate source code.
#[derive(Debug, Clone)]
pub struct RustCrateSource {
    /// The crate name (e.g. "tomet-parser").
    pub crate_name: String,
    /// The root source file path (e.g. "src/lib.rs").
    pub root_file: PathBuf,
    /// In-memory mapping of relative file paths to their source text contents.
    pub files: HashMap<PathBuf, String>,
}

/// Extracts documentation for a single Rust source code string into a Tomet [`Document`].
pub fn extract_file_doc(
    crate_name: &str,
    source: &str,
    options: &RustExtractOptions,
) -> Result<Document, String> {
    let mut files = HashMap::new();
    let root_file = PathBuf::from("src/lib.rs");
    files.insert(root_file.clone(), source.to_string());

    let crate_source = RustCrateSource {
        crate_name: crate_name.to_string(),
        root_file,
        files,
    };

    extract_crate_doc(&crate_source, options)
}

/// Extracts documentation from an entire Rust crate into a single unified Tomet [`Document`].
pub fn extract_crate_doc(
    crate_source: &RustCrateSource,
    options: &RustExtractOptions,
) -> Result<Document, String> {
    let root_content = crate_source
        .files
        .get(&crate_source.root_file)
        .ok_or_else(|| {
            format!(
                "root file '{}' not found in crate source files",
                crate_source.root_file.display()
            )
        })?;

    let file_ast: syn::File = syn::parse_file(root_content)
        .map_err(|e| format!("failed to parse {}: {e}", crate_source.root_file.display()))?;

    let mut blocks = Vec::new();

    // 1. Header elements: @kind(api) and @meta{ crate: "..." }
    let mut kind_el = element_new(Sigil::named("kind")).with_placement(Placement::Block);
    kind_el.args = Some(Value::String("api".to_string()));
    blocks.push(Block::Element(kind_el));

    let mut meta_el = element_new(Sigil::named("meta")).with_placement(Placement::Block);
    meta_el.value = Some(ElementValue::from_map(Value::Map(vec![(
        "crate".to_string(),
        Value::String(crate_source.crate_name.clone()),
    )])));
    blocks.push(Block::Element(meta_el));

    // 2. Root crate section: =[ crate_name ]
    blocks.push(Block::Section(Section::new(
        1,
        vec![text(&crate_source.crate_name)],
        dummy(),
    )));

    // 3. Root inner doc comments (//! ...)
    let root_docs = extract_docs(&file_ast.attrs, true);
    if !root_docs.is_empty() {
        blocks.extend(parse_doc_to_blocks(&root_docs));
    }

    // 4. Collect and process items in the root module & submodules
    let mut collector = CrateItemCollector {
        crate_source,
        options,
    };

    collector.process_items(&file_ast.items, &crate_source.root_file, 2, &mut blocks)?;

    Ok(Document::new(blocks, dummy()))
}

struct CrateItemCollector<'a> {
    crate_source: &'a RustCrateSource,
    options: &'a RustExtractOptions,
}

impl<'a> CrateItemCollector<'a> {
    fn process_items(
        &mut self,
        items: &[syn::Item],
        current_file: &Path,
        section_level: usize,
        out: &mut Vec<Block>,
    ) -> Result<(), String> {
        let mut modules = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut traits = Vec::new();
        let mut functions = Vec::new();
        let mut type_aliases = Vec::new();
        let mut constants = Vec::new();

        for item in items {
            match item {
                syn::Item::Mod(m) => {
                    if self.is_visible(&m.vis) && !is_doc_hidden(&m.attrs) {
                        modules.push(m);
                    }
                }
                syn::Item::Struct(s) => {
                    if self.is_visible(&s.vis) && !is_doc_hidden(&s.attrs) {
                        structs.push(s);
                    }
                }
                syn::Item::Enum(e) => {
                    if self.is_visible(&e.vis) && !is_doc_hidden(&e.attrs) {
                        enums.push(e);
                    }
                }
                syn::Item::Trait(t) => {
                    if self.is_visible(&t.vis) && !is_doc_hidden(&t.attrs) {
                        traits.push(t);
                    }
                }
                syn::Item::Fn(f) => {
                    if self.is_visible(&f.vis) && !is_doc_hidden(&f.attrs) {
                        functions.push(f);
                    }
                }
                syn::Item::Type(t) => {
                    if self.is_visible(&t.vis) && !is_doc_hidden(&t.attrs) {
                        type_aliases.push(t);
                    }
                }
                syn::Item::Const(c) => {
                    if self.is_visible(&c.vis) && !is_doc_hidden(&c.attrs) {
                        constants.push((c.ident.to_string(), &c.attrs));
                    }
                }
                syn::Item::Static(s) => {
                    if self.is_visible(&s.vis) && !is_doc_hidden(&s.attrs) {
                        constants.push((s.ident.to_string(), &s.attrs));
                    }
                }
                _ => {}
            }
        }

        // Modules
        for m in modules {
            let mod_name = m.ident.to_string();
            out.push(Block::Section(Section::new(
                section_level,
                vec![text(&format!("Module {mod_name}"))],
                dummy(),
            )));

            let mod_docs = extract_docs(&m.attrs, false);
            if !mod_docs.is_empty() {
                out.extend(parse_doc_to_blocks(&mod_docs));
            }

            if let Some((_, inline_items)) = &m.content {
                let inner_docs = extract_docs(&m.attrs, true);
                if !inner_docs.is_empty() {
                    out.extend(parse_doc_to_blocks(&inner_docs));
                }
                self.process_items(inline_items, current_file, section_level + 1, out)?;
            } else {
                // Out-of-line module: locate target file
                let candidates = resolve_submodule_path(current_file, &mod_name);
                let found_file = candidates
                    .iter()
                    .find(|path| self.crate_source.files.contains_key(*path));

                if let Some(target_file) = found_file {
                    let content = &self.crate_source.files[target_file];
                    let sub_ast = syn::parse_file(content).map_err(|e| {
                        format!("failed to parse module {}: {e}", target_file.display())
                    })?;

                    let inner_docs = extract_docs(&sub_ast.attrs, true);
                    if !inner_docs.is_empty() {
                        out.extend(parse_doc_to_blocks(&inner_docs));
                    }

                    self.process_items(&sub_ast.items, target_file, section_level + 1, out)?;
                }
            }
        }

        // Structs
        if !structs.is_empty() {
            out.push(Block::Section(Section::new(
                section_level,
                vec![text("Structs")],
                dummy(),
            )));
            for s in structs {
                let name = s.ident.to_string();
                out.push(Block::Section(Section::new(
                    section_level + 1,
                    vec![text(&format!("struct {name}"))],
                    dummy(),
                )));
                let docs = extract_docs(&s.attrs, false);
                if !docs.is_empty() {
                    out.extend(parse_doc_to_blocks(&docs));
                }
            }
        }

        // Enums
        if !enums.is_empty() {
            out.push(Block::Section(Section::new(
                section_level,
                vec![text("Enums")],
                dummy(),
            )));
            for e in enums {
                let name = e.ident.to_string();
                out.push(Block::Section(Section::new(
                    section_level + 1,
                    vec![text(&format!("enum {name}"))],
                    dummy(),
                )));
                let docs = extract_docs(&e.attrs, false);
                if !docs.is_empty() {
                    out.extend(parse_doc_to_blocks(&docs));
                }
            }
        }

        // Traits
        if !traits.is_empty() {
            out.push(Block::Section(Section::new(
                section_level,
                vec![text("Traits")],
                dummy(),
            )));
            for t in traits {
                let name = t.ident.to_string();
                out.push(Block::Section(Section::new(
                    section_level + 1,
                    vec![text(&format!("trait {name}"))],
                    dummy(),
                )));
                let docs = extract_docs(&t.attrs, false);
                if !docs.is_empty() {
                    out.extend(parse_doc_to_blocks(&docs));
                }
            }
        }

        // Functions
        if !functions.is_empty() {
            out.push(Block::Section(Section::new(
                section_level,
                vec![text("Functions")],
                dummy(),
            )));
            for f in functions {
                let name = f.sig.ident.to_string();
                out.push(Block::Section(Section::new(
                    section_level + 1,
                    vec![text(&format!("fn {name}"))],
                    dummy(),
                )));
                let docs = extract_docs(&f.attrs, false);
                if !docs.is_empty() {
                    out.extend(parse_doc_to_blocks(&docs));
                }
            }
        }

        // Type Aliases
        if !type_aliases.is_empty() {
            out.push(Block::Section(Section::new(
                section_level,
                vec![text("Type Aliases")],
                dummy(),
            )));
            for t in type_aliases {
                let name = t.ident.to_string();
                out.push(Block::Section(Section::new(
                    section_level + 1,
                    vec![text(&format!("type {name}"))],
                    dummy(),
                )));
                let docs = extract_docs(&t.attrs, false);
                if !docs.is_empty() {
                    out.extend(parse_doc_to_blocks(&docs));
                }
            }
        }

        // Constants / Statics
        if !constants.is_empty() {
            out.push(Block::Section(Section::new(
                section_level,
                vec![text("Constants")],
                dummy(),
            )));
            for (name, attrs) in constants {
                out.push(Block::Section(Section::new(
                    section_level + 1,
                    vec![text(&format!("const {name}"))],
                    dummy(),
                )));
                let docs = extract_docs(attrs, false);
                if !docs.is_empty() {
                    out.extend(parse_doc_to_blocks(&docs));
                }
            }
        }

        Ok(())
    }

    fn is_visible(&self, vis: &syn::Visibility) -> bool {
        if self.options.private {
            return true;
        }
        matches!(vis, syn::Visibility::Public(_))
    }
}

/// Resolves candidate file paths for a submodule `mod <name>;`.
fn resolve_submodule_path(current_file: &Path, mod_name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let parent = current_file.parent().unwrap_or(Path::new(""));
    let file_stem = current_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if file_stem == "lib" || file_stem == "main" || file_stem == "mod" {
        candidates.push(parent.join(format!("{mod_name}.rs")));
        candidates.push(parent.join(mod_name).join("mod.rs"));
    } else {
        candidates.push(parent.join(file_stem).join(format!("{mod_name}.rs")));
        candidates.push(parent.join(file_stem).join(mod_name).join("mod.rs"));
    }
    candidates
}

/// Extracts documentation strings from attributes.
fn extract_docs(attrs: &[syn::Attribute], inner: bool) -> String {
    let mut doc_lines = Vec::new();
    for attr in attrs {
        let is_target_style = if inner {
            matches!(attr.style, syn::AttrStyle::Inner(_))
        } else {
            matches!(attr.style, syn::AttrStyle::Outer)
        };
        if !is_target_style {
            continue;
        }
        if let syn::Meta::NameValue(meta) = &attr.meta {
            if meta.path.is_ident("doc") {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit_str),
                    ..
                }) = &meta.value
                {
                    let val = lit_str.value();
                    let line = val.strip_prefix(' ').unwrap_or(&val);
                    doc_lines.push(line.to_string());
                }
            }
        }
    }
    if doc_lines.is_empty() {
        String::new()
    } else {
        doc_lines.join("\n") + "\n"
    }
}

/// Checks if an item has `#[doc(hidden)]`.
fn is_doc_hidden(attrs: &[syn::Attribute]) -> bool {
    for attr in attrs {
        if let syn::Meta::List(meta) = &attr.meta {
            if meta.path.is_ident("doc") {
                if let Ok(nested) = attr.parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                ) {
                    for m in nested {
                        if m.path().is_ident("hidden") {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Parses extracted doc comment text into Tomet AST blocks.
fn parse_doc_to_blocks(doc_text: &str) -> Vec<Block> {
    let trimmed = doc_text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    match tomet_parser::parse_document(trimmed) {
        Ok(doc) => doc.blocks,
        Err(_) => vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Text(Text::new(trimmed, dummy()))],
            dummy(),
        ))],
    }
}

fn dummy() -> Span {
    Span::dummy()
}

fn text(s: &str) -> Inline {
    Inline::Text(Text::new(s, dummy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_crate() {
        let src = r#"//! Top level module documentation.
//! See @file(docs/spec.tmt).

/// A public struct representation.
pub struct MyStruct {
    pub field: u32,
}

struct PrivateStruct;

/// A public function.
pub fn calculate() -> u32 {
    42
}

fn private_helper() {}
"#;
        let doc = extract_file_doc("my-crate", src, &RustExtractOptions::default()).unwrap();
        assert!(!doc.blocks.is_empty());

        // Print to verify structure
        let printed = tomet_printer::document_to_tm(&doc);
        eprintln!("PRINTED:\n{printed}");
        assert!(printed.contains("=[my-crate]"));
        assert!(printed.contains("Top level module documentation."));
        assert!(printed.contains("==[Structs]"));
        assert!(printed.contains("===[struct MyStruct]"));
        assert!(printed.contains("A public struct representation."));
        assert!(printed.contains("==[Functions]"));
        assert!(printed.contains("===[fn calculate]"));
        assert!(!printed.contains("PrivateStruct"));
        assert!(!printed.contains("private_helper"));
    }

    #[test]
    fn test_extract_with_private_option() {
        let src = r#"
struct PrivateStruct;
fn private_helper() {}
"#;
        let doc = extract_file_doc("my-crate", src, &RustExtractOptions { private: true }).unwrap();
        let printed = tomet_printer::document_to_tm(&doc);
        assert!(printed.contains("PrivateStruct"));
        assert!(printed.contains("private_helper"));
    }

    #[test]
    fn test_extract_doc_hidden_is_ignored() {
        let src = r#"
#[doc(hidden)]
pub struct HiddenStruct;

pub struct VisibleStruct;
"#;
        let doc = extract_file_doc("my-crate", src, &RustExtractOptions::default()).unwrap();
        let printed = tomet_printer::document_to_tm(&doc);
        assert!(!printed.contains("HiddenStruct"));
        assert!(printed.contains("VisibleStruct"));
    }

    #[test]
    fn test_extract_submodule() {
        let lib_src = r#"//! Crate root.
pub mod sub;
"#;
        let sub_src = r#"//! Submodule documentation.
pub fn sub_fn() {}
"#;
        let mut files = HashMap::new();
        files.insert(PathBuf::from("src/lib.rs"), lib_src.to_string());
        files.insert(PathBuf::from("src/sub.rs"), sub_src.to_string());

        let crate_source = RustCrateSource {
            crate_name: "test-crate".to_string(),
            root_file: PathBuf::from("src/lib.rs"),
            files,
        };

        let doc = extract_crate_doc(&crate_source, &RustExtractOptions::default()).unwrap();
        let printed = tomet_printer::document_to_tm(&doc);
        assert!(printed.contains("Module sub"));
        assert!(printed.contains("Submodule documentation."));
        assert!(printed.contains("fn sub_fn"));
    }

    #[test]
    fn test_extract_tomet_elements_in_doc_comments() {
        let src = r#"//! Module root.
//!
//! @callout(info)[Note]{Important invariant.}

/// A function using tomet markup in doc comment.
///
/// - First item
/// - Second item
///
/// See @file(docs/spec.tmt).
pub fn run() {}
"#;
        let doc = extract_file_doc("tomet-test", src, &RustExtractOptions::default()).unwrap();
        let printed = tomet_printer::document_to_tm(&doc);
        assert!(printed.contains("@callout(info)"));
        assert!(printed.contains("Important invariant."));
        assert!(printed.contains("- First item"));
        assert!(printed.contains("@file(docs/spec.tmt)"));
    }
}
