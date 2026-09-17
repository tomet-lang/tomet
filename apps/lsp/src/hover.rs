use std::ops::ControlFlow;

use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range, Uri};
use tomet_ast::{Document, Element, ElementValue, Value};
use tomet_semantics::{classify_std_lenient, heading_level, normalized_element_args, ElementKind};
use tomet_tree::{ValueExt, Visitor, walk_document};

use crate::format::resolve_effective_config;
use crate::position::{span_contains, span_to_range};
use crate::symbol::{list_item_marker_text, sigil_display_name};

/// Provides hover information for the symbol under the cursor.
pub fn hover_for(text: &str, pos: Position, uri: Option<&Uri>) -> Option<Hover> {
    let doc = tomet_parser::parse_document(text).ok()?;
    let target_line = pos.line as usize + 1;
    let target_col = pos.character as usize + 1;

    let config = resolve_effective_config(&doc, uri);

    struct HoverFinder<'a> {
        doc: &'a Document,
        config: &'a tomet_semantics::DocumentConfig,
        text: &'a str,
        pos: Position,
        line: usize,
        col: usize,
        best_match: Option<Hover>,
    }

    impl<'a> Visitor<()> for HoverFinder<'a> {
        fn visit(&mut self, el: &Element) -> ControlFlow<()> {
            let span = el.span;
            if span_contains(&span, self.line, self.col) {
                let kind = classify_std_lenient(el);
                let hover = if kind == ElementKind::Table {
                    table_hover(self.text, self.pos, el)
                } else if let Some(h) = macro_hover(self.doc, self.config, el) {
                    Some(h)
                } else if kind == ElementKind::Bare {
                    Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!(
                                "**List Item** (marker: `{}`)",
                                list_item_marker_text(el)
                            ),
                        }),
                        range: Some(span_to_range(&span)),
                    })
                } else if kind == ElementKind::Heading {
                    Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("**Heading Level {}**", heading_level(el).unwrap_or(1)),
                        }),
                        range: Some(span_to_range(&span)),
                    })
                } else if kind == ElementKind::Kind {
                    let declared_kind = normalized_element_args(el)
                        .and_then(|v| {
                            v.get("kind")
                                .and_then(|k| k.as_str())
                                .map(|s| s.to_string())
                        })
                        .or_else(|| {
                            el.args
                                .as_ref()
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_else(|| "unknown".to_string());
                    let mut desc = format!("**Document Kind**: `{declared_kind}`\n\n");
                    desc.push_str("Declares the document kind, and binds the blueprint and vocabulary that go with it.");
                    Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: desc,
                        }),
                        range: Some(span_to_range(&span)),
                    })
                } else if kind == ElementKind::Blueprint {
                    let target_kind = normalized_element_args(el)
                        .and_then(|v| {
                            v.get("target")
                                .or_else(|| v.get("kind"))
                                .and_then(|k| k.as_str())
                                .map(|s| s.to_string())
                        })
                        .or_else(|| {
                            el.args
                                .as_ref()
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_else(|| "unknown".to_string());
                    let mut desc = format!("**Blueprint Archetype**: `{target_kind}`\n\n");
                    desc.push_str("Defines the structure documents of this kind must have, and what `tomet new` instantiates.");
                    Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: desc,
                        }),
                        range: Some(span_to_range(&span)),
                    })
                } else if kind == ElementKind::Version {
                    let declared_ver = normalized_element_args(el)
                        .and_then(|v| {
                            v.get("version").map(|v| match v {
                                Value::String(s) => s.clone(),
                                Value::Int(i) => i.to_string(),
                                Value::Float(f) => f.to_string(),
                                _ => format!("{v:?}"),
                            })
                        })
                        .unwrap_or_else(|| "unknown".to_string());
                    let mut desc = format!("**Tomet Version**: `{declared_ver}`\n\n");
                    desc.push_str("Declares the Tomet language and syntax specification edition for this document.");
                    Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: desc,
                        }),
                        range: Some(span_to_range(&span)),
                    })
                } else {
                    let mut desc = format!("**Element**: `{}`\n", sigil_display_name(&el.sigil));
                    desc.push_str(&format!("- **Classification**: `{}`\n", kind.as_str()));
                    if let Some(norm_args) = normalized_element_args(el) {
                        desc.push_str(&format!("- **Normalized Args**: `{norm_args:?}`\n"));
                    }
                    Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: desc,
                        }),
                        range: Some(span_to_range(&span)),
                    })
                };
                if let Some(h) = hover {
                    self.best_match = Some(h);
                }
            }
            ControlFlow::Continue(())
        }
    }

    let mut finder = HoverFinder {
        doc: &doc,
        config: &config,
        text,
        pos,
        line: target_line,
        col: target_col,
        best_match: None,
    };
    let _ = walk_document(&doc, &mut finder);

    finder.best_match
}

