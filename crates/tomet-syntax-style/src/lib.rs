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

mod group_order;
mod separator;

pub use group_order::GroupOrder;
pub use separator::{ParagraphSeparator, stands_like_a_paragraph};
