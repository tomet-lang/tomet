//! AST extraction of `$cache.<kind>(key, local: ..., inputs: ...)` expressions.

use tomet_ast::{File, InterpExpr, InterpExprKind, Value};
use tomet_expr::CacheRequest;
use tomet_scalar::Scalar;
use tomet_span::Span;

/// A statically extracted `$cache.<kind>(...)` call paired with its source [`Span`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedCacheRequest {
    /// The extracted cache request.
    pub request: CacheRequest,
    /// Byte span of the `cache.<kind>(...)` call expression in the document.
    pub span: Span,
}

/// Walks the entire AST of `doc` (header declarations, sections, blocks, elements,
/// lists, connects, and inline spans) via `tomet-tree` and extracts every static
/// `$cache.<kind>(key, local: ..., inputs: ...)` call in document order.
pub fn collect_cache_requests(doc: &File) -> Vec<LocatedCacheRequest> {
    let mut requests = Vec::new();
    tomet_tree::for_each_interpolation(doc, |interp| {
        collect_from_expr(&interp.expr, &mut requests);
    });
    requests
}

/// Walks a parsed Cotomet data file (`File<Value>`, header declarations and body
/// value) and extracts every static `$cache.<kind>(key, local: ..., inputs: ...)`
/// call in document order.
pub fn collect_cache_requests_from_data_file(doc: &File<Value>) -> Vec<LocatedCacheRequest> {
    let mut requests = Vec::new();
    for decl in &doc.header {
        if let Some(args) = &decl.data.args {
            for entry in args.entries() {
                tomet_tree::for_each_interpolation_in_value(entry.value(), |interp| {
                    collect_from_expr(&interp.expr, &mut requests);
                });
            }
        }
        if let Some(group) = &decl.data.value {
            for entry in &group.0 {
                if let tomet_ast::Entry::Pair(_, value) = entry {
                    tomet_tree::for_each_interpolation_in_value(value, |interp| {
                        collect_from_expr(&interp.expr, &mut requests);
                    });
                }
            }
        }
    }
    tomet_tree::for_each_interpolation_in_value(&doc.body, |interp| {
        collect_from_expr(&interp.expr, &mut requests);
    });
    requests
}

fn unwrap_group(mut expr: &InterpExpr) -> &InterpExpr {
    while let InterpExprKind::Group(inner) = &expr.kind {
        expr = inner;
    }
    expr
}

fn cache_kind(callee: &InterpExpr) -> Option<&str> {
    let InterpExprKind::Member { object, member } = &unwrap_group(callee).kind else {
        return None;
    };
    let InterpExprKind::Identifier(name) = &unwrap_group(object).kind else {
        return None;
    };
    (name == "cache").then_some(member.as_str())
}

fn string_literal(expr: &InterpExpr) -> Option<String> {
    match &unwrap_group(expr).kind {
        InterpExprKind::Literal(Scalar::Text(text)) => Some(text.clone()),
        _ => None,
    }
}

fn bool_literal(expr: &InterpExpr) -> Option<bool> {
    match &unwrap_group(expr).kind {
        InterpExprKind::Literal(Scalar::Bool(b)) => Some(*b),
        _ => None,
    }
}

fn inputs_literals(expr: &InterpExpr) -> Option<Vec<String>> {
    match &expr.kind {
        InterpExprKind::Seq(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(string_literal(item)?);
            }
            Some(out)
        }
        // Single-item `("scripts/deploy.sh")` parses as `Group(Literal(...))` or `Group(Seq(...))`.
        InterpExprKind::Group(inner) => match &inner.kind {
            InterpExprKind::Seq(_) | InterpExprKind::Group(_) => inputs_literals(inner),
            _ => Some(vec![string_literal(inner)?]),
        },
        _ => None,
    }
}

fn extract_request(kind: &str, args: &[InterpExpr]) -> Option<CacheRequest> {
    let mut positional = Vec::new();
    let mut local: Option<bool> = None;
    let mut inputs: Option<Vec<String>> = None;

    for arg in args {
        match &arg.kind {
            InterpExprKind::NamedArg { name, value } => match name.as_str() {
                "local" => {
                    if local.is_some() {
                        return None;
                    }
                    local = Some(bool_literal(value)?);
                }
                "inputs" => {
                    if inputs.is_some() {
                        return None;
                    }
                    inputs = Some(inputs_literals(value)?);
                }
                _ => return None,
            },
            _ => positional.push(arg),
        }
    }

    if positional.len() != 1 {
        return None;
    }
    let key = string_literal(positional[0])?;

    Some(CacheRequest {
        kind: kind.to_string(),
        key,
        local: local.unwrap_or(false),
        inputs: inputs.unwrap_or_default(),
    })
}

