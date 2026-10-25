use tomet_ast::{Element, Name, Placement, Sigil};
use tomet_declaration::DeclKind;

/// What a parsed `Element` officially means, replacing the ad-hoc
/// stringly-typed `kind: String` that `tomet-html` and
/// `tomet-markdown/export.rs` each used to compute independently.
///
/// This only expresses *recognition* ("this element is `::meta`"), not
/// *action* ("`::meta` produces no output") -- consumers still decide for
/// themselves what to do with a given kind, since the same kind can mean
/// different things for different output formats (e.g. `Meta` renders as
/// nothing in both HTML and CommonMark today, but for unrelated reasons
/// specific to each format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementKind {
    /// A standard declaration about the document: `::kind`, `::meta`, `::macros`, `::use` and the
    /// rest (`DeclKind`).
    ///
    /// What each one *is* -- its positional slots, how often it may be written, whether a `.ctmt`
    /// may have it, what reading it gives -- is `tomet-declaration`'s. What `use`, `blueprint`,
    /// `config` and the vocabulary pair *do* is this crate's and `tomet-config`'s, which match on
    /// the `DeclKind`. Eleven variants used to stand here, one per declaration; they were a second
    /// table of the same names.
    Declaration(DeclKind),
    /// `@data{...}` -- the description of an element's `{...}` slot.
    /// Declared by no vocabulary yet.
    Data,
    /// `@draft[ what is missing ]` -- there is no text here yet, and the
    /// element stands in place of it.
    ///
    /// The test that keeps this from collapsing into [`Fixme`] is
    /// decidable by looking: `Draft` is an absence, `Fixme` is a presence
    /// that is wrong. They also differ at export -- a draft marks output
    /// that is not there, a fixme marks output that is publishable with a
    /// note against it.
    ///
    /// In `std` because an unfinished spot happens in every kind, so no
    /// kind's vocabulary can own it and a shared one would need `::use` in
    /// every document that ever has a gap.
    Draft,
    /// `@fixme[ what is wrong ]` -- there is text here and it needs
    /// revisiting. See [`Draft`](ElementKind::Draft) for the difference.
    Fixme,
    /// `@conflict(a: ..., b: ...)` -- two divergent candidates, from a
    /// 3-way merge that touched the same spot from both sides and gave up
    /// reconciling it automatically. `a`/`b` name structural slots, not
    /// perspective: unlike a VCS's transient `ours`/`theirs` (local to one
    /// working tree, gone the moment the conflict resolves), this marker
    /// is written into a document other people may read before it is
    /// resolved, and "mine" said by the writer reads as "theirs" to
    /// everyone else. No timestamp either -- that is what `git log`
    /// already answers, and letting assignment order double as a
    /// recency signal is exactly the perspective leak the names already
    /// avoid.
    ///
    /// `a: [...]`/`b: [...]` hold block content (see
    /// `tmtroot/docs/spec/feature/content-shape.tmt`'s `Value::Blocks`)
    /// when `@conflict` stands in the document body, or a scalar when
    /// it sits in a `{data}`/`(args)` value position -- same element,
    /// same `(args)` shape either way, so [`builtin_content_shape`]
    /// returns `None` for it like every other kind whose payload is
    /// `(args)` rather than `[content]` (`@conflict` never uses
    /// `[content]`/`|content` at all).
    ///
    /// In `std`, bare, for the same reason as [`Draft`](ElementKind::Draft)/
    /// [`Fixme`](ElementKind::Fixme): a conflict can happen while editing
    /// any kind of document, so no single kind's vocabulary can own it,
    /// and a shared vocabulary would need `::use` everywhere a merge might
    /// ever collide.
    Conflict,
    /// The one officially-supported link element, `@link(target:...)`
    /// (or the positional `@link(...)` shorthand -- see
    /// `crate::positional::builtin_positional_arg_key`). What kind of
    /// target it is (url/file/tm/id/ref) is not carried by the element or
    /// its key anymore -- it's derived from the `target` string's own
    /// scheme prefix via `crate::target::target_scheme`, once the target
    /// has been extracted with `crate::target::link_target`.
    Link,
    /// `@file(path/to/x)` -- a path, named and not navigated to.
    ///
    /// Separate from [`Link`](ElementKind::Link) because the two differ
    /// in what they produce: `@link` is an `<a>`, and this is a mention.
    /// Prose that says "read `raw.rs`" is not offering to take the
    /// reader there. Before this, such a mention was written in
    /// backticks, which no checker can tell from a sentence that happens
    /// to look like a path -- 453 of them here, and of the 164 distinct
    /// ones only 64 name something that exists.
    ///
    /// Checked like `@link(file:...)`: the path has to exist and be a
    /// file. Not a duplicate of it -- one navigates, one names.
    File,
    /// `@dir(path/to/x)` -- the same for a directory, checked with
    /// `is_dir` rather than `is_file`, for the reason `dir:` exists at
    /// all (see `crate::target::TargetScheme::Dir`).
    Dir,
    Embed,
    Hr,
    Em,
    Strong,
    Mark,
    /// `~~x~~` -- struck through. Says the words were withdrawn,
    /// not how to draw them, which is why this is here and `smallcaps`
    /// is not.
    Strikeout,
    /// `@ruby[漢字](rt:"かんじ")` -- a reading annotation. `[content]` is
    /// the base text, `rt` the reading, mirroring `@link(target:...)[title]`'s
    /// "named entry + content" shape rather than the shortcut-inline family
    /// (`em`/`strong`/`mark`/`strikeout`), since no comparable CommonMark
    /// spelling exists to shortcut against.
    Ruby,
    Raw,
    Quote,
    /// `@callout(variant)[ ... ]` -- an admonition.
    ///
    /// Late to this list, and the list was the only thing that did not
    /// know. `callout` is special-cased in eight places:
    /// `positional::builtin_positional_arg_key` (`variant` is its
    /// positional), the Markdown and Typst writers' `render_callout`,
    /// the Markdown *reader*, which turns `> [!note]` into
    /// `Sigil::named("callout")`, `tomet-format-printer`'s style branch,
    /// two arms in `tomet-config`, and the LSP.
    ///
    /// The reader is what settles it: a Markdown round-trip produced an
    /// element that validation then rejected as unknown. The three
    /// writers already match `callout` in the same arm list as `heading`,
    /// `raw` and `link`, on `kind.as_str()`, so joining the list
    /// changes nothing for them.
    ///
    /// This records that the name exists and stands as a block. What its
    /// variants are, and whether it ever gets styling of its own, is
    /// still undecided -- `docs/spec/builtin-elements.tmt` used to say it
    /// was not built in at all, which was the part that was false.
    Callout,
    /// `Sigil::named("card")` -- a plain title + content box, with no
    /// admonition semantics (no `variant`, no color/icon). Exists
    /// alongside [`ElementKind::Callout`] rather than as a `variant` of
    /// it: callout's whole value is signaling a category to the reader
    /// (info/warning/tip), and a "neutral" callout would ask a reader to
    /// notice the *absence* of that signal, which is a worse interface
    /// than a differently-named element for the case where there is
    /// nothing to signal.
    Card,
    Table,
    Heading,
    /// A namespaced `@ns.name` that doesn't match any of the built-in
    /// kinds above -- consumers fall back to their own generic rendering,
    /// keyed on the name. A *bare* name that matches nothing is
    /// [`UnknownName`] rather than this, which is the whole point of
    /// namespaces.
    Custom(String),
    /// `Sigil::Bare` -- the untyped `(key)[content]` entries inside a
    /// container like `@links{}`. Its string form ("bare") can collide
    /// with a user writing a literal `@bare` element (which
    /// would classify as `Custom("bare")` instead), but that's the same
    /// ambiguity the old stringly-typed `element_kind()` already had --
    /// not a regression.
    Bare,
    /// `Sigil::Dollar` -- `${...}` interpolation. A dedicated kind (not
    /// `Custom`) since, like `Em`/`Hr`/`Ref`, it's built-in recognized
    /// syntax with fixed grammar-level meaning, not an arbitrary
    /// user-chosen element name.
    Interp,
    /// `@ftn` -- a footnote: a definition when it has content, a
    /// reference to the one with the same id when it has none.
    Footnote,
    /// `@tag` -- topic/category tag.
    Tag,
    /// `@task(status)[ ... ]` -- an actionable task item.
    Task,
}

