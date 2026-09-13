//! Language server library for Tomet (`tomet-lsp`).
//! Provides diagnostics (parser & validator), document formatting,
//! hover, document symbols, goto definition, and completions.

use lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol,
    GotoDefinitionResponse, Hover, HoverContents, Location, MarkupContent, MarkupKind, Position,
    Range, SymbolKind, TextEdit, Uri,
};
use std::ops::ControlFlow;
use tomet_ast::{Document, Element, ElementValue, Inline, InterpExprKind, Sigil, Span, Value};
use tomet_semantics::{
    BUILTIN_KINDS, ElementKind, Shape, classify_std_lenient, heading_level, normalized_element_args,
};
use tomet_tree::{ElementExt, ValueExt, Visitor, walk_document};

/// Converts an AST [`Span`] to an LSP [`Range`].
pub fn span_to_range(span: &Span) -> Range {
    let start_line = span.start.line.saturating_sub(1) as u32;
    let start_col = span.start.column.saturating_sub(1) as u32;
    let end_line = span.end.line.saturating_sub(1) as u32;
    let end_col = span.end.column.saturating_sub(1) as u32;
    Range::new(
        Position::new(start_line, start_col),
        Position::new(end_line, end_col),
    )
}

/// Checks if a 1-indexed `(line, col)` falls within an AST [`Span`].
fn span_contains(span: &Span, line: usize, col: usize) -> bool {
    if span.start.line == 0 || span.end.line == 0 {
        return false;
    }
    let start_ok = line > span.start.line || (line == span.start.line && col >= span.start.column);
    let end_ok = line < span.end.line || (line == span.end.line && col <= span.end.column);
    start_ok && end_ok
}

/// Converts a CST [`tomet_cst::TextRange`] to an LSP [`Range`].
pub fn text_range_to_lsp_range(text: &str, range: tomet_cst::TextRange) -> Range {
    let start_offset = usize::from(range.start());
    let end_offset = usize::from(range.end());
    let start_pos = offset_to_position(text, start_offset);
    let end_pos = offset_to_position(text, end_offset);
    Range::new(start_pos, end_pos)
}

fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut col = 0;
    for (i, c) in text.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += c.len_utf16() as u32;
        }
    }
    Position::new(line, col)
}

/// Parses `text` and produces diagnostics (both parse errors and CST validation rules).
pub fn diagnostics_for(text: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let cst = tomet_parser::parse_cst(text);

    // 1. CST-based lint validation (exact token wave lines!)
    let cst_errors = tomet_validator::validate_cst(&cst);
    for err in cst_errors {
        diags.push(Diagnostic {
            range: text_range_to_lsp_range(text, err.range()),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("tomet".to_string()),
            message: err.to_string(),
            ..Diagnostic::default()
        });
    }

    // 2. High-level parser errors if document structure is invalid
    if let Err(err) = tomet_parser::parse_document(text) {
        diags.push(Diagnostic {
            range: error_range(&err, text),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("tomet".to_string()),
            message: err.message.clone(),
            ..Diagnostic::default()
        });
    }

    diags
}

fn error_range(err: &tomet_parser::Error, text: &str) -> Range {
    let line = (err.line.saturating_sub(1)) as u32;
    let col = (err.column.saturating_sub(1)) as u32;
    let start = Position::new(line, col);
    let line_len = text
        .lines()
        .nth(line as usize)
        .map(|l| l.chars().count() as u32)
        .unwrap_or(col);
    let end = Position::new(line, line_len.max(col));
    Range::new(start, end)
}

/// Formats the document using `tomet-formatter` with loaded or inferred configuration.
pub fn format_edits(text: &str, uri: Option<&Uri>) -> Vec<TextEdit> {
    let path = uri.and_then(uri_to_file_path);
    let config = tomet_config::config_for(path.as_deref(), text);
    let formatted = tomet_formatter::format_source_with_config(text, &config);
    if formatted == text {
        return Vec::new();
    }
    vec![TextEdit {
        range: whole_document_range(text),
        new_text: formatted,
    }]
}

fn uri_to_file_path(u: &Uri) -> Option<std::path::PathBuf> {
    let s = u.as_str();
    let stripped = s.strip_prefix("file://")?;
    let decoded = percent_decode_str(stripped);
    Some(std::path::PathBuf::from(decoded))
}

