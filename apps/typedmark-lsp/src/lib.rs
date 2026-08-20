//! Language server library for TypedMark (`typedmark-lsp`).
//! Provides diagnostics (parser & validator), document formatting,
//! hover, document symbols, goto definition, and completions.

use lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol,
    GotoDefinitionResponse, Hover, HoverContents, Location, MarkupContent, MarkupKind, Position,
    Range, SymbolKind, TextEdit, Uri,
};
use std::ops::ControlFlow;
use typedmark_ast::{ElementValue, Inline, InterpExprKind, Sigil, Span, Value};
use typedmark_semantics::{classify, infer_at_kind, normalized_element_args};
use typedmark_walk::{Node, Visitor, walk_document};

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

/// Parses `text` and produces diagnostics (both parse errors and AST validation rules).
pub fn diagnostics_for(text: &str) -> Vec<Diagnostic> {
    match typedmark_parser::parse_document(text) {
        Ok(doc) => {
            let validation_errors = typedmark_validator::validate_document(&doc);
            validation_errors
                .into_iter()
                .map(|err| match &err {
                    typedmark_validator::ValidationError::DuplicateId { duplicate, .. } => {
                        Diagnostic {
                            range: span_to_range(duplicate),
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some("typedmark".to_string()),
                            message: err.to_string(),
                            ..Diagnostic::default()
                        }
                    }
                })
                .collect()
        }
        Err(err) => vec![Diagnostic {
            range: error_range(&err, text),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("typedmark".to_string()),
            message: err.message.clone(),
            ..Diagnostic::default()
        }],
    }
}

fn error_range(err: &typedmark_parser::Error, text: &str) -> Range {
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

/// Formats the document using `typedmark-formatter`.
pub fn format_edits(text: &str) -> Vec<TextEdit> {
    let formatted = typedmark_formatter::format_source(text);
    if formatted == text {
        return Vec::new();
    }
    vec![TextEdit {
        range: whole_document_range(text),
        new_text: formatted,
    }]
}

fn whole_document_range(text: &str) -> Range {
    let lines: Vec<&str> = text.split('\n').collect();
    let end_line = lines.len().saturating_sub(1) as u32;
    let end_char = lines.last().map(|l| l.chars().count()).unwrap_or(0) as u32;
    Range::new(Position::new(0, 0), Position::new(end_line, end_char))
}

/// Provides hover information for the symbol under the cursor.
pub fn hover_for(text: &str, pos: Position) -> Option<Hover> {
    let doc = typedmark_parser::parse_document(text).ok()?;
    let target_line = pos.line as usize + 1;
    let target_col = pos.character as usize + 1;

    struct HoverFinder {
        line: usize,
        col: usize,
        best_match: Option<(Span, String)>,
    }

    impl Visitor<()> for HoverFinder {
        fn visit(&mut self, node: Node<'_>) -> ControlFlow<()> {
            let span = node.span();
            if span_contains(&span, self.line, self.col) {
                let hover_text = match node {
                    Node::Heading(h) => {
                        format!("**Heading Level {}**", h.level)
                    }
                    Node::ListItem(item) => {
                        format!(
                            "**List Item** (marker: `{}`)",
                            item.marker.as_deref().unwrap_or("-")
                        )
                    }
                    Node::Element(el) => {
                        let kind = classify(el);
                        let inferred = infer_at_kind(el.args.as_ref());
                        let mut desc = format!("**Element**: `{}`\n", sigil_display_name(&el.sigil));
                        desc.push_str(&format!("- **Classification**: `{}`\n", kind.as_str()));
                        if let Some(inf) = inferred {
                            desc.push_str(&format!("- **Inferred Kind**: `{inf}`\n"));
                        }
                        if let Some(norm_args) = normalized_element_args(el) {
                            desc.push_str(&format!("- **Normalized Args**: `{norm_args:?}`\n"));
                        }
                        desc
                    }
                };
                self.best_match = Some((span, hover_text));
            }
            ControlFlow::Continue(())
        }
    }

    let mut finder = HoverFinder {
        line: target_line,
        col: target_col,
        best_match: None,
    };
    let _ = walk_document(&doc, &mut finder);

    finder.best_match.map(|(span, value)| Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(span_to_range(&span)),
    })
}

fn sigil_display_name(sigil: &Sigil) -> String {
    match sigil {
        Sigil::Type(name) => format!("<{name}>"),
        Sigil::At(Some(name)) => format!("@{name}"),
        Sigil::At(None) => "@".to_string(),
        Sigil::Bare => "(bare)".to_string(),
        Sigil::Dollar => "${...}".to_string(),
    }
}

