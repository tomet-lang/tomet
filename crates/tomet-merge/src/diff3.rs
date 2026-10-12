//! A generic three-way merge over any `PartialEq` slice, independent of
//! `tomet_ast`. This is the primitive `merge()` in `lib.rs` applies at
//! each level of the tree (`Document.blocks`, `Element.content`, ...);
//! it is tested here against plain `char`/`i32` fixtures precisely so
//! those tests don't also have to construct AST nodes to exercise the
//! alignment logic.
//!
//! This is the same algorithm GNU `diff3 -m` uses: diff `base` against
//! each side independently, then merge the two sets of changed regions
//! ("hunks"). Two hunks that touch disjoint stretches of `base` apply
//! independently, in base order. Two hunks that touch overlapping
//! stretches are the same disagreement and are combined into one
//! conflict, with `mine`/`theirs` each being the concatenation of every
//! hunk from that side inside the combined region -- concatenation,
//! not just the one hunk, because merging overlapping hunks can chain
//! (a hunk from `local` can overlap two separate `remote` hunks that
//! were not adjacent to each other before `local`'s hunk pulled them
//! into the same group).

use std::ops::Range;

/// One contiguous stretch of `base` that a side (`local` or `remote`)
/// replaced with different content. `base_range` may be empty (a pure
/// insertion at that position) and `content` may be empty (a pure
/// deletion).
#[derive(Debug, Clone, PartialEq)]
struct Hunk<T> {
    base_range: Range<usize>,
    content: Vec<T>,
}

/// The outcome of merging one stretch of the sequence.
#[derive(Debug, Clone, PartialEq)]
pub enum Region<T> {
    /// Neither side touched this stretch of `base` (or both sides
    /// changed it to the same thing) -- here is the agreed content.
    Same(Vec<T>),
    /// `local` and `remote` changed the same stretch of `base`
    /// differently. `mine` is what `local` made of it, `theirs` is what
    /// `remote` made of it; `base` is kept too, since a caller may want
    /// to show what the disagreement started from.
    Conflict {
        base: Vec<T>,
        mine: Vec<T>,
        theirs: Vec<T>,
    },
}

/// Merges `local` and `remote`, both descended from `base`, into an
/// ordered list of regions covering the whole merged sequence.
pub fn merge3<T: PartialEq + Clone>(base: &[T], local: &[T], remote: &[T]) -> Vec<Region<T>> {
    let local_hunks = diff_hunks(base, local);
    let remote_hunks = diff_hunks(base, remote);
    let groups = group_overlapping(base.len(), &local_hunks, &remote_hunks);

    let mut regions = Vec::with_capacity(groups.len());
    let mut cursor = 0;
    for group in groups {
        if group.base_range.start > cursor {
            // Nothing touched this gap: copy straight from base.
            regions.push(Region::Same(base[cursor..group.base_range.start].to_vec()));
        }

        let mine = concat_content(base, &group.base_range, &local_hunks, &group.local_idx);
        let theirs = concat_content(base, &group.base_range, &remote_hunks, &group.remote_idx);

        if group.local_idx.is_empty() {
            // Only remote touched this stretch.
            regions.push(Region::Same(theirs));
        } else if group.remote_idx.is_empty() {
            // Only local touched this stretch.
            regions.push(Region::Same(mine));
        } else if mine == theirs {
            // Both changed it, but to the same thing.
            regions.push(Region::Same(mine));
        } else {
            regions.push(Region::Conflict {
                base: base[group.base_range.clone()].to_vec(),
                mine,
                theirs,
            });
        }

        cursor = group.base_range.end;
    }
    if cursor < base.len() {
        regions.push(Region::Same(base[cursor..].to_vec()));
    }
    coalesce_same(regions)
}

/// Merges adjacent `Same` regions into one. The main loop above emits a
/// separate `Same` for each untouched gap and for each group only one
/// side touched, so a stretch with no conflicts nearby would otherwise
/// come back needlessly fragmented.
fn coalesce_same<T>(regions: Vec<Region<T>>) -> Vec<Region<T>> {
    let mut out: Vec<Region<T>> = Vec::with_capacity(regions.len());
    for r in regions {
        match r {
            Region::Same(mut items) => match out.last_mut() {
                Some(Region::Same(prev)) => prev.append(&mut items),
                _ => out.push(Region::Same(items)),
            },
            other => out.push(other),
        }
    }
    out
}

