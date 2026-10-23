//! Three-way merge for `.tmt` notes, at the AST level rather than the text
//! level.
//!
//! A textual `diff3` treats a note as lines. Tomet documents are trees of
//! `@element(args)[content]{data}` nodes, and most real edits touch
//! different nodes -- editing one `{data}` key on a device while another
//! device edits an unrelated block's `[content]` is not a conflict in any
//! meaningful sense, even though the two edits may land on adjacent lines.
//! Diffing the tree instead of the text is what lets those edits merge
//! silently, which is the entire point of doing this instead of shelling
//! out to `git merge-file`.
//!
//! # Algorithm
//!
//! [`merge`] walks `base`, `local` and `remote` together, top-down:
//!
//! 1. At each level, align the child sequence of `base` against `local`
//!    and against `remote` with an LCS-style alignment (the same technique
//!    a line-based diff uses, applied to sibling nodes instead of lines).
//!    Alignment is structural -- two nodes are "the same" position if they
//!    are AST-equal, not by any synthetic identity tomet would have to
//!    invent and inject into the user's document.
//! 2. A child unchanged on one side and changed on the other resolves to
//!    the changed side. A child added on only one side is kept. A child
//!    removed on only one side is dropped.
//! 3. A single base block changed on *both* sides recurses via
//!    `merge_one`, into every part of it independently:
//!    - `content: Vec<Block>` (a `Paragraph`'s `Vec<Inline>`, or an
//!      `Element`'s own `[content]`/`children`, both `Vec<Block>`) gets
//!      the same diff3 treatment as step 1; a single-`Text`-per-side
//!      inline conflict (within a `Paragraph`) recurses once more,
//!      character by character (`merge_text`). This is what lets two
//!      edits to different words in the same sentence merge silently.
//!    - `args`/`{data}` (`Element.value`'s `Group`) recurse by key
//!      (`merge_value`/`merge_value_map`): editing two different
//!      keys never conflicts, and a `Value::Map`/`Value::Seq` inside a
//!      value recurses further still. This only narrows a `{data}`
//!      group made of plain `key: value` pairs -- one holding a nested
//!      element (rather than only pairs) isn't narrowed
//!      (`.agents/tasks/merge-tree-diff.md`).
//!    Each part narrows independently; the recursion only gives up on
//!    the *whole* block (step 4) when at least one part's shape itself
//!    differs (e.g. `[content]` present on one side and absent on the
//!    other) rather than merely disagreeing in value.
//! 4. A conflict `merge_one` can't narrow any further is recorded
//!    in-place as one `@conflict` element -- `tomet`'s own `std` element
//!    for exactly this, bare and needing no `::use`. `a`/`b` are
//!    structural names, not "mine"/"theirs": the side that merged the
//!    note has no way to tell a later reader (on another device, or
//!    itself after the app restarts) which one was whose, so the stored
//!    document doesn't claim to know either -- see an editor or UI resolution
//!    module for how a mine/theirs *display* label still gets derived, without
//!    being stored. Two roles, depending on where the conflict sits:
//!    - **As a sequence of blocks/inlines**, `a`/`b` each hold the
//!      disputed stretch's own blocks directly (`Value::Blocks`, `tomet`'s
//!      `key: [...]` value grammar) -- no marker triple, no terminator;
//!      `@conflict`'s own `(args)` is the whole thing. At the
//!      `Document.blocks`/`children`/`content` level this is a block-
//!      placed element; inside a paragraph's inline content, the same
//!      element inline, with `a`/`b` each wrapping their inline run in
//!      one `Paragraph` to fit the same `Vec<Block>` shape.
//!    - **As a single value** (a `{data}` key, or `args`), it becomes
//!      `@conflict(a: ..., b: ...)` sitting where the value went, using
//!      named args rather than `{value}` -- a value slot only permits
//!      `(args)` on an embedded element -- with `a`/`b` as plain scalars
//!      (whatever the disputed value itself was), not `Value::Blocks`.
//!
//! # What this crate assumes
//!
//! A `base` is always available: a caller only invokes this crate for an
//! actual three-way merge, where a merge base commit or document ancestor
//! is known. Linking two vaults or documents that do not share history is a
//! separate operation (such as vault adoption), not a merge, and is not this
//! crate's problem.
//!
//! # Module layout
//!
//! - [`merge`] (the module, via [`merge()`] the function): the merge half
//!   of the algorithm above.
//! - [`resolve`]: strips markers back out once a side has been picked.
//! - [`find_conflicts`]: reads markers back out without removing them, for
//!   showing the user what's in conflict.
//! - `markers`: the `@conflict` marker shape itself (construction and
//!   detection), shared by all three.
//! - `diff3`: the plain three-way sequence alignment every level of the
//!   tree reuses; has no idea it's being used for an AST.

mod diff3;
mod find_conflicts;
mod markers;
mod merge;
mod resolve;