fn percent_decode_str(s: &str) -> String {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn whole_document_range(text: &str) -> Range {
    let lines: Vec<&str> = text.split('\n').collect();
    let end_line = lines.len().saturating_sub(1) as u32;
    let end_char = lines.last().map(|l| l.chars().count()).unwrap_or(0) as u32;
    Range::new(Position::new(0, 0), Position::new(end_line, end_char))
}

/// Resolves the merged configuration for a document by combining:
/// 1. Workspace config discovered from ancestor directories (`default.config.tmt` / `tomet.config.tmt`)
/// 2. Explicit `@config(import: ...)` / `@config(file: ...)` imported configs
/// 3. Document's local `@config` definitions (highest precedence)
pub fn resolve_effective_config(
    doc: &Document,
    uri: Option<&Uri>,
) -> tomet_semantics::DocumentConfig {
    let mut config = tomet_semantics::document_config(doc);
    let file_path_opt = uri.and_then(uri_to_file_path);

    // 1. Merge macros from workspace config file (e.g. default.config.tmt / tomet.config.tmt)
    if let Some(fp) = &file_path_opt {
        if let Some((_, cfg_path, _)) = tomet_config::find_config_file(fp) {
            if let Ok(src) = std::fs::read_to_string(&cfg_path) {
                if let Ok(cfg_doc) = tomet_parser::parse_document(&src) {
                    let ext_cfg = tomet_semantics::document_config(&cfg_doc);
                    for (k, v) in ext_cfg.macros {
                        config.macros.entry(k).or_insert(v);
                    }
                }
            }
        }
    }

    // 2. Merge macros from explicit @config(import: ...) / @settings references in the document
    let mut import_targets = config.imports.clone();
    for block in &doc.blocks {
        if let tomet_ast::Block::Element(el) = block {
            if let Some(target) = tomet_resolver::config_import_ref(el) {
                if !import_targets.iter().any(|t| t == target) {
                    import_targets.push(target.to_string());
                }
            }
        }
    }

    for target in import_targets {
        let clean = target.strip_prefix("file:").unwrap_or(&target).trim();
        let candidate_paths = if let Some(fp) = &file_path_opt {
            let start_dir = if fp.is_file() {
                fp.parent().unwrap_or(fp)
            } else {
                fp.as_path()
            };
            let mut paths = Vec::new();
            let mut cur = start_dir.to_path_buf();
            loop {
                paths.push(cur.join(clean));
                if !cur.pop() {
                    break;
                }
            }
            paths
        } else {
            vec![std::path::PathBuf::from(clean)]
        };
        for p in candidate_paths {
            if p.exists() {
                if let Ok(src) = std::fs::read_to_string(&p) {
                    if let Ok(ext_doc) = tomet_parser::parse_document(&src) {
                        let ext_cfg = tomet_semantics::document_config(&ext_doc);
                        for (k, v) in ext_cfg.macros {
                            config.macros.entry(k).or_insert(v);
                        }
                    }
                }
                break;
            }
        }
    }

    config
}

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
                // `${...}` evaluation never produces a call literal itself
                // (that's parse-time-only syntax), but stay exhaustive.
                Value::Call(name, args) => format!("{name:?}({args:?})"),
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

fn sigil_display_name(sigil: &Sigil) -> String {
    match sigil {
        Sigil::Named(name) => format!("@{name}"),
        Sigil::Bare => "(bare)".to_string(),
        Sigil::Dollar => "${...}".to_string(),
    }
}

/// Provides document symbols for outline navigation in the editor.
pub fn document_symbols_for(text: &str) -> Vec<DocumentSymbol> {
    let Ok(doc) = tomet_parser::parse_document(text) else {
        return Vec::new();
    };

    struct SymbolCollector(Vec<DocumentSymbol>);

    impl Visitor<()> for SymbolCollector {
        fn visit(&mut self, el: &Element) -> ControlFlow<()> {
            let range = span_to_range(&el.span);
            let kind = classify_std_lenient(el);
            let symbol = if kind == ElementKind::Heading {
                let level = heading_level(el).unwrap_or(1);
                let title = extract_inlines_text(el.content.as_deref().unwrap_or(&[]));
                let name = format!("{} {}", "#".repeat(level as usize), title);
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
            self.0.push(symbol);
            ControlFlow::Continue(())
        }
    }

    let mut collector = SymbolCollector(Vec::new());
    let _ = walk_document(&doc, &mut collector);
    collector.0
}

fn list_item_marker_text(item: &Element) -> String {
    if let Some(marker) = &item.args {
        return format!("({marker:?})");
    }
    "-".to_string()
}

fn extract_inlines_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&t.value),
            Inline::Element(el) => out.push_str(&sigil_display_name(&el.sigil)),
        }
    }
    out
}

