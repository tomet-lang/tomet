use tomet_ast::{File, Value};
use tomet_span::Span;

use crate::kind::{DeclKind, Prose};

/// A declaration: its name, the path after the name, the `(args)` it was written with, the
/// `[prose]` after them, and its `{data}`.
///
/// Owned, because a tomet declaration's data is built from its group, not stored. A `.tmt`
/// document gives one through [`Declaration::from_ast`]; a `.ctmt` file's header is the same
/// things. A standard declaration is written on its `(args)`; `{data}` is the body of
/// `::blueprint` alone (see [`Shape`](crate::Shape)).
///
/// The head is a path into the document's tree: `::field.url.wiki(...)` is `name: "field"` and
/// `path: ["url", "wiki"]`. What the declaration says about that place is read through
/// [`entries`](Declaration::entries), which gives the dotted head and the grouped
/// `::field(url.wiki: ...)` the same answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// The first segment of the head, after `::`: the declaration this is.
    pub name: String,
    /// The segments after it, `["url", "wiki"]` for `::field.url.wiki`. Empty for `::field`.
    pub path: Vec<String>,
    /// `(args)` as written: a positional argument is still unnamed
    /// ([`bound_args`](Declaration::bound_args) names it).
    pub args: Option<Value>,
    /// `[prose]` as written, a [`Value::Blocks`].
    pub prose: Option<Value>,
    /// `{data}`, the group read as a map. Only a declaration of shape
    /// [`Shape::ArgsAndData`](crate::Shape::ArgsAndData) has one.
    pub data: Option<Value>,
}

/// Why a declaration's head, `(args)` and `[prose]` do not make entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntriesError {
    /// A value without a name where `name: value` entries are: `::macro(gh)`, `::meta(a: 1, 2)`.
    /// Written with a path it would be a value: `::macro.gh("...")`.
    ValueWithoutName,
    /// A `[prose]` where there is nothing it could be the prose of: on a declaration with no path
    /// (`::field[ x ]`), or on one that has no use for it (`::format.table()[ x ]`).
    UnexpectedProse,
    /// A `[prose]` on a declaration whose value it is, along with a value written some other way:
    /// `::macro.card("x")[ y ]`.
    ProseAndValue,
}

impl Declaration {
    /// A declaration built from its parts, `name` being the head as written (`"field.url.wiki"`).
    pub fn new(name: impl AsRef<str>, args: Option<Value>, data: Option<Value>) -> Declaration {
        Declaration::with_prose(name, args, None, data)
    }

    /// [`new`](Declaration::new), with the `[prose]` it was written with.
    pub fn with_prose(
        name: impl AsRef<str>,
        args: Option<Value>,
        prose: Option<Value>,
        data: Option<Value>,
    ) -> Declaration {
        let mut segments = name.as_ref().split('.').map(str::to_string);
        Declaration {
            name: segments.next().unwrap_or_default(),
            path: segments.collect(),
            args,
            prose,
            data,
        }
    }

    /// The declaration a written `::name` is. Written `@name`, an element is not a declaration,
    /// whatever the name -- the validator says so, and nothing reads it as one.
    ///
    /// `::field.author` and `::deck.card` are both read here, as `field` with a path and as `deck`
    /// with a path; whether the first segment is a standard declaration is [`kind`](Declaration::kind).
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
    /// `::blueprint(daily)` and `::dictionary(name: tomet)` are, `::blueprint.daily(@file(...))` and
    /// `::blueprint(daily: @file(...))` are not. Only a declaration with a header of its own
    /// ([`Spec::own_header`](crate::Spec::own_header)) can be one.
    pub fn is_own_header(&self) -> bool {
        let Some(kind) = self.kind() else {
            return false;
        };
        if !kind.spec().own_header || !self.path.is_empty() {
            return false;
        }
        match &self.args {
            Some(Value::Map(entries)) => entries.iter().any(|(key, _)| {
                key == tomet_semantics_core::POSITIONAL_ENTRY_KEY
                    || kind.positional_keys().contains(&key.as_str())
            }),
            _ => true,
        }
    }

