use tomet_semantics::DeclKind;

use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range, Uri};
use tomet_ast::{Declaration, Element, File, InterpExpr, Tuple, Value};
use tomet_eval::evaluate;
use tomet_expr::{DocRef, EvalContext, EvalValue, MemoryResolver};
use tomet_scalar::Scalar;
use tomet_semantics::{
    Bindings, ElementKind, LoadedVocabularies, TupleView, declaration_args_view_in,
    element_args_view, element_args_view_in, heading_level,
};
use tomet_span::Span;
use tomet_spelling::scalar_spelling;
use tomet_tree::{
    ValueExt, for_each_declaration, for_each_element, for_each_interpolation, for_each_list_item,
};

use crate::format::resolve_effective_config;
use crate::position::{TextPositions, span_contains};
use crate::symbol::list_item_marker_text;

/// What an expression comes to, for a hover: given the document it is written
/// in, and the expression.
pub type Evaluate<'a> =
    &'a dyn Fn(&File, &tomet_ast::InterpExpr) -> Result<EvalValue, tomet_expr::EvalError>;

/// Provides hover information for the symbol under the cursor, from the text
/// alone.
///
/// An expression is evaluated against this one document and its settings, so
/// what reaches for another file fails and says so, and its names are bound
/// against the vocabularies tomet carries alone. A document that has a vault is
/// better served by [`hover_in`], with the vault's bindings and evaluation.
pub fn hover_for(text: &str, pos: Position, uri: Option<&Uri>) -> Option<Hover> {
    let doc = tomet_ast_parser::parse_file(text).ok()?;
    let config = resolve_effective_config(&doc, uri);
    let bindings = LoadedVocabularies::default().bindings_for(&doc);
    hover_in(text, &doc, &bindings, pos, &|doc, expr| {
        evaluate_for_hover(doc, expr, &config)
    })
}

/// [`hover_for`] over `doc`, parsed from `text`, with `bindings` saying what its names are (an
/// element or declaration is shown as what it resolves to there: `@ng.node` under
/// `::use(node-graph, as: ng)` is `node-graph.node`) and `evaluate` what an expression comes to.
pub fn hover_in(
    text: &str,
    doc: &File,
    bindings: &Bindings,
    pos: Position,
    evaluate: Evaluate<'_>,
) -> Option<Hover> {
    let positions = TextPositions::new(text);
    let offset = positions.offset(pos);

    struct HoverFinder<'a> {
        doc: &'a File,
        bindings: &'a Bindings,
        evaluate: Evaluate<'a>,
        text: &'a str,
        positions: &'a TextPositions<'a>,
        pos: Position,
        offset: usize,
        best_match: Option<Hover>,
        /// The width of the span `best_match` is for: the narrowest wins.
        best_width: usize,
    }

    impl HoverFinder<'_> {
        /// Keeps `hover` if it is for a span no wider than the best so far, so the
        /// innermost thing under the cursor wins whatever order they are found in.
        fn offer(&mut self, hover: Hover, span: &Span) {
            let width = span.end.saturating_sub(span.start);
            if width <= self.best_width {
                self.best_width = width;
                self.best_match = Some(hover);
            }
        }
    }

    impl HoverFinder<'_> {
        fn visit(&mut self, el: &Element) {
            let span = el.span;
            if span_contains(&span, self.offset) {
                let view = el.as_view();
                let kind = self.bindings.classify_lenient(&el.name);
                let hover = if kind == ElementKind::Table {
                    table_hover(self.positions, self.text, self.pos, el)
                } else if let Some(h) = macro_hover(self.positions, self.doc, self.evaluate, el) {
                    Some(h)
                } else if kind == ElementKind::Heading {
                    Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!(
                                "**Heading Level {}**",
                                heading_level(view).unwrap_or(1)
                            ),
                        }),
                        range: Some(self.positions.range(&span)),
                    })
                } else {
                    let mut desc = format!("**Element**: `{}`\n", format!("@{}", el.name));
                    desc.push_str(&format!("- **Classification**: `{}`\n", kind.as_str()));
                    if let Some(norm_args) = element_args_view_in(view, self.bindings) {
                        desc.push_str(&format!(
                            "- **Normalized Args**: `{:?}`\n",
                            norm_args.tuple()
                        ));
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
                    self.offer(h, &span);
                }
            }
        }
    }

    let mut finder = HoverFinder {
        doc,
        bindings,
        evaluate,
        text,
        positions: &positions,
        pos,
        offset,
        best_match: None,
        best_width: usize::MAX,
    };
    // A list item is not an element, so the walk below never meets it.
    for_each_list_item(doc, |item| {
        if span_contains(&item.span, offset) && item.name.is_none() {
            finder.offer(
                Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("**List Item** (marker: `{}`)", list_item_marker_text(item)),
                    }),
                    range: Some(positions.range(&item.span)),
                },
                &item.span,
            );
        }
    });
    for_each_element(doc, |el| finder.visit(el));

    // An interpolation is not an element, so the walk above never meets it.
    // It is a leaf, hence the innermost thing under the cursor.
    for_each_interpolation(doc, |interp| {
        if span_contains(&interp.span, offset) {
            finder.best_match = Some(macro_result_hover(
                &positions,
                doc,
                evaluate,
                &interp.expr,
                interp.span,
            ));
        }
    });

    // A declaration is not an element either; it is a block of its own.
    for_each_declaration(doc, |declaration| {
        if span_contains(&declaration.span, offset) {
            finder.best_match = Some(declaration_hover(&positions, bindings, declaration));
        }
    });

    finder.best_match
}