/// Provides Goto Definition target for reference links or interpolation expressions.
pub fn definition_for(text: &str, pos: Position, uri: &Uri) -> Option<GotoDefinitionResponse> {
    let doc = tomet_parser::parse_document(text).ok()?;
    let target_line = pos.line as usize + 1;
    let target_col = pos.character as usize + 1;

    // Find the node under cursor to get the referenced ID
    struct TargetIdFinder {
        line: usize,
        col: usize,
        ref_id: Option<String>,
    }

    impl Visitor<()> for TargetIdFinder {
        fn visit(&mut self, el: &Element) -> ControlFlow<()> {
            if span_contains(&el.span, self.line, self.col) {
                // Check whether it carries an `id` arg
                if let Some(Value::Map(entries)) = &el.args {
                    for (k, v) in entries {
                        if k == "id" {
                            if let Value::String(s) = v {
                                self.ref_id = Some(s.clone());
                            }
                        }
                    }
                }
                // Or if it's an interpolation expression ${id}
                if let Some(ElementValue::Interp(expr)) = &el.value {
                    if let InterpExprKind::Identifier(id) = &expr.kind {
                        self.ref_id = Some(id.clone());
                    }
                }
            }
            ControlFlow::Continue(())
        }
    }

    let mut id_finder = TargetIdFinder {
        line: target_line,
        col: target_col,
        ref_id: None,
    };
    let _ = walk_document(&doc, &mut id_finder);

    let target_id = id_finder.ref_id?;

    // Find the node that defines this id
    struct DefFinder {
        target_id: String,
        found_span: Option<Span>,
    }

    impl Visitor<()> for DefFinder {
        fn visit(&mut self, el: &Element) -> ControlFlow<()> {
            if let Some(Value::Map(entries)) = el.attrs_view() {
                for (k, v) in entries {
                    if k == "id" {
                        if let Value::String(s) = v {
                            if s == self.target_id {
                                self.found_span = Some(el.span);
                                return ControlFlow::Break(());
                            }
                        }
                    }
                }
            }
            ControlFlow::Continue(())
        }
    }

    let mut def_finder = DefFinder {
        target_id,
        found_span: None,
    };
    let _ = walk_document(&doc, &mut def_finder);

    def_finder.found_span.map(|span| {
        GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: span_to_range(&span),
        })
    })
}

/// The shape a built-in kind must take, or `None` when either is legal.
fn shape_of(kind: &ElementKind) -> Option<Shape> {
    // Asked directly rather than inferred. This used to place a probe
    // element as a block and read the mismatch back, which cannot tell
    // "block only" from "either shape" -- both come back without a
    // mismatch. `link` became the second of those and started reading as
    // block-only.
    tomet_semantics::required_shape(kind)
}

/// Completion detail for a built-in: its description, plus where it may
/// be placed when only one placement is legal.
///
/// Shape used to be visible in the completion's sigil (`#meta` vs
/// `@link`). One sigil spells every element now, so the constraint that
/// `required_shape` still enforces is said in words here instead.
fn detail_for(name: &str, kind: &ElementKind) -> String {
    match shape_of(kind) {
        Some(Shape::Block) => format!("{} (block)", describe_kind(name)),
        Some(Shape::Inline) => format!("{} (inline)", describe_kind(name)),
        None => describe_kind(name).to_string(),
    }
}

/// One-line description for a built-in element name.
fn describe_kind(name: &str) -> &'static str {
    match name {
        "version" => "Tomet language specification version",
        "kind" => "Document kind (archetype / schema) declaration",
        "blueprint" => {
            "Declares the structure of one document kind, and what `tomet new` instantiates"
        }
        "codeblock" => "Verbatim code block",
        "quote" => "Quote",
        "hr" => "Horizontal rule divider",
        "meta" => "Metadata key-value declaration",
        "config" => "Document-wide configuration",
        "settings" => "Schema and settings block",
        "use" => "Bind a vocabulary as a namespace",
        "include" => "Splice another document in at this point",
        "references" => "Remote connection container",
        "id" => "Attach attributes to a remote element by id",
        "links" => "Link reference definitions table",
        "link" => "Link to a url, file, or document",
        "embed" => "Embed another document or asset",
        "icon" => "Inline icon",
        "table" => "Table",
        "heading" => "Section heading",
        "em" => "Emphasis",
        "strong" => "Strong emphasis",
        "mark" => "Highlighted text",
        "ol" => "Ordered list",
        "ul" => "Unordered list",
        _ => "Built-in element",
    }
}

/// Provides autocompletion items for elements, keywords, and builtin compute functions.
pub fn completions_for(text: &str, pos: Position) -> Vec<CompletionItem> {
    completions_for_with_uri(text, pos, None)
}

