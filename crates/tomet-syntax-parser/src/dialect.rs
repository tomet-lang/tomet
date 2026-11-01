//! Which sigils start an element in what is being read.

/// The notation the reader is reading.
///
/// Both read the same grammar and share every function. They differ in one
/// thing, which characters may open an element: `@` in both, and `?` (a hole,
/// [`tomet_ast::Hole`]) only in a [`Dialect::Pattern`].
///
/// A dialect is a property of the text being read, not of a position in it,
/// so it is passed along as its own argument and not kept in the cursor.
/// Every place that asks "does an element start here" takes it, which is how
/// a place that forgets to is found by the compiler and not by a bug report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Dialect {
    /// An ordinary document. `?` is text, so prose such as `本当に?(はい)`
    /// is never a hole.
    #[default]
    Document,
    /// A pattern: a document in which `?` makes a hole.
    Pattern,
}
