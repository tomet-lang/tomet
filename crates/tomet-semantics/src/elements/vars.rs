//! `::vars{ name: { type, default, about } }` -- the variables a document or
//! a vault declares. See `tmtroot/spec/elements/std.vars.element.tmt`.
//!
//! Every entry is a declaration, a map of fields; there is no spelling that
//! writes a value directly, so a map-valued default is never mistaken for a
//! declaration (it goes in `default:`).

use tomet_ast::{Block, Element, Inline, Value};
use tomet_tree::ValueExt;

use crate::normalized_element_args;

/// The types a variable can be declared with: the vocabulary of `type:` in the
/// settings' `meta` fields, less the ones a variable has no use for.
pub const VAR_TYPES: [&str; 6] = ["string", "int", "float", "bool", "datetime", "list"];

/// The type of a variable that does not say: what the command line passes.
pub const DEFAULT_VAR_TYPE: &str = "string";

/// The keys a declaration may have.
pub const VAR_KEYS: [&str; 3] = ["type", "default", "about"];

/// One variable, as declared.
#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    /// `type:` as written; `None` is a string ([`DEFAULT_VAR_TYPE`]).
    pub ty: Option<String>,
    /// `default:`. A variable without one is required.
    pub default: Option<Value>,
    /// `about:`, for hover and completion.
    pub about: Option<String>,
}

impl VarDecl {
    /// The declared type, a string when the declaration does not say.
    pub fn type_name(&self) -> &str {
        self.ty.as_deref().unwrap_or(DEFAULT_VAR_TYPE)
    }

    /// Whether a value has to be given: there is no `default`.
    pub fn is_required(&self) -> bool {
        self.default.is_none()
    }

    /// Reads one entry of `::vars`. The reason is for `tomet check`.
    pub fn from_value(value: &Value) -> Result<VarDecl, String> {
        let Value::Map(fields) = value else {
            return Err(
                "a variable is declared as a map: `{ type: ..., default: ..., about: ... }`"
                    .to_string(),
            );
        };
        let mut decl = VarDecl {
            ty: None,
            default: None,
            about: None,
        };
        for (key, field) in fields {
            match key.as_str() {
                "type" => match field.as_str() {
                    Some(ty) => decl.ty = Some(ty.to_string()),
                    None => return Err("`type` is a name, such as `string`".to_string()),
                },
                "default" => decl.default = Some(field.clone()),
                "about" => match plain_text(field) {
                    Some(about) => decl.about = Some(about),
                    None => return Err("`about` is text".to_string()),
                },
                other => {
                    return Err(format!(
                        "unknown key `{other}`; a variable has {}",
                        VAR_KEYS.map(|k| format!("`{k}`")).join(", ")
                    ));
                }
            }
        }
        Ok(decl)
    }
}

/// The words of a text value: a string, or the paragraphs `about: | ...` reads
/// as, joined by a newline. Anything inside a paragraph that is not text (an
/// element) is left out.
fn plain_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Blocks(blocks) => {
            let mut out = String::new();
            for block in blocks {
                let Block::Paragraph(paragraph) = block else {
                    continue;
                };
                if !out.is_empty() {
                    out.push('\n');
                }
                for inline in &paragraph.content {
                    match inline {
                        Inline::Text(text) => out.push_str(&text.value),
                        Inline::SoftBreak(_) => out.push(' '),
                        _ => {}
                    }
                }
            }
            Some(out.trim().to_string())
        }
        _ => None,
    }
}

/// The entries of one `::vars` element, each read, or the reason it cannot be.
///
/// The args form (`::vars(a: {...})`) and the data form (`::vars{ a: {...} }`)
/// are both read, as for `::macros`.
pub fn read_var_decls(el: &Element) -> Vec<(String, Result<VarDecl, String>)> {
    let args = match normalized_element_args(el) {
        Some(Value::Map(entries)) => entries,
        _ => Vec::new(),
    };
    let data = match crate::data::element_data(el) {
        Some(Value::Map(entries)) => entries,
        _ => Vec::new(),
    };
    args.iter()
        .chain(data.iter())
        .map(|(name, value)| (name.clone(), VarDecl::from_value(value)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_parser::parse_document;

    fn decls(src: &str) -> Vec<(String, Result<VarDecl, String>)> {
        let doc = parse_document(src).unwrap();
        let mut found = Vec::new();
        tomet_tree::for_each_top_level_element(&doc, |el| {
            if crate::classify_std_lenient(el) == crate::ElementKind::Vars {
                found.extend(read_var_decls(el));
            }
        });
        found
    }

    #[test]
    fn a_declaration_has_a_type_a_default_and_an_about() {
        let found = decls(
            "::vars{\n  title: {\n    type: string\n    about: | 題。\n  }\n  n: { type: int, default: 3 }\n}\n",
        );
        let title = found[0].1.as_ref().unwrap();
        assert_eq!(title.type_name(), "string");
        assert!(title.is_required());
        assert_eq!(title.about.as_deref(), Some("題。"));
        let n = found[1].1.as_ref().unwrap();
        assert_eq!(n.type_name(), "int");
        assert_eq!(n.default, Some(Value::Int(3)));
        assert!(!n.is_required());
    }

    #[test]
    fn the_type_is_a_string_when_it_is_left_out() {
        let found = decls("::vars{\n  who: { default: \"me\" }\n}\n");
        assert_eq!(found[0].1.as_ref().unwrap().type_name(), "string");
    }

    #[test]
    fn a_value_written_directly_is_not_a_declaration() {
        let found = decls("::vars{\n  title: \"x\"\n}\n");
        assert!(found[0].1.is_err());
    }

    #[test]
    fn an_unknown_key_is_refused_with_the_keys_there_are() {
        let found = decls("::vars{\n  title: { kind: string }\n}\n");
        let reason = found[0].1.as_ref().unwrap_err();
        assert!(
            reason.contains("`kind`") && reason.contains("`default`"),
            "{reason}"
        );
    }
}
