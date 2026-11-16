use tomet_ast::{File, Tuple, Value};
use tomet_semantics_core::TupleView;
use tomet_span::Span;

use crate::kind::DeclKind;

/// A declaration: its name, the `(args)` it was written with, the
/// `[prose]` after them, and its `{data}`.
///
/// Owned, because a tomet declaration's data is built from its group, not stored. A `.tmt`
/// document gives one through [`Declaration::from_ast`]; a `.ctmt` file's header is the same
/// things. A standard declaration is written on its `(args)`; `{data}` is the body of
/// `::blueprint` alone (see [`Shape`](crate::Shape)).
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// The name of the declaration, after `::`.
    pub name: String,
    /// `(args)` as written: a positional argument is still unnamed
    /// ([`args_view`](Declaration::args_view) views it with its aliases).
    pub args: Option<Tuple>,
    /// `[prose]` as written, a [`Value::Blocks`].
    pub prose: Option<Value>,
    /// `{data}`, the group read as a tuple. Only a declaration of shape
    /// [`Shape::ArgsAndData`](crate::Shape::ArgsAndData) has one.
    pub data: Option<Tuple>,
}

/// Why a declaration's `(args)` and `[prose]` do not make entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntriesError {
    /// A value without a name where `name: value` entries are: `::macro(gh)`, `::meta(2, a: 1)`.
    ValueWithoutName,
    /// A `[prose]` where there is nothing it could be the prose of: on a declaration head
    /// (`::field[ x ]`).
    UnexpectedProse,
    /// A `[prose]` along with a value written some other way.
    ProseAndValue,
}

impl Declaration {
    /// A declaration built from its parts.
    pub fn new(name: impl AsRef<str>, args: Option<Tuple>, data: Option<Tuple>) -> Declaration {
        Declaration::with_prose(name, args, None, data)
    }

    /// [`new`](Declaration::new), with the `[prose]` it was written with.
    pub fn with_prose(
        name: impl AsRef<str>,
        args: Option<Tuple>,
        prose: Option<Value>,
        data: Option<Tuple>,
    ) -> Declaration {
        Declaration {
            name: name.as_ref().to_string(),
            args,
            prose,
            data,
        }
    }

    /// The declaration a written `::name` is. Written `@name`, an element is not a declaration,
    /// whatever the name -- the validator says so, and nothing reads it as one.
    ///
    /// Whether the name is a standard declaration is [`kind`](Declaration::kind).
    pub fn from_ast(declaration: &tomet_ast::Declaration) -> Option<Declaration> {
        Some(Declaration::with_prose(
            declaration.name.to_string(),
            declaration.data.args.clone(),
            declaration
                .content
                .as_ref()
                .map(|blocks| Value::Blocks(blocks.clone())),
            declaration.data.value.as_ref().map(|value| value.as_data()),
        ))
    }

    /// The standard declaration this is, when its name is one.
    pub fn kind(&self) -> Option<DeclKind> {
        DeclKind::from_name(&self.name)
    }

    /// Whether this is the header a document names itself with, as against an entry of a registry:
    /// `::blueprint(daily)` and `::dictionary(name: tomet)` are, `::blueprint(daily: @file(...))` is not.
    /// Only a declaration with a header of its own ([`Spec::own_header`](crate::Spec::own_header)) can be one.
    pub fn is_own_header(&self) -> bool {
        let Some(kind) = self.kind() else {
            return false;
        };
        if !kind.spec().own_header {
            return false;
        }
        match &self.args {
            Some(tuple) => tuple.entries().any(|entry| match entry.name() {
                None => true,
                Some(name) => kind.positional_keys().contains(&name),
            }),
            None => true,
        }
    }

    /// A view over `(args)` and the declaration's signature: slot names alias the unnamed entries,
    /// without rewriting the tuple.
    pub fn args_view(&self) -> Option<TupleView<'_, '_>> {
        let args = self.args.as_ref()?;
        let keys = self.kind().map_or(&[][..], DeclKind::positional_keys);
        Some(TupleView::new(args, keys))
    }

