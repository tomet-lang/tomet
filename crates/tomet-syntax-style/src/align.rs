//! How the values of consecutive `key: value` lines are lined up.

use unicode_width::UnicodeWidthStr;

/// Lining up the values of a run of `key: value` lines: the colon stays on the key and the spaces
/// that follow it are chosen so the values start in one column.
///
/// A run is the consecutive lines whose value starts on the key's line. A blank line, a comment, or
/// an entry whose value runs over several lines ends one. Finding the runs is each writer's own
/// work, since one reads the tree and the other the text; what is shared is the rule below.
///
/// Width is display width, so a wide letter counts for two columns. A key wider than `widest_key`
/// is left out: written `key: value`, it neither widens the others nor ends the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueAlignment {
    /// The widest key, in display columns, that is lined up.
    pub widest_key: usize,
}

impl ValueAlignment {
    /// The width of the widest key of a run that is lined up: the column the values start after.
    pub fn column<'a>(&self, run: impl IntoIterator<Item = &'a str>) -> usize {
        run.into_iter()
            .map(UnicodeWidthStr::width)
            .filter(|&width| width <= self.widest_key)
            .max()
            .unwrap_or(0)
    }

    /// The spaces that follow the colon of `key`, in a run whose [`column`](Self::column) is
    /// `column`.
    pub fn padding(&self, key: &str, column: usize) -> usize {
        let width = key.width();
        if width <= self.widest_key {
            column - width + 1
        } else {
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pads(alignment: ValueAlignment, keys: &[&str]) -> Vec<usize> {
        let column = alignment.column(keys.iter().copied());
        keys.iter()
            .map(|key| alignment.padding(key, column))
            .collect()
    }

    #[test]
    fn the_values_of_a_run_start_in_one_column() {
        let alignment = ValueAlignment { widest_key: 20 };
        assert_eq!(pads(alignment, &["id", "title", "kind"]), [4, 1, 2]);
        assert_eq!(pads(alignment, &["only"]), [1]);
    }

    #[test]
    fn a_key_wider_than_the_limit_is_left_out() {
        let alignment = ValueAlignment { widest_key: 4 };
        assert_eq!(
            pads(alignment, &["id", "a_field_that_is_far_too_long", "kind"]),
            [3, 1, 1]
        );
    }

    #[test]
    fn a_wide_letter_counts_for_two_columns() {
        let alignment = ValueAlignment { widest_key: 20 };
        // `見出し` is six columns wide, so `id` is padded to it.
        assert_eq!(pads(alignment, &["見出し", "id"]), [1, 5]);
    }

    #[test]
    fn a_run_of_keys_that_are_all_too_wide_is_not_padded() {
        let alignment = ValueAlignment { widest_key: 2 };
        assert_eq!(pads(alignment, &["long", "longer"]), [1, 1]);
    }
}
