//! Heading state carried through one render: numbering, slugs and the outline.

use crate::HeadingInfo;

/// Per-document state threaded through heading rendering: the nesting
/// counters for `RenderOptions::number_headings` and the slug registry for
/// `RenderOptions::auto_slug_headings`, kept together since both are
/// "remember what came before while walking the same heading sequence".
/// The outline rides along for the same reason -- it records what those two
/// decided, as each heading is emitted.
#[derive(Default)]
pub(crate) struct HeadingState {
    pub(crate) counters: HeadingCounters,
    pub(crate) slugs: SlugTracker,
    pub(crate) outline: Vec<HeadingInfo>,
}

/// Running per-level counters for `RenderOptions::number_headings`, e.g.
/// `[1, 2]` mid-document means "currently under section 1.2". Index `i`
/// holds the count for heading level `i + 1`.
#[derive(Default)]
pub(crate) struct HeadingCounters(Vec<u32>);

impl HeadingCounters {
    /// Advances to the next heading at `level`, resetting any deeper
    /// levels' counters (a new "1.2" starts a fresh "1.2.1" for whatever
    /// level-3 heading comes next), and returns the dotted label (`"1.2"`).
    pub(crate) fn advance(&mut self, level: u8) -> String {
        let level = level.clamp(1, 6) as usize;
        if self.0.len() < level {
            self.0.resize(level, 0);
        } else {
            self.0.truncate(level);
        }
        self.0[level - 1] += 1;
        self.0
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// Hands out unique slugs for `RenderOptions::auto_slug_headings`: the
/// first heading with a given base text keeps the bare slug, each further
/// one with the same base gets `-2`, `-3`, ... appended. Only tracks slugs
/// it generated itself -- an explicit `{id:...}` elsewhere in the document
/// is never consulted or reserved, matching the "explicit id always wins,
/// auto-slugging only fills gaps" rule in `RenderOptions::auto_slug_headings`.
#[derive(Default)]
pub(crate) struct SlugTracker(std::collections::HashMap<String, u32>);

impl SlugTracker {
    pub(crate) fn slug_for(&mut self, text: &str) -> String {
        let base = slugify(text);
        let count = self.0.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            base
        } else {
            format!("{base}-{count}")
        }
    }
}

/// Lowercases and collapses runs of whitespace/punctuation into a single
/// `-`, trimming leading/trailing `-`. Keeps non-ASCII letters (e.g.
/// Japanese kanji/kana) as-is rather than stripping them -- most of this
/// project's own docs are Japanese, so an ASCII-only slugifier would
/// produce empty or near-empty ids for them.
fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut pending_dash = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.extend(c.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    slug
}
