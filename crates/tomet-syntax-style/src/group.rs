//! How the entries of a `(...)` group are laid out over lines.

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
    /// first stand under it.
    pub fn render<S: AsRef<str>>(self, entries: &[S], indent: &str) -> String {
        match self {
            GroupLayout::OneLine => {
                let entries: Vec<&str> = entries.iter().map(AsRef::as_ref).collect();
                format!("({})", entries.join(", "))
            }
            GroupLayout::OnePerLine => {
                let mut out = String::from("(\n");
                for entry in entries {
                    out.push_str(&format!("{indent}  {}\n", entry.as_ref()));
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
}