/// For a merged group spanning `range`, the content a side contributed:
/// each of that side's hunks inside the group, in base order, with the
/// untouched gaps *inside* the group (base content neither hunk on this
/// side covers, because it was only the other side that pulled this
/// stretch into the group) copied through from `base`.
fn concat_content<T: Clone>(
    base: &[T],
    range: &Range<usize>,
    hunks: &[Hunk<T>],
    idxs: &[usize],
) -> Vec<T> {
    let mut out = Vec::new();
    let mut cursor = range.start;
    for &i in idxs {
        let h = &hunks[i];
        if h.base_range.start > cursor {
            out.extend_from_slice(&base[cursor..h.base_range.start]);
        }
        out.extend_from_slice(&h.content);
        cursor = h.base_range.end;
    }
    if cursor < range.end {
        out.extend_from_slice(&base[cursor..range.end]);
    }
    out
}

/// One merged group: the union `base_range` of every hunk pulled into
/// it, and which hunks (by index into `local_hunks`/`remote_hunks`)
/// landed inside.
struct Group {
    base_range: Range<usize>,
    local_idx: Vec<usize>,
    remote_idx: Vec<usize>,
}

/// Whether two base-index ranges represent the same disagreement.
///
/// Two hunks that are merely *adjacent* (one ends exactly where the other
/// begins) are independent edits and must not be joined -- `"ab"` ->
/// local `"Xb"` (hunk `0..1`) and remote `"aY"` (hunk `1..2`) both apply
/// cleanly to give `"XY"`, the same way two adjacent-but-different line
/// edits merge cleanly in an ordinary text `diff3`. Only a real overlap
/// (sharing at least one base index) is a conflict.
///
/// The one exception is two *insertions* (empty ranges) at the exact
/// same position: those have no base index to share, since neither
/// consumes any base content, but they are still the same disagreement
/// ("what, if anything, goes in this gap") and must be joined -- see
/// `pure_appends_at_the_same_end_do_not_conflict`.
fn touches(a: &Range<usize>, b: &Range<usize>) -> bool {
    if a.start == a.end && b.start == b.end {
        a.start == b.start
    } else {
        a.start < b.end && b.start < a.end
    }
}

/// Merges overlapping hunks from both sides into groups, transitively: a
/// hunk from `local` can pull two `remote` hunks into one group even if
/// those two `remote` hunks don't overlap each other, because both
/// overlap the same `local` hunk.
fn group_overlapping<T>(base_len: usize, local: &[Hunk<T>], remote: &[Hunk<T>]) -> Vec<Group> {
    #[derive(Clone, Copy)]
    enum Side {
        Local(usize),
        Remote(usize),
    }

    let mut tagged: Vec<(Range<usize>, Side)> = local
        .iter()
        .enumerate()
        .map(|(i, h)| (h.base_range.clone(), Side::Local(i)))
        .chain(
            remote
                .iter()
                .enumerate()
                .map(|(i, h)| (h.base_range.clone(), Side::Remote(i))),
        )
        .collect();
    tagged.sort_by_key(|(r, _)| (r.start, r.end));

    let mut groups: Vec<Group> = Vec::new();
    for (range, side) in tagged {
        let joinable = groups.last_mut().filter(|g| touches(&range, &g.base_range));

        let group = if let Some(g) = joinable {
            g.base_range = g.base_range.start.min(range.start)..g.base_range.end.max(range.end);
            g
        } else {
            groups.push(Group {
                base_range: range,
                local_idx: Vec::new(),
                remote_idx: Vec::new(),
            });
            groups.last_mut().unwrap()
        };

        match side {
            Side::Local(i) => group.local_idx.push(i),
            Side::Remote(i) => group.remote_idx.push(i),
        }
    }
    debug_assert!(groups.iter().all(|g| g.base_range.end <= base_len));
    groups
}

/// The hunks turning `base` into `derived`: every stretch of `base` an
/// LCS alignment did *not* match, paired with whatever `derived`
/// content sits in the corresponding gap.
fn diff_hunks<T: PartialEq + Clone>(base: &[T], derived: &[T]) -> Vec<Hunk<T>> {
    let matches = lcs_matches(base, derived);

    let mut hunks = Vec::new();
    let mut base_cursor = 0;
    let mut derived_cursor = 0;
    for m in matches.iter().chain(std::iter::once(&MatchBlock {
        base_start: base.len(),
        derived_start: derived.len(),
        len: 0,
    })) {
        if m.base_start > base_cursor || m.derived_start > derived_cursor {
            hunks.push(Hunk {
                base_range: base_cursor..m.base_start,
                content: derived[derived_cursor..m.derived_start].to_vec(),
            });
        }
        base_cursor = m.base_start + m.len;
        derived_cursor = m.derived_start + m.len;
    }
    hunks
}

struct MatchBlock {
    base_start: usize,
    derived_start: usize,
    len: usize,
}