struct CellInfo {
    col_index: usize,
    start_char: u32,
    end_char: u32,
    text: String,
}

fn extract_line_cells(line: &str) -> Vec<CellInfo> {
    let mut cells = Vec::new();
    let mut depth = 0;
    let mut cell_start_char = 0u32;
    let mut char_count = 0u32;
    let mut cell_content = String::new();

    for c in line.chars() {
        let u16_len = c.len_utf16() as u32;
        if c == '[' {
            if depth == 0 {
                cell_start_char = char_count;
                cell_content.clear();
            } else {
                cell_content.push(c);
            }
            depth += 1;
        } else if c == ']' {
            if depth > 0 {
                depth -= 1;
                if depth == 0 {
                    let end_char = char_count + u16_len;
                    cells.push(CellInfo {
                        col_index: cells.len(),
                        start_char: cell_start_char,
                        end_char,
                        text: cell_content.trim().to_string(),
                    });
                } else {
                    cell_content.push(c);
                }
            }
        } else {
            if depth > 0 {
                cell_content.push(c);
            }
        }
        char_count += u16_len;
    }

    cells
}

fn extract_table_aligns_from_args(args: Option<&Value>) -> Vec<String> {
    let Some(Value::Map(entries)) = args else {
        return Vec::new();
    };
    for (k, v) in entries {
        if k == "align" {
            match v {
                Value::Seq(seq) => {
                    return seq
                        .iter()
                        .filter_map(|item| match item {
                            Value::String(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect();
                }
                Value::String(s) => {
                    return vec![s.clone(); 50];
                }
                _ => {}
            }
        }
    }
    Vec::new()
}

fn table_hover(text: &str, pos: Position, el: &Element) -> Option<Hover> {
    let lines: Vec<&str> = text.lines().collect();
    let start_line = el.span.start.line.saturating_sub(1);
    let end_line = el
        .span
        .end
        .line
        .saturating_sub(1)
        .min(lines.len().saturating_sub(1));

    let aligns = extract_table_aligns_from_args(el.args.as_ref());

    let mut table_rows: Vec<(usize, Vec<CellInfo>)> = Vec::new();
    for line_idx in start_line..=end_line {
        if line_idx >= lines.len() {
            break;
        }
        let line_str = lines[line_idx];
        let trimmed = line_str.trim();
        // Skip the element's own head line (`@table[`, `@table(...)`, ...).
        if trimmed.starts_with("@table") || trimmed.starts_with("@table") {
            continue;
        }
        if trimmed == "]" || trimmed.starts_with("]{") || trimmed.starts_with("] ") {
            continue;
        }

        let cells = extract_line_cells(line_str);
        if !cells.is_empty() {
            table_rows.push((line_idx, cells));
        }
    }

    if table_rows.is_empty() {
        return None;
    }

    let header_cells: Vec<String> = table_rows
        .first()
        .map(|(_, cells)| cells.iter().map(|c| c.text.clone()).collect())
        .unwrap_or_default();

    let target_line = pos.line as usize;
    let target_char = pos.character;

    for (row_idx, (line_idx, cells)) in table_rows.iter().enumerate() {
        if *line_idx == target_line {
            for cell in cells {
                if target_char >= cell.start_char && target_char <= cell.end_char {
                    let col_num = cell.col_index + 1;
                    let row_num = row_idx + 1;
                    let is_header = row_num == 1;
                    let header_name = header_cells
                        .get(cell.col_index)
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    let align_str = aligns.get(cell.col_index).map(|s| s.as_str());

                    let mut md = String::new();
                    if is_header {
                        md.push_str(&format!("### 📊 Table Header (Column {col_num})\n\n"));
                        md.push_str(&format!("- **Header**: `{}`\n", cell.text));
                        if let Some(align) = align_str {
                            md.push_str(&format!("- **Align**: `{align}`\n"));
                        }
                        md.push_str(&format!("- **Total Columns**: {}\n", header_cells.len()));
                    } else {
                        md.push_str(&format!(
                            "### 📊 Table Cell (Column {col_num}, Row {row_num})\n\n"
                        ));
                        if !header_name.is_empty() {
                            md.push_str(&format!("- **Header**: `{header_name}`\n"));
                        }
                        md.push_str(&format!("- **Value**: `{}`\n", cell.text));
                        if let Some(align) = align_str {
                            md.push_str(&format!("- **Align**: `{align}`\n"));
                        }
                    }

                    let range = Range::new(
                        Position::new(*line_idx as u32, cell.start_char),
                        Position::new(*line_idx as u32, cell.end_char),
                    );

                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: md,
                        }),
                        range: Some(range),
                    });
                }
            }
        }
    }

    let mut summary = format!("**Table** (`{}`)\n\n", sigil_display_name(&el.sigil));
    summary.push_str(&format!("- **Rows**: {}\n", table_rows.len()));
    summary.push_str(&format!("- **Columns**: {}\n", header_cells.len()));
    if !aligns.is_empty() {
        summary.push_str(&format!("- **Align**: `[{}]`\n", aligns.join(", ")));
    }
    if !header_cells.is_empty() {
        summary.push_str(&format!("- **Headers**: `{}`\n", header_cells.join("`, `")));
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: summary,
        }),
        range: Some(span_to_range(&el.span)),
    })
}

