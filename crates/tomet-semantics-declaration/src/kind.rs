//! The standard declarations, and what is fixed about each: how many may be written, where, and
//! what its positional arguments are called.

/// A standard declaration: what `::name` can be, for a name the language gives a meaning.
///
/// Closed on purpose. The standard declarations are the language's own, and a host matches on
/// this to do what only it knows (a `.tmt` binds a vocabulary for `Use`, a vault reads `Workspace`);
/// a user's own preamble element is a vocabulary's, and is not here.
///
/// Every name is singular. A declaration is a node of the document's tree: `::field` is the
/// `field` node, and the fields it holds are its children, written either in the head as a
/// dot-path (`::field.author(type: string)`) or grouped under the parent
/// (`::field(author: { type: string })`); both are the same tree. The plural spellings
/// (`::fields`, `::macros`, ...) are retired; see [`DeclKind::retired`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclKind {
    /// `::kind(name, 2)`: what kind of document this is, and the major version of that kind.
    Kind,
    /// `::id(std.config)`: the identity of this document, unique across the documents of a vault.
    Id,
    /// `::version(1.0)`: the language version the document is written in.
    Version,
    /// `::profile(oneline, standalone)`: the subsets of tomet the document promises to stay inside. A profile
    /// never changes what the document means; see [`Profile`](crate::Profile).
    Profile,
    /// `::meta(title: T, ...)`: information about the document: its free attributes, as `name: value` entries.
    Meta,
    /// `::format.table(max_col_width: 50)`: how documents are formatted: the rules `tomet format` and
    /// the printers follow. One declaration per element it styles, or all grouped under `::format(...)`.
    Format,
    /// `::field.author(type: string)[ about ]`: a `::meta` field this vault or folder declares: its
    /// type and shape. The `[...]` is the field's `about`.
    Field,
    /// `::workspace.ignore("target")`: which paths are part of the vault. Only the vault config may write it.
    Workspace,
    /// `::export(type, path)`: where and in which formats this document is exported.
    Export,
    /// Two things, told apart by whether the head has a path. `::blueprint(kind)` is the header of a
    /// blueprint document: what `tomet new` makes a document of that kind from. `::blueprint.name(@file(path))`
    /// registers the blueprint file `path` under `name` in a vault or folder config.
    ///
    /// The registered name must be the name the file gives itself (`::blueprint(name)`); `tomet check`
    /// reports one that is not. The value is `@file`, not a new element: it is a link `tomet` already
    /// tracks and can rewrite when the file moves.
    Blueprint,
    /// Two things, by the value: `::vocabulary.ns(version: "1.0.0")` is the header of a vocabulary
    /// document (named arguments), and `::vocabulary.ns(@file(path))` registers the file under the
    /// namespace `ns` in a vault or folder config. The registered name must be the namespace the file
    /// gives itself.
    Vocabulary,
    /// Two things, told apart by whether the head has a path. `::dictionary(name){ "term": ... }` is the
    /// header of a dictionary document (the name its terms are mentioned by, `@term(name#id)`, and its
    /// terms); `::dictionary.name(@file(path))` registers the file in a vault or folder config, with
    /// the path relative to the vault root.
    Dictionary,
    /// `::migration(...)`: the settings of the migration tool. `tomet` reads nothing from it; it is
    /// registered so that the vault config may carry it until the tool declares its own vocabulary.
    Migration,
    /// `::macro.name("template")`: a macro this document or vault defines. A text macro is a string;
    /// a block macro is the `[...]` after the head, `::macro.card[ @card[ ${1} ] ]`.
    Macro,
    /// `::var.name(type: string, default: x)[ about ]`: a variable this document or vault declares.
    Var,
    /// `::use(ns, 3)`: brings a vocabulary into scope.
    Use,
    /// `::element.name(display: block)[ about ]`: an element a vocabulary declares.
    Element,
}

