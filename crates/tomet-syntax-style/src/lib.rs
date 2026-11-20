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
//! - [`SectionTitleStyle`]: how the title of a section heading is written (`bracketed` or `bare`).
//! - [`BracketSpacing`] and [`ColonSpacing`]: the spaces inside a section heading's brackets and after the
//!   colon of an element's lone `target` argument.
//! - [`GroupLayout`]: how the entries of a `(...)` group are laid out over lines.
//! - [`MultilineComma`]: how commas are written on the entries of a multi-line `(...)` group.
//! - [`ContentLayout`]: how content that runs over several lines is laid out between its brackets.
//! - [`FenceLines`]: how the body lines of a fenced code block are written (`Marked` or `Bare`).

mod align;
mod comma;
mod content;
mod fence;
mod group;
mod group_order;
mod section;
mod separator;
mod spacing;

pub use align::ValueAlignment;
pub use comma::MultilineComma;
pub use content::ContentLayout;
pub use fence::FenceLines;
pub use group::GroupLayout;
pub use group_order::GroupOrder;
pub use section::SectionTitleStyle;
pub use separator::{ParagraphSeparator, stands_like_a_paragraph};
pub use spacing::{BracketSpacing, ColonSpacing};