impl ElementKind {
    /// The kind's name as `tomet-html`/`tomet-markdown` (and
    /// the TUI's structural search) use it: as a `data-*`/CSS-class
    /// fragment, or to match against `match kind.as_str() { "hr" => ..., ...
    /// }`. `Custom(name)` returns `name` itself.
    pub fn as_str(&self) -> &str {
        match self {
            ElementKind::Declaration(declaration) => declaration.name(),
            ElementKind::Data => "data",
            ElementKind::Draft => "draft",
            ElementKind::Fixme => "fixme",
            ElementKind::Conflict => "conflict",
            ElementKind::File => "file",
            ElementKind::Dir => "dir",
            ElementKind::Link => "link",
            ElementKind::Embed => "embed",
            ElementKind::Hr => "hr",
            ElementKind::Em => "em",
            ElementKind::Strong => "strong",
            ElementKind::Mark => "mark",
            ElementKind::Strikeout => "strikeout",
            ElementKind::Ruby => "ruby",
            ElementKind::Raw => "raw",
            ElementKind::Quote => "quote",
            ElementKind::Callout => "callout",
            ElementKind::Card => "card",
            ElementKind::Table => "table",
            ElementKind::Heading => "heading",
            ElementKind::Custom(name) => name,
            ElementKind::Bare => "bare",
            ElementKind::Interp => "interp",
            ElementKind::Footnote => "ftn",
            ElementKind::Tag => "tag",
            ElementKind::Task => "task",
        }
    }
}