/// Maximal matching runs between `base` and `derived`, in order, via a
/// textbook LCS dynamic-programming table. `O(n*m)`, which is fine for
/// note-sized documents; this is not meant for diffing megabyte files.
fn lcs_matches<T: PartialEq>(base: &[T], derived: &[T]) -> Vec<MatchBlock> {
    let (n, m) = (base.len(), derived.len());
    // Flat row-major DP table: `dp[i * stride + j]` == LCS length of
    // `base[..i]` and `derived[..j]`.
    let stride = m + 1;
    let mut dp = vec![0u32; (n + 1) * stride];

    for i in 1..=n {
        for j in 1..=m {
            dp[i * stride + j] = if base[i - 1] == derived[j - 1] {
                dp[(i - 1) * stride + (j - 1)] + 1
            } else {
                dp[(i - 1) * stride + j].max(dp[i * stride + (j - 1)])
            };
        }
    }

    // Backtrack from (n, m) to collect matched index pairs, then reverse
    // and coalesce adjacent pairs into runs.
    let mut pairs = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        if base[i - 1] == derived[j - 1] {
            pairs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[(i - 1) * stride + j] >= dp[i * stride + (j - 1)] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs.reverse();

    let mut blocks: Vec<MatchBlock> = Vec::new();
    for (bi, di) in pairs {
        if let Some(last) = blocks.last_mut() {
            if last.base_start + last.len == bi && last.derived_start + last.len == di {
                last.len += 1;
                continue;
            }
        }
        blocks.push(MatchBlock {
            base_start: bi,
            derived_start: di,
            len: 1,
        });
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn identical_everywhere_is_all_same() {
        let base = v("abc");
        let regions = merge3(&base, &base, &base);
        assert_eq!(regions, vec![Region::Same(v("abc"))]);
    }

    #[test]
    fn only_local_changed_is_taken_cleanly() {
        let base = v("abc");
        let local = v("axc");
        let remote = v("abc");
        let regions = merge3(&base, &local, &remote);
        assert_eq!(regions, vec![Region::Same(v("axc"))]);
    }

    #[test]
    fn only_remote_changed_is_taken_cleanly() {
        let base = v("abc");
        let local = v("abc");
        let remote = v("abz");
        let regions = merge3(&base, &local, &remote);
        assert_eq!(regions, vec![Region::Same(v("abz"))]);
    }

    #[test]
    fn disjoint_edits_both_apply() {
        // local edits the front, remote edits the back -- both should
        // land, with the untouched middle copied from base.
        let base = v("aXbXc");
        let local = v("LXbXc");
        let remote = v("aXbXR");
        let regions = merge3(&base, &local, &remote);
        let flat: Vec<char> = regions
            .into_iter()
            .flat_map(|r| match r {
                Region::Same(v) => v,
                Region::Conflict { .. } => panic!("expected no conflict"),
            })
            .collect();
        assert_eq!(flat, v("LXbXR"));
    }

    #[test]
    fn same_edit_on_both_sides_is_not_a_conflict() {
        let base = v("abc");
        let local = v("azc");
        let remote = v("azc");
        let regions = merge3(&base, &local, &remote);
        assert_eq!(regions, vec![Region::Same(v("azc"))]);
    }

    #[test]
    fn overlapping_different_edits_conflict() {
        let base = v("abc");
        let local = v("axc");
        let remote = v("ayc");
        let regions = merge3(&base, &local, &remote);
        assert_eq!(
            regions,
            vec![
                Region::Same(v("a")),
                Region::Conflict {
                    base: v("b"),
                    mine: v("x"),
                    theirs: v("y"),
                },
                Region::Same(v("c")),
            ]
        );
    }

    #[test]
    fn pure_appends_at_the_same_end_do_not_conflict() {
        // Two independent insertions at the very same gap (end of the
        // sequence) merge by concatenation rather than being flagged --
        // this is the "two people added a line at the end" case.
        let base = v("ab");
        let local = v("abc");
        let remote = v("abd");
        let regions = merge3(&base, &local, &remote);
        // They touch the same (empty) base range, so they land in one
        // group; since the inserted content differs, that group is a
        // real conflict rather than a silent pick.
        assert_eq!(
            regions,
            vec![
                Region::Same(v("ab")),
                Region::Conflict {
                    base: v(""),
                    mine: v("c"),
                    theirs: v("d"),
                },
            ]
        );
    }

    #[test]
    fn chained_overlap_pulls_two_remote_hunks_into_one_group() {
        // local replaces the whole middle stretch that remote had split
        // into two separate, non-adjacent edits -- both remote hunks
        // must end up in the same conflict group as local's one hunk.
        let base = v("aXbXcXd");
        let local = v("aZZZZZd");
        let remote = v("aYbXcZd");
        let regions = merge3(&base, &local, &remote);
        assert_eq!(
            regions,
            vec![
                Region::Same(v("a")),
                Region::Conflict {
                    base: v("XbXcX"),
                    mine: v("ZZZZZ"),
                    theirs: v("YbXcZ"),
                },
                Region::Same(v("d")),
            ]
        );
    }
}