pub use find_conflicts::{ConflictItem, find_conflicts, has_conflicts};
pub use markers::CONFLICT_MARKER;
pub use merge::{MergeResult, merge};
pub use resolve::{ConflictResolution, resolve, resolve_all, resolve_single};

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{Block, Document};
    use tomet_parser::parse_document;

    fn doc(src: &str) -> Document {
        parse_document(src).unwrap_or_else(|e| panic!("failed to parse {src:?}: {e}"))
    }

    #[test]
    fn disjoint_edits_merge_cleanly() {
        let base = doc("Alpha.\n\nBeta.\n");
        let local = doc("Alpha changed.\n\nBeta.\n");
        let remote = doc("Alpha.\n\nBeta changed.\n");

        let result = merge(&base, &local, &remote);

        assert!(result.clean);
        assert_eq!(result.document, doc("Alpha changed.\n\nBeta changed.\n"));
    }

    #[test]
    fn mismatched_block_kinds_fall_back_to_block_level_markers() {
        // local turns the block into a different kind of node entirely
        // (an element, not a paragraph) -- merge_one can't narrow across
        // a change in node kind, so this exercises the whole-block
        // marker fallback rather than the inline recursion.
        let base = doc("Original.\n");
        let local = doc("::meta{ x: 1 }\n");
        let remote = doc("Theirs.\n");

        let result = merge(&base, &local, &remote);

        assert!(!result.clean);
        assert_eq!(
            result.document,
            doc(concat!(
                "@conflict(\n",
                "  a: [\n",
                "    ::meta{ x: 1 }\n",
                "  ]\n",
                "  b: [ Theirs. ]\n",
                ")\n",
            ))
        );
    }

    #[test]
    fn resolve_a_keeps_local() {
        let base = doc("Original.\n");
        let local = doc("Mine.\n");
        let remote = doc("Theirs.\n");
        let merged = merge(&base, &local, &remote).document;

        let resolved = resolve(&merged, &[ConflictResolution::A]);

        assert_eq!(resolved, doc("Mine.\n"));
    }

    #[test]
    fn resolve_b_keeps_remote() {
        let base = doc("Original.\n");
        let local = doc("Mine.\n");
        let remote = doc("Theirs.\n");
        let merged = merge(&base, &local, &remote).document;

        let resolved = resolve(&merged, &[ConflictResolution::B]);

        assert_eq!(resolved, doc("Theirs.\n"));
    }

    #[test]
    fn edits_to_different_words_in_one_sentence_merge_cleanly() {
        let base = doc("The quick fox jumps.\n");
        let local = doc("The slow fox jumps.\n");
        let remote = doc("The quick fox runs.\n");

        let result = merge(&base, &local, &remote);

        assert!(result.clean);
        assert_eq!(result.document, doc("The slow fox runs.\n"));
    }

    #[test]
    fn edits_to_the_same_word_produce_an_inline_conflict() {
        // The exact character split isn't asserted here -- that's an
        // implementation detail of the LCS alignment, not a contract --
        // only that it's flagged unclean and stays a single paragraph
        // (the point of narrowing to the inline level at all) rather
        // than duplicating the whole sentence as block-level markers.
        // `resolving_an_inline_conflict_leaves_one_plain_sentence` below
        // is what actually pins down correctness, by checking what
        // resolving each side produces.
        let base = doc("The quick fox jumps.\n");
        let local = doc("The slow fox jumps.\n");
        let remote = doc("The lazy fox jumps.\n");

        let result = merge(&base, &local, &remote);

        assert!(!result.clean);
        assert_eq!(result.document.blocks.len(), 1, "one paragraph");
        assert!(matches!(result.document.blocks[0], Block::Paragraph(_)));
    }

    #[test]
    fn resolving_an_inline_conflict_leaves_one_plain_sentence() {
        let base = doc("The quick fox jumps.\n");
        let local = doc("The slow fox jumps.\n");
        let remote = doc("The lazy fox jumps.\n");
        let merged = merge(&base, &local, &remote).document;

        let resolved = resolve(&merged, &[ConflictResolution::B]);

        assert_eq!(resolved, doc("The lazy fox jumps.\n"));
    }

    #[test]
    fn resolve_all_resolves_every_conflict() {
        let base = doc("Zzz.\n\nAnchor.\n\nQqq.\n");
        let local = doc("Mmm.\n\nAnchor.\n\nNnn.\n");
        let remote = doc("Ppp.\n\nAnchor.\n\nRrr.\n");
        let merged = merge(&base, &local, &remote).document;

        let resolved_a = resolve_all(&merged, ConflictResolution::A);
        assert_eq!(resolved_a, doc("Mmm.\n\nAnchor.\n\nNnn.\n"));

        let resolved_b = resolve_all(&merged, ConflictResolution::B);
        assert_eq!(resolved_b, doc("Ppp.\n\nAnchor.\n\nRrr.\n"));
    }

    #[test]
    fn resolving_one_conflict_leaves_another_untouched() {
        // An unchanged paragraph in the middle gives the LCS an anchor,
        // so this is two independent conflicts rather than one hunk
        // spanning the whole document. The two conflicting paragraphs
        // share no characters with each other (besides the trailing
        // "."), on purpose -- so the character-level recursion can't
        // find a partial anchor and muddle the two together, which kept
        // this fixture's first draft (reusing "One"/"Two" inside the
        // replacement text) producing a garbled cross-conflict result.
        let base = doc("Zzz.\n\nAnchor.\n\nQqq.\n");
        let local = doc("Mmm.\n\nAnchor.\n\nNnn.\n");
        let remote = doc("Ppp.\n\nAnchor.\n\nRrr.\n");
        let merged = merge(&base, &local, &remote).document;

        let resolved = resolve(&merged, &[ConflictResolution::A, ConflictResolution::B]);

        assert_eq!(resolved, doc("Mmm.\n\nAnchor.\n\nRrr.\n"));
    }

    #[test]
    fn edits_to_different_data_keys_merge_cleanly() {
        let base = doc("::meta{ x: 1, y: 2 }\n");
        let local = doc("::meta{ x: 10, y: 2 }\n");
        let remote = doc("::meta{ x: 1, y: 20 }\n");

        let result = merge(&base, &local, &remote);

        assert!(result.clean);
        assert_eq!(result.document, doc("::meta{ x: 10, y: 20 }\n"));
    }

    #[test]
    fn editing_the_same_data_key_produces_a_value_conflict() {
        let base = doc("::meta{ x: 1 }\n");
        let local = doc("::meta{ x: 2 }\n");
        let remote = doc("::meta{ x: 3 }\n");

        let result = merge(&base, &local, &remote);

        assert!(!result.clean);
        assert_eq!(result.document, doc("::meta{ x: @conflict(a: 2, b: 3) }\n"));
    }

    #[test]
    fn resolving_a_data_key_conflict_leaves_a_plain_value() {
        let base = doc("::meta{ x: 1 }\n");
        let local = doc("::meta{ x: 2 }\n");
        let remote = doc("::meta{ x: 3 }\n");
        let merged = merge(&base, &local, &remote).document;

        let resolved = resolve(&merged, &[ConflictResolution::B]);

        assert_eq!(resolved, doc("::meta{ x: 3 }\n"));
    }

    #[test]
    fn a_key_deleted_on_one_side_and_untouched_on_the_other_is_dropped() {
        let base = doc("::meta{ x: 1, y: 2 }\n");
        let local = doc("::meta{ y: 2 }\n"); // local deleted x
        let remote = doc("::meta{ x: 1, y: 2 }\n"); // remote left it alone

        let result = merge(&base, &local, &remote);

        assert!(result.clean);
        assert_eq!(result.document, doc("::meta{ y: 2 }\n"));
    }

    #[test]
    fn a_key_deleted_on_one_side_and_modified_on_the_other_keeps_the_edit() {
        let base = doc("::meta{ x: 1 }\n");
        let local = doc("::meta{}\n"); // local deleted x
        let remote = doc("::meta{ x: 9 }\n"); // remote modified it

        let result = merge(&base, &local, &remote);

        assert!(result.clean);
        assert_eq!(result.document, doc("::meta{ x: 9 }\n"));
    }

    #[test]
    fn find_conflicts_and_resolve_single_and_both() {
        let base = doc("Alpha.\n\nKeep.\n\nBeta.\n");
        let local = doc("Alpha local.\n\nKeep.\n\nBeta local.\n");
        let remote = doc("Alpha remote.\n\nKeep.\n\nBeta remote.\n");

        let merged = merge(&base, &local, &remote);
        assert!(!merged.clean);

        let conflicts = find_conflicts(&merged.document);
        assert_eq!(conflicts.len(), 2);
        assert_eq!(conflicts[0].a, "l");
        assert_eq!(conflicts[0].b, " remote");
        assert_eq!(conflicts[1].a, "l");
        assert_eq!(conflicts[1].b, " remote");

        // Resolve only the first conflict, keeping its `a` side.
        let resolved_first = resolve_single(&merged.document, 0, ConflictResolution::A);
        let remaining = find_conflicts(&resolved_first);
        assert_eq!(remaining.len(), 1);
        let text_after_first = tomet_printer::document_to_tm(&resolved_first);
        assert!(text_after_first.contains("Alpha local."));
        assert!(!text_after_first.contains("Alpha remote."));

        // Resolve remaining conflict with Both
        let resolved_all = resolve_single(&resolved_first, 0, ConflictResolution::Both);
        assert!(find_conflicts(&resolved_all).is_empty());
        let final_text = tomet_printer::document_to_tm(&resolved_all);
        assert!(final_text.contains("Alpha local."));
        assert!(final_text.contains("Keep."));
        assert!(final_text.contains("Beta local remote."));
    }
}