/// Recognized **bare** names of the body elements with fixed meaning -- kept as one list so
/// `classify_std`'s name -> variant match and `ElementKind::as_str`'s variant
/// -> name match can't silently drift apart (see
/// `builtin_kind_round_trips_through_as_str` below, which checks every
/// entry here). The declarations are not here: `tomet-declaration`'s `DeclKind` is their
/// one list, and [`builtin_kinds`] puts the two together.
///
/// This is the `std` namespace. `@link` is shorthand for `@std.link`, and
/// bare names resolve here and nowhere else -- a user-defined element must
/// be namespaced (`deck.bookmark`), which is why an unrecognized bare name
/// is an error rather than a `Custom` kind. The only other namespace whose
/// names may be written bare is the document's own `::kind`, and `std` wins
/// any collision between the two: a vocabulary may not declare a name that
/// appears here, so the shorthand is never ambiguous.
///
/// **Why `std` lives in Rust rather than in a `::vocabulary(std)` document,
/// and what has to change.** These entries carry behavior, not just shape:
/// `@link`'s target kind is derived from the target string's own scheme
/// prefix (`crate::target::target_scheme`), `::meta` selects an embedded
/// format, `@raw` takes a raw body. A vocabulary document declares
/// names, arguments and constraints; it cannot yet declare any of that, so
/// moving `std` into one would either lose the behavior or smuggle it in
/// under a key that means "call into the binary".
///
/// That is a limit of the vocabulary format, not a property of `std`, and
/// the author's decision is that it has to go: `::vocabulary(std)` must
/// eventually be writable. Until then, treat this list as the one
/// hard-coded namespace and not as a permanent exemption. The useful test
/// while designing the format is to try to express `@link` in it -- what
/// that cannot say is exactly what is still missing.
const BODY_KINDS: [(&str, ElementKind); 23] = [
    ("data", ElementKind::Data),
    // Marks on the document about the document. `//(TODO)` was the first
    // spelling tried and cannot work: `tomet_ast` has no comment node, so
    // a comment marker is invisible to every consumer and is dropped by
    // anything that re-prints from the tree.
    ("draft", ElementKind::Draft),
    ("fixme", ElementKind::Fixme),
    ("conflict", ElementKind::Conflict),
    ("file", ElementKind::File),
    ("dir", ElementKind::Dir),
    ("link", ElementKind::Link),
    ("embed", ElementKind::Embed),
    ("hr", ElementKind::Hr),
    ("em", ElementKind::Em),
    ("strong", ElementKind::Strong),
    ("mark", ElementKind::Mark),
    ("strikeout", ElementKind::Strikeout),
    ("ruby", ElementKind::Ruby),
    ("raw", ElementKind::Raw),
    ("quote", ElementKind::Quote),
    ("callout", ElementKind::Callout),
    ("card", ElementKind::Card),
    ("table", ElementKind::Table),
    ("heading", ElementKind::Heading),
    ("ftn", ElementKind::Footnote),
    ("tag", ElementKind::Tag),
    ("task", ElementKind::Task),
];