    /// What this declaration says, as `(name, value)` entries of its declaration.
    ///
    /// Named arguments are the entries: `::field(author: (type: string))` has entry `("author", ...)`.
    /// `::name()` has no entries, and is not an error.
    pub fn entries(&self) -> Result<Vec<(String, Value)>, EntriesError> {
        if self.prose.is_some() {
            return Err(EntriesError::UnexpectedProse);
        }
        match &self.args {
            None => Ok(Vec::new()),
            Some(tuple) if tuple.has_no_unnamed() => Ok(tuple
                .named()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect()),
            Some(_) => Err(EntriesError::ValueWithoutName),
        }
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

/// The standard declarations at the top level of `file`, in the order written.
pub fn declarations<B>(file: &File<B>) -> Vec<Found> {
    declarations_from_header(&file.header)
}

/// The standard declarations from a file header, in the order written.
pub fn declarations_from_header(header: &[tomet_ast::Declaration]) -> Vec<Found> {
    let mut found = Vec::new();
    for written in header {
        if let Some(declaration) = Declaration::from_ast(written)
            && let Some(kind) = declaration.kind()
        {
            found.push(Found {
                kind,
                declaration,
                span: written.span,
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> File {
        tomet_ast_parser::parse_file(src).expect("parses")
    }

    #[test]
    fn a_double_colon_element_is_a_declaration() {
        let doc = parse("::kind(config, 2)\n::meta(id: x)\n\nBody.\n");
        let found = declarations(&doc);
        assert_eq!(
            found.iter().map(|f| f.kind).collect::<Vec<_>>(),
            [DeclKind::Kind, DeclKind::Meta]
        );
        let kind = &found[0].declaration;
        assert_eq!(kind.name, "kind");
        let view0 = kind.args_view().unwrap();
        assert_eq!(view0.get("kind"), Some(&Value::String("config".into())));
        assert_eq!(view0.get("0"), Some(&Value::String("config".into())));
        assert_eq!(view0.get("version"), Some(&Value::Int(2)));
        assert_eq!(view0.get("1"), Some(&Value::Int(2)));
        assert_eq!(
            kind.args.as_ref(),
            Some(&Tuple::from_unnamed([
                Value::String("config".into()),
                Value::Int(2),
            ]))
        );
        let view1 = found[1].declaration.args_view().unwrap();
        assert_eq!(view1.get("id"), Some(&Value::String("x".into())));
        assert_eq!(view1.get("0"), None);
        assert_eq!(found[1].declaration.data, None);
    }

    #[test]
    fn an_at_element_is_not_a_declaration_whatever_its_name() {
        let doc = parse("@meta(id: x)\n");
        assert!(declarations(&doc).is_empty());
    }

    #[test]
    fn a_name_that_is_not_standard_is_not_found() {
        // a user vocabulary's preamble element is the vocabulary's, not this table's
        assert!(declarations(&parse("::character(birth: 1990)\n")).is_empty());
    }

    fn entries_of(src: &str) -> Vec<(String, Value)> {
        let doc = parse(src);
        let mut all = Vec::new();
        for found in declarations(&doc) {
            all.extend(found.declaration.entries().expect("entries"));
        }
        all
    }

    fn text(s: &str) -> Value {
        Value::String(s.into())
    }

    #[test]
    fn a_dotted_head_in_source_is_an_error() {
        assert!(tomet_ast_parser::parse_file("::field.url.wiki(type: list)\n").is_err());
        assert!(tomet_ast_parser::parse_file("::macro.copyright(\"(C)\")\n").is_err());
        assert!(tomet_ast_parser::parse_file("::workspace.ignore(\"target\")\n").is_err());
    }

    #[test]
    fn the_grouped_form_produces_entries() {
        let grouped = entries_of("::macro(copyright: \"(C)\")\n");
        assert_eq!(grouped, [("copyright".to_string(), text("(C)"))]);

        let grouped = entries_of("::field(author: (type: string, required: true))\n");
        assert_eq!(grouped[0].0, "author");

        assert_eq!(
            entries_of("::field(url.wiki: (type: list))\n")[0].0,
            "url.wiki"
        );
    }

    #[test]
    fn several_positional_arguments_are_a_sequence() {
        assert_eq!(
            entries_of("::workspace(ignore: (\"target\", \".git\"))\n"),
            [(
                "ignore".to_string(),
                Value::Tuple(Tuple::from_unnamed([text("target"), text(".git")]))
            )]
        );
    }

    #[test]
    fn the_prose_of_a_macro_is_the_macro() {
        let entries = entries_of("::macro(card: [ @card[ $(1) ] ])\n");
        assert_eq!(entries[0].0, "card");
        assert!(matches!(entries[0].1, Value::Blocks(_)));
    }

    #[test]
    fn a_declaration_that_joined_a_paragraph_is_still_found() {
        let doc = parse("::kind(a)\n::use(deck)\n");
        assert_eq!(declarations(&doc).len(), 2);
    }
}
