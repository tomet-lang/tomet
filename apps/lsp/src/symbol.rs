use lsp_types::{DocumentSymbol, SymbolKind};
use tomet_ast::{Block, Connect, Declaration, Element, Inline, Interpolation, List, Sigil};
use tomet_semantics::{ElementKind, classify_std_lenient, heading_level};

use crate::position::TextPositions;

/// Provides document symbols for outline navigation in the editor.
pub fn document_symbols_for(text: &str) -> Vec<DocumentSymbol> {
    let Ok(doc) = tomet_parser::parse_document(text) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();
    let positions = TextPositions::new(text);
    for block in &doc.blocks {
        collect_block_symbols(&positions, block, &mut symbols);
    }
    symbols
}

fn collect_block_symbols(
    positions: &TextPositions<'_>,
    block: &Block,
    symbols: &mut Vec<DocumentSymbol>,
) {
    match block {
        Block::Section(sec) => {
            let range = positions.range(&sec.span);
            let title = extract_inlines_text(&sec.title);
            let name = if title.is_empty() {
                "=".repeat(sec.level)
            } else {
                format!("{} {}", "=".repeat(sec.level), title)
            };
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
                collect_inline_symbols(positions, inline, symbols);
            }
            for conn in &sec.connects {
                collect_connect_symbol(positions, conn, symbols);
            }
            for child in &sec.blocks {
                collect_block_symbols(positions, child, symbols);
            }
        }
        Block::Element(el) => collect_element_symbol(positions, el, symbols),
        Block::Paragraph(p) => {
            for inline in &p.content {
                collect_inline_symbols(positions, inline, symbols);
            }
        }
        Block::List(list) => collect_list_symbol(positions, list, symbols),
        Block::Interp(interp) => collect_interp_symbol(positions, interp, symbols),
        Block::Declaration(declaration) => {
            collect_declaration_symbol(positions, declaration, symbols)
        }
        Block::Hole(_) => {}
    }
}

fn collect_list_symbol(
    positions: &TextPositions<'_>,
    list: &List,
    symbols: &mut Vec<DocumentSymbol>,
) {
    for item in &list.items {
        collect_element_symbol(positions, &item.element, symbols);
        if let Some(sub) = &item.sublist {
            collect_list_symbol(positions, sub, symbols);
        }
    }
}

fn collect_element_symbol(
    positions: &TextPositions<'_>,
    el: &Element,
    symbols: &mut Vec<DocumentSymbol>,
) {
    let range = positions.range(&el.span);
    let kind = classify_std_lenient(el);
    let symbol = if kind == ElementKind::Heading {
        let level = heading_level(el).unwrap_or(1);
        let title = extract_blocks_text(el.content.as_deref().unwrap_or(&[]));
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
        let text = extract_blocks_text(el.content.as_deref().unwrap_or(&[]));
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
            detail: el.data.args.as_ref().map(|a| format!("{a:?}")),
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
        for block in content {
            collect_block_symbols(positions, block, symbols);
        }
    }
    for conn in &el.connects {
        collect_connect_symbol(positions, conn, symbols);
    }
}

fn collect_declaration_symbol(
    positions: &TextPositions<'_>,
    declaration: &Declaration,
    symbols: &mut Vec<DocumentSymbol>,
) {
    let range = positions.range(&declaration.span);
    #[allow(deprecated)]
    symbols.push(DocumentSymbol {
        name: format!("::{}", declaration.name),
        detail: declaration.data.args.as_ref().map(|a| format!("{a:?}")),
        kind: SymbolKind::OBJECT,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    });
    for block in declaration.content.iter().flatten() {
        collect_block_symbols(positions, block, symbols);
    }
}

fn collect_connect_symbol(
    positions: &TextPositions<'_>,
    conn: &Connect,
    symbols: &mut Vec<DocumentSymbol>,
) {
    let range = positions.range(&conn.span);
    #[allow(deprecated)]
    symbols.push(DocumentSymbol {
        name: format!(":{}", conn.name),
        detail: conn.data.args.as_ref().map(|a| format!("{a:?}")),
        kind: SymbolKind::OBJECT,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    });
    for block in conn.content.iter().flatten() {
        collect_block_symbols(positions, block, symbols);
    }
}

fn collect_interp_symbol(
    positions: &TextPositions<'_>,
    interp: &Interpolation,
    symbols: &mut Vec<DocumentSymbol>,
) {
    let range = positions.range(&interp.span);
    #[allow(deprecated)]
    symbols.push(DocumentSymbol {
        name: "${...}".to_string(),
        detail: None,
        kind: SymbolKind::OBJECT,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    });
}

fn collect_inline_symbols(
    positions: &TextPositions<'_>,
    inline: &Inline,
    symbols: &mut Vec<DocumentSymbol>,
) {
    match inline {
        Inline::Element(el) => collect_element_symbol(positions, el, symbols),
        Inline::Interp(interp) => collect_interp_symbol(positions, interp, symbols),
        _ => {}
    }
}

pub(crate) fn list_item_marker_text(item: &Element) -> String {
    if let Some(marker) = &item.data.args {
        return format!("({marker:?})");
    }
    "-".to_string()
}

/// [`extract_inlines_text`] over `Element.content`'s `Vec<Block>` shape.
pub(crate) fn extract_blocks_text(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Paragraph(p) => out.push_str(&extract_inlines_text(&p.content)),
            Block::Element(el) => {
                if let Some(content) = &el.content {
                    out.push_str(&extract_blocks_text(content));
                }
            }
            Block::Section(sec) => out.push_str(&extract_inlines_text(&sec.title)),
            Block::List(list) => {
                for item in &list.items {
                    if let Some(content) = &item.element.content {
                        out.push_str(&extract_blocks_text(content));
                    }
                }
            }
            Block::Interp(_) => out.push_str("${...}"),
            Block::Declaration(_) | Block::Hole(_) => {}
        }
    }
    out
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
            Inline::Interp(_) => out.push_str("${...}"),
            Inline::Hole(_) => {}
        }
    }
    out
}

pub(crate) fn sigil_display_name(sigil: &Sigil) -> String {
    match sigil {
        Sigil::Named(name) => format!("@{name}"),
        Sigil::Bare => "(bare)".to_string(),
    }
}