/// Every recognized bare name and what it is: the standard declarations, then the body elements.
pub fn builtin_kinds() -> impl Iterator<Item = (&'static str, ElementKind)> + Clone {
    DeclKind::ALL
        .into_iter()
        .map(|kind| (kind.name(), ElementKind::Declaration(kind)))
        .chain(BODY_KINDS.iter().cloned())
}

fn builtin_kind(name: &str) -> Option<ElementKind> {
    DeclKind::from_name(name)
        .map(ElementKind::Declaration)
        .or_else(|| {
            BODY_KINDS
                .iter()
                .find(|(builtin, _)| *builtin == name)
                .map(|(_, kind)| kind.clone())
        })
}

/// Classifies a [`Name`] against the `std` vocabulary alone.
///
/// `std` alone is the whole of what this sees. A document may also write
/// its own `::kind`'s namespace bare, and names it brought in with `::use`
/// resolve too -- none of that is here, because none of it is knowable
/// from a `Name`. That resolution is `Bindings::classify`; this is the
/// narrower question underneath it.
///
/// A namespaced name is always `Custom` -- namespaces are exactly how a
/// user-defined element declares it is not part of the built-in
/// vocabulary. A bare name must be in [`builtin_kinds`]; anything else is
/// [`UnknownName`], not a silent `Custom`.
///
/// That silent fallback -- `builtin_kind(name).unwrap_or_else(|| Custom(name))`
/// -- is where the old sigil ambiguity actually lived. `<T>` and `@name`
/// were meant to separate official from user-defined elements, but both
/// classified through this one arm, so the distinction never reached the
/// tree. Namespaces carry it now, and this returns an error instead of
/// inventing a kind.
pub fn classify_std_name(name: &Name) -> Result<ElementKind, UnknownName> {
    if !name.is_bare() {
        return Ok(ElementKind::Custom(name.to_string()));
    }
    builtin_kind(&name.name).ok_or_else(|| UnknownName {
        name: name.to_string(),
        unbound_namespace: None,
        declaration: false,
    })
}

/// A declaration about the document is written `::name`. Written `@name` it
/// is refused -- the element sigil is for content -- and means nothing: it is
/// not classified as the declaration, so nothing reads it as one.
///
/// `kind` is what the bare name resolved to; `name` is as written.
pub fn refuse_declaration_as_element(
    name: &Name,
    kind: ElementKind,
) -> Result<ElementKind, UnknownName> {
    if name.is_bare() && builtin_region(&kind) == crate::vocabulary::Region::Preamble {
        return Err(UnknownName {
            name: name.to_string(),
            unbound_namespace: None,
            declaration: true,
        });
    }
    Ok(kind)
}

/// An unrecognized bare element name.
///
/// A bare name resolves only in `std` or the document's own `::kind`
/// namespace, so this is what a `#tag` alone on a line reports --
/// deliberately, since hashtags are not a feature and a bare unknown name
/// is far more likely to be a typo or a missing namespace binding.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnknownName {
    /// The name as written, namespace included -- `deck.ref`, not `ref`.
    ///
    /// It used to hold only the local part, so writing `@deck.ref` with
    /// `deck` out of scope was reported as "unknown element `ref`",
    /// naming something the author had not written.
    pub name: String,
    /// Set when the name is namespaced and that namespace is not in
    /// scope at all. The distinction matters to whoever has to fix it:
    /// a missing `::use` is a different edit from a typo in the element.
    pub unbound_namespace: Option<String>,
    /// Set when the name is one of the declarations about the document
    /// (`kind`, `meta`, ...) written as an element, `::kind`. It is not unknown
    /// -- it is spelled for something else: a declaration is `::kind`.
    pub declaration: bool,
}

