//! Reverse URL-to-macro pattern matching and rewriting.

use std::collections::HashMap;
use tomet_ast::{Document, Value};
use tomet_semantics::{ElementKind, classify_lenient};
use tomet_tree::{ElementExt, transform_elements};

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
        while let Some(pos) = rest.find(&format!("${{{idx}}}")) {
            segments.push(rest[..pos].to_string());
            rest = &rest[pos + format!("${{{idx}}}").len()..];
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
                let Some(sep_pos) = url[current_pos..].find(next_sep) else {
                    return None;
                };
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

    /// Formats extracted args into `$macro("arg1", "arg2")` invocation syntax.
    pub fn format_invocation(&self, args: &[String]) -> String {
        let formatted_args: Vec<String> = args
            .iter()
            .map(|a| {
                if a.parse::<i64>().is_ok() {
                    a.clone()
                } else {
                    format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\""))
                }
            })
            .collect();

        format!("${}({})", self.name, formatted_args.join(", "))
    }
}

/// A collection of macro patterns sorted by specificity (longer constant prefix first).
#[derive(Debug, Clone, Default)]
pub struct MacroSet {
    patterns: Vec<MacroPattern>,
}

impl MacroSet {
    pub fn from_map(macros: &HashMap<String, String>) -> Self {
        let mut patterns: Vec<MacroPattern> = macros
            .iter()
            .map(|(name, tmpl)| MacroPattern::from_template(name, tmpl))
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
    pub fn rewrite_url(&self, url: &str) -> Option<String> {
        for pat in &self.patterns {
            if let Some(args) = pat.match_url(url) {
                return Some(pat.format_invocation(&args));
            }
        }
        None
    }
}

/// Transforms link targets in `doc` matching any macro in `macro_set` into `$macro(...)` invocations.
pub fn transform_link_targets_with_macros(doc: &mut Document, macro_set: &MacroSet) -> usize {
    transform_elements(
        doc,
        |el| {
            let kind = classify_lenient(el);
            matches!(kind, ElementKind::Link | ElementKind::Embed)
        },
        |el| {
            let mut changed = false;

            if let Some(target_val) = el.get_attr("target") {
                if let Value::String(url) = target_val {
                    if let Some(macro_call) = macro_set.rewrite_url(url) {
                        el.transform_prop("target", |_| Value::String(macro_call));
                        changed = true;
                    }
                }
            } else if let Some(Value::String(url)) = &el.args {
                if let Some(macro_call) = macro_set.rewrite_url(url) {
                    el.args = Some(Value::String(macro_call));
                    changed = true;
                }
            }

            changed
        },
    )
}
