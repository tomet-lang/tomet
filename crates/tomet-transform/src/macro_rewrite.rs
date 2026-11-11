//! Reverse URL-to-macro pattern matching and rewriting.

use std::collections::HashMap;
use tomet_ast::{File, InterpExpr, InterpExprKind, Interpolation, Literal, Value};
use tomet_semantics::{ElementKind, classify_std_lenient};
use tomet_span::Span;
use tomet_tree::{DataExt, for_each_element_mut};

/// A compiled macro matcher for reverse URL-to-macro rewriting.
#[derive(Debug, Clone)]
pub struct MacroPattern {
    pub name: String,
    pub template: String,
    /// Static segments and placeholder positions.
    pub segments: Vec<String>,
}

impl MacroPattern {
    /// Compiles a macro template into a pattern matcher.
    pub fn from_template(name: &str, template: &str) -> Self {
        let mut segments = Vec::new();
        let mut rest = template;

        let mut idx = 1;
        while let Some(pos) = rest.find(&format!("$({idx})")) {
            segments.push(rest[..pos].to_string());
            rest = &rest[pos + format!("$({idx})").len()..];
            idx += 1;
        }
        segments.push(rest.to_string());

        Self {
            name: name.to_string(),
            template: template.to_string(),
            segments,
        }
    }

    /// Attempts to match `url` against this pattern. If matched, returns the extracted arguments.
    pub fn match_url(&self, url: &str) -> Option<Vec<String>> {
        if self.segments.is_empty() {
            return None;
        }

        if self.segments.len() == 1 {
            return if url == self.segments[0] {
                Some(Vec::new())
            } else {
                None
            };
        }

        let first_prefix = &self.segments[0];
        if !url.starts_with(first_prefix) {
            return None;
        }

        let mut current_pos = first_prefix.len();
        let mut args = Vec::new();

        for i in 1..self.segments.len() {
            let next_sep = &self.segments[i];
            if next_sep.is_empty() {
                let captured = &url[current_pos..];
                if captured.is_empty() {
                    return None;
                }
                args.push(captured.to_string());
                current_pos = url.len();
            } else {
                let sep_pos = url[current_pos..].find(next_sep)?;
                let captured = &url[current_pos..current_pos + sep_pos];
                if captured.is_empty() {
                    return None;
                }
                args.push(captured.to_string());
                current_pos += sep_pos + next_sep.len();
            }
        }

        if current_pos == url.len() {
            Some(args)
        } else {
            None
        }
    }

    /// The call `$macro.<name>(args)` for the arguments `match_url` extracted. An argument that
    /// reads as the integer it is written as stays an integer; every other one is a string.
    pub fn invocation(&self, args: &[String]) -> Interpolation {
        let node = |kind| InterpExpr {
            kind,
            span: Span::dummy(),
        };
        let callee = node(InterpExprKind::Member {
            object: Box::new(node(InterpExprKind::Identifier("macro".into()))),
            member: self.name.clone(),
        });
        let args = args
            .iter()
            .map(|a| {
                node(InterpExprKind::Literal(match a.parse::<i64>() {
                    Ok(n) if n.to_string() == *a => Literal::Int(n),
                    _ => Literal::String(a.clone()),
                }))
            })
            .collect();
        Interpolation {
            expr: node(InterpExprKind::Call {
                callee: Box::new(callee),
                args,
            }),
            span: Span::dummy(),
        }
    }
}

/// A collection of macro patterns sorted by specificity (longer constant prefix first).
#[derive(Debug, Clone, Default)]
pub struct MacroSet {
    patterns: Vec<MacroPattern>,
}

impl MacroSet {
    /// The patterns of the text macros in `macros`. A URL is text, so only a text macro can stand
    /// for one; a block macro is not a pattern a link target could match, and is left out.
    pub fn from_map(macros: &HashMap<String, Value>) -> Self {
        let mut patterns: Vec<MacroPattern> = macros
            .iter()
            .filter_map(|(name, tmpl)| match tmpl {
                Value::String(tmpl) => Some(MacroPattern::from_template(name, tmpl)),
                _ => None,
            })
            .collect();

        patterns.sort_by(|a, b| {
            let a_prefix_len = a.segments.first().map_or(0, |s| s.len());
            let b_prefix_len = b.segments.first().map_or(0, |s| s.len());
            b_prefix_len
                .cmp(&a_prefix_len)
                .then_with(|| b.template.len().cmp(&a.template.len()))
        });

        Self { patterns }
    }

    /// Rewrites `url` to a macro invocation if any pattern matches.
    pub fn rewrite_url(&self, url: &str) -> Option<Interpolation> {
        self.patterns
            .iter()
            .find_map(|pat| pat.match_url(url).map(|args| pat.invocation(&args)))
    }
}

/// Transforms link targets in `doc` matching any macro in `macro_set` into `$macro.<name>(...)` invocations.
pub fn transform_link_targets_with_macros(doc: &mut File, macro_set: &MacroSet) -> usize {
    let mut count = 0;
    for_each_element_mut(doc, |el| {
        let kind = classify_std_lenient(el.as_view());
        if !matches!(kind, ElementKind::Link | ElementKind::Embed) {
            return;
        }

        if let Some(Value::String(url)) = el.data.get_attr("target") {
            if let Some(macro_call) = macro_set.rewrite_url(url) {
                el.data
                    .transform_prop("target", |_| Value::Interp(macro_call));
                count += 1;
            }
        } else if let Some(Value::String(url)) = &el.data.args
            && let Some(macro_call) = macro_set.rewrite_url(url)
        {
            el.data.args = Some(Value::Interp(macro_call));
            count += 1;
        }
    });
    count
}