/// How many times a declaration may be written in one document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    /// At most once: a second `::kind` or `::meta` is a mistake.
    Once,
    /// As often as needed: `::use` brings in one vocabulary each, and `::field.author`, `::field.created`
    /// are one declaration per field. A path in the head is what makes several declarations one tree.
    Many,
}

/// Where a declaration may be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// In a `.tmt` document and in a `.ctmt` file.
    Both,
    /// In a `.tmt` document only: it presumes elements, vocabularies or a vault.
    ///
    /// This is data, not structure. `config` and `settings` may open to `.ctmt` later; that is a
    /// change of this value and of nothing else.
    Tmt,
}

/// A document the declarations are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    /// A `.tmt` document.
    Tmt,
    /// A `.ctmt` file.
    Ctmt,
}

impl Scope {
    /// Whether a declaration of this scope may be written in `host`.
    pub fn allows(self, host: Host) -> bool {
        match self {
            Scope::Both => true,
            Scope::Tmt => host == Host::Tmt,
        }
    }
}

/// What a standard declaration is written with. A nexus stands on `(...)`; `{...}` only ever adds
/// to it, so which declarations may carry one is fixed here and not left to each reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `::kind(config, 2)`: `(args)` bound to the declaration's positional slots, no `{...}`, and no
    /// path after the name.
    Args,
    /// `::meta(title: T)` or `::field.author(type: string)`: a tree of entries, written as named
    /// arguments of the declaration or as a dot-path in its head. No `{...}`. See
    /// [`Declaration::entries`](crate::Declaration::entries) for how the two spellings meet.
    Entries,
    /// `::dictionary(ns){ ... }`: `(args)` for the slots, and a `{...}` body read as data. The same
    /// name with a path in the head is an [`Entries`](Shape::Entries) registration and takes no body.
    ArgsAndData,
}

/// What is fixed about one standard declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub kind: DeclKind,
    /// The name after `::`.
    pub name: &'static str,
    /// What the positional arguments are called, in order: `::kind(config, 2)` is `kind` and
    /// `version`. Once, here, for every host that reads a declaration.
    pub positional: &'static [&'static str],
    pub cardinality: Cardinality,
    /// Whether the form without a path names the document itself and so may be written once, even
    /// though the declaration as a whole is [`Cardinality::Many`]: `::blueprint(daily)` is the
    /// header of a blueprint, `::blueprint.daily(@file(...))` one of many registrations.
    pub own_header: bool,
    pub scope: Scope,
    /// What it is written with: whether `(...)` holds entries and whether it takes a `{...}`.
    pub shape: Shape,
    /// Whether only the vault config may write it. A folder config or a document that writes it is
    /// reported by `tomet check`, and the loader ignores it.
    pub vault_only: bool,
}

/// What a trailing `[...]` on a declaration with a path becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prose {
    /// It is the declaration's `about:` entry: `::field.author(type: string)[ the author ]` is
    /// `::field.author(type: string, about: [ the author ])`.
    About,
    /// It is the whole value: `::macro.card[ @card[ ${1} ] ]` is `::macro.card([ @card[ ${1} ] ])`.
    Value,
    /// The declaration takes none.
    None,
}

const fn spec(
    kind: DeclKind,
    name: &'static str,
    cardinality: Cardinality,
    scope: Scope,
    shape: Shape,
) -> Spec {
    Spec {
        kind,
        name,
        positional: &[],
        cardinality,
        own_header: false,
        scope,
        shape,
        vault_only: false,
    }
}

