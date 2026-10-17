use std::ops::ControlFlow;

use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range, Uri};
use tomet_ast::{Document, Element, ElementValue, Value};
use tomet_expr::{DocRef, EvalContext, EvalValue, MemoryResolver, Phase, SystemGenerate, evaluate};
use tomet_semantics::{ElementKind, classify_std_lenient, heading_level, normalized_element_args};
use tomet_tree::{ValueExt, Visitor, walk_document};

use crate::format::resolve_effective_config;
use crate::position::{TextPositions, span_contains};
use crate::symbol::{list_item_marker_text, sigil_display_name};

/// What an expression comes to, for a hover: given the document it is written
/// in, and the expression.
pub type Evaluate<'a> =
    &'a dyn Fn(&Document, &tomet_ast::InterpExpr) -> Result<EvalValue, tomet_expr::EvalError>;

/// Provides hover information for the symbol under the cursor, from the text
/// alone.
///
/// An expression is evaluated against this one document and its settings, so
/// what reaches for another file fails and says so. A document that has a
/// vault is better served by [`hover_in`], with the vault's evaluation.
pub fn hover_for(text: &str, pos: Position, uri: Option<&Uri>) -> Option<Hover> {
    let doc = tomet_parser::parse_document(text).ok()?;
    let config = resolve_effective_config(&doc, uri);
    hover_in(text, pos, &|doc, expr| {
        evaluate_for_hover(doc, expr, &config)
    })
}

/// [`hover_for`], with `evaluate` deciding what an expression comes to.
pub fn hover_in(text: &str, pos: Position, evaluate: Evaluate<'_>) -> Option<Hover> {
    let doc = tomet_parser::parse_document(text).ok()?;
    let positions = TextPositions::new(text);
    let offset = positions.offset(pos);

    struct HoverFinder<'a> {
        doc: &'a Document,
        evaluate: Evaluate<'a>,
        text: &'a str,
        positions: &'a TextPositions<'a>,
        pos: Position,
        offset: usize,
        best_match: Option<Hover>,
    }

    impl<'a> Visitor<()> for HoverFinder<'a> {
        fn visit(&mut self, el: &Element) -> ControlFlow<()> {
            let span = el.span;
            if span_contains(&span, self.offset) {
                let kind = classify_std_lenient(el);
                let hover = if kind == ElementKind::Table {
                    table_hover(self.positions, self.text, self.pos, el)
                } else if let Some(h) = macro_hover(self.positions, self.doc, self.evaluate, el) {
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
                        range: Some(self.positions.range(&span)),
                    })
                } else if kind == ElementKind::Heading {
                    Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("**Heading Level {}**", heading_level(el).unwrap_or(1)),
                        }),
                        range: Some(self.positions.range(&span)),
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
                        range: Some(self.positions.range(&span)),
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
                        range: Some(self.positions.range(&span)),
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
                        range: Some(self.positions.range(&span)),
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
                        range: Some(self.positions.range(&span)),
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
        evaluate,
        text,
        positions: &positions,
        pos,
        offset,
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

fn table_hover(
    positions: &TextPositions<'_>,
    text: &str,
    pos: Position,
    el: &Element,
) -> Option<Hover> {
    let lines: Vec<&str> = text.lines().collect();
    let start_line = positions.line(el.span.start) as usize;
    let end_line = (positions.line(el.span.end) as usize).min(lines.len().saturating_sub(1));

    // `normalized_element_args`, not raw `el.args`: `align: list(...)` is
    // a `Value::Call` until normalized, and `extract_table_aligns_from_args`
    // only recognizes `Value::Seq`.
    let normalized_args = normalized_element_args(el);
    let aligns = extract_table_aligns_from_args(normalized_args.as_ref());

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
        range: Some(positions.range(&el.span)),
    })
}

fn find_interp_in_value(val: &Value) -> Option<tomet_ast::InterpExpr> {
    match val {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.starts_with('$')
                && let Ok(parsed) = tomet_parser::parse_document(trimmed)
            {
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
    positions: &TextPositions<'_>,
    doc: &Document,
    evaluate: Evaluate<'_>,
    el: &Element,
) -> Option<Hover> {
    let expr = if let Some(ElementValue::Interp(expr)) = el.value.as_ref() {
        Some(expr.clone())
    } else if let Some(args) = el.args.as_ref() {
        find_interp_in_value(args)
    } else {
        None
    }?;

    let md = match evaluate(doc, &expr) {
        Ok(val) => {
            let rendered = render_value(&val);
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
        range: Some(positions.range(&el.span)),
    })
}

fn is_web_url(s: &str) -> bool {
    (s.starts_with("http://") || s.starts_with("https://"))
        && !s.contains('\n')
        && !s.contains('\r')
        && !s.contains(' ')
}

/// Evaluates `expr` the way rendering would, against this one document.
///
/// The editor has the document and its settings and nothing else, so what
/// reaches for another file fails and says so; `generate.*` fails too,
/// because rendering never runs it.
fn evaluate_for_hover(
    doc: &Document,
    expr: &tomet_ast::InterpExpr,
    config: &tomet_semantics::DocumentConfig,
) -> Result<EvalValue, tomet_expr::EvalError> {
    let here = "document.tmt";
    let resolver = MemoryResolver::new().with_document(here, std::sync::Arc::new(doc.clone()));
    let vars = EvalValue::Null;
    let cx = EvalContext {
        phase: Phase::Output,
        resolver: &resolver,
        generate: &SystemGenerate,
        self_doc: DocRef(here.to_string()),
        macros: &config.macros,
        vars: &vars,
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    evaluate(expr, &cx)
}

fn render_value(value: &EvalValue) -> String {
    match value {
        EvalValue::Null => "null".to_string(),
        EvalValue::Bool(b) => b.to_string(),
        EvalValue::Int(i) => i.to_string(),
        EvalValue::Float(f) => f.to_string(),
        EvalValue::String(s) => s.clone(),
        EvalValue::Seq(items) => {
            let items: Vec<String> = items.iter().map(render_value).collect();
            format!("[{}]", items.join(", "))
        }
        EvalValue::Map(entries) => {
            let entries: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{k}: {}", render_value(v)))
                .collect();
            format!("{{{}}}", entries.join(", "))
        }
        EvalValue::DateTime(dt) => tomet_expr::format_datetime(dt, "YYYY-MM-DD HH:mm:ss"),
        EvalValue::Document(doc) => format!("document {}", doc.0),
        EvalValue::Directory(dir) => format!("directory {}", dir.0),
        EvalValue::Node { node, .. } => match node.id() {
            Some(id) => format!("element #{id}"),
            None => "element".to_string(),
        },
        EvalValue::Fragment(blocks) => format!("fragment of {} block(s)", blocks.len()),
        EvalValue::Function(_) => "function".to_string(),
        EvalValue::Namespace(_) => "namespace".to_string(),
        EvalValue::Call(name, args) => {
            let args: Vec<String> = args.iter().map(render_value).collect();
            format!("{name}({})", args.join(", "))
        }
    }
}