/// The hover for a `::name` declaration: what the standard ones declare, and
/// what any other resolves to in `bindings`.
fn declaration_hover(
    positions: &TextPositions<'_>,
    bindings: &Bindings,
    declaration: &Declaration,
) -> Hover {
    let span = declaration.span;
    let kind = bindings.classify_attribute_lenient(&declaration.name);
    let args = declaration_args_view_in(declaration, bindings);
    let text_of = |keys: &[&str]| {
        args.as_ref()
            .and_then(|v| keys.iter().find_map(|key| v.get(key)))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                declaration
                    .data
                    .args
                    .as_ref()
                    .and_then(|t| t.nth_unnamed(0))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "unknown".to_string())
    };
    let value = if kind == ElementKind::Declaration(DeclKind::Kind) {
        let declared_kind = text_of(&["kind"]);
        let mut desc = format!("**Document Kind**: `{declared_kind}`\n\n");
        desc.push_str(
            "Declares the document kind, and binds the blueprint and vocabulary that go with it.",
        );
        desc
    } else if kind == ElementKind::Declaration(DeclKind::Blueprint) {
        let target_kind = text_of(&["target", "kind"]);
        let mut desc = format!("**Blueprint Archetype**: `{target_kind}`\n\n");
        desc.push_str("Defines the structure documents of this kind must have, and what `tomet new` instantiates.");
        desc
    } else if kind == ElementKind::Declaration(DeclKind::Version) {
        let declared_ver = args
            .as_ref()
            .and_then(|v| {
                v.get("version").map(|v| match v {
                    Value::Scalar(s) => scalar_spelling(s),
                    _ => format!("{v:?}"),
                })
            })
            .unwrap_or_else(|| "unknown".to_string());
        let mut desc = format!("**Tomet Version**: `{declared_ver}`\n\n");
        desc.push_str(
            "Declares the Tomet language and syntax specification edition for this document.",
        );
        desc
    } else {
        let mut desc = format!("**Declaration**: `::{}`\n", declaration.name);
        desc.push_str(&format!("- **Classification**: `{}`\n", kind.as_str()));
        if let Some(norm_args) = &args {
            desc.push_str(&format!(
                "- **Normalized Args**: `{:?}`\n",
                norm_args.tuple()
            ));
        }
        desc
    };
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(positions.range(&span)),
    }
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

