use serde::{Deserialize, Serialize};
use std::fmt;
use tomet_ast::Span;
use tomet_cst::TextRange;
use tomet_lexer::LineIndex;

/// How much a [`Diagnostic`] matters.
///
/// Derived from the variant rather than stored: severity is a property of
/// the rule, not of the occurrence, so keeping a field would be a second
/// place the same fact lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// The document is wrong. `tomet check` fails.
    Error,
    /// The document says something about itself that a reader should see,
    /// and is otherwise fine. `tomet check` reports it and still passes.
    Warning,
}

/// Something `validate_document` found and a reader should know about.
///
/// Named for what the callers already called it: `bindings/{js,java,
/// python}` each bind the result to `diagnostics`. It was
/// `ValidationError` while every variant really was an error; `Draft` and
/// `Fixme` are not, so the name moved rather than the meaning stretching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Diagnostic {
    /// The same `id` value appears on more than one node. `first` is where
    /// it was first seen; `duplicate` is the later, offending occurrence.
    DuplicateId {
        id: String,
        first: Span,
        duplicate: Span,
    },
    /// An intra-document anchor reference points to an anchor ID that does not exist in the document.
    DanglingAnchor { anchor: String, span: Span },
    /// A required `::meta` field defined in the blueprint is missing from the document.
    MissingRequiredMetaKey {
        key: String,
        kind: String,
        span: Span,
    },
    /// A required section/heading defined in the blueprint is missing from the document.
    MissingRequiredSection {
        title: String,
        id: Option<String>,
        kind: String,
        span: Span,
    },
    /// An element written with a bare name that is not one of Tomet's own.
    ///
    /// Bare names are reserved for the built-in vocabulary; a user-defined
    /// element must be namespaced. This is the diagnostic that replaces the
    /// old silent fall-back to `ElementKind::Custom`, which is where the
    /// official-vs-user-defined ambiguity actually lived.
    UnknownElement {
        name: String,
        /// Set when the name is namespaced and that namespace is not in
        /// scope -- a missing `::use` rather than a misspelled element.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unbound_namespace: Option<String>,
        span: Span,
    },
    /// A second copy of an element that may appear at most once.
    ///
    /// `singleton` was declared in `docs/spec/builtin-settings.tmt` for
    /// six elements and enforced nowhere, so a document with two `::meta`
    /// -- or two `::kind`, which is two answers to what the document is --
    /// passed every check this project had.
    DuplicateSingleton {
        name: String,
        first: Span,
        duplicate: Span,
    },
    /// An element that may only sit in the preamble, found after the
    /// document's body has started.
    ///
    /// The preamble is the run of such elements at the top of the file;
    /// the first block that is not one of them ends it. A `::kind` below
    /// that point is a directive nobody reading top-down would meet in
    /// time.
    OutsidePreamble { name: String, span: Span },
    /// An element written with the sigil of the other kind: a declaration
    /// about the document written `@name` (it is `::name`), or content written
    /// `::name` (it is `@name`).
    ///
    /// The sigil says whether something describes the document, and which an
    /// element is follows from the `Region` its vocabulary declares.
    WrongSigil {
        name: String,
        written: &'static str,
        expected: &'static str,
        span: Span,
    },
    /// An element in the wrong position for its shape -- a block element
    /// inside running text, or an inline one standing alone as a block.
    ShapeMismatch {
        name: String,
        found: &'static str,
        expected: &'static str,
        span: Span,
    },
    /// An argument the element's vocabulary entry does not declare.
    ///
    /// Only reported for an element whose declaration has `args:` at all.
    /// No `args:` means "nothing said", not "takes none" -- saying the
    /// second is `@data`'s `open: false`, and there is no `args:`
    /// equivalent yet.
    UnknownArgument {
        element: String,
        argument: String,
        span: Span,
    },
    /// An argument that is not of the `type:` its parameter declares:
    /// `@card(id: abc)` where `id` is `type: uint`.
    ///
    /// A parameter whose `type:` is not a type is not checked, and the
    /// vocabulary says so where it is read.
    InvalidArgument {
        element: String,
        argument: String,
        ty: String,
        /// What is wrong, as `tomet-type` says it: `` `abc` is not a non-negative integer ``.
        reason: String,
        span: Span,
    },
    /// A parameter marked `required: true` with no argument to fill it.
    MissingRequiredArgument {
        element: String,
        argument: String,
        span: Span,
    },
    /// The version after the name in `::kind(name, 2)` or `::use(ns, 2)` is not an
    /// integer of 1 or more. `found` is the value as written.
    InvalidVersion {
        element: String,
        found: String,
        span: Span,
    },
    /// A built-in element that takes a fixed number of positional arguments was
    /// given more: `::kind(a, 1, 2)`.
    TooManyArguments {
        element: String,
        takes: usize,
        span: Span,
    },
    /// `@draft[ ... ]` -- the document says it is unfinished here.
    ///
    /// A warning, not an error: an unfinished document is a normal state
    /// to be in and to commit, and the whole point of writing the marker
    /// is that a reader (and `tomet check`) can find it. Failing the run
    /// would make the honest thing the expensive one.
    ///
    /// This is why it is an element and not `//(TODO)`: `tomet_ast` has no
    /// comment node, so a comment marker cannot be counted here at all,
    /// and is dropped by anything that re-prints from the tree.
    Draft { note: String, span: Span },
    /// `@fixme[ ... ]` -- there is text here and it needs revisiting.
    Fixme { note: String, span: Span },
    /// `@conflict(a: ..., b: ...)` -- an unresolved merge conflict is
    /// still in the document.
    ///
    /// A warning, same reasoning as `Draft`/`Fixme`: a document can be
    /// committed and shared with an unresolved conflict still in it (a
    /// reviewer may need to see both candidates to resolve it), so
    /// failing the run outright would punish leaving the marker for
    /// someone else to find rather than resolving it under pressure
    /// just to make `tomet check` pass.
    Conflict { span: Span },
    /// A `:name(...)` connect whose name is not in the closed set
    /// `tomet_semantics::CONNECT_MEMBERS` declares (`rule` is the only
    /// member so far). Distinct from [`Diagnostic::UnknownElement`]: a
    /// connect is never fed through vocabulary resolution at all (see
    /// `tomet_tree::for_each_descendant`'s doc comment), so this is its
    /// own diagnostic rather than a special case of that one.
    UnknownConnect { name: String, span: Span },
    /// A descendant found inside an element carrying an active
    /// `:rule(allow:list(...))` whose classified name is not in that
    /// rule's `allow` list. `span` is the offending descendant's own
    /// span; `rule_span` is where the `:rule(...)` itself was written,
    /// so a reader can see both "found here" and "restricted here".
    DisallowedByRule {
        name: String,
        allowed: Vec<String>,
        rule_span: Span,
        span: Span,
    },
    /// `[content]`/`|content` holds more than a single plain paragraph
    /// (multiple paragraphs, a nested heading/list/element, ...) where
    /// `name`'s content-shape -- built in (`tomet_semantics::
    /// builtin_content_shape`), or a vocabulary's own `content: { allow:
    /// inline }` -- only permits one.
    ContentNotInline { name: String, span: Span },
    /// A descendant found inside `[content]`/`|content` whose classified
    /// name is not in the `content: { allow: list(...) }` that `element`'s
    /// vocabulary declares. `span` is the offending descendant's own
    /// span; `element_span` is the enclosing element's, so a reader can
    /// see both "found here" and "restricted here" -- same split
    /// [`Diagnostic::DisallowedByRule`] makes for `:rule(...)`.
    DisallowedInContent {
        name: String,
        element: String,
        allowed: Vec<String>,
        element_span: Span,
        span: Span,
    },
    /// A top-level `::settings`/`::config` key that has been retired.
    ///
    /// `elements:` described what a custom element takes -- its `args`,
    /// `required`, `positional`, `content`, `placement` and `singleton`.
    /// A `::vocabulary` document says all of that now, at the declaration
    /// rather than in a settings map beside it, so the settings copy was
    /// the second place one fact lived.
    ///
    /// It is reported rather than ignored because ignoring is how it got
    /// here: of the six keys only `positional` was ever read, by a
    /// `SettingsSchema` no caller ever built, so a document could declare
    /// `singleton: true` and be obeyed by nothing at all.
    RetiredSettingsKey { key: String, span: Span },
    /// A setting only the vault config may make, written in a folder config or
    /// in a document's own `::config` -- where it has no effect.
    ///
    /// Reported rather than ignored quietly, for the reason
    /// [`Diagnostic::RetiredSettingsKey`] is: a setting that does nothing and says
    /// nothing is how a reader comes to believe it is obeyed. `span` is the
    /// `::config` element; an entry of a `{...}` group has no span of its own.
    VaultOnlySetting { key: String, span: Span },
    /// `id` written in `::meta{ ... }`: a document's identity is the declaration `::id(...)`.
    ///
    /// `::meta` keeps the free attributes of a document; what tomet itself reads about it does
    /// not live there. `span` is the `::meta` element.
    MetaIdMoved { span: Span },
    /// An entry of `::macros` or `::vars` that cannot be used as written: a macro
    /// whose template is not a string, or a variable whose declaration is not a
    /// map, has an unknown key or type, or has a `default` its `type` does not hold.
    ///
    /// `span` is the whole declaration: an entry of a `{...}` group has no span
    /// of its own, so the entry is named instead of pointed at.
    InvalidDefinition {
        declaration: &'static str,
        name: String,
        reason: String,
        span: Span,
    },
    /// A `::meta` field the settings declare with a `type:` that has a form,
    /// holding something that does not have it -- a `type: uuid` field that
    /// is not a UUID.
    ///
    /// `span` is the `::meta` element: a value in a `{...}` group has no span
    /// of its own, so the field is named instead of pointed at.
    InvalidFieldValue {
        field: String,
        field_type: String,
        value: String,
        span: Span,
    },
    /// A declaration written in a host where it is not permitted.
    NotForThisHost {
        declaration: &'static str,
        span: Span,
    },
}