/// Provides document symbols for outline navigation in the editor.
pub fn document_symbols_for(text: &str) -> Vec<DocumentSymbol> {
    let Ok(doc) = typedmark_parser::parse_document(text) else {
        return Vec::new();
    };

    struct SymbolCollector(Vec<DocumentSymbol>);

    impl Visitor<()> for SymbolCollector {
        fn visit(&mut self, node: Node<'_>) -> ControlFlow<()> {
            let span = node.span();
            let range = span_to_range(&span);
            let symbol = match node {
                Node::Heading(h) => {
                    let title = extract_inlines_text(&h.content);
                    let name = format!("{} {}", "#".repeat(h.level as usize), title);
                    #[allow(deprecated)]
                    DocumentSymbol {
                        name,
                        detail: Some(format!("Heading Level {}", h.level)),
                        kind: SymbolKind::STRING,
                        tags: None,
                        deprecated: None,
                        range,
                        selection_range: range,
                        children: None,
                    }
                }
                Node::Element(el) => {
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
                }
                Node::ListItem(item) => {
                    let text = extract_inlines_text(&item.content);
                    let marker = item.marker.as_deref().unwrap_or("-");
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
    let doc = typedmark_parser::parse_document(text).ok()?;
    let target_line = pos.line as usize + 1;
    let target_col = pos.character as usize + 1;

    // Find the node under cursor to get the referenced ID
    struct TargetIdFinder {
        line: usize,
        col: usize,
        ref_id: Option<String>,
    }

    impl Visitor<()> for TargetIdFinder {
        fn visit(&mut self, node: Node<'_>) -> ControlFlow<()> {
            let span = node.span();
            if span_contains(&span, self.line, self.col) {
                if let Node::Element(el) = node {
                    // Check if it's an @(ref: "some_id") or has a ref arg
                    if let Some(Value::Map(entries)) = &el.args {
                        for (k, v) in entries {
                            if k == "ref" {
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
        fn visit(&mut self, node: Node<'_>) -> ControlFlow<()> {
            if let Some(Value::Map(entries)) = node.attrs() {
                for (k, v) in entries {
                    if k == "id" {
                        if let Value::String(s) = v {
                            if s == &self.target_id {
                                self.found_span = Some(node.span());
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

/// Provides autocompletion items for elements, keywords, and builtin compute functions.
pub fn completions_for(_text: &str, _pos: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    let builtins = [
        ("callout", "Callout container block"),
        ("warning", "Warning alert block"),
        ("caution", "Caution alert block"),
        ("info", "Information alert block"),
        ("note", "Note alert block"),
        ("tip", "Tip alert block"),
        ("important", "Important alert block"),
        ("codeblock", "Verbatim code block"),
        ("blockquote", "Quote block"),
        ("hr", "Horizontal rule divider"),
        ("meta", "Metadata key-value declaration"),
        ("config", "Document-wide configuration"),
        ("settings", "Settings file reference"),
        ("links", "Link reference definitions table"),
        ("connect", "Connected data element target"),
    ];

    for (name, detail) in builtins {
        items.push(CompletionItem {
            label: format!("<{name}>"),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(detail.to_string()),
            ..CompletionItem::default()
        });
        items.push(CompletionItem {
            label: format!("@{name}"),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(detail.to_string()),
            ..CompletionItem::default()
        });
    }

    let inferred_keys = [
        ("url", "URL link reference"),
        ("file", "File reference path"),
        ("ref", "Internal element ID reference"),
        ("id", "Element unique identifier"),
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
        let diags = diagnostics_for("<caution>[ unterminated\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diags[0].source.as_deref(), Some("typedmark"));
    }

    #[test]
    fn duplicate_id_document_reports_validator_diagnostic() {
        let diags = diagnostics_for("#[ One ]{id: a}\n#[ Two ]{id: a}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diags[0].source.as_deref(), Some("typedmark"));
        assert!(diags[0].message.contains("duplicate id `a`"));
    }

    #[test]
    fn hover_returns_element_info() {
        let text = "<callout>(type: info)[ Message ]\n";
        let hover = hover_for(text, Position::new(0, 2)).expect("hover found");
        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("callout"));
        } else {
            panic!("expected markup contents");
        }
    }

    #[test]
    fn document_symbols_returns_headings_and_elements() {
        let text = "#[ Heading ]\n\n<info>[ Note ]\n";
        let symbols = document_symbols_for(text);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "# Heading");
        assert_eq!(symbols[1].name, "<info>");
    }

    #[test]
    fn definition_finds_matching_id() {
        let text = "#[ Target ]{id: target1}\n\n@(ref: target1)\n";
        let uri = Uri::from_str("file:///test.tm").unwrap();
        let def = definition_for(text, Position::new(2, 4), &uri);
        assert!(def.is_some());
    }

    #[test]
    fn completions_returns_items() {
        let items = completions_for("", Position::new(0, 0));
        assert!(!items.is_empty());
        assert!(items.iter().any(|i| i.label == "<callout>"));
    }
}