impl std::fmt::Display for UnknownName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.declaration {
            return write!(
                f,
                "`{0}` is a declaration about the document: write `::{0}`, not `@{0}`",
                self.name
            );
        }
        if let Some(namespace) = &self.unbound_namespace {
            return write!(
                f,
                "`{}`: the namespace `{namespace}` is not in scope -- declare its \
                 vocabulary in `vocabularies` and write `::use({namespace})`",
                self.name
            );
        }
        if let Some((namespace, local)) = self.name.split_once('.') {
            return write!(
                f,
                "`{}`: the namespace `{namespace}` declares no `{local}`",
                self.name
            );
        }
        write!(
            f,
            "unknown element `{}`: only `std` and this document's own `::kind` may be \
             written bare; namespace it (`ns.{}`), or declare the vocabulary that has it \
             and bind it with `::use`",
            self.name, self.name
        )
    }
}

impl std::error::Error for UnknownName {}

/// Classifies `el` by its `Sigil`, against the `std` vocabulary alone.
///
/// The `_std` is the whole caveat. For a document carrying a `::kind`,
/// this returns [`UnknownName`] for every element that document's own
/// vocabulary declares, which is correct for what it asks and wrong for
/// what a caller usually wants. `Bindings::classify` is the one that
/// answers "what does this name mean *in this document*".
///
/// Reach for this when there is no document to resolve against -- the
/// validator uses it for the non-`Named` sigils, which carry no name to
/// resolve.
///
/// `Sigil::Bare` is `ElementKind::Bare`, `Sigil::Dollar` is
/// `ElementKind::Interp`. Everything else goes through [`classify_std_name`].
///
/// The name is all the sigil carries. Where the element sits is
/// `Element::placement`, and a placement that disagrees with the kind's
/// definition is reported separately by [`shape_mismatch`].
///
/// No inference from `args` happens here -- `@(url:...)`-style key-based
/// guessing was retired; the only way to get `ElementKind::Link` is to
/// write `@link` explicitly.
pub fn classify_std(el: &Element) -> Result<ElementKind, UnknownName> {
    match &el.sigil {
        // `::name` and `@name` mean the same by name; whether the spelling
        // fits the element's `Region` is a separate check.
        Sigil::Named(name) => refuse_declaration_as_element(name, classify_std_name(name)?),
        Sigil::Document(name) => classify_std_name(name),
        Sigil::Bare => Ok(ElementKind::Bare),
        Sigil::Dollar => Ok(ElementKind::Interp),
    }
}

/// [`classify_std`], falling back to `Custom` for an unknown bare name.
///
/// Same `std`-only scope, and the same caveat: a user vocabulary's
/// elements come back as `Custom` here rather than as themselves.
///
/// For consumers that render whatever they are given and have no way to
/// report a diagnostic -- an HTML writer mid-document, say. Validation
/// belongs to [`classify_std`]; this is the lenient read of the same thing.
pub fn classify_std_lenient(el: &Element) -> ElementKind {
    classify_std(el).unwrap_or_else(|e| ElementKind::Custom(e.name))
}

/// Whether a kind is a directive -- an element that configures or
/// annotates the document and has no rendering of its own.
///
/// Every writer needs this and each used to keep its own copy of the
/// list, which is how `settings` and `import` came to be missing from all
/// three of them: they joined [`builtin_kinds`] after those lists were
/// written, so `::settings(file:...)` exported as a stray `<div>`.
pub fn is_directive(kind: &ElementKind) -> bool {
    matches!(kind, ElementKind::Declaration(_))
}

/// The shape a built-in element must take.
///
/// This table is the only place shape lives. The surface syntax does not
/// carry it: an element is spelled `@name` wherever it appears, and the
/// parser decides placement from position without consulting this.
///
/// `None` means either shape is legal.
pub fn required_shape(kind: &ElementKind) -> Option<Shape> {
    use ElementKind::*;
    Some(match kind {
        Declaration(_) | Hr | Callout | Card | Table | Heading | Data => Shape::Block,
        Em | Strong | Mark | Strikeout | Ruby => Shape::Inline,
        // Either shape. A link or an embed alone on a line is not a
        // structural error -- it is how you show one file or one image.
        // What `shape_mismatch` is for is the case that breaks something:
        // a `heading` inside a paragraph, or `*emphasis*` standing as a
        // block. Constraining these caught nothing but ordinary writing,
        // in this repository's own `.writ.tmt` among other places.
        // A path mention goes both ways too: inside a sentence, and alone
        // in a table cell where the cell is the reference.
        Link | Embed | File | Dir => return None,
        // Either shape. Standing alone it renders <pre><code>, inside running
        // text it renders <code>.
        Raw => return None,
        // Either shape, and the reason the element is not called
        // `blockquote`. HTML needs two names because `<blockquote>` and
        // `<q>` are two elements; Tomet decides placement from position,
        // so one name covers both and the prefix has nothing left to
        // distinguish. Markdown never had an inline quote to distinguish
        // it from either.
        Quote => return None,
        // Either shape, for the same reason: a gap is sometimes a phrase
        // inside a sentence and sometimes a whole missing section.
        Draft | Fixme => return None,
        // Either shape too: a conflicting edit can land mid-sentence (one
        // word disputed) or span a whole section, same as a gap can.
        Conflict => return None,
        Footnote => return None,
        Tag => return None,
        Task => return None,
        Custom(_) | Bare | Interp => return None,
    })
}

