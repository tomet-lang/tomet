//! =[ tomet-ast ]
//!
//! Shared AST and `Value` data models for the Tomet toolchain ([`File`], [`Block`],
//! [`Inline`], [`Element`], [`ElementView`], [`BodyView`], [`Declaration`], [`Interpolation`],
//! [`Hole`], [`TypeSeat`], [`ListItem`], [`Value`], [`Tuple`], [`TupleEntry`], [`Span`](tomet_span::Span)).
//! A [`Value`] holds a [`Scalar`](tomet_scalar::Scalar) for scalar data. [`PlainValue`] is a [`Value`] as data and nothing else.
//!
//! =[ Design Policy ]
//!
//! - (Pure Data Models & Structural Accessors Only)
//!   | `tomet-ast` is the foundational AST crate referenced across the workspace,
//!   | depending only on `tomet-scalar`, `tomet-span`, `tomet-spelling`, `serde`, and `unicode-width` (and `schemars`, if asked).
//!   | It contains only struct/enum definitions and pure inherent methods for construction
//!   | (`new`, `with_*`) and structural inspection (`as_str()`, `get()`, `is_none()`).
//! - (Two Serde Forms, For Two Readers)
//!   | Every node is plain `#[derive(Serialize, Deserialize)]`, and that derived form is the AST wire:
//!   | the shape a document takes when it is handed to another process and handed back. It is lossless
//!   | and reads back as exactly what was written, including a [`Value`] (an externally tagged enum) and a
//!   | [`Tuple`] (its entries in order, each with the kind of its key). There is no hand-written serde
//!   | in this crate for the wire, because a wire that is hand-written is a wire that is lossy where
//!   | the author did not look.
//!   | A reader that wants data, not the AST, has a second form that says so by its type:
//!   | [`Value::to_plain`] gives a [`PlainValue`] (`null`, booleans, numbers, strings, arrays, objects),
//!   | which is lossy on purpose, does not read back into a [`Value`], and refuses (as [`NotPlain`])
//!   | the nodes that are not data. Which form a consumer wants is its choice, never a flag.
//! - (The Schema Is Derived, Behind A Feature)
//!   | With the `schema` feature every node (and [`PlainValue`]) also derives `schemars::JsonSchema`, so the JSON Schema
//!   | of the wire is the shape serde writes and not a second description of it; `tomet spec ast-schema` generates it.
//!   | The feature is off by default, so nothing that parses or prints compiles `schemars`. A doc comment on a type is
//!   | that type's description in the schema: write it for a reader of JSON, and put what only a Rust reader needs
//!   | under a `=[ In Rust ]` heading, which the schema leaves out.
//! - (A Node's Role Is Its Type)
//!   | A `{$ ... }`, `$(...)` or `$name(...)` is an [`Interpolation`] (an expression and a span), not an [`Element`]:
//!   | it has no name, no groups and no connects, so it is a variant of [`Block`],
//!   | [`Inline`] and [`Value`] and no walker over elements ever meets one. Reach every
//!   | interpolation of a file through `tomet_tree::for_each_interpolation`, and every type seat
//!   | through `tomet_tree::for_each_type_seat`.
//!   | A `::name(...)` is a [`Declaration`]: sits in the file preamble header ([`File::header`]),
//!   | about the file and attached to nothing, so it has no placement and takes no connect.
//!   | A `::name(...)` is a [`Connect`]: metadata about its owner, with no id, no connects of its own
//!   | and no placement. A `<?matcher(...)>` is a [`Hole`] (a matcher, optional, in place of a name).
//!   | A `<$type(...)>` is a [`TypeSeat`]: the type as the expression it is written (`uuid`,
//!   | `list(string, length: 6)`), and nothing else; it is not a [`Hole`], which has a matcher and
//!   | groups, and not an [`Interpolation`], which is evaluated where it stands. `tomet-ast` never
//!   | says which type names are valid, nor what `auto` in `default: auto` is: those are
//!   | `tomet-type`'s and the semantics'.
//!   | `<...>` is one family in the *syntax* (the CST has one `SLOT` node, whatever the mark inside); the
//!   | AST has no node for the family. It names nodes by role, never by spelling, and has a node only
//!   | for what tomet interprets or what carries structure to walk: a hole is a [`Hole`], a type seat
//!   | a [`TypeSeat`], a name tag (`<PATH>`) is text. Merging nodes of different structure into one
//!   | type with a field to tell them apart is what split `Element` before; a new kind of seat is
//!   | judged by that rule.
//!   | A list item is a [`ListItem`]: it has no name, so it is not an [`Element`], whose name is
//!   | mandatory.
//!   | [`ListItem::as_element_view`] gives a zero-copy [`ElementView`] of the element a `-@name(...)`
//!   | item is.
//!   | What nodes share is [`Data`] (`(args)` and the `{...}` [`Group`]), and their body groups can
//!   | be inspected uniformly via [`BodyView`]; `[content]` and `#(id)` stay on the roles that take them.
//! - (Clear Crate Boundaries)
//!   | Semantics: Classification of element meanings (links, `::meta`, `::config`) lives in `tomet-semantics`.
//!   | Traversal & Manipulation: Generic tree traversal algorithms live in `tomet-tree`.
//!   | Formatting & Printing: Serialization to `.tmt` text lives in `tomet-printer`; how a key and a string are spelled, in `tomet-spelling`.
//!   | Validation: Field formatting and lint validation live in `tomet-validator`.
//! - (Span Equality Contract)
//!   | [`Span`](tomet_span::Span) implements a custom `PartialEq` that always returns `true` so that structural
//!   | equality comparisons (`assert_eq!(doc1, doc2)`) ignore source positions. Use
//!   | [`Span::exact_eq()`](tomet_span::Span::exact_eq) when exact source position comparisons are required.

#![allow(clippy::large_enum_variant)]

mod document;
mod interpolation;
mod nexus;
mod plain;
mod value;

pub use document::{
    Block, File, Inline, LineBreak, List, ListItem, Paragraph, RawText, Section, SoftBreak, Text,
    softbreak_join,
};
pub use interpolation::{
    BinaryOp, InterpExpr, InterpExprKind, InterpSpelling, Interpolation, TypeSeat,
};
pub use nexus::{
    BodyView, Connect, Data, Declaration, Element, ElementView, Entry, Group, Hole, Id, Name,
    Placement,
};
pub use plain::{NotPlain, NotPlainReason, PlainValue};
pub use value::{DuplicateKey, EntryKey, Tuple, TupleEntry, Value};