fn extract_table_aligns_from_args(args: Option<&TupleView<'_, '_>>) -> Vec<String> {
    let Some(args) = args else {
        return Vec::new();
    };
    if let Some(v) = args.get("align") {
        match v {
            Value::Tuple(tuple) => {
                return tuple
                    .values()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect();
            }
            Value::Scalar(Scalar::Text(s)) => {
                return vec![s.clone(); 50];
            }
            Value::Call(_, tuple) => {
                return tuple
                    .values()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect();
            }
            Value::Scalar(_)
            | Value::Interp(_)
            | Value::Hole(_)
            | Value::TypeSeat(_)
            | Value::Element(_)
            | Value::Blocks(_) => {}
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

    // `element_args_view`, not raw `el.args`: `align: (...)` is
    // a `Value::Call` until viewed, and `extract_table_aligns_from_args`
    // only recognizes a tuple of strings.
    let normalized_args = element_args_view(el.as_view());
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

    let mut summary = format!("**Table** (`{}`)\n\n", format!("@{}", el.name));
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

fn find_interp_in_tuple(tuple: &Tuple) -> Option<tomet_ast::InterpExpr> {
    for v in tuple.values() {
        if let Some(expr) = find_interp_in_value(v) {
            return Some(expr);
        }
    }
    None
}

fn find_interp_in_value(val: &Value) -> Option<tomet_ast::InterpExpr> {
    match val {
        Value::Scalar(Scalar::Text(s)) => {
            let trimmed = s.trim();
            if trimmed.starts_with('$')
                && let Ok(parsed) = tomet_ast_parser::parse_file(trimmed)
            {
                let mut found = None;
                for_each_interpolation(&parsed, |interp| {
                    if found.is_none() {
                        found = Some(interp.expr.clone());
                    }
                });
                if found.is_some() {
                    return found;
                }
            }
            None
        }
        Value::Interp(interp) => Some(interp.expr.clone()),
        Value::Tuple(tuple) => find_interp_in_tuple(tuple),
        Value::Call(_, tuple) => find_interp_in_tuple(tuple),
        Value::Element(el) => {
            if let Some(args) = &el.data.args {
                find_interp_in_tuple(args)
            } else {
                None
            }
        }
        Value::Scalar(_) | Value::Hole(_) | Value::TypeSeat(_) | Value::Blocks(_) => None,
    }
}

fn macro_hover(
    positions: &TextPositions<'_>,
    doc: &File,
    evaluate: Evaluate<'_>,
    el: &Element,
) -> Option<Hover> {
    let expr = find_interp_in_tuple(el.data.args.as_ref()?)?;
    Some(macro_result_hover(positions, doc, evaluate, &expr, el.span))
}

/// The hover for the expression `expr`, written at `span`: what it comes to,
/// or why it does not.
fn macro_result_hover(
    positions: &TextPositions<'_>,
    doc: &File,
    evaluate: Evaluate<'_>,
    expr: &InterpExpr,
    span: Span,
) -> Hover {
    let md = match evaluate(doc, expr) {
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
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: Some(positions.range(&span)),
    }
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
/// reaches for another file fails and says so. A variable is at its `default`.
fn evaluate_for_hover(
    doc: &File,
    expr: &tomet_ast::InterpExpr,
    config: &tomet_semantics::DocumentConfig,
) -> Result<EvalValue, tomet_expr::EvalError> {
    let here = "document.tmt";
    let resolver = MemoryResolver::new().with_document(here, std::sync::Arc::new(doc.clone()));
    // Nothing is passed while hovering, so the variables are at their defaults.
    let vars = tomet_expr::var_scope(&config.vars, &DocRef(here.to_string()), &[])
        .expect("nothing is passed, so nothing is undeclared or mistyped");
    let cx = EvalContext {
        resolver: &resolver,
        self_doc: DocRef(here.to_string()),
        macros: &config.macros,
        var: &vars.values,
        var_unset: &vars.unset,
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    evaluate(expr, &cx)
}

fn render_value(value: &EvalValue) -> String {
    match value {
        // A result as it reads, not as it is written: a text is itself (a macro that makes a link
        // is shown as the link), and `none`, which has no text, is shown as `none`.
        EvalValue::Scalar(s) => s
            .text()
            .map(|text| text.into_owned())
            .unwrap_or_else(|| "none".to_string()),
        EvalValue::Tuple(tuple) => {
            let entries: Vec<String> = tuple
                .entries()
                .map(|e| match e.name() {
                    Some(name) => format!("{name}: {}", render_value(e.value())),
                    None => render_value(e.value()),
                })
                .collect();
            format!("({})", entries.join(", "))
        }
        EvalValue::DateTime(dt) => tomet_expr::format_datetime(dt, "YYYY-MM-DD HH:mm:ss"),
        EvalValue::Version(v) => v.to_string(),
        EvalValue::Document(doc) => format!("document {}", doc.0),
        EvalValue::Directory(dir) => format!("directory {}", dir.0),
        EvalValue::Part { part, .. } => match part.id() {
            Some(id) => format!("element #{id}"),
            None => "element".to_string(),
        },
        EvalValue::Fragment(blocks) => format!("fragment of {} block(s)", blocks.len()),
        EvalValue::Function(_) => "function".to_string(),
        EvalValue::Namespace(_) => "namespace".to_string(),
        EvalValue::Type(ty) => ty.to_string(),
        EvalValue::Call(name, args) => {
            let entries: Vec<String> = args
                .entries()
                .map(|e| match e.name() {
                    Some(name) => format!("{name}: {}", render_value(e.value())),
                    None => render_value(e.value()),
                })
                .collect();
            format!("{name}({})", entries.join(", "))
        }
    }
}