/// Provides autocompletion items with document URI awareness for path completions.
pub fn completions_for_with_uri(
    text: &str,
    pos: Position,
    uri: Option<&Uri>,
) -> Vec<CompletionItem> {
    let prefix = get_line_prefix(text, pos);

    // If typing a path inside `file:`, `dir:`, or `./`, provide path completions.
    if let Some(path_items) = path_completions(&prefix, uri) {
        return path_items;
    }

    let mut items = Vec::new();

    // Driven by `BUILTIN_KINDS` rather than a hand-kept list. The old
    // list had drifted badly -- it offered `callout`/`warning`/`connect`,
    // which are not built-in at all, and omitted `link`/`embed`/`table`.
    // Bare names are reserved for exactly this set now, so offering a
    // non-builtin one would suggest something that fails to validate.
    // Every element is spelled `@name`, whatever its shape. Shape decides
    // where the element may be *placed*, not how it is spelled, so it is
    // reported in the completion detail instead of splitting the list
    // across two sigils.
    if prefix.ends_with('@') {
        for (name, kind) in BUILTIN_KINDS.iter() {
            items.push(CompletionItem {
                label: name.to_string(),
                insert_text: Some(name.to_string()),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(detail_for(name, kind)),
                ..CompletionItem::default()
            });
        }
        return items;
    }

    if prefix.ends_with("${") || prefix.ends_with("${ ") {
        let compute_funcs = [
            ("add", "Arithmetic addition function `add(a, b)`"),
            ("sub", "Arithmetic subtraction function `sub(a, b)`"),
            ("mul", "Arithmetic multiplication function `mul(a, b)`"),
            ("div", "Floating-point division function `div(a, b)`"),
            ("mod", "Integer modulo function `mod(a, b)`"),
            ("uuid", "Generate UUID v4 identifier `uuid()`"),
            ("date", "Current date formatted string `date(format)`"),
            ("time", "Current time formatted string `time(format)`"),
        ];

        for (func, detail) in compute_funcs {
            items.push(CompletionItem {
                label: format!("{func}(...)"),
                insert_text: Some(format!("{func}()")),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(detail.to_string()),
                ..CompletionItem::default()
            });
        }
        return items;
    }

    // Unprefixed: offer every built-in under the one element sigil.
    for (name, kind) in BUILTIN_KINDS.iter() {
        items.push(CompletionItem {
            label: format!("@{name}"),
            insert_text: Some(format!("@{name}")),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(detail_for(name, kind)),
            ..CompletionItem::default()
        });
    }

    let inferred_keys = [
        ("url", "URL link reference"),
        ("file", "File reference path"),
        ("tm", "Reference to another Tomet document"),
        (
            "id",
            "Element id (definition attribute, or a same-document reference key)",
        ),
        ("ref", "Search project by filename/title"),
        ("tag", "Tag classification"),
        ("format", "Embedded format (json, yaml, toml)"),
    ];

    for (key, detail) in inferred_keys {
        items.push(CompletionItem {
            label: key.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(detail.to_string()),
            ..CompletionItem::default()
        });
    }

    let compute_funcs = [
        ("add", "Arithmetic addition function `add(a, b)`"),
        ("sub", "Arithmetic subtraction function `sub(a, b)`"),
        ("mul", "Arithmetic multiplication function `mul(a, b)`"),
        ("div", "Floating-point division function `div(a, b)`"),
        ("mod", "Integer modulo function `mod(a, b)`"),
    ];

    for (func, detail) in compute_funcs {
        items.push(CompletionItem {
            label: format!("{func}(...)"),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(detail.to_string()),
            ..CompletionItem::default()
        });
    }

    items
}

fn get_line_prefix<'a>(text: &'a str, pos: Position) -> &'a str {
    let line = text.lines().nth(pos.line as usize).unwrap_or("");
    let target_utf16 = pos.character as usize;
    let mut current_utf16 = 0;
    let mut byte_offset = line.len();

    for (b_idx, c) in line.char_indices() {
        if current_utf16 >= target_utf16 {
            byte_offset = b_idx;
            break;
        }
        current_utf16 += c.len_utf16();
    }
    &line[..byte_offset]
}

fn path_completions(prefix: &str, uri: Option<&Uri>) -> Option<Vec<CompletionItem>> {
    let (raw_path, is_dir_only) = detect_path_context(prefix)?;

    let current_dir = uri
        .and_then(uri_to_file_path)
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });

    let (search_dir, base_prefix) = if let Some(stripped) = raw_path.strip_prefix('/') {
        let root = find_project_root(&current_dir);
        if let Some(last_slash) = stripped.rfind('/') {
            let dir_part = &stripped[..=last_slash];
            let base = &stripped[last_slash + 1..];
            (root.join(dir_part), base.to_string())
        } else {
            (root, stripped.to_string())
        }
    } else if let Some(last_slash) = raw_path.rfind('/') {
        let dir_part = &raw_path[..=last_slash];
        let base = &raw_path[last_slash + 1..];
        (current_dir.join(dir_part), base.to_string())
    } else {
        (current_dir, raw_path.to_string())
    };

    let entries = std::fs::read_dir(&search_dir).ok()?;
    let mut dir_items = Vec::new();
    let mut file_items = Vec::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();

        if file_name.starts_with('.') && !base_prefix.starts_with('.') {
            continue;
        }

        if !base_prefix.is_empty()
            && !file_name
                .to_lowercase()
                .starts_with(&base_prefix.to_lowercase())
        {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            if file_name == "target" || file_name == "node_modules" || file_name == ".git" {
                continue;
            }
            dir_items.push(CompletionItem {
                label: format!("{file_name}/"),
                insert_text: Some(format!("{file_name}/")),
                kind: Some(CompletionItemKind::FOLDER),
                detail: Some("Directory".to_string()),
                sort_text: Some(format!("0_{file_name}")),
                ..CompletionItem::default()
            });
        } else if !is_dir_only && file_type.is_file() {
            let detail = file_extension_detail(&file_name);
            file_items.push(CompletionItem {
                label: file_name.clone(),
                insert_text: Some(file_name.clone()),
                kind: Some(CompletionItemKind::FILE),
                detail: Some(detail),
                sort_text: Some(format!("1_{file_name}")),
                ..CompletionItem::default()
            });
        }
    }

    dir_items.sort_by(|a, b| a.label.cmp(&b.label));
    file_items.sort_by(|a, b| a.label.cmp(&b.label));

    let mut all = dir_items;
    all.extend(file_items);
    Some(all)
}

