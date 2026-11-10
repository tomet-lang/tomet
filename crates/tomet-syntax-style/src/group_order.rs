//! In which order an element writes its `(args)` and `[content]` groups.

/// The order of an element's `(args)` and `[content]`. Both read back as the same element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupOrder {
    #[default]
    ArgsFirst, // @link(args)[content]{value}
    ContentFirst, // @link[content](args){value}
}