/// Every standard declaration, in the order they are listed to people.
const SPECS: [Spec; 17] = [
    Spec {
        // The second slot is the major version of that kind, an integer of 1 or more, and may be
        // left out. The validator checks the value; nothing reads it yet.
        positional: &["kind", "version"],
        ..spec(
            DeclKind::Kind,
            "kind",
            Cardinality::Once,
            Scope::Both,
            Shape::Args,
        )
    },
    Spec {
        // The identity itself. Dots are part of it (`std.config`, `doc-kWBcIUUz`): it is one bare
        // argument.
        positional: &["id"],
        ..spec(
            DeclKind::Id,
            "id",
            Cardinality::Once,
            Scope::Both,
            Shape::Args,
        )
    },
    Spec {
        positional: &["version"],
        ..spec(
            DeclKind::Version,
            "version",
            Cardinality::Once,
            Scope::Both,
            Shape::Args,
        )
    },
    // Bare names, one or more, in no particular order: `::profile(oneline, standalone)`. There is
    // no slot to name them after, so the arguments are read as they were written. `oneline` and
    // `standalone` mean nothing yet for a `.ctmt` file; opening it is this value and nothing else.
    spec(
        DeclKind::Profile,
        "profile",
        Cardinality::Once,
        Scope::Tmt,
        Shape::Args,
    ),
    spec(
        DeclKind::Meta,
        "meta",
        Cardinality::Once,
        Scope::Both,
        Shape::Entries,
    ),
    spec(
        DeclKind::Format,
        "format",
        Cardinality::Many,
        Scope::Tmt,
        Shape::Entries,
    ),
    spec(
        DeclKind::Field,
        "field",
        Cardinality::Many,
        Scope::Tmt,
        Shape::Entries,
    ),
    Spec {
        vault_only: true,
        ..spec(
            DeclKind::Workspace,
            "workspace",
            Cardinality::Many,
            Scope::Tmt,
            Shape::Entries,
        )
    },
    spec(
        DeclKind::Export,
        "export",
        Cardinality::Once,
        Scope::Tmt,
        Shape::Entries,
    ),
    Spec {
        // The kind the blueprint produces. `target` and not `kind`: `::kind(blueprint)` sits
        // directly above it, and `kind` there would mean the output's kind while `::kind` means
        // this file's.
        positional: &["target"],
        own_header: true,
        ..spec(
            DeclKind::Blueprint,
            "blueprint",
            Cardinality::Many,
            Scope::Tmt,
            Shape::ArgsAndData,
        )
    },
    spec(
        DeclKind::Vocabulary,
        "vocabulary",
        Cardinality::Many,
        Scope::Tmt,
        Shape::Entries,
    ),
    Spec {
        // The name the dictionary is mentioned by: `@term(tomet#id)` names it `tomet`.
        positional: &["name"],
        own_header: true,
        ..spec(
            DeclKind::Dictionary,
            "dictionary",
            Cardinality::Many,
            Scope::Tmt,
            Shape::ArgsAndData,
        )
    },
    // `tomet` reads nothing from it.
    spec(
        DeclKind::Migration,
        "migration",
        Cardinality::Once,
        Scope::Tmt,
        Shape::Entries,
    ),
    spec(
        DeclKind::Macro,
        "macro",
        Cardinality::Many,
        Scope::Both,
        Shape::Entries,
    ),
    spec(
        DeclKind::Var,
        "var",
        Cardinality::Many,
        Scope::Both,
        Shape::Entries,
    ),
    Spec {
        // The namespace to bring into scope. The vault says where it lives, so this names it, not
        // its file. The second slot is the version wanted, an integer of 1 or more, and may be
        // left out.
        //
        // One per vocabulary brought in; the two axes -- where it may sit and how often -- are
        // independent, and this is the proof.
        positional: &["target", "version"],
        ..spec(
            DeclKind::Use,
            "use",
            Cardinality::Many,
            Scope::Tmt,
            Shape::Args,
        )
    },
    spec(
        DeclKind::Element,
        "element",
        Cardinality::Many,
        Scope::Tmt,
        Shape::Entries,
    ),
];