/// Whether an element stands on its own or belongs to running text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Block,
    Inline,
}

impl From<Placement> for Shape {
    fn from(placement: Placement) -> Self {
        match placement {
            Placement::Block => Shape::Block,
            Placement::Inline => Shape::Inline,
        }
    }
}

/// What a built-in kind's `[content]`/`|content` (`Vec<Block>`, see
/// `tmtroot/docs/spec/feature/content-shape.tmt`) may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentShape {
    /// At most one plain paragraph -- no nested blocks, no multiple
    /// paragraphs. What every inline-only built-in (`em`, `link`, a
    /// heading's title, ...) has always meant by "content", now that the
    /// parser itself no longer enforces it by construction.
    Inline,
    /// Any blocks -- multiple paragraphs, headings, lists, nested
    /// elements.
    Block,
}

/// Whether `kind`'s `[content]`/`|content` may hold more than one plain
/// paragraph.
///
/// `None` means `kind` doesn't use `[content]` at all (its own slots are
/// `{data}`/`(args)`, or it has no content concept, or -- for `Custom`,
/// `Bare`, `Interp` -- it isn't a built-in kind in the first place, and a
/// vocabulary's own `@content{allow:}` governs it instead). This table is
/// the classification agreed in `.agents/tasks/
/// conflict-element-and-content-model.md`'s "Step 1".
pub fn builtin_content_shape(kind: &ElementKind) -> Option<ContentShape> {
    use ElementKind::*;
    match kind {
        // Directives and vocabulary-document bookkeeping: `{data}`/
        // `(args)` only, no `[content]`.
        Declaration(_) | Data => None,
        // Self-closing; a fenced/backtick code body's `[content]` is
        // synthesized by the parser itself (`raw.rs`), not shaped
        // by an author, so there is nothing here for a shape check to
        // classify. No observed `@tag[...]` usage to classify yet.
        // Lists are no longer `Element`s at all (see `tomet_ast::List`),
        // so there is nothing list-shaped left to classify here.
        Hr | Raw | Tag => None,
        // `@conflict`'s payload (`a`/`b`) lives in `(args)`, via
        // `Value::Blocks` -- it never uses `[content]`/`|content` at all,
        // same bucket as the directives above for that reason (not
        // because it can't hold blocks: `a`/`b` each can, just not
        // through this slot).
        Conflict => None,
        // Short single-run labels: a one-line note, a display label,
        // character-level running text, a heading's title.
        Draft | Fixme | File | Dir | Link | Embed | Em | Strong | Mark | Strikeout | Ruby
        | Heading => Some(ContentShape::Inline),
        // May hold multiple paragraphs, headings, lists: a blockquote
        // commonly spans several paragraphs; an admonition/card/footnote
        // can too; a table's rows/cells; a task's details.
        Quote | Callout | Card | Table | Footnote | Task => Some(ContentShape::Block),
        // Not a built-in bare name at all.
        Custom(_) | Bare | Interp => None,
    }
}

/// Whether at most one of `kind` may appear in a document.
///
/// A standard declaration says so itself (`tomet-declaration`'s `Spec`): all but `::use`, which
/// brings in one vocabulary each and is the proof that where a declaration may sit and how often are
/// two axes. No body element is a singleton.
pub fn builtin_singleton(kind: &ElementKind) -> bool {
    matches!(kind, ElementKind::Declaration(declaration) if declaration.is_singleton())
}

