//! The standard declarations, and what is fixed about each: how many may be written, where, and
//! what its positional arguments are called.

/// A standard declaration: what `::name` can be, for a name the language gives a meaning.
///
/// Closed on purpose. The standard declarations are the language's own, and a host matches on
/// this to do what only it knows (a `.tmt` binds a vocabulary for `Use`, a vault reads `Config`);
/// a user's own preamble element is a vocabulary's, and is not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclKind {
    /// `::kind(name, 2)`: what kind of document this is, and the major version of that kind.
    Kind,
    /// `::version(1.0)`: the language version the document is written in.
    Version,
    /// `::meta{ ... }`: information about the document.
    Meta,
    /// `::config{ ... }`: settings for the document or the vault it sits in.
    Config,
    /// `::settings{ ... }`: the same, in the file that holds a vault's.
    Settings,
    /// `::macros{ name: "template" }`: the macros this document or vault defines.
    Macros,
    /// `::vars{ name: { type, default, about } }`: the variables this document or vault declares.
    Vars,
    /// `::use(ns, 3)`: brings a vocabulary into scope.
    Use,
    /// `::blueprint(kind)`: what `tomet new` makes a document of that kind from.
    Blueprint,
    /// `::vocabulary(ns){ ... }`: the header of a vocabulary document.
    Vocabulary,
    /// `::elements{ ... }`: the elements a vocabulary declares.
    Elements,
}

/// How many times a declaration may be written in one document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    /// At most once: a second `::meta` is a mistake.
    Once,
    /// As often as needed: `::use` brings in one vocabulary each.
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
    pub scope: Scope,
}

/// Every standard declaration, in the order they are listed to people.
const SPECS: [Spec; 11] = [
    Spec {
        kind: DeclKind::Kind,
        name: "kind",
        // The second slot is the major version of that kind, an integer of 1 or more, and may be
        // left out. The validator checks the value; nothing reads it yet.
        positional: &["kind", "version"],
        cardinality: Cardinality::Once,
        scope: Scope::Both,
    },
    Spec {
        kind: DeclKind::Version,
        name: "version",
        positional: &["version"],
        cardinality: Cardinality::Once,
        scope: Scope::Both,
    },
    Spec {
        kind: DeclKind::Meta,
        name: "meta",
        positional: &[],
        cardinality: Cardinality::Once,
        scope: Scope::Both,
    },
    Spec {
        kind: DeclKind::Config,
        name: "config",
        positional: &[],
        cardinality: Cardinality::Once,
        scope: Scope::Tmt,
    },
    Spec {
        kind: DeclKind::Settings,
        name: "settings",
        positional: &[],
        cardinality: Cardinality::Once,
        scope: Scope::Tmt,
    },
    Spec {
        kind: DeclKind::Macros,
        name: "macros",
        positional: &[],
        cardinality: Cardinality::Once,
        scope: Scope::Both,
    },
    Spec {
        kind: DeclKind::Vars,
        name: "vars",
        positional: &[],
        cardinality: Cardinality::Once,
        scope: Scope::Both,
    },
    Spec {
        kind: DeclKind::Use,
        name: "use",
        // The namespace to bring into scope. The vault says where it lives, so this names it, not
        // its file. The second slot is the version wanted, an integer of 1 or more, and may be
        // left out.
        positional: &["target", "version"],
        // One per vocabulary brought in; the two axes -- where it may sit and how often -- are
        // independent, and this is the proof.
        cardinality: Cardinality::Many,
        scope: Scope::Tmt,
    },
    Spec {
        kind: DeclKind::Blueprint,
        name: "blueprint",
        // The kind the blueprint produces. `target` and not `kind`: `::kind(blueprint)` sits
        // directly above it, and `kind` there would mean the output's kind while `::kind` means
        // this file's.
        positional: &["target"],
        cardinality: Cardinality::Once,
        scope: Scope::Tmt,
    },
    Spec {
        kind: DeclKind::Vocabulary,
        name: "vocabulary",
        positional: &[],
        cardinality: Cardinality::Once,
        scope: Scope::Tmt,
    },
    Spec {
        kind: DeclKind::Elements,
        name: "elements",
        positional: &[],
        cardinality: Cardinality::Once,
        scope: Scope::Tmt,
    },
];

impl DeclKind {
    /// Every standard declaration.
    pub const ALL: [DeclKind; 11] = [
        DeclKind::Kind,
        DeclKind::Version,
        DeclKind::Meta,
        DeclKind::Config,
        DeclKind::Settings,
        DeclKind::Macros,
        DeclKind::Vars,
        DeclKind::Use,
        DeclKind::Blueprint,
        DeclKind::Vocabulary,
        DeclKind::Elements,
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

    /// What its positional arguments are called, in order.
    pub fn positional_keys(self) -> &'static [&'static str] {
        self.spec().positional
    }

    /// Whether at most one may be written in a document.
    pub fn is_singleton(self) -> bool {
        self.spec().cardinality == Cardinality::Once
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
    fn the_signatures_are_the_ones_the_language_has_always_had() {
        assert_eq!(DeclKind::Kind.positional_keys(), ["kind", "version"]);
        assert_eq!(DeclKind::Version.positional_keys(), ["version"]);
        assert_eq!(DeclKind::Use.positional_keys(), ["target", "version"]);
        assert_eq!(DeclKind::Blueprint.positional_keys(), ["target"]);
        assert!(DeclKind::Meta.positional_keys().is_empty());
    }

    #[test]
    fn only_use_may_be_written_more_than_once() {
        for kind in DeclKind::ALL {
            assert_eq!(kind.is_singleton(), kind != DeclKind::Use, "{kind}");
        }
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
                DeclKind::Version,
                DeclKind::Meta,
                DeclKind::Macros,
                DeclKind::Vars
            ]
        );
        assert!(DeclKind::ALL.iter().all(|k| k.allowed_in(Host::Tmt)));
    }
}
