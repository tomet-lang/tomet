//! Format-preserving three-way merge and conflict resolution for Tomet
//! documents, built on [`tomet_edit::EditDoc`].
//!
//! When the same note has been edited in two places since the last sync,
//! [`merge`] produces a single valid `.tmt` document that combines both
//! sets of changes where they do not overlap, and embeds `@conflict`
//! markers where they do, while preserving untouched formatting, heading
//! padding, blank lines, inline sugar (`*italic*`, `**bold**`, `` `code` ``),
//! comments (`//`, `/* */`), `{ ... }` brace spacing, and fenced code blocks
//! byte-for-byte.
//!
//! # Algorithm
//!
//! A conflict is narrowed as far down the CST/EditDoc hierarchy as the three
//! versions' structure allows before giving up and embedding a marker:
//!
//! 1. **Block sequence (`ChildItem`s)** — [`diff3`] aligns top-level blocks,
//!    section children, and element `[content]` blocks. Non-overlapping
//!    insertions, deletions, and replacements merge cleanly, preserving
//!    original trivia and comments.
//! 2. **Single-block narrowing** — when a conflict region has equal block
//!    counts on `base`, `local`, and `remote`, each triple is narrowed:
//!    - three `Paragraph`s -> inline item diff, then word/whitespace/punct
//!      diff on conflicting plain text runs -> inline
//!      `@conflict(a: [ ... ], b: [ ... ])`;
//!    - three `Section`s of the same level and `#(id)` -> narrow heading
//!      inlines and section children independently;
//!    - three `BlockElement`s with the same sigil and `#(id)` -> narrow
//!      `(args)`, `[content]`, and `{data}` slots independently;
//!    - three `List`s / `ListItem`s -> narrow list children or item text;
//!    - two `{data}` / `(args)` maps -> key-by-key three-way merge preserving
//!      untouched entry slices and brace padding, narrowing conflicting
//!      values to `@conflict(a: <local>, b: <remote>)`.
//! 3. **Block-level fallback** — when block kinds or counts disagree,
//!    emit a block-level `@conflict(a: [ ... ], b: [ ... ])` element.

mod diff3;
mod find_conflicts;
mod markers;
mod merge;
mod resolve;