fn detect_path_context(prefix: &str) -> Option<(&str, bool)> {
    if let Some(idx) = prefix.rfind("dir:") {
        let after = &prefix[idx + 4..];
        if !after.contains(')') && !after.contains(']') && !after.contains('}') {
            let trimmed = after.trim_start();
            let path_str = trimmed.trim_matches(['"', '\'']);
            return Some((path_str, true));
        }
    }

    if let Some(idx) = prefix.rfind("@dir(") {
        let after = &prefix[idx + 5..];
        if !after.contains(')') && !after.contains(']') && !after.contains('}') {
            let trimmed = after.trim_start();
            let path_str = trimmed.trim_matches(['"', '\'']);
            return Some((path_str, true));
        }
    }

    if let Some(idx) = prefix.rfind("file:") {
        let after = &prefix[idx + 5..];
        if !after.contains(')') && !after.contains(']') && !after.contains('}') {
            let trimmed = after.trim_start();
            let path_str = trimmed.trim_matches(['"', '\'']);
            return Some((path_str, false));
        }
    }

    if let Some(idx) = prefix.rfind("./").or_else(|| prefix.rfind("../")) {
        let before = &prefix[..idx];
        if before.ends_with('(')
            || before.ends_with(':')
            || before.ends_with(',')
            || before.ends_with(' ')
            || before.ends_with('\t')
            || before.ends_with('"')
            || before.ends_with('\'')
        {
            let after = &prefix[idx..];
            if !after.contains(')') && !after.contains(']') && !after.contains('}') {
                let path_str = after.trim_matches(['"', '\'']);
                return Some((path_str, false));
            }
        }
    }

    None
}

fn find_project_root(start: &std::path::Path) -> std::path::PathBuf {
    let mut cur = start.to_path_buf();
    loop {
        if cur.join("default.config.tmt").exists() || cur.join(".git").exists() {
            return cur;
        }
        if !cur.pop() {
            break;
        }
    }
    start.to_path_buf()
}

