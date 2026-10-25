//! A declaration as the readers see it, whichever document it was written in.

use tomet_ast::{Document, Element, Sigil, Span, Value};

use crate::kind::DeclKind;

/// A declaration: a name, the `(args)` it was written with, and its `{data}`.
///
/// Owned, because a tomet element's data is built from its group, not stored. A `.tmt` document
/// gives one through [`Declaration::from_element`]; a `.ctmt` file's header is the same three
/// things.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// The name after `::`.
    pub name: String,
    /// `(args)` as written: a positional argument is still unnamed
    /// ([`bound_args`](Declaration::bound_args) names it).
    pub args: Option<Value>,
    /// `{data}`, the group read as a map.
    pub data: Option<Value>,
}

impl Declaration {
    pub fn new(name: impl Into<String>, args: Option<Value>, data: Option<Value>) -> Declaration {
        Declaration {
            name: name.into(),
            args,
            data,
        }
    }

    /// The declaration an element is: one written `::name` with a bare name. Written `@name`, an
    /// element is not a declaration, whatever the name -- the validator says so, and nothing
    /// reads it as one.
    pub fn from_element(el: &Element) -> Option<Declaration> {
        let Sigil::Document(name) = &el.sigil else {
            return None;
        };
        if !name.is_bare() {
            return None;
        }
        Some(Declaration {
            name: name.name.clone(),
            args: el.args.clone(),
            data: el.value.as_ref().and_then(|value| value.as_data()),
        })
    }

    /// The standard declaration this is, when its name is one.
    pub fn kind(&self) -> Option<DeclKind> {
        DeclKind::from_name(&self.name)
    }

    /// `(args)` with the positional ones named after the declaration's signature
    /// (`::kind(config, 2)` is `kind: config, version: 2`). For a name that is not a standard
    /// declaration, the arguments as they were written.
    pub fn bound_args(&self) -> Option<Value> {
        let args = self.args.as_ref()?;
        let keys = self.kind().map_or(&[][..], DeclKind::positional_keys);
        Some(cotomet_syntax::bind_positional(args, keys))
    }
}

/// A standard declaration found at the top of a document.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub kind: DeclKind,
    pub declaration: Declaration,
    /// Where it is written.
    pub span: Span,
}

/// The standard declarations at the top level of `doc`, in the order written.
///
/// Top level is [`tomet_tree::for_each_top_level_element`]: a declaration that joined an
/// adjacent paragraph still counts, which is a choice of tree shape and not of meaning.
pub fn declarations(doc: &Document) -> Vec<Found> {
    let mut found = Vec::new();
    tomet_tree::for_each_top_level_element(doc, |el| {
        if let Some(declaration) = Declaration::from_element(el)
            && let Some(kind) = declaration.kind()
        {
            found.push(Found {
                kind,
                declaration,
                span: el.span,
            });
        }
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Document {
        tomet_parser::parse_document(src).expect("parses")
    }

    #[test]
    fn a_double_colon_element_is_a_declaration() {
        let doc = parse("::kind(config, 2)\n::meta{ id: x }\n\nBody.\n");
        let found = declarations(&doc);
        assert_eq!(
            found.iter().map(|f| f.kind).collect::<Vec<_>>(),
            [DeclKind::Kind, DeclKind::Meta]
        );
        let kind = &found[0].declaration;
        assert_eq!(kind.name, "kind");
        assert_eq!(
            kind.bound_args(),
            Some(Value::Map(vec![
                ("kind".into(), Value::String("config".into())),
                ("version".into(), Value::Int(2)),
            ]))
        );
        assert_eq!(
            found[1].declaration.data,
            Some(Value::Map(vec![("id".into(), Value::String("x".into()))]))
        );
    }

    #[test]
    fn an_at_element_is_not_a_declaration_whatever_its_name() {
        let doc = parse("@meta{ id: x }\n");
        assert!(declarations(&doc).is_empty());
    }

    #[test]
    fn a_name_that_is_not_standard_is_not_found() {
        // a user vocabulary's preamble element is the vocabulary's, not this table's
        assert!(declarations(&parse("::character(birth: 1990)\n")).is_empty());
    }

    #[test]
    fn a_declaration_that_joined_a_paragraph_is_still_found() {
        let doc = parse("::kind(a)\n::use(deck)\n");
        assert_eq!(declarations(&doc).len(), 2);
    }
}