impl Diagnostic {
    /// Returns the primary span where the validation violation occurred.
    pub fn span(&self) -> Span {
        match self {
            Diagnostic::DuplicateId { duplicate, .. } => *duplicate,
            Diagnostic::DanglingAnchor { span, .. } => *span,
            Diagnostic::MissingRequiredMetaKey { span, .. } => *span,
            Diagnostic::MissingRequiredSection { span, .. } => *span,
            Diagnostic::UnknownElement { span, .. } => *span,
            Diagnostic::UnknownConnect { span, .. } => *span,
            Diagnostic::DisallowedByRule { span, .. } => *span,
            Diagnostic::ShapeMismatch { span, .. } => *span,
            Diagnostic::DuplicateSingleton { duplicate, .. } => *duplicate,
            Diagnostic::OutsidePreamble { span, .. } => *span,
            Diagnostic::WrongSigil { span, .. } => *span,
            Diagnostic::RetiredSettingsKey { span, .. } => *span,
            Diagnostic::VaultOnlySetting { span, .. } => *span,
            Diagnostic::MetaIdMoved { span } => *span,
            Diagnostic::InvalidDefinition { span, .. } => *span,
            Diagnostic::InvalidFieldValue { span, .. } => *span,
            Diagnostic::UnknownArgument { span, .. } => *span,
            Diagnostic::InvalidArgument { span, .. } => *span,
            Diagnostic::MissingRequiredArgument { span, .. } => *span,
            Diagnostic::InvalidVersion { span, .. } => *span,
            Diagnostic::TooManyArguments { span, .. } => *span,
            Diagnostic::Draft { span, .. } => *span,
            Diagnostic::Fixme { span, .. } => *span,
            Diagnostic::Conflict { span, .. } => *span,
            Diagnostic::ContentNotInline { span, .. } => *span,
            Diagnostic::DisallowedInContent { span, .. } => *span,
            Diagnostic::NotForThisHost { span, .. } => *span,
        }
    }