fn collect_from_expr(expr: &InterpExpr, out: &mut Vec<LocatedCacheRequest>) {
    match &expr.kind {
        InterpExprKind::Call { callee, args } => {
            if let Some(kind) = cache_kind(callee)
                && let Some(request) = extract_request(kind, args)
            {
                out.push(LocatedCacheRequest {
                    request,
                    span: expr.span,
                });
            }
            collect_from_expr(callee, out);
            for arg in args {
                collect_from_expr(arg, out);
            }
        }
        InterpExprKind::Group(inner) => collect_from_expr(inner, out),
        InterpExprKind::Seq(items) => {
            for item in items {
                collect_from_expr(item, out);
            }
        }
        InterpExprKind::Lambda { body, .. } => collect_from_expr(body, out),
        InterpExprKind::NamedArg { value, .. } => collect_from_expr(value, out),
        InterpExprKind::Binary { lhs, rhs, .. } => {
            collect_from_expr(lhs, out);
            collect_from_expr(rhs, out);
        }
        InterpExprKind::Member { object, .. }
        | InterpExprKind::Connect { object, .. }
        | InterpExprKind::DocId { doc: object, .. } => collect_from_expr(object, out),
        InterpExprKind::Literal(_)
        | InterpExprKind::Identifier(_)
        | InterpExprKind::Id(_)
        | InterpExprKind::Addr { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_across_header_sections_blocks_elements_and_nested_exprs() {
        let src = r#"::kind(guide)
::meta(
  rev: $cache.command("git rev-parse --short HEAD")
)

=[ Status: $cache.command("whoami", local: true) ]

Here is inline $cache.http("https://example.com").body and nested {$ raw(cache.command("tomet --help"), lang: "text") }.

@note(label: $cache.command("wc -l < scripts/deploy.sh", inputs: ("scripts/deploy.sh")))[
  - $cache.ai("summarize", inputs: ("a.tmt", "b.tmt"), local: false)
]
"#;
        let doc = tomet_ast_parser::parse_file(src).expect("valid tmt");
        let found = collect_cache_requests(&doc);
        let requests: Vec<&CacheRequest> = found.iter().map(|r| &r.request).collect();
        assert_eq!(
            requests,
            vec![
                &CacheRequest {
                    kind: "command".into(),
                    key: "git rev-parse --short HEAD".into(),
                    local: false,
                    inputs: vec![],
                },
                &CacheRequest {
                    kind: "command".into(),
                    key: "whoami".into(),
                    local: true,
                    inputs: vec![],
                },
                &CacheRequest {
                    kind: "http".into(),
                    key: "https://example.com".into(),
                    local: false,
                    inputs: vec![],
                },
                &CacheRequest {
                    kind: "command".into(),
                    key: "tomet --help".into(),
                    local: false,
                    inputs: vec![],
                },
                &CacheRequest {
                    kind: "command".into(),
                    key: "wc -l < scripts/deploy.sh".into(),
                    local: false,
                    inputs: vec!["scripts/deploy.sh".into()],
                },
                &CacheRequest {
                    kind: "ai".into(),
                    key: "summarize".into(),
                    local: false,
                    inputs: vec!["a.tmt".into(), "b.tmt".into()],
                },
            ]
        );
        for item in &found {
            assert!(item.span.start < item.span.end);
            assert!(src[item.span.start..item.span.end].contains("cache."));
        }
    }

    #[test]
    fn handles_empty_and_trailing_comma_inputs() {
        let src = r#"
$cache.command("a", inputs: ())
$cache.command("b", inputs: ("only.txt",))
"#;
        let doc = tomet_ast_parser::parse_file(src).expect("valid tmt");
        let found = collect_cache_requests(&doc);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].request.inputs, Vec::<String>::new());
        assert_eq!(found[1].request.inputs, vec!["only.txt".to_string()]);
    }

    #[test]
    fn collects_from_cotomet_data_file() {
        let src = r#"::kind(config)

rev:   $cache.command("git rev-parse --short HEAD")
lines: $cache.command("wc -l < a.sh", inputs: ("a.sh"))
user:  $cache.command("whoami", local: true)
"#;
        let doc = tomet_ast_parser::parse_data_file(src).expect("valid ctmt");
        let found = collect_cache_requests_from_data_file(&doc);
        let keys: Vec<&str> = found.iter().map(|r| r.request.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["git rev-parse --short HEAD", "wc -l < a.sh", "whoami"]
        );
        assert_eq!(found[1].request.inputs, vec!["a.sh".to_string()]);
        assert!(found[2].request.local);
    }
}
