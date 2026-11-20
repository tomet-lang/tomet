//! How the entries of a `(...)` group are laid out over lines.

use crate::MultilineComma;

/// How the entries of a `(...)` group, such as the fields of `::meta(...)`, are laid out. Both
/// read back as the same document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupLayout {
    /// `(a: 1, b: 2)`.
    OneLine,
    /// The entries one to a line, two columns in, and the `)` on a line of its own.
    OnePerLine,
}

impl GroupLayout {
    /// The group of `entries`, from `(` to `)`, in a place indented by `indent`: the lines after the
    /// first stand under it, with no line-ending commas when laid out one per line.
    pub fn render<S: AsRef<str>>(self, entries: &[S], indent: &str) -> String {
        self.render_with_comma(entries, indent, MultilineComma::Never)
    }

    /// The group of `entries`, from `(` to `)`, in a place indented by `indent`, writing
    /// line-ending commas according to `comma` when laid out [`GroupLayout::OnePerLine`].
    pub fn render_with_comma<S: AsRef<str>>(
        self,
        entries: &[S],
        indent: &str,
        comma: MultilineComma,
    ) -> String {
        match self {
            GroupLayout::OneLine => {
                let entries: Vec<&str> = entries.iter().map(AsRef::as_ref).collect();
                format!("({})", entries.join(", "))
            }
            GroupLayout::OnePerLine => {
                let mut out = String::from("(\n");
                let total = entries.len();
                for (i, entry) in entries.iter().enumerate() {
                    let suffix = if comma.trailing_for(i, total) {
                        ","
                    } else {
                        ""
                    };
                    out.push_str(&format!("{indent}  {}{suffix}\n", entry.as_ref()));
                }
                out.push_str(indent);
                out.push(')');
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_is_on_one_line_or_one_entry_to_a_line() {
        let entries = ["a: 1", "b: 2"];
        assert_eq!(GroupLayout::OneLine.render(&entries, ""), "(a: 1, b: 2)");
        assert_eq!(
            GroupLayout::OnePerLine.render(&entries, ""),
            "(\n  a: 1\n  b: 2\n)"
        );
        assert_eq!(
            GroupLayout::OnePerLine.render(&entries, "  "),
            "(\n    a: 1\n    b: 2\n  )"
        );
    }

    #[test]
    fn one_per_line_respects_multiline_comma_variants() {
        let entries = ["a: 1", "b: 2"];
        assert_eq!(
            GroupLayout::OnePerLine.render_with_comma(&entries, "", MultilineComma::Never),
            "(\n  a: 1\n  b: 2\n)"
        );
        assert_eq!(
            GroupLayout::OnePerLine.render_with_comma(&entries, "", MultilineComma::Between),
            "(\n  a: 1,\n  b: 2\n)"
        );
        assert_eq!(
            GroupLayout::OnePerLine.render_with_comma(&entries, "", MultilineComma::Always),
            "(\n  a: 1,\n  b: 2,\n)"
        );
        assert_eq!(
            GroupLayout::OneLine.render_with_comma(&entries, "", MultilineComma::Always),
            "(a: 1, b: 2)"
        );
    }
}