fn find_interp_in_value(val: &Value) -> Option<tomet_ast::InterpExpr> {
    match val {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.starts_with('$') {
                if let Ok(parsed) = tomet_parser::parse_document(trimmed) {
                    struct InterpFinder(Option<tomet_ast::InterpExpr>);
                    impl Visitor<()> for InterpFinder {
                        fn visit(&mut self, el: &Element) -> ControlFlow<()> {
                            if let Some(ElementValue::Interp(expr)) = &el.value {
                                self.0 = Some(expr.clone());
                                return ControlFlow::Break(());
                            }
                            ControlFlow::Continue(())
                        }
                    }
                    let mut finder = InterpFinder(None);
                    let _ = walk_document(&parsed, &mut finder);
                    if finder.0.is_some() {
                        return finder.0;
                    }
                }
            }
            None
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                if let Some(expr) = find_interp_in_value(v) {
                    return Some(expr);
                }
            }
            None
        }
        Value::Seq(items) => {
            for v in items {
                if let Some(expr) = find_interp_in_value(v) {
                    return Some(expr);
                }
            }
            None
        }
        _ => None,
    }
}

fn macro_hover(
    doc: &Document,
    config: &tomet_semantics::DocumentConfig,
    el: &Element,
) -> Option<Hover> {
    let expr = if let Some(ElementValue::Interp(expr)) = el.value.as_ref() {
        Some(expr.clone())
    } else if let Some(args) = el.args.as_ref() {
        find_interp_in_value(args)
    } else {
        None
    }?;

    let md = match tomet_compute::evaluate_with_config(doc, &expr, config) {
        Ok(val) => {
            let rendered = match val {
                Value::String(s) => s,
                Value::Int(i) => i.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Seq(seq) => format!("{seq:?}"),
                Value::Map(map) => format!("{map:?}"),
                Value::Null => "null".to_string(),
                Value::Call(name, args) => format!("{name:?}({args:?})"),
                Value::Element(el) => format!(
                    "@{}({:?})",
                    el.sigil.name().map(|n| n.to_string()).unwrap_or_default(),
                    el.args
                ),
            };
            let trimmed = rendered.trim();
            if is_web_url(trimmed) {
                format!("**Macro Result**\n\n[{trimmed}]({trimmed})")
            } else {
                format!("**Macro Result**\n\n```text\n{rendered}\n```")
            }
        }
        Err(err) => {
            format!("**Macro**\n\n- **Error**: `{err}`")
        }
    };
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: Some(span_to_range(&el.span)),
    })
}

fn is_web_url(s: &str) -> bool {
    (s.starts_with("http://") || s.starts_with("https://"))
        && !s.contains('\n')
        && !s.contains('\r')
        && !s.contains(' ')
}
