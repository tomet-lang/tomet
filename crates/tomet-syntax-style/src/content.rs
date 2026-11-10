//! How the content of an element or an item that runs over several lines is laid out.

/// How the content of an element or an item that runs over several lines is laid out between its
/// brackets. Every layout reads back as the same document.
///
/// The content `one`, `two`, with the content padded by `  ` and the closing bracket not:
///
/// ```text
/// Expanded    Block      Box
/// [           [ one      [ one
///   one         two        two ]
///   two       ]
/// ]
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentLayout {
    /// `[` alone on its line, each line of content under it, and `]` on a line of its own.
    Expanded,
    /// The first line of content after `[ `, the others under it, and `]` on a line of its own.
    Block,
    /// The first line of content after `[ `, the others under it, and ` ]` closing the last.
    Box,
}

impl ContentLayout {
    /// Every layout, in the order they are listed to people.
    pub const ALL: [ContentLayout; 3] = [
        ContentLayout::Expanded,
        ContentLayout::Block,
        ContentLayout::Box,
    ];

    /// The name a setting writes it by.
    pub fn name(self) -> &'static str {
        match self {
            ContentLayout::Expanded => "expanded",
            ContentLayout::Block => "block",
            ContentLayout::Box => "box",
        }
    }

    /// The layout a name stands for.
    pub fn from_name(name: &str) -> Option<ContentLayout> {
        ContentLayout::ALL
            .into_iter()
            .find(|layout| layout.name() == name)
    }

    /// The text from `[` to `]` that lays `lines` out. The lines after the first stand at `pad`, and
    /// the `]` of `Expanded` and `Block` at `close`. Content of no line or of one line is written
    /// `[ ]` and `[ one ]` by `Block` and `Box`.
    pub fn render<S: AsRef<str>>(self, lines: &[S], pad: &str, close: &str) -> String {
        let mut out = String::from("[");
        match (self, lines) {
            (ContentLayout::Expanded, _) => {
                out.push('\n');
                for line in lines {
                    out.push_str(&format!("{pad}{}\n", line.as_ref()));
                }
                out.push_str(close);
                out.push(']');
            }
            (_, []) => out.push_str(" ]"),
            (_, [only]) => out.push_str(&format!(" {} ]", only.as_ref())),
            (ContentLayout::Block, [first, rest @ ..]) => {
                out.push_str(&format!(" {}\n", first.as_ref()));
                for line in rest {
                    out.push_str(&format!("{pad}{}\n", line.as_ref()));
                }
                out.push_str(close);
                out.push(']');
            }
            (ContentLayout::Box, [first, rest @ ..]) => {
                out.push_str(&format!(" {}", first.as_ref()));
                for line in rest {
                    out.push_str(&format!("\n{pad}{}", line.as_ref()));
                }
                out.push_str(" ]");
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_layout_is_laid_out_as_drawn() {
        let lines = ["one", "two"];
        assert_eq!(
            ContentLayout::Expanded.render(&lines, "  ", ""),
            "[\n  one\n  two\n]"
        );
        assert_eq!(
            ContentLayout::Block.render(&lines, "  ", ""),
            "[ one\n  two\n]"
        );
        assert_eq!(
            ContentLayout::Box.render(&lines, "  ", ""),
            "[ one\n  two ]"
        );
    }

    #[test]
    fn the_closing_bracket_stands_where_it_is_told_to() {
        let lines = ["one", "two"];
        assert_eq!(
            ContentLayout::Expanded.render(&lines, "    ", "  "),
            "[\n    one\n    two\n  ]"
        );
        assert_eq!(
            ContentLayout::Block.render(&lines, "    ", "  "),
            "[ one\n    two\n  ]"
        );
    }

    #[test]
    fn content_of_no_line_or_one_line() {
        assert_eq!(
            ContentLayout::Expanded.render::<&str>(&[], "  ", ""),
            "[\n]"
        );
        assert_eq!(
            ContentLayout::Expanded.render(&["one"], "  ", ""),
            "[\n  one\n]"
        );
        for layout in [ContentLayout::Block, ContentLayout::Box] {
            assert_eq!(layout.render::<&str>(&[], "  ", ""), "[ ]");
            assert_eq!(layout.render(&["one"], "  ", ""), "[ one ]");
        }
    }

    #[test]
    fn a_layout_is_known_by_its_name() {
        for layout in ContentLayout::ALL {
            assert_eq!(ContentLayout::from_name(layout.name()), Some(layout));
        }
        assert_eq!(ContentLayout::from_name("quote"), None);
    }
}