    /// How much this matters. See [`Severity`].
    pub fn severity(&self) -> Severity {
        match self {
            Diagnostic::Draft { .. } | Diagnostic::Fixme { .. } | Diagnostic::Conflict { .. } => {
                Severity::Warning
            }
            _ => Severity::Error,
        }
    }
}

/// `Display` has no source text, so it cannot say where a *related* span (the first
/// occurrence of a duplicate, the `:rule` that forbids something) is, and leaves
/// that clause out. A renderer that holds the source asks for the full message
/// with [`Diagnostic::message_in`].
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_message(f, None)
    }
}

impl Diagnostic {
    /// The message with related locations resolved against `index`, e.g.
    /// ``duplicate id `x` (first defined at 3:1)``.
    pub fn message_in(&self, index: &LineIndex<'_>) -> String {
        struct Located<'a, 'b>(&'a Diagnostic, &'a LineIndex<'b>);
        impl fmt::Display for Located<'_, '_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.write_message(f, Some(self.1))
            }
        }
        Located(self, index).to_string()
    }

    fn write_message(
        &self,
        f: &mut fmt::Formatter<'_>,
        index: Option<&LineIndex<'_>>,
    ) -> fmt::Result {
        let at = |span: &Span| {
            index.map(|i| {
                let p = i.line_col(span.start);
                format!("{}:{}", p.line, p.column)
            })
        };
        match self {
            Diagnostic::DuplicateId { id, first, .. } => {
                write!(f, "duplicate id `{id}`")?;
                match at(first) {
                    Some(loc) => write!(f, " (first defined at {loc})"),
                    None => Ok(()),
                }
            }
            Diagnostic::DanglingAnchor { anchor, .. } => {
                write!(f, "dangling anchor `#{anchor}`")
            }
            Diagnostic::MissingRequiredMetaKey { key, kind, .. } => {
                write!(
                    f,
                    "document of kind `{kind}` is missing required ::meta field `{key}`"
                )
            }
            Diagnostic::MissingRequiredSection {
                title, id, kind, ..
            } => {
                if let Some(sec_id) = id {
                    write!(
                        f,
                        "document of kind `{kind}` is missing required section `{title}` (id: {sec_id})"
                    )
                } else {
                    write!(
                        f,
                        "document of kind `{kind}` is missing required section `{title}`"
                    )
                }
            }
            Diagnostic::UnknownElement {
                name,
                unbound_namespace,
                ..
            } => {
                // One wording, in `tomet_semantics::UnknownName`. This arm
                // used to carry its own copy and the two drifted -- it was
                // still advising `@import(file:..., as:ns)` after that
                // element had split into `::use` and the `include` expression.
                write!(
                    f,
                    "{}",
                    tomet_semantics::UnknownName {
                        name: name.clone(),
                        unbound_namespace: unbound_namespace.clone(),
                        declaration: false,
                    }
                )
            }
            Diagnostic::UnknownConnect { name, .. } => {
                // One wording, in `tomet_semantics::UnknownConnectMember`,
                // for the same reason `UnknownElement` above delegates to
                // `UnknownName`: a second copy of the message here would
                // drift from the first change made to that one.
                write!(
                    f,
                    "{}",
                    tomet_semantics::UnknownConnectMember { name: name.clone() }
                )
            }
            Diagnostic::DisallowedByRule {
                name,
                allowed,
                rule_span,
                ..
            } => {
                write!(f, "`{name}` is not allowed here; the `:rule(allow:...)`")?;
                if let Some(loc) = at(rule_span) {
                    write!(f, " at {loc}")?;
                }
                write!(f, " permits only {}", allowed.join(", "))
            }
            Diagnostic::DuplicateSingleton { name, first, .. } => {
                write!(f, "`{name}` may appear once in a document")?;
                match at(first) {
                    Some(loc) => write!(f, "; the first is at {loc}"),
                    None => Ok(()),
                }
            }
            Diagnostic::OutsidePreamble { name, .. } => {
                write!(
                    f,
                    "`{name}` belongs in the preamble -- before the document's body \
                     starts, not after it"
                )
            }
            Diagnostic::WrongSigil {
                name,
                written,
                expected,
                ..
            } => {
                write!(
                    f,
                    "`{name}` is written `{written}{name}`, but it is content, not a \
                     declaration about the document: write `{expected}{name}`"
                )
            }
            Diagnostic::ShapeMismatch {
                name,
                found,
                expected,
                ..
            } => {
                // Not "use `#name`" any more. `#` stopped being the block
                // sigil when the shape axis was retracted, so the old
                // advice named an edit that cannot be made: `@` is the
                // only element sigil and shape comes from position.
                write!(
                    f,
                    "`{name}` is {expected}, but is written as {found}; \
                     shape comes from position -- {}",
                    if *expected == "a block element" {
                        "give it a line of its own, in block context"
                    } else {
                        "put it inside a paragraph, not alone on its line"
                    }
                )
            }
            Diagnostic::UnknownArgument {
                element, argument, ..
            } => {
                write!(
                    f,
                    "`{element}` has no argument `{argument}`; its vocabulary \
                     declares the ones it takes under `args:`"
                )
            }
            Diagnostic::InvalidArgument {
                element,
                argument,
                ty,
                reason,
                ..
            } => {
                write!(
                    f,
                    "`{element}`'s argument `{argument}` is declared `type: {ty}`, but {reason}"
                )
            }
            Diagnostic::MissingRequiredArgument {
                element, argument, ..
            } => {
                write!(f, "`{element}` requires the argument `{argument}`")
            }
            Diagnostic::InvalidVersion { element, found, .. } => {
                write!(
                    f,
                    "the version in `::{element}(...)` must be an integer of 1 or more, found {found}"
                )
            }
            Diagnostic::TooManyArguments { element, takes, .. } => {
                write!(
                    f,
                    "`{element}` takes at most {takes} arguments: the name, then an optional version"
                )
            }
            Diagnostic::Draft { note, .. } => {
                if note.is_empty() {
                    write!(f, "unfinished here")
                } else {
                    write!(f, "unfinished here: {note}")
                }
            }
            Diagnostic::Fixme { note, .. } => {
                if note.is_empty() {
                    write!(f, "marked to fix")
                } else {
                    write!(f, "marked to fix: {note}")
                }
            }
            Diagnostic::Conflict { .. } => {
                write!(
                    f,
                    "unresolved conflict -- pick `a` or `b` and remove the marker"
                )
            }
            Diagnostic::ContentNotInline { name, .. } => {
                write!(
                    f,
                    "`{name}`'s content can only be a single paragraph of plain text, \
                     not multiple blocks"
                )
            }
            Diagnostic::DisallowedInContent {
                name,
                element,
                allowed,
                ..
            } => {
                write!(
                    f,
                    "`{name}` is not allowed in `{element}`'s content; its vocabulary's \
                     `content: {{ allow: ... }}` permits only {}",
                    allowed.join(", ")
                )
            }
            Diagnostic::RetiredSettingsKey { key, .. } => {
                // Named per key rather than one generic sentence: `elements`
                // has somewhere to go and `types` does not, and telling a
                // reader to move something that has no destination is worse
                // than telling them it is gone.
                let advice = match key.as_str() {
                    "elements" => {
                        "declare elements in a `::vocabulary` document instead -- \
                         each entry of `::elements` carries `display`, `region` and \
                         `singleton`, and its `args` carry the arguments"
                    }
                    "macros" | "macro" => {
                        "write `::macros{ ... }` instead -- a macro is a definition \
                         of a name, not a setting"
                    }
                    "types" => {
                        "it has no replacement: nothing read it, and per-element \
                         rendering style has no home in settings yet -- \
                         `format.callout.style` and `format.list.multiline.style` \
                         are the only two that are read"
                    }
                    _ => "remove it",
                };
                write!(f, "`{key}` in `::settings` is retired; {advice}")
            }
            Diagnostic::MetaIdMoved { .. } => write!(
                f,
                "`id` is not written in `::meta`; the identity of a document is `::id(...)`"
            ),
            Diagnostic::VaultOnlySetting { key, .. } => write!(
                f,
                "`{key}` can only be set in the vault config; here it has no effect"
            ),
            Diagnostic::InvalidDefinition {
                declaration,
                name,
                reason,
                ..
            } => write!(f, "`{name}` in `::{declaration}`: {reason}"),
            Diagnostic::InvalidFieldValue {
                field,
                field_type,
                value,
                ..
            } => {
                write!(
                    f,
                    "`{field}` in `::meta` is declared `type: {field_type}`, \
                     but `{value}` is not a valid {field_type}"
                )
            }
            Diagnostic::NotForThisHost { declaration, .. } => {
                write!(f, "`::{declaration}` is not permitted in this host")
            }
        }
    }
}

impl std::error::Error for Diagnostic {}

/// A validation error tied directly to precise byte ranges in the CST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CstValidationError {
    DuplicateId {
        id: String,
        first_range: TextRange,
        duplicate_range: TextRange,
    },
}

impl CstValidationError {
    pub fn range(&self) -> TextRange {
        match self {
            CstValidationError::DuplicateId {
                duplicate_range, ..
            } => *duplicate_range,
        }
    }
}

impl fmt::Display for CstValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CstValidationError::DuplicateId {
                id, first_range, ..
            } => {
                write!(
                    f,
                    "duplicate id `{id}` (first defined at byte offset {})",
                    u32::from(first_range.start())
                )
            }
        }
    }
}

impl std::error::Error for CstValidationError {}
