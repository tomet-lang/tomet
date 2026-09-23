use lsp_types::{DocumentSymbol, SymbolKind};
use tomet_ast::{Block, Element, Inline, Sigil};
use tomet_semantics::{ElementKind, classify_std_lenient, heading_level};

use crate::position::span_to_range;

/// Provides document symbols for outline navigation in the editor.
pub fn document_symbols_for(text: &str) -> Vec<DocumentSymbol> {
    let Ok(doc) = tomet_parser::parse_document(text) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();
    for block in &doc.blocks {
        collect_block_symbols(block, &mut symbols);
    }
    symbols
}

fn collect_block_symbols(block: &Block, symbols: &mut Vec<DocumentSymbol>) {
    match block {
        Block::Section(sec) => {
            let range = span_to_range(&sec.span);
            let title = extract_inlines_text(&sec.title);
            let name = format!("{} {}", "=".repeat(sec.level), title);
            #[allow(deprecated)]
            symbols.push(DocumentSymbol {
                name,
                detail: Some(format!("Section Level {}", sec.level)),
                kind: SymbolKind::STRING,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children: None,
            });
            for inline in &sec.title {
                collect_inline_symbols(inline, symbols);
            }
            for conn in &sec.connects {
                collect_element_symbol(conn, symbols);
            }
            for child in &sec.blocks {
                collect_block_symbols(child, symbols);
            }
        }
        Block::Element(el) => collect_element_symbol(el, symbols),
        Block::Paragraph(p) => {
            for inline in &p.content {
                collect_inline_symbols(inline, symbols);
            }
        }
    }
}

fn collect_element_symbol(el: &Element, symbols: &mut Vec<DocumentSymbol>) {
    let range = span_to_range(&el.span);
    let kind = classify_std_lenient(el);
    let symbol = if kind == ElementKind::Heading {
        let level = heading_level(el).unwrap_or(1);
        let title = extract_inlines_text(el.content.as_deref().unwrap_or(&[]));
        let name = format!("{} {}", "=".repeat(level as usize), title);
        #[allow(deprecated)]
        DocumentSymbol {
            name,
            detail: Some(format!("Heading Level {level}")),
            kind: SymbolKind::STRING,
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children: None,
        }
    } else if kind == ElementKind::Bare {
        let text = extract_inlines_text(el.content.as_deref().unwrap_or(&[]));
        let marker = list_item_marker_text(el);
        #[allow(deprecated)]
        DocumentSymbol {
            name: format!("{} {}", marker, text),
            detail: None,
            kind: SymbolKind::ENUM_MEMBER,
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children: None,
        }
    } else {
        let name = sigil_display_name(&el.sigil);
        #[allow(deprecated)]
        DocumentSymbol {
            name,
            detail: el.args.as_ref().map(|a| format!("{a:?}")),
            kind: SymbolKind::OBJECT,
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children: None,
        }
    };
    symbols.push(symbol);

    if let Some(content) = &el.content {
        for inline in content {
            collect_inline_symbols(inline, symbols);
        }
    }
    if let Some(children) = &el.children {
        for child in children {
            collect_block_symbols(child, symbols);
        }
    }
    for conn in &el.connects {
        collect_element_symbol(conn, symbols);
    }
}

fn collect_inline_symbols(inline: &Inline, symbols: &mut Vec<DocumentSymbol>) {
    if let Inline::Element(el) = inline {
        collect_element_symbol(el, symbols);
    }
}

pub(crate) fn list_item_marker_text(item: &Element) -> String {
    if let Some(marker) = &item.args {
        return format!("({marker:?})");
    }
    "-".to_string()
}

pub(crate) fn extract_inlines_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for (idx, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::Text(t) => out.push_str(&t.value),
            Inline::Raw(t) => out.push_str(&t.value),
            Inline::SoftBreak(_) => {
                let before = out.chars().last();
                let after = inlines.get(idx + 1).and_then(Inline::first_char);
                out.push_str(tomet_ast::softbreak_join(before, after));
            }
            Inline::LineBreak(_) => out.push(' '),
            Inline::Element(el) => out.push_str(&sigil_display_name(&el.sigil)),
        }
    }
    out
}

pub(crate) fn sigil_display_name(sigil: &Sigil) -> String {
    match sigil {
        Sigil::Named(name) => format!("@{name}"),
        Sigil::Bare => "(bare)".to_string(),
        Sigil::Dollar => "${...}".to_string(),
        Sigil::Caret(name) => match name {
            Some(n) => format!("^{n}"),
            None => "^(...)".to_string(),
        },
    }
}
