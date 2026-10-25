//! Which sigils start an element in what is being read.

/// The notation the reader is reading.
///
/// Both read the same grammar and share every function. They differ in one
/// thing, which characters may open an element: `@` in both, and `?` (a hole,
/// [`tomet_ast::Sigil::Question`]) only in a [`Dialect::Pattern`].
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

impl Dialect {
    /// Whether `c`, the character under the cursor, may open an element in
    /// this dialect. A cheap first filter; `is_element_start` decides.
    pub(crate) fn opens_element(self, c: Option<char>) -> bool {
        match c {
            Some('@') => true,
            Some('?') => self == Dialect::Pattern,
            _ => false,
        }
    }
}
