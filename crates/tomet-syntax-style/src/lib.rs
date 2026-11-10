//! The choices the grammar offers among spellings that read back as the same document.
//!
//! Where [`tomet-spelling`](../tomet_spelling/index.html) states what the parser reads from a
//! given text, this crate names the alternatives the grammar leaves open. It holds which
//! alternatives exist and which constructs they apply to, never which one is chosen: that is
//! `tomet-config`'s, and writing it out is the formatter's.
//!
//! - [`GroupOrder`]: in which order an element writes its `(args)` and `[content]` groups.
//! - [`ParagraphSeparator`]: how one paragraph is set off from the next, and
//!   [`stands_like_a_paragraph`], the constructs that choice applies to.
//! - [`ValueAlignment`]: how the values of consecutive `key: value` lines are lined up.
//! - [`BracketSpacing`] and [`ColonSpacing`]: the spaces inside a heading's brackets and after the
//!   colon of an element's lone `target` argument.
//! - [`GroupLayout`]: how the entries of a `(...)` group are laid out over lines.
//! - [`ContentLayout`]: how content that runs over several lines is laid out between its brackets.

mod align;
mod content;
mod group;
mod group_order;
mod separator;
mod spacing;

pub use align::ValueAlignment;
pub use content::ContentLayout;
pub use group::GroupLayout;
pub use group_order::GroupOrder;
pub use separator::{ParagraphSeparator, stands_like_a_paragraph};
pub use spacing::{BracketSpacing, ColonSpacing};
