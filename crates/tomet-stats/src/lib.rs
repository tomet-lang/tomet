//! Pure in-memory measurement and statistics for Tomet documents.
//!
//! This crate calculates document metrics (prose characters, words,
//! elements, headings, and internal links) directly from a parsed
//! [`Document`] AST. It performs no disk I/O, filesystem access, or database
//! queries, making it suitable for WebAssembly, mobile apps, and parallel
//! processing pipelines.
//!
//! =[ Counting rules ]
//!
//! - @strong[Characters] : Reader-visible prose text inside `[...]` and section
//!   headings, with whitespace omitted. `(args)`, `{value}`, and code block
//!   bodies (`Inline::Raw`) are data, not prose, and are not counted.
//! - @strong[Characters with whitespace] : Same reader-visible prose text, including
//!   whitespace characters.
//! - @strong[Words] : Reader-visible prose text split by Unicode whitespace.
//! - @strong[Elements] : Counts by sigil (`@name`, `${...}`, `^...`, `- ...`) and by
//!   element name.
//! - @strong[Links] : Structural internal document links (`file`, `dir`, `embed`,
//!   `tm`, `ref`). External web URLs and same-document `#id` links are not
//!   counted as workspace document links.
//! - @strong[Headings] : Count of section headings and maximum heading depth.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tomet_ast::{Block, Document, Inline, Sigil};
use tomet_semantics::{ElementKind, TargetScheme, link_target_of, path_target_of, target_scheme};

/// Internal workspace link kinds recognized by Tomet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    File,
    Dir,
    Embed,
    Tm,
    Ref,
}

impl LinkKind {
    /// All recognized link kinds in fixed order.
    pub const ALL: [LinkKind; 5] = [
        LinkKind::File,
        LinkKind::Dir,
        LinkKind::Embed,
        LinkKind::Tm,
        LinkKind::Ref,
    ];

    /// Canonical string identifier for this link kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkKind::File => "file",
            LinkKind::Dir => "dir",
            LinkKind::Embed => "embed",
            LinkKind::Tm => "tm",
            LinkKind::Ref => "ref",
        }
    }

    /// Determines the link kind from a semantic element kind and its target.
    fn from_semantic(kind: &ElementKind, target: &str) -> Option<Self> {
        match kind {
            ElementKind::Embed => Some(LinkKind::Embed),
            ElementKind::File => Some(LinkKind::File),
            ElementKind::Dir => Some(LinkKind::Dir),
            ElementKind::Link => {
                let (scheme, _) = target_scheme(target);
                match scheme {
                    TargetScheme::File => Some(LinkKind::File),
                    TargetScheme::Dir => Some(LinkKind::Dir),
                    TargetScheme::Tm => Some(LinkKind::Tm),
                    TargetScheme::Ref => Some(LinkKind::Ref),
                    TargetScheme::Url | TargetScheme::Id | TargetScheme::Unresolved => None,
                }
            }
            _ => None,
        }
    }
}

/// Structural statistics for one or more Tomet documents.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    /// Number of documents measured.
    pub files: usize,
    /// Prose characters reader sees (whitespace excluded).
    pub characters: usize,
    /// Prose characters including whitespace.
    pub characters_with_whitespace: usize,
    /// Prose word count (whitespace-separated tokens).
    pub words: usize,
    /// Named elements (`@name`).
    pub named: usize,
    /// Dynamic substitution elements (`${...}`).
    pub dollar: usize,
    /// Caret elements (`^` and `^name`).
    pub caret: usize,
    /// Bare list item elements (`- ...`).
    pub bare: usize,
    /// Frequency of named elements by their tag name.
    pub by_name: BTreeMap<String, usize>,
    /// Link counts indexed by [`LinkKind::ALL`] order.
    pub links: [usize; LinkKind::ALL.len()],
    /// Total number of section headings.
    pub headings: usize,
    /// Maximum heading level encountered.
    pub max_heading_level: usize,
}

impl Stats {
    /// Total number of elements (`named + dollar + caret + bare`).
    pub fn elements(&self) -> usize {
        self.named + self.dollar + self.caret + self.bare
    }

    /// Total number of internal links.
    pub fn total_links(&self) -> usize {
        self.links.iter().sum()
    }

    /// Count of a specific link kind.
    pub fn link_count(&self, kind: LinkKind) -> usize {
        if let Some(i) = LinkKind::ALL.iter().position(|k| *k == kind) {
            self.links[i]
        } else {
            0
        }
    }