/// The plural spellings the singular declarations replaced, and the name that replaces each.
const RETIRED: [(&str, DeclKind); 7] = [
    ("fields", DeclKind::Field),
    ("macros", DeclKind::Macro),
    ("vars", DeclKind::Var),
    ("vocabularies", DeclKind::Vocabulary),
    ("blueprints", DeclKind::Blueprint),
    ("dictionaries", DeclKind::Dictionary),
    ("elements", DeclKind::Element),
];

impl DeclKind {
    /// Every standard declaration.
    pub const ALL: [DeclKind; 17] = [
        DeclKind::Kind,
        DeclKind::Id,
        DeclKind::Version,
        DeclKind::Profile,
        DeclKind::Meta,
        DeclKind::Format,
        DeclKind::Field,
        DeclKind::Workspace,
        DeclKind::Export,
        DeclKind::Blueprint,
        DeclKind::Vocabulary,
        DeclKind::Dictionary,
        DeclKind::Migration,
        DeclKind::Macro,
        DeclKind::Var,
        DeclKind::Use,
        DeclKind::Element,
    ];

    /// What is fixed about this declaration.
    pub fn spec(self) -> &'static Spec {
        SPECS
            .iter()
            .find(|spec| spec.kind == self)
            .expect("every DeclKind has a Spec")
    }

    /// The name after `::`.
    pub fn name(self) -> &'static str {
        self.spec().name
    }

    /// The standard declaration a bare name stands for.
    pub fn from_name(name: &str) -> Option<DeclKind> {
        SPECS
            .iter()
            .find(|spec| spec.name == name)
            .map(|spec| spec.kind)
    }

    /// The declaration that replaced `name`, when `name` is a plural spelling that is retired:
    /// `fields` is `field`, `vocabularies` is `vocabulary`. A retired name is not a declaration any
    /// more ([`from_name`](DeclKind::from_name) does not find it); this is what lets a checker say
    /// what to write instead of only that the name is unknown.
    pub fn retired(name: &str) -> Option<DeclKind> {
        RETIRED
            .iter()
            .find(|(retired, _)| *retired == name)
            .map(|(_, kind)| *kind)
    }

    /// What its positional arguments are called, in order.
    pub fn positional_keys(self) -> &'static [&'static str] {
        self.spec().positional
    }

    /// Whether at most one may be written in a document.
    pub fn is_singleton(self) -> bool {
        self.spec().cardinality == Cardinality::Once
    }

    /// What it is written with.
    pub fn shape(self) -> Shape {
        self.spec().shape
    }

    /// What a trailing `[...]` on this declaration, written with a path, becomes.
    pub fn prose(self) -> Prose {
        match self {
            DeclKind::Field | DeclKind::Var | DeclKind::Element => Prose::About,
            DeclKind::Macro => Prose::Value,
            _ => Prose::None,
        }
    }

    /// Whether only the vault config may write it.
    pub fn is_vault_only(self) -> bool {
        self.spec().vault_only
    }

    /// Whether it may be written in `host`.
    pub fn allowed_in(self, host: Host) -> bool {
        self.spec().scope.allows(host)
    }
}

