//! Configuration reference generator from [`ConfigGroupSpec`] schemas.

use tomet_ast::{Block, Document, Element, ElementValue, Placement, Section, Sigil, Value};
use tomet_config::{ConfigEntrySpec, ConfigGroupSpec, PrinterConfig};
use tomet_tree::{ElementExt, element_new};

use crate::util::{code, dummy, inlines, list, paragraph, paragraphs, text};

/// Renders a complete formatted configuration specification `.tmt` string from [`ConfigGroupSpec`] slices.
pub fn render(
    groups: &[ConfigGroupSpec],
    doc_id: &str,
    title: &str,
    intro_paragraphs: &[&str],
    config: &PrinterConfig,
) -> String {
    let doc = document(groups, doc_id, title, intro_paragraphs);
    let printed = tomet_printer::document_to_tm_with_config(&doc, config);
    tomet_formatter::format_source_with_config(&printed, config)
}

/// Builds a Tomet [`Document`] AST representing the configuration groups and entries.
pub fn document(
    groups: &[ConfigGroupSpec],
    doc_id: &str,
    title: &str,
    intro_paragraphs: &[&str],
) -> Document {
    let mut meta = element_new(Sigil::document("meta")).with_placement(Placement::Block);
    meta.value = Some(ElementValue::from_map(Value::Map(vec![(
        "id".to_string(),
        Value::String(doc_id.to_string()),
    )])));

    let mut blocks = vec![
        Block::Element(meta),
        Block::Section(Section::new(1, vec![text(title)], dummy())),
    ];

    for intro in intro_paragraphs {
        if !intro.is_empty() {
            blocks.push(paragraph(inlines(intro)));
        }
    }

    for group in groups {
        push_group(group, &mut blocks);
    }

    Document::new(blocks, dummy())
}

/// Appends a level-2 section and entry list for `group` into `out`.
pub fn push_group(group: &ConfigGroupSpec, out: &mut Vec<Block>) {
    out.push(Block::Section(Section::new(
        2,
        vec![text(group.name)],
        dummy(),
    )));

    if !group.description.is_empty() {
        for para in paragraphs(group.description) {
            out.push(paragraph(inlines(&para)));
        }
    }

    if !group.entries.is_empty() {
        out.push(Block::List(list(group.entries.iter().map(entry_item))));
    }
}

/// Builds a list item element describing a single [`ConfigEntrySpec`].
pub fn entry_item(entry: &ConfigEntrySpec) -> Element {
    let mut content = vec![code(entry.key)];

    let mut type_info = format!(" ({})", entry.value_type);
    if let Some(default) = entry.default_value {
        type_info = format!(" ({}, default: `{default}`)", entry.value_type);
    }
    content.extend(inlines(&type_info));

    if !entry.description.is_empty() {
        let desc = entry.description.trim();
        let end = if desc.ends_with(['.', '!', '?']) {
            ""
        } else {
            "."
        };
        content.extend(inlines(&format!(": {desc}{end}")));
    }

    if !entry.aliases.is_empty() {
        content.push(text(" Aliases: "));
        for (i, alias) in entry.aliases.iter().enumerate() {
            if i > 0 {
                content.push(text(", "));
            }
            content.push(code(alias));
        }
        content.push(text("."));
    }

    let mut el = element_new(Sigil::Bare).with_placement(Placement::Block);
    el.content = Some(vec![paragraph(content)]);
    el
}