pub use find_conflicts::{ConflictItem, find_conflicts, find_conflicts_doc, has_conflicts, has_conflicts_doc};
pub use markers::CONFLICT_MARKER;
pub use merge::{MergeResult, merge, merge_doc};
pub use resolve::{
    ConflictResolution, resolve, resolve_all, resolve_all_doc, resolve_doc, resolve_single,
    resolve_single_doc,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_edits_merge_cleanly() {
        let base = "=[ Title ]\n\nFirst paragraph.\n\nSecond paragraph.\n";
        let local = "=[ Title ]\n\nFirst paragraph, edited locally.\n\nSecond paragraph.\n";
        let remote = "=[ Title ]\n\nFirst paragraph.\n\nSecond paragraph, edited remotely.\n";

        let out = merge(base, local, remote);
        assert!(out.clean);
        assert!(!has_conflicts(&out.text));
        assert_eq!(
            out.text,
            "=[ Title ]\n\nFirst paragraph, edited locally.\n\nSecond paragraph, edited remotely.\n"
        );
    }

    #[test]
    fn edits_to_different_words_in_one_sentence_merge_cleanly() {
        let base = "The quick brown fox jumps over the lazy dog.\n";
        let local = "The slow brown fox jumps over the lazy dog.\n";
        let remote = "The quick brown fox jumps over the sleepy dog.\n";

        let out = merge(base, local, remote);
        assert!(out.clean);
        assert!(!has_conflicts(&out.text));
        assert_eq!(out.text, "The slow brown fox jumps over the sleepy dog.\n");
    }

    #[test]
    fn edits_to_the_same_word_produce_an_inline_conflict() {
        let base = "The quick brown fox.\n";
        let local = "The slow brown fox.\n";
        let remote = "The fast brown fox.\n";

        let out = merge(base, local, remote);
        assert!(!out.clean);
        assert!(has_conflicts(&out.text));
        assert_eq!(
            out.text,
            "The @conflict(a: [ slow ], b: [ fast ]) brown fox.\n"
        );
    }

    #[test]
    fn resolving_an_inline_conflict_leaves_one_plain_sentence() {
        let base = "The quick brown fox.\n";
        let local = "The slow brown fox.\n";
        let remote = "The fast brown fox.\n";
        let merged = merge(base, local, remote);

        let a = resolve(&merged.text, &[ConflictResolution::A]);
        assert!(!has_conflicts(&a));
        assert_eq!(a, "The slow brown fox.\n");

        let b = resolve(&merged.text, &[ConflictResolution::B]);
        assert!(!has_conflicts(&b));
        assert_eq!(b, "The fast brown fox.\n");

        let custom = resolve(
            &merged.text,
            &[ConflictResolution::Rewritten("agile".into())],
        );
        assert!(!has_conflicts(&custom));
        assert_eq!(custom, "The agile brown fox.\n");
    }

    #[test]
    fn edits_to_different_data_keys_merge_cleanly() {
        let base = "::meta{\n  title: \"Old\"\n  status: \"draft\"\n}\n\nBody.\n";
        let local = "::meta{\n  title: \"New\"\n  status: \"draft\"\n}\n\nBody.\n";
        let remote = "::meta{\n  title: \"Old\"\n  status: \"published\"\n  tag: \"v1\"\n}\n\nBody.\n";

        let out = merge(base, local, remote);
        assert!(out.clean);
        assert!(!has_conflicts(&out.text));
        assert_eq!(
            out.text,
            "::meta{\n  title: \"New\"\n  status: \"published\"\n  tag: \"v1\"\n}\n\nBody.\n"
        );
    }

    #[test]
    fn a_key_deleted_on_one_side_and_untouched_on_the_other_is_dropped() {
        let base = "::meta{\n  keep: 1\n  drop_me: 2\n}\n";
        let local = "::meta{\n  keep: 1\n}\n";
        let remote = "::meta{\n  keep: 10\n  drop_me: 2\n}\n";

        let out = merge(base, local, remote);
        assert!(out.clean);
        assert_eq!(out.text, "::meta{\n  keep: 10\n}\n");
    }

    #[test]
    fn a_key_deleted_on_one_side_and_modified_on_the_other_keeps_the_edit() {
        let base = "::meta{\n  k: \"v0\"\n}\n";
        let local = "::meta{\n  other: 1\n}\n";
        let remote = "::meta{\n  k: \"v1\"\n  other: 1\n}\n";

        let out = merge(base, local, remote);
        assert!(out.clean);
        assert_eq!(out.text, "::meta{\n  other: 1\n  k: \"v1\"\n}\n");
    }

    #[test]
    fn editing_the_same_data_key_produces_a_value_conflict() {
        let base = "::meta{\n  title: \"Base\"\n  keep: 1\n}\n";
        let local = "::meta{\n  title: \"Local\"\n  keep: 1\n}\n";
        let remote = "::meta{\n  title: \"Remote\"\n  keep: 1\n}\n";

        let out = merge(base, local, remote);
        assert!(!out.clean);
        assert!(has_conflicts(&out.text));
        assert_eq!(
            out.text,
            "::meta{\n  title: @conflict(a: \"Local\", b: \"Remote\")\n  keep: 1\n}\n"
        );
    }

    #[test]
    fn resolving_a_data_key_conflict_leaves_a_plain_value() {
        let base = "::meta{\n  title: \"Base\"\n}\n";
        let local = "::meta{\n  title: \"Local\"\n}\n";
        let remote = "::meta{\n  title: \"Remote\"\n}\n";
        let merged = merge(base, local, remote);

        let picked_a = resolve(&merged.text, &[ConflictResolution::A]);
        assert!(!has_conflicts(&picked_a));
        assert_eq!(picked_a, "::meta{\n  title: \"Local\"\n}\n");

        let picked_b = resolve(&merged.text, &[ConflictResolution::B]);
        assert!(!has_conflicts(&picked_b));
        assert_eq!(picked_b, "::meta{\n  title: \"Remote\"\n}\n");

        let rewritten = resolve(
            &merged.text,
            &[ConflictResolution::Rewritten("Both".into())],
        );
        assert!(!has_conflicts(&rewritten));
        assert_eq!(rewritten, "::meta{\n  title: \"Both\"\n}\n");
    }

    #[test]
    fn mismatched_block_kinds_fall_back_to_block_level_markers() {
        let base = "Original paragraph.\n";
        let local = "::kind(note)\n";
        let remote = "Replaced with a different paragraph.\n";

        let out = merge(base, local, remote);
        assert!(!out.clean);
        assert!(has_conflicts(&out.text));
        assert_eq!(
            out.text,
            concat!(
                "@conflict(\n",
                "  a: [\n",
                "    ::kind(note)\n",
                "  ]\n",
                "  b: [ Replaced with a different paragraph. ]\n",
                ")\n",
            )
        );
    }

    #[test]
    fn resolve_a_keeps_local() {
        let base = "Original.\n";
        let local = "::kind(local)\n";
        let remote = "Remote.\n";
        let merged = merge(base, local, remote);

        let resolved = resolve(&merged.text, &[ConflictResolution::A]);
        assert!(!has_conflicts(&resolved));
        assert_eq!(resolved, local);
    }

    #[test]
    fn resolve_b_keeps_remote() {
        let base = "Original.\n";
        let local = "::kind(local)\n";
        let remote = "Remote.\n";
        let merged = merge(base, local, remote);

        let resolved = resolve(&merged.text, &[ConflictResolution::B]);
        assert!(!has_conflicts(&resolved));
        assert_eq!(resolved, remote);
    }

    #[test]
    fn resolving_one_conflict_leaves_another_untouched() {
        let base = "First.\n\nSecond.\n";
        let local = "::kind(a1)\n\n::kind(a2)\n";
        let remote = "B1.\n\nB2.\n";
        let merged = merge(base, local, remote);

        let half = resolve(&merged.text, &[ConflictResolution::A]);
        assert!(has_conflicts(&half));

        let done = resolve(&half, &[ConflictResolution::B]);
        assert!(!has_conflicts(&done));
        assert_eq!(done, "::kind(a1)\n\nB2.\n");
    }

    #[test]
    fn resolve_all_resolves_every_conflict() {
        let base = "First.\n\nSecond.\n";
        let local = "::kind(a1)\n\n::kind(a2)\n";
        let remote = "B1.\n\nB2.\n";
        let merged = merge(base, local, remote);

        let all_a = resolve_all(&merged.text, ConflictResolution::A);
        assert!(!has_conflicts(&all_a));
        assert_eq!(all_a, local);

        let all_b = resolve_all(&merged.text, ConflictResolution::B);
        assert!(!has_conflicts(&all_b));
        assert_eq!(all_b, remote);
    }

    #[test]
    fn find_conflicts_and_resolve_single_and_both() {
        let base = "::meta{\n  title: \"Base\"\n}\n\nHello world.\n";
        let local = "::meta{\n  title: \"Local\"\n}\n\nHello there.\n";
        let remote = "::meta{\n  title: \"Remote\"\n}\n\nHello friend.\n";
        let merged = merge(base, local, remote);

        let items = find_conflicts(&merged.text);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].index, 0);
        assert_eq!(items[0].a, "Local");
        assert_eq!(items[0].b, "Remote");
        assert_eq!(items[1].index, 1);
        assert_eq!(items[1].a, "there");
        assert_eq!(items[1].b, "friend");

        let step1 = resolve_single(&merged.text, 1, ConflictResolution::Both);
        let remaining = find_conflicts(&step1);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].a, "Local");

        let step2 = resolve_single(&step1, 0, ConflictResolution::Both);
        assert!(!has_conflicts(&step2));
        assert_eq!(
            step2,
            "::meta{\n  title: \"Local / Remote\"\n}\n\nHello there friend.\n"
        );
    }

    // =========================================================================
    // Format-Preserving & Comment-Preserving Tests (6 `tomet-cms` cases)
    // =========================================================================

    #[test]
    fn source_merge_identity_invariants_hold_byte_for_byte() {
        let base = concat!(
            "::meta{ title: \"Base\", count: 1 }\n\n",
            "=[ Main Heading ]\n\n",
            "// Leading comment\n",
            "First paragraph with *italic* and **bold** and `code`.\n\n",
            "@callout(type: \"info\")[\n",
            "  Multi-line callout content.\n",
            "]\n\n",
            "```rust\n",
            "fn main() {\n",
            "    println!(\"hello\");\n",
            "}\n",
            "```\n",
        );
        let local = base.replace("count: 1", "count: 2");
        let remote = base.replace("First paragraph", "Updated first paragraph");

        assert_eq!(merge(base, &local, base).text, local);
        assert_eq!(merge(base, base, &remote).text, remote);
        assert_eq!(merge(base, &local, &local).text, local);
    }

    #[test]
    fn case_1_untouched_heading_padding_and_blank_lines_preserved() {
        let base = "=[ Main Heading ]\n\nFirst paragraph.\n\n\nSecond paragraph.\n";
        let local = "=[ Main Heading ]\n\nFirst paragraph edited.\n\n\nSecond paragraph.\n";
        let remote = "=[ Main Heading ]\n\nFirst paragraph.\n\n\nSecond paragraph edited.\n";

        let merged = merge(base, local, remote);
        assert!(merged.clean);
        assert_eq!(
            merged.text,
            "=[ Main Heading ]\n\nFirst paragraph edited.\n\n\nSecond paragraph edited.\n"
        );
    }

    #[test]
    fn case_2_untouched_inline_sugar_preserved() {
        let base = "Paragraph with *italic* and **bold** and `inline_code`.\n\nOther paragraph.\n";
        let local = "Paragraph with *italic* and **bold** and `inline_code`.\n\nOther paragraph edited.\n";
        let remote = "Updated paragraph with *italic* and **bold** and `inline_code`.\n\nOther paragraph.\n";

        let merged = merge(base, local, remote);
        assert!(merged.clean);
        assert_eq!(
            merged.text,
            "Updated paragraph with *italic* and **bold** and `inline_code`.\n\nOther paragraph edited.\n"
        );
    }

    #[test]
    fn case_3_comments_in_untouched_regions_preserved() {
        let base = concat!(
            "// Top-level header comment\n",
            "=[ Section ]\n\n",
            "/* Block comment before paragraph */\n",
            "First paragraph.\n\n",
            "// Comment before second paragraph\n",
            "Second paragraph.\n",
        );
        let local = base.replace("First paragraph.", "First paragraph local.");
        let remote = base.replace("Second paragraph.", "Second paragraph remote.");

        let merged = merge(base, &local, &remote);
        assert!(merged.clean);
        let expected = concat!(
            "// Top-level header comment\n",
            "=[ Section ]\n\n",
            "/* Block comment before paragraph */\n",
            "First paragraph local.\n\n",
            "// Comment before second paragraph\n",
            "Second paragraph remote.\n",
        );
        assert_eq!(merged.text, expected);
    }

    #[test]
    fn case_4_multiline_elements_and_quotes_preserved() {
        let base = concat!(
            "Intro paragraph.\n\n",
            "@callout(type: \"info\")[\n",
            "  Multi-line callout content.\n",
            "]\n",
        );
        let local = concat!(
            "Intro paragraph edited locally.\n\n",
            "@callout(type: \"info\")[\n",
            "  Multi-line callout content.\n",
            "]\n",
        );
        let remote = concat!(
            "Intro paragraph.\n\n",
            "@callout(type: \"info\")[\n",
            "  Multi-line callout content edited remotely.\n",
            "]\n",
        );

        let merged = merge(base, local, remote);
        assert!(merged.clean);
        assert_eq!(
            merged.text,
            concat!(
                "Intro paragraph edited locally.\n\n",
                "@callout(type: \"info\")[\n",
                "  Multi-line callout content edited remotely.\n",
                "]\n",
            )
        );
    }

    #[test]
    fn case_5_meta_brace_padding_and_key_merge_preserved() {
        let base = "::meta{ title: \"A\", count: 1 }\n\nBody text.\n";
        let local = "::meta{ title: \"B\", count: 1 }\n\nBody text.\n";
        let remote = "::meta{ title: \"A\", count: 2 }\n\nBody text.\n";

        let merged = merge(base, local, remote);
        assert!(merged.clean);
        assert_eq!(
            merged.text,
            "::meta{ title: \"B\", count: 2 }\n\nBody text.\n"
        );
    }

    #[test]
    fn case_6_fenced_code_blocks_remain_byte_identical() {
        let base = concat!(
            "Intro.\n\n",
            "```rust\n",
            "fn main() {\n",
            "    println!(\"hello\");\n",
            "}\n",
            "```\n\n",
            "Outro.\n",
        );
        let local = base.replace("Intro.", "Intro edited.");
        let remote = base.replace("Outro.", "Outro edited.");

        let merged = merge(base, &local, &remote);
        assert!(merged.clean);
        let expected = concat!(
            "Intro edited.\n\n",
            "```rust\n",
            "fn main() {\n",
            "    println!(\"hello\");\n",
            "}\n",
            "```\n\n",
            "Outro edited.\n",
        );
        assert_eq!(merged.text, expected);
    }

    #[test]
    fn full_tomet_cms_regression_preserves_all_untouched_constructs() {
        let base = concat!(
            "::meta{ title: \"Test\", count: 1 }\n\n",
            "=[ Main Heading ]\n\n",
            "First paragraph with *italic* and **bold**.\n\n",
            "// A comment that must survive\n",
            "Second paragraph.\n\n",
            "@callout(type: \"info\")[\n",
            "  Multi-line callout content.\n",
            "]\n\n",
            "```rust\n",
            "fn main() {\n",
            "    println!(\"hello\");\n",
            "}\n",
            "```\n",
        );
        let local = base.replace("count: 1", "count: 2");
        let remote = base.replace("Second paragraph.", "Second paragraph edited on remote.");

        let merged = merge(base, &local, &remote);
        assert!(merged.clean);
        let expected = concat!(
            "::meta{ title: \"Test\", count: 2 }\n\n",
            "=[ Main Heading ]\n\n",
            "First paragraph with *italic* and **bold**.\n\n",
            "// A comment that must survive\n",
            "Second paragraph edited on remote.\n\n",
            "@callout(type: \"info\")[\n",
            "  Multi-line callout content.\n",
            "]\n\n",
            "```rust\n",
            "fn main() {\n",
            "    println!(\"hello\");\n",
            "}\n",
            "```\n",
        );
        assert_eq!(merged.text, expected);
    }

    #[test]
    fn resolving_conflicts_preserves_surrounding_formatting_byte_for_byte() {
        let base = concat!(
            "::meta{ title: \"Test\", count: 1 }\n\n",
            "=[ Main Heading ]\n\n",
            "// Comment before paragraph\n",
            "The quick *brown* fox jumps.\n\n",
            "Original block.\n",
        );
        let local = concat!(
            "::meta{ title: \"Test\", count: 2 }\n\n",
            "=[ Main Heading ]\n\n",
            "// Comment before paragraph\n",
            "The slow *brown* fox jumps.\n\n",
            "::kind(note)\n",
        );
        let remote = concat!(
            "::meta{ title: \"Test\", count: 3 }\n\n",
            "=[ Main Heading ]\n\n",
            "// Comment before paragraph\n",
            "The lazy *brown* fox jumps.\n\n",
            "Remote replacement paragraph.\n",
        );

        let merged = merge(base, local, remote);
        assert!(!merged.clean);
        assert!(has_conflicts(&merged.text));
        let conflicts = find_conflicts(&merged.text);
        assert_eq!(conflicts.len(), 3);

        let resolved_a = resolve_all(&merged.text, ConflictResolution::A);
        assert_eq!(resolved_a, local);
        assert!(!has_conflicts(&resolved_a));

        let resolved_b = resolve_all(&merged.text, ConflictResolution::B);
        assert_eq!(resolved_b, remote);
        assert!(!has_conflicts(&resolved_b));

        let step1 = resolve_single(&merged.text, 0, ConflictResolution::B);
        assert_eq!(find_conflicts(&step1).len(), 2);
        assert!(step1.starts_with("::meta{ title: \"Test\", count: 3 }\n\n"));
    }
}