impl std::fmt::Display for DeclKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declaration_has_one_spec_and_is_found_by_its_name() {
        assert_eq!(SPECS.len(), DeclKind::ALL.len());
        for (kind, spec) in DeclKind::ALL.into_iter().zip(&SPECS) {
            assert_eq!(kind, spec.kind, "the two lists are in the same order");
            assert_eq!(DeclKind::from_name(kind.name()), Some(kind));
            assert_eq!(kind.spec().kind, kind);
        }
        assert_eq!(DeclKind::from_name("callout"), None);
        assert_eq!(DeclKind::from_name("rule"), None);
        assert_eq!(DeclKind::from_name("data"), None);
    }

    #[test]
    fn every_declaration_is_singular_and_the_plurals_are_retired() {
        for (retired, instead) in RETIRED {
            assert_eq!(DeclKind::from_name(retired), None, "{retired}");
            assert_eq!(DeclKind::retired(retired), Some(instead));
            assert_eq!(
                format!("{}s", instead.name()).replace("ys", "ies"),
                retired,
                "{retired} is the plural of {instead}"
            );
        }
        assert_eq!(DeclKind::retired("field"), None);
        assert_eq!(DeclKind::retired("callout"), None);
    }

    #[test]
    fn the_signatures_are_the_ones_the_language_has_always_had() {
        assert_eq!(DeclKind::Kind.positional_keys(), ["kind", "version"]);
        assert_eq!(DeclKind::Id.positional_keys(), ["id"]);
        assert_eq!(DeclKind::Version.positional_keys(), ["version"]);
        assert_eq!(DeclKind::Use.positional_keys(), ["target", "version"]);
        assert_eq!(DeclKind::Blueprint.positional_keys(), ["target"]);
        assert_eq!(DeclKind::Dictionary.positional_keys(), ["name"]);
        assert!(DeclKind::Meta.positional_keys().is_empty());
        // Its arguments are names, not slots.
        assert!(DeclKind::Profile.positional_keys().is_empty());
    }

    #[test]
    fn only_two_declarations_take_a_brace_body() {
        let with_body: Vec<DeclKind> = DeclKind::ALL
            .into_iter()
            .filter(|k| k.shape() == Shape::ArgsAndData)
            .collect();
        assert_eq!(with_body, [DeclKind::Blueprint, DeclKind::Dictionary]);
        // The ones that hold entries have no positional slot to bind them to.
        for kind in DeclKind::ALL {
            if kind.shape() == Shape::Entries {
                assert!(kind.positional_keys().is_empty(), "{kind}");
            }
        }
    }

    #[test]
    fn only_the_workspace_is_the_vaults_alone() {
        let vault_only: Vec<DeclKind> = DeclKind::ALL
            .into_iter()
            .filter(|k| k.is_vault_only())
            .collect();
        assert_eq!(vault_only, [DeclKind::Workspace]);
    }

    #[test]
    fn what_a_document_says_about_itself_is_written_once() {
        let once: Vec<DeclKind> = DeclKind::ALL
            .into_iter()
            .filter(|k| k.is_singleton())
            .collect();
        assert_eq!(
            once,
            [
                DeclKind::Kind,
                DeclKind::Id,
                DeclKind::Version,
                DeclKind::Profile,
                DeclKind::Meta,
                DeclKind::Export,
                DeclKind::Migration
            ]
        );
        // Two of the many have a header of their own, which is written once.
        let own_header: Vec<DeclKind> = DeclKind::ALL
            .into_iter()
            .filter(|k| k.spec().own_header)
            .collect();
        assert_eq!(own_header, [DeclKind::Blueprint, DeclKind::Dictionary]);
    }

    #[test]
    fn a_trailing_prose_is_the_about_of_what_is_described() {
        assert_eq!(DeclKind::Field.prose(), Prose::About);
        assert_eq!(DeclKind::Var.prose(), Prose::About);
        assert_eq!(DeclKind::Element.prose(), Prose::About);
        assert_eq!(DeclKind::Macro.prose(), Prose::Value);
        assert_eq!(DeclKind::Meta.prose(), Prose::None);
    }

    #[test]
    fn a_ctmt_file_has_the_declarations_that_do_not_presume_elements() {
        let in_ctmt: Vec<DeclKind> = DeclKind::ALL
            .into_iter()
            .filter(|k| k.allowed_in(Host::Ctmt))
            .collect();
        assert_eq!(
            in_ctmt,
            [
                DeclKind::Kind,
                DeclKind::Id,
                DeclKind::Version,
                DeclKind::Meta,
                DeclKind::Macro,
                DeclKind::Var
            ]
        );
        assert!(DeclKind::ALL.iter().all(|k| k.allowed_in(Host::Tmt)));
    }
}