    /// The head as written, `field.url.wiki`.
    pub fn head(&self) -> String {
        std::iter::once(self.name.as_str())
            .chain(self.path.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(".")
    }

    /// `(args)` with the positional ones named after the declaration's signature
    /// (`::kind(config, 2)` is `kind: config, version: 2`). For a name that is not a standard
    /// declaration, the arguments as they were written.
    pub fn bound_args(&self) -> Option<Value> {
        let args = self.args.as_ref()?;
        let keys = self.kind().map_or(&[][..], DeclKind::positional_keys);
        Some(tomet_semantics_core::bind_positional(args, keys))
    }

    /// What this declaration says, as `(name, value)` entries of its declaration: the one answer
    /// whichever way it was written.
    ///
    /// - **Grouped**, no path: `::field(author: { type: string }, created: { type: datetime })` is two
    ///   entries, named by the arguments.
    /// - **Dotted**, a path: `::field.author(type: string)` is one entry, named by the path
    ///   (`author`, or `url.wiki` for `::field.url.wiki`), whose value is what the parentheses hold:
    ///   named arguments are the fields of a map. This is the grouped entry
    ///   `author: { type: string }`, with the same value.
    ///
    ///   Positional arguments are the value itself, the entry's own slot (`X.X` is `X`):
    ///   `::macro.copyright("(C)")` is `copyright: "(C)"`, and several are a sequence, so
    ///   `::workspace.ignore("target", ".git")` is `ignore: ("target", ".git")`. Beside named
    ///   arguments they are the field named like the entry, as `::kind(config, 2)` names its
    ///   first `kind`: `::x.y("a", b: 1)` is `y: (y: "a", b: 1)`.
    /// - **Prose**: a trailing `[...]` on a dotted head is the entry's `about` for a `field`, a `var`
    ///   or an `element` (`::field.author(type: string)[ the author ]`), and the whole value of a
    ///   `macro` (`::macro.card[ @card[ ${1} ] ]`). See [`DeclKind::prose`].
    ///
    /// `::name()` has no entries, and is not an error.
    pub fn entries(&self) -> Result<Vec<(String, Value)>, EntriesError> {
        let Some(leaf) = self.path.last() else {
            if self.prose.is_some() {
                return Err(EntriesError::UnexpectedProse);
            }
            return match &self.args {
                None => Ok(Vec::new()),
                Some(Value::Map(entries))
                    if entries
                        .iter()
                        .all(|(key, _)| key != tomet_semantics_core::POSITIONAL_ENTRY_KEY) =>
                {
                    Ok(entries.clone())
                }
                Some(_) => Err(EntriesError::ValueWithoutName),
            };
        };

        let (positional, named): (Vec<Value>, Vec<(String, Value)>) = match &self.args {
            None => (Vec::new(), Vec::new()),
            Some(Value::Map(entries)) => {
                let (positional, named): (Vec<_>, Vec<_>) = entries
                    .iter()
                    .cloned()
                    .partition(|(key, _)| key == tomet_semantics_core::POSITIONAL_ENTRY_KEY);
                (positional.into_iter().map(|(_, v)| v).collect(), named)
            }
            Some(one) => (vec![one.clone()], Vec::new()),
        };
        let positional = match positional.len() {
            0 => None,
            1 => positional.into_iter().next(),
            _ => Some(Value::Seq(positional)),
        };

        let prose = self.kind().map_or(Prose::None, DeclKind::prose);
        let value = match (prose, &self.prose) {
            (_, None) => entry_value(leaf, positional, named, None),
            (Prose::About, Some(blocks)) => {
                entry_value(leaf, positional, named, Some(("about", blocks.clone())))
            }
            (Prose::Value, Some(blocks)) => {
                if positional.is_some() || !named.is_empty() {
                    return Err(EntriesError::ProseAndValue);
                }
                blocks.clone()
            }
            (Prose::None, Some(_)) => return Err(EntriesError::UnexpectedProse),
        };
        Ok(vec![(self.path.join("."), value)])
    }
}

/// The value of the entry named `leaf`: what was written positionally, the named arguments, and the
/// extra field the prose sugar adds.
fn entry_value(
    leaf: &str,
    positional: Option<Value>,
    mut named: Vec<(String, Value)>,
    extra: Option<(&str, Value)>,
) -> Value {
    if let Some((key, value)) = extra {
        named.push((key.to_string(), value));
    }
    match (positional, named.is_empty()) {
        (Some(value), true) => value,
        (None, _) => Value::Map(named),
        (Some(value), false) => {
            named.insert(0, (leaf.to_string(), value));
            Value::Map(named)
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
        assert_eq!(
            kind.bound_args(),
            Some(Value::Map(vec![
                ("kind".into(), Value::String("config".into())),
                ("version".into(), Value::Int(2)),
            ]))
        );
        assert_eq!(
            found[1].declaration.bound_args(),
            Some(Value::Map(vec![("id".into(), Value::String("x".into()))]))
        );
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
    fn a_dotted_head_keeps_its_path_and_the_name_stays_the_declaration() {
        let doc = parse("::field.url.wiki(type: list)\n");
        let found = declarations(&doc);
        assert_eq!(found[0].kind, DeclKind::Field);
        assert_eq!(found[0].declaration.name, "field");
        assert_eq!(found[0].declaration.path, ["url", "wiki"]);
        assert_eq!(found[0].declaration.head(), "field.url.wiki");
    }

    #[test]
    fn the_dotted_head_and_the_grouped_form_are_the_same_entries() {
        let grouped = entries_of("::macro(copyright: \"(C)\")\n");
        assert_eq!(grouped, [("copyright".to_string(), text("(C)"))]);
        // `X.X` is `X`: the first positional argument is the entry itself.
        assert_eq!(entries_of("::macro.copyright(\"(C)\")\n"), grouped);

        let grouped = entries_of("::field(author: { type: string, required: true })\n");
        assert_eq!(
            entries_of("::field.author(type: string, required: true)\n"),
            grouped
        );
        // The path may be longer: the entry is named by all of it.
        assert_eq!(
            entries_of("::field.url.wiki(type: list)\n")[0].0,
            "url.wiki"
        );
    }

    #[test]
    fn several_positional_arguments_are_a_sequence() {
        assert_eq!(
            entries_of("::workspace.ignore(\"target\", \".git\")\n"),
            entries_of("::workspace(ignore: (\"target\", \".git\"))\n")
        );
        assert_eq!(
            entries_of("::workspace.ignore(\"target\", \".git\")\n"),
            [(
                "ignore".to_string(),
                Value::Seq(vec![text("target"), text(".git")])
            )]
        );
    }

    #[test]
    fn a_positional_argument_beside_named_ones_is_the_field_of_the_entrys_name() {
        assert_eq!(
            entries_of("::format.table(\"x\", max_col_width: 50)\n"),
            [(
                "table".to_string(),
                Value::Map(vec![
                    ("table".into(), text("x")),
                    ("max_col_width".into(), Value::Int(50))
                ])
            )]
        );
    }

    #[test]
    fn a_prose_after_the_head_is_the_about_of_a_field_a_var_and_an_element() {
        for (name, src) in [
            ("author", "::field.author(type: string)[ the author ]\n"),
            ("draft", "::var.draft(type: bool)[ the author ]\n"),
            ("card", "::element.card(display: block)[ the author ]\n"),
        ] {
            let entries = entries_of(src);
            assert_eq!(entries[0].0, name);
            let Value::Map(fields) = &entries[0].1 else {
                panic!("{src}")
            };
            assert!(
                matches!(fields.last(), Some((key, Value::Blocks(_))) if key == "about"),
                "{src}: {fields:?}"
            );
        }
        // And without `()` entries, it is all there is.
        let entries = entries_of("::element.card[ a card ]\n");
        assert!(matches!(&entries[0].1, Value::Map(f) if f.len() == 1 && f[0].0 == "about"));
    }

    #[test]
    fn the_prose_of_a_macro_is_the_macro() {
        let entries = entries_of("::macro.card[ @card[ ${1} ] ]\n");
        assert_eq!(entries[0].0, "card");
        assert!(matches!(entries[0].1, Value::Blocks(_)));
        assert_eq!(
            entries_of("::macro.card[ @card[ ${1} ] ]\n"),
            entries_of("::macro(card: [ @card[ ${1} ] ])\n")
        );
    }

    #[test]
    fn a_declaration_that_joined_a_paragraph_is_still_found() {
        let doc = parse("::kind(a)\n::use(deck)\n");
        assert_eq!(declarations(&doc).len(), 2);
    }
}