    /// Merges another set of statistics into `self`.
    pub fn merge(&mut self, other: Stats) {
        self.files += other.files;
        self.characters += other.characters;
        self.characters_with_whitespace += other.characters_with_whitespace;
        self.words += other.words;
        self.named += other.named;
        self.dollar += other.dollar;
        self.caret += other.caret;
        self.bare += other.bare;
        for (name, n) in other.by_name {
            *self.by_name.entry(name).or_default() += n;
        }
        for (mine, theirs) in self.links.iter_mut().zip(other.links) {
            *mine += theirs;
        }
        self.headings += other.headings;
        self.max_heading_level = self.max_heading_level.max(other.max_heading_level);
    }

    /// Returns `by_name` sorted by frequency descending, then by name ascending.
    pub fn names_by_use(&self) -> Vec<(&str, usize)> {
        let mut rows: Vec<(&str, usize)> =
            self.by_name.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        rows
    }
}

/// Measures one parsed document.
pub fn measure(doc: &Document) -> Stats {
    let mut stats = Stats {
        files: 1,
        ..Stats::default()
    };

    tomet_tree::for_each_element(doc, |el| {
        match &el.sigil {
            Sigil::Named(name) => {
                stats.named += 1;
                *stats.by_name.entry(name.to_string()).or_default() += 1;
            }
            Sigil::Dollar => stats.dollar += 1,
            Sigil::Caret(_) => stats.caret += 1,
            Sigil::Bare => stats.bare += 1,
        }

        if let Some((kind, target)) = link_target_of(el).or_else(|| path_target_of(el))
            && let Some(link_kind) = LinkKind::from_semantic(&kind, &target)
            && let Some(i) = LinkKind::ALL.iter().position(|k| *k == link_kind)
        {
            stats.links[i] += 1;
        }
    });

    tomet_tree::for_each_inline(doc, |inline| {
        if let Inline::Text(text) = inline {
            stats.characters += text.value.chars().filter(|c| !c.is_whitespace()).count();
            stats.characters_with_whitespace += text.value.chars().count();
            stats.words += text.value.split_whitespace().count();
        }
    });

    count_headings(&doc.blocks, &mut stats);
    stats
}

fn count_headings(blocks: &[Block], stats: &mut Stats) {
    for block in blocks {
        if let Block::Section(section) = block {
            stats.headings += 1;
            stats.max_heading_level = stats.max_heading_level.max(section.level);
            count_headings(&section.blocks, stats);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_document() {
        let doc = tomet_parser::parse_document("").unwrap();
        let stats = measure(&doc);
        assert_eq!(stats.files, 1);
        assert_eq!(stats.characters, 0);
        assert_eq!(stats.characters_with_whitespace, 0);
        assert_eq!(stats.words, 0);
        assert_eq!(stats.elements(), 0);
        assert_eq!(stats.headings, 0);
        assert_eq!(stats.total_links(), 0);
    }

    #[test]
    fn test_prose_and_elements() {
        let input = r#"
=[ Section 1 ]
This is a paragraph with @link(readme.md)[custom text].

==[ Subsection ]
- Bare item 1
- Bare item 2
@custom_tag[Hello world]
"#;
        let doc = tomet_parser::parse_document(input).unwrap();
        let stats = measure(&doc);

        assert_eq!(stats.files, 1);
        assert_eq!(stats.headings, 2);
        assert_eq!(stats.max_heading_level, 2);
        assert_eq!(stats.bare, 2);
        assert_eq!(stats.named, 3); // @link, @custom_tag, and @ul wrapper
        assert_eq!(stats.link_count(LinkKind::File), 1);
        assert_eq!(stats.total_links(), 1);
        assert!(stats.characters > 0);
        assert!(stats.characters_with_whitespace >= stats.characters);
        assert!(stats.words > 0);
    }

    #[test]
    fn test_merge() {
        let doc1 = tomet_parser::parse_document("=[ H1 ]\nHello").unwrap();
        let doc2 = tomet_parser::parse_document("==[ H2 ]\nWorld").unwrap();

        let mut s1 = measure(&doc1);
        let s2 = measure(&doc2);
        s1.merge(s2);

        assert_eq!(s1.files, 2);
        assert_eq!(s1.headings, 2);
        assert_eq!(s1.max_heading_level, 2);
        assert_eq!(s1.words, 4); // "H1", "Hello", "H2", "World"
    }
}