fn file_extension_detail(name: &str) -> String {
    let path = std::path::Path::new(name);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext.to_lowercase().as_str() {
        "tmt" | "tm" => "Tomet document".to_string(),
        "md" | "markdown" => "Markdown document".to_string(),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => "Image".to_string(),
        "typ" => "Typst document".to_string(),
        "pdf" => "PDF document".to_string(),
        "json" | "yaml" | "yml" | "toml" => "Data file".to_string(),
        _ => "File".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn valid_document_has_no_diagnostics() {
        assert_eq!(diagnostics_for("#[ Hello ]\n"), Vec::new());
    }

    #[test]
    fn invalid_document_reports_one_diagnostic() {
        let diags = diagnostics_for("@caution[ unterminated\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diags[0].source.as_deref(), Some("tomet"));
    }

    #[test]
    fn duplicate_id_document_reports_validator_diagnostic() {
        let diags = diagnostics_for("#[ One ]{id: a}\n#[ Two ]{id: a}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diags[0].source.as_deref(), Some("tomet"));
        assert!(diags[0].message.contains("duplicate id `a`"));
    }

    #[test]
    fn hover_returns_element_info() {
        let text = "@callout(type: info)[ Message ]\n";
        let hover = hover_for(text, Position::new(0, 2), None).expect("hover found");
        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("callout"));
        } else {
            panic!("expected markup contents");
        }
    }

    #[test]
    fn document_symbols_returns_headings_and_elements() {
        let text = "#[ Heading ]\n\n@info[ Note ]\n";
        let symbols = document_symbols_for(text);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "# Heading");
        assert_eq!(symbols[1].name, "@info");
    }

    #[test]
    fn definition_finds_matching_id() {
        let text = "#[ Target ]{id: target1}\n\n@deck.ref(id: target1)\n";
        let uri = Uri::from_str("file:///test.tmt").unwrap();
        let def = definition_for(text, Position::new(2, 4), &uri);
        assert!(def.is_some());
    }

    #[test]
    fn completions_returns_items() {
        let items = completions_for("", Position::new(0, 0));
        assert!(!items.is_empty());
        // Each built-in is offered with the sigil its shape requires, so
        // an accepted completion is one that validates.
        assert!(items.iter().any(|i| i.label == "@meta"));
        assert!(items.iter().any(|i| i.label == "@link"));
        // `callout` joined `BUILTIN_KINDS`, so it is offered again. It
        // had been special-cased in eight places while the list did not
        // know about it -- including the Markdown reader, which produced
        // an element validation then called unknown.
        assert!(items.iter().any(|i| i.label == "@callout"));
        // `memo` really is not built in -- nothing special-cases it --
        // so it is still not offered.
        assert!(!items.iter().any(|i| i.label.ends_with("memo")));
    }

    #[test]
    fn completions_trigger_prefix() {
        // One sigil, so `@` offers every built-in whatever its shape.
        // Where each may be placed is said in the detail line instead.
        let at_items = completions_for("@", Position::new(0, 1));
        assert!(at_items.iter().any(|i| i.label == "link"));
        assert!(at_items.iter().any(|i| i.label == "config"));
        let config = at_items.iter().find(|i| i.label == "config").unwrap();
        assert!(config.detail.as_deref().unwrap().ends_with("(block)"));
        // `link` takes either shape -- a link alone on a line is how you
        // show one file -- so its detail carries no placement note.
        let link = at_items.iter().find(|i| i.label == "link").unwrap();
        let detail = link.detail.as_deref().unwrap();
        assert!(!detail.ends_with("(inline)") && !detail.ends_with("(block)"));
        let em = at_items.iter().find(|i| i.label == "em").unwrap();
        assert!(em.detail.as_deref().unwrap().ends_with("(inline)"));

        // `#` is the heading marker and offers no elements.
        assert!(
            completions_for("#", Position::new(0, 1))
                .iter()
                .all(|i| { i.label != "config" && i.label != "link" })
        );

        let interp_items = completions_for("${", Position::new(0, 2));
        assert!(interp_items.iter().any(|i| i.label == "add(...)"));
    }

    #[test]
    fn exact_cst_diagnostic_range_on_duplicate_id() {
        let text = "#[ A ]{id: my_id}\n\n#[ B ]{id: my_id}\n";
        let diags = diagnostics_for(text);
        assert_eq!(diags.len(), 1);
        let diag = &diags[0];
        assert_eq!(diag.range.start.line, 2);
        assert_eq!(diag.range.start.character, 11);
        assert_eq!(diag.range.end.character, 16);
    }

    #[test]
    fn format_edits_formats_tables() {
        let text = "@table[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n]\n";
        let edits = format_edits(text, None);
        assert_eq!(edits.len(), 1);
        assert!(
            edits[0]
                .new_text
                .contains("[ 殻  ][ 主量子数 n ][ 電子数 2n² ][")
        );
    }

    #[test]
    fn hover_on_table_header_cell() {
        let text = "@table(align: [left, right, right, left])[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n]\n";
        // Position on line 1, inside "[ 電子数 2n² ]" (e.g. character 25)
        let hover =
            hover_for(text, Position::new(1, 25), None).expect("hover found for table header");
        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("Table Header (Column 3)"));
            assert!(m.value.contains("電子数 2n²"));
            assert!(m.value.contains("right"));
            assert!(m.value.contains("Total Columns"));
            assert!(m.value.contains("4"));
        } else {
            panic!("expected markup contents");
        }
    }

    #[test]
    fn hover_on_table_data_cell() {
        let text = "@table(align: [left, right, right, left])[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n]\n";
        // Position on line 2, inside "[ 2 ]" (column 3, character 15)
        let hover =
            hover_for(text, Position::new(2, 15), None).expect("hover found for table data cell");
        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("Table Cell (Column 3, Row 2)"));
            assert!(m.value.contains("電子数 2n²"));
            assert!(m.value.contains("Value"));
            assert!(m.value.contains("2"));
            assert!(m.value.contains("right"));
        } else {
            panic!("expected markup contents");
        }
    }

    #[test]
    fn hover_on_table_overview() {
        let text = "@table(align: [left, right, right, left])[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n]\n";
        let hover =
            hover_for(text, Position::new(0, 2), None).expect("hover found for table overview");
        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("Table"));
            assert!(m.value.contains("Rows"));
            assert!(m.value.contains("2"));
            assert!(m.value.contains("Columns"));
            assert!(m.value.contains("4"));
            assert!(m.value.contains("電子数 2n²"));
        } else {
            panic!("expected markup contents");
        }
    }

    #[test]
    fn completions_with_multibyte_characters() {
        // "海岸@" where '海' and '岸' are 3 bytes each
        let text = "海岸@\n";
        // Position at character 3 (after '@')
        let items = completions_for(text, Position::new(0, 3));
        assert!(!items.is_empty());
        assert!(items.iter().any(|i| i.label == "link"));

        // Position at character 2 (inside '岸' in byte terms, but character 2 in LSP)
        let items2 = completions_for(text, Position::new(0, 2));
        assert!(!items2.is_empty());
    }

    #[test]
    fn hover_on_macro_evaluation() {
        let text = "@config{\n  macros: {\n    gh: \"https://github.com/tomet/tomet/issues/${1}\"\n    greet: \"Hello, ${1} ${2}!\"\n    copyright: \"(C) 2026 Tomet Projects\"\n  }\n}\n\n$gh(42)\n\n$greet(\"Alice\", \"Bob\")\n\n${copyright}\n\n$emoji(\"sparkles\")\n";

        // Hover on $gh(42) (line 8, char 2)
        let hover = hover_for(text, Position::new(8, 2), None).expect("hover found for $gh");
        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("Macro Result"));
            assert!(m.value.contains("[https://github.com/tomet/tomet/issues/42](https://github.com/tomet/tomet/issues/42)"));
        } else {
            panic!("expected markup contents");
        }

        // Hover on $greet("Alice", "Bob") (line 10, char 3)
        let hover2 = hover_for(text, Position::new(10, 3), None).expect("hover found for $greet");
        if let HoverContents::Markup(m) = hover2.contents {
            assert!(m.value.contains("Macro Result"));
            assert!(m.value.contains("```text\nHello, Alice Bob!\n```"));
        } else {
            panic!("expected markup contents");
        }

        // Hover on ${copyright} (line 12, char 3)
        let hover3 =
            hover_for(text, Position::new(12, 3), None).expect("hover found for ${copyright}");
        if let HoverContents::Markup(m) = hover3.contents {
            assert!(m.value.contains("Macro Result"));
            assert!(m.value.contains("(C) 2026 Tomet Projects"));
        } else {
            panic!("expected markup contents");
        }

        // Hover on $emoji("sparkles") (line 14, char 2)
        let hover4 = hover_for(text, Position::new(14, 2), None).expect("hover found for $emoji");
        if let HoverContents::Markup(m) = hover4.contents {
            assert!(m.value.contains("Macro Result"));
            assert!(m.value.contains("✨"));
        } else {
            panic!("expected markup contents");
        }
    }

    #[test]
    fn hover_on_undefined_macro_shows_error() {
        let text = "$undefined_macro(123)\n";
        let hover =
            hover_for(text, Position::new(0, 5), None).expect("hover found for undefined macro");
        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("Macro"));
            assert!(m.value.contains("Error"));
            assert!(m.value.contains("undefined_macro"));
        } else {
            panic!("expected markup contents");
        }
    }

    #[test]
    fn hover_on_macro_defined_in_settings_file_ref() {
        let unique = format!(
            "tomet_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("default.config.tmt");
        std::fs::write(
            &config_path,
            "@config(format:json)+++\n{\n  \"macros\": {\n    \"youtube_video\": \"https://www.youtube.com/watch?v=${1}\"\n  }\n}\n+++\n",
        )
        .unwrap();

        let doc_path = dir.join("sub/note.tmt");
        std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
        let doc_text =
            "@settings(file:\"file:default.config.tmt\")\n\n$youtube_video(\"Pm_h6FnF8HU\")\n";
        let uri = Uri::from_str(&format!("file://{}", doc_path.display())).unwrap();

        let hover = hover_for(doc_text, Position::new(2, 5), Some(&uri))
            .expect("hover found for external config macro");
        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("Macro Result"));
            assert!(
                m.value
                    .contains("https://www.youtube.com/watch?v=Pm_h6FnF8HU")
            );
        } else {
            panic!("expected markup contents");
        }

        // Test embed with macro
        let embed_doc = "@settings(file:\"file:default.config.tmt\")\n\n@embed($youtube_video(\"Pm_h6FnF8HU\"))[Flo Rida]\n";
        let hover2 = hover_for(embed_doc, Position::new(2, 10), Some(&uri))
            .expect("hover found for embed macro");
        if let HoverContents::Markup(m) = hover2.contents {
            assert!(m.value.contains("Macro Result"));
            assert!(
                m.value
                    .contains("https://www.youtube.com/watch?v=Pm_h6FnF8HU")
            );
        } else {
            panic!("expected markup contents");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hover_on_macro_auto_discovered_from_workspace_config() {
        let unique = format!(
            "tomet_auto_cfg_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("default.config.tmt");
        std::fs::write(
            &config_path,
            "@config(format:json)+++\n{\n  \"macros\": {\n    \"youtube_video\": \"https://www.youtube.com/watch?v=${1}\",\n    \"twitter_post\": \"https://x.com/${1}/status/${2}\"\n  }\n}\n+++\n",
        )
        .unwrap();

        // Note has NO header at all!
        let doc_path = dir.join("10-19 Journal/12 Daily/2024/12/$2024-12-26.tmt");
        std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
        let doc_text = "@embed($youtube_video(\"Pm_h6FnF8HU\"))[Low]\n\n@embed($twitter_post(\"kosekibijou\", \"1807568682631254496\"))[Bijou]\n";
        let uri =
            Uri::from_str(&format!("file://{}", doc_path.display()).replace(' ', "%20")).unwrap();

        // Hover on youtube_video
        let hover = hover_for(doc_text, Position::new(0, 10), Some(&uri))
            .expect("hover found for auto-discovered youtube macro");
        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("Macro Result"));
            assert!(
                m.value
                    .contains("https://www.youtube.com/watch?v=Pm_h6FnF8HU")
            );
        } else {
            panic!("expected markup contents");
        }

        // Hover on twitter_post (two arguments)
        let hover2 = hover_for(doc_text, Position::new(2, 10), Some(&uri))
            .expect("hover found for auto-discovered twitter macro");
        if let HoverContents::Markup(m) = hover2.contents {
            assert!(m.value.contains("Macro Result"));
            assert!(
                m.value
                    .contains("https://x.com/kosekibijou/status/1807568682631254496")
            );
        } else {
            panic!("expected markup contents");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hover_on_macro_defined_in_config_import() {
        let unique = format!(
            "tomet_import_cfg_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("custom.config.tmt");
        std::fs::write(
            &config_path,
            "@config{\n  macros: {\n    wiki: \"https://ja.wikipedia.org/wiki/${1}\"\n  }\n}\n",
        )
        .unwrap();

        let doc_path = dir.join("note.tmt");
        let doc_text = "@config(import: \"custom.config.tmt\")\n\n$wiki(\"Rust\")\n";
        let uri = Uri::from_str(&format!("file://{}", doc_path.display())).unwrap();

        let hover = hover_for(doc_text, Position::new(2, 5), Some(&uri))
            .expect("hover found for @config(import:...) macro");
        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("Macro Result"));
            assert!(m.value.contains("https://ja.wikipedia.org/wiki/Rust"));
        } else {
            panic!("expected markup contents");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hover_on_kind_and_version() {
        let doc_text = "@version(1.0)\n@kind(j.daily)\n\n#[ Title ]\n";
        let hover_ver =
            hover_for(doc_text, Position::new(0, 3), None).expect("hover found for #version");
        if let HoverContents::Markup(m) = hover_ver.contents {
            assert!(m.value.contains("Tomet Version"));
            assert!(m.value.contains("1"));
        } else {
            panic!("expected markup contents");
        }

        let hover_kind =
            hover_for(doc_text, Position::new(1, 3), None).expect("hover found for #kind");
        if let HoverContents::Markup(m) = hover_kind.contents {
            assert!(m.value.contains("Document Kind"));
            assert!(m.value.contains("j.daily"));
        } else {
            panic!("expected markup contents");
        }
    }

    #[test]
    fn completions_suggest_kind_and_version() {
        // `kind` and `version` are block directives, offered under the one
        // element sigil like everything else.
        let items = completions_for("@", Position::new(0, 1));
        assert!(items.iter().any(|i| i.label == "kind"));
        assert!(items.iter().any(|i| i.label == "version"));
    }

    #[test]
    fn completions_for_file_path() {
        let current = std::env::current_dir().unwrap();
        let test_file = current.join("test_dummy.tmt");
        let uri = Uri::from_str(&format!("file://{}", test_file.to_string_lossy())).unwrap();

        let doc_text = "@link(file: ./";
        let items = completions_for_with_uri(doc_text, Position::new(0, 14), Some(&uri));
        assert!(!items.is_empty(), "path completions should return entries");
        assert!(
            items.iter().any(|i| i.label == "Cargo.toml"),
            "should suggest Cargo.toml"
        );
        assert!(
            items.iter().any(|i| i.label == "src/" && i.kind == Some(CompletionItemKind::FOLDER)),
            "should suggest src/ directory"
        );
    }

    #[test]
    fn completions_for_dir_only_suggests_directories() {
        let current = std::env::current_dir().unwrap();
        let test_file = current.join("test_dummy.tmt");
        let uri = Uri::from_str(&format!("file://{}", test_file.to_string_lossy())).unwrap();

        let doc_text = "@dir(./";
        let items = completions_for_with_uri(doc_text, Position::new(0, 7), Some(&uri));
        assert!(!items.is_empty(), "dir completions should return directories");
        assert!(
            items.iter().any(|i| i.label == "src/"),
            "should suggest src/ directory"
        );
        assert!(
            !items.iter().any(|i| i.label == "Cargo.toml"),
            "should NOT suggest Cargo.toml file for @dir"
        );
    }

    #[test]
    fn completions_for_nested_directory_path() {
        let current = std::env::current_dir().unwrap();
        let test_file = current.join("test_dummy.tmt");
        let uri = Uri::from_str(&format!("file://{}", test_file.to_string_lossy())).unwrap();

        let doc_text = "@link(file: ./src/";
        let items = completions_for_with_uri(doc_text, Position::new(0, 18), Some(&uri));
        assert!(!items.is_empty(), "nested directory should return entries");
        assert!(
            items.iter().any(|i| i.label == "lib.rs"),
            "should suggest src/lib.rs"
        );
    }
}