/// Where in a document `kind` may appear.
///
/// Every standard declaration is in the preamble, and nothing else the language names is: a body
/// element goes anywhere. What the spec spelled `placement: head` -- a name that also carried the
/// block/inline rule, which is why the key is being retired in favour of these two plus `display`.
pub fn builtin_region(kind: &ElementKind) -> crate::vocabulary::Region {
    use crate::vocabulary::Region;
    match kind {
        ElementKind::Declaration(_) => Region::Preamble,
        _ => Region::Body,
    }
}

/// Reports an element put where its kind cannot go -- a `meta` in the
/// middle of a paragraph, or a `heading` standing as a block inside one.
///
/// The comparison is between the element's *placement*, which the parser
/// derived from position, and [`required_shape`]. It used to compare the
/// author's sigil against the same table, which made the report about
/// spelling (`#em`) rather than about the document.
///
/// `Placement::Inline` on a `Block`-required kind is always a mismatch,
/// with no exception for directives or for column 1: the parser only
/// ever produces that shape when the author wrote an explicit `\`
/// continuation trigger (`docs/spec/syntax.tmt`'s `##[ 継続 ]`) to join a
/// bare element into a paragraph, and the parser stays kind-oblivious
/// about what it joined (heading, directive, or anything else) -- this
/// function is where a join that made no sense for that kind gets
/// reported back, same as it always has.
///
/// The reverse (`Inline`-required found as `Block`) has no exception
/// either: nothing about the join rule ever makes an inline-only element
/// isolated, so `Placement::Block` there is exactly as wrong as always.
///
/// Returns `Some((found, expected))` when they disagree.
pub fn shape_mismatch(el: &Element) -> Option<(Shape, Shape)> {
    if matches!(el.sigil, Sigil::Bare | Sigil::Dollar) {
        return None;
    }
    let found = Shape::from(el.placement);
    let kind = classify_std(el).ok()?;
    let expected = required_shape(&kind)?;
    (found != expected).then_some((found, expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Value;
    use tomet_tree::{ElementExt, element_new};

    #[test]
    fn builtin_kind_round_trips_through_as_str() {
        for (name, kind) in builtin_kinds() {
            assert_eq!(kind.as_str(), name, "as_str() mismatch for {name:?}");
            assert_eq!(
                classify_std_name(&Name::bare(name)),
                Ok(kind.clone()),
                "classify_std_name() mismatch for {name:?}"
            );
        }
    }

    #[test]
    fn unknown_bare_name_is_an_error() {
        // Bare names are reserved for the built-in vocabulary, so this is
        // the diagnostic that replaces the old silent `Custom` fallback.
        let el = element_new(Sigil::named("caution"));
        assert_eq!(
            classify_std(&el),
            Err(UnknownName {
                name: "caution".to_string(),
                unbound_namespace: None,
                declaration: false,
            })
        );
    }

    #[test]
    fn a_declaration_is_classified_by_its_double_colon_and_refused_as_an_element() {
        for name in [
            "kind",
            "meta",
            "config",
            "settings",
            "macros",
            "vars",
            "use",
            "blueprint",
            "version",
            "vocabulary",
        ] {
            assert!(
                classify_std(&element_new(Sigil::document(name))).is_ok(),
                "::{name}"
            );
            let err = classify_std(&element_new(Sigil::named(name))).unwrap_err();
            assert!(err.declaration, "@{name}");
            assert!(
                err.to_string().contains(&format!("write `::{name}`")),
                "{err}"
            );
            // and it is not read as the declaration, however leniently
            assert_eq!(
                classify_std_lenient(&element_new(Sigil::named(name))),
                ElementKind::Custom(name.to_string())
            );
        }
        // content keeps its sigil
        assert!(classify_std(&element_new(Sigil::named("callout"))).is_ok());
        assert!(classify_std(&element_new(Sigil::document("callout"))).is_ok());
    }

    #[test]
    fn namespaced_name_is_custom() {
        let el = element_new(Sigil::Named(Name::namespaced("deck", "caution")));
        assert_eq!(
            classify_std(&el),
            Ok(ElementKind::Custom("deck.caution".to_string()))
        );
    }

    #[test]
    fn namespacing_never_shadows_a_builtin() {
        // `deck.meta` is the user's element, not Tomet's `#meta`.
        let el = element_new(Sigil::Named(Name::namespaced("deck", "meta")));
        assert_eq!(
            classify_std(&el),
            Ok(ElementKind::Custom("deck.meta".to_string()))
        );
    }

    #[test]
    fn classify_std_lenient_falls_back_for_an_unknown_bare_name() {
        let el = element_new(Sigil::named("caution"));
        assert_eq!(
            classify_std_lenient(&el),
            ElementKind::Custom("caution".to_string())
        );
    }

    #[test]
    fn type_sigil_with_builtin_name_is_recognized() {
        let el = element_new(Sigil::named("raw"));
        assert_eq!(classify_std_lenient(&el), ElementKind::Raw);
    }

    #[test]
    fn double_colon_sigil_is_recognized() {
        let el = element_new(Sigil::document("meta"));
        assert_eq!(
            classify_std_lenient(&el),
            ElementKind::Declaration(DeclKind::Meta)
        );
    }

    #[test]
    fn named_at_sigil_with_link_name_is_recognized() {
        let mut el = element_new(Sigil::named("link"));
        el.args = Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("https://example.com".to_string()),
        )]));
        assert_eq!(classify_std_lenient(&el), ElementKind::Link);
    }

    #[test]
    fn named_at_sigil_with_conflict_name_is_recognized() {
        let el = element_new(Sigil::named("conflict"));
        assert_eq!(classify_std_lenient(&el), ElementKind::Conflict);
        // Either shape, and no `[content]`/`|content` concept of its own
        // -- `a`/`b` live in `(args)` instead.
        assert_eq!(required_shape(&ElementKind::Conflict), None);
        assert_eq!(builtin_content_shape(&ElementKind::Conflict), None);
    }

    #[test]
    fn named_at_sigil_with_task_name_is_recognized() {
        let el = element_new(Sigil::named("task"));
        assert_eq!(classify_std_lenient(&el), ElementKind::Task);
        assert_eq!(required_shape(&ElementKind::Task), None);
        assert_eq!(
            builtin_content_shape(&ElementKind::Task),
            Some(ContentShape::Block)
        );
    }

    #[test]
    fn named_at_sigil_with_ruby_name_is_recognized() {
        let mut el = element_new(Sigil::named("ruby"));
        el.args = Some(Value::Map(vec![(
            "rt".to_string(),
            Value::String("かんじ".to_string()),
        )]));
        assert_eq!(classify_std_lenient(&el), ElementKind::Ruby);
        assert_eq!(required_shape(&ElementKind::Ruby), Some(Shape::Inline));
    }

    #[test]
    fn shape_mismatch_reports_a_block_element_put_in_running_text() {
        let el = element_new(Sigil::document("meta")).with_placement(Placement::Inline);
        assert_eq!(shape_mismatch(&el), Some((Shape::Inline, Shape::Block)));
    }

    #[test]
    fn shape_mismatch_reports_an_inline_element_standing_as_a_block() {
        let el = element_new(Sigil::named("em")).with_placement(Placement::Block);
        assert_eq!(shape_mismatch(&el), Some((Shape::Block, Shape::Inline)));
    }

    #[test]
    fn a_correctly_placed_element_has_no_mismatch() {
        let meta = element_new(Sigil::document("meta")).with_placement(Placement::Block);
        let em = element_new(Sigil::named("em")).with_placement(Placement::Inline);
        assert_eq!(shape_mismatch(&meta), None);
        assert_eq!(shape_mismatch(&em), None);
    }

    #[test]
    fn a_custom_element_may_take_either_shape() {
        let name = Name::namespaced("deck", "card");
        let block = element_new(Sigil::Named(name.clone())).with_placement(Placement::Block);
        let inline = element_new(Sigil::Named(name)).with_placement(Placement::Inline);
        assert_eq!(shape_mismatch(&block), None);
        assert_eq!(shape_mismatch(&inline), None);
    }

    #[test]
    fn bare_sigil_is_always_bare() {
        let el = element_new(Sigil::Bare);
        assert_eq!(classify_std_lenient(&el), ElementKind::Bare);
    }

    #[test]
    fn dollar_sigil_is_always_interp() {
        let el = element_new(Sigil::Dollar);
        assert_eq!(classify_std_lenient(&el), ElementKind::Interp);
    }
}
