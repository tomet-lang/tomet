# Block-level link API: `collect_links_in_blocks`

Found while following tomet main in tomet-shelf (`.agents/tasks/follow-tomet-main.md` there). Not started.

## Why

`tomet_workspace_links::collect_links(&File)` and `tomet_tree::for_each_element(&File, ..)` are the only ways to walk
links, and both take a whole `File`. A consumer that holds only a slice of blocks has no entry point:

- tomet-shelf `crates/tomet-shelf-headless/src/book/catalog.rs` `first_page_target(blocks: &[Block])` finds the first
  `file` / `document` / `ref` link inside one catalog item's blocks (a list item's content, a section's body).
  It cannot call `collect_links`, because there is no `File` around those blocks. Building a throwaway
  `File::new(Vec::new(), blocks.to_vec(), Default::default())` clones the blocks and invents an empty header, so shelf
  now has a hand-written walker (`first_block_link_target` / `first_element_link_target`) that repeats
  `collect_links`'s classification (`link_target_of` / `path_target_of` / `target_scheme`) and visits only
  Paragraph / Element / Section / List blocks (`_ => {}` for the rest).
- Two copies of "which blocks can hold an element, and which targets count as a link" will drift. The shelf copy
  already differs: it returns the first match and ignores `Embed` / `Dir`; `collect_links` returns all of them.

## Design (to decide, not to assume)

- [ ] `tomet_tree`: a traversal that takes `&[Block]` (the existing `for_each_element(&File, ..)` becomes a thin
      caller of it, over `file.body`). Check what `for_each_element` does with `header` today, so the new
      function's contract says what it does and does not visit.
- [ ] `tomet_workspace_links`: `collect_links_in_blocks(&[Block]) -> Vec<DocumentLink>`; `collect_links(&File)` calls
      it. The shared classification (`from_element_kind_and_target`) stays in one place.
- [ ] Decide whether shelf needs "first link" (`link_target_in_blocks`, early exit) or can take
      `collect_links_in_blocks(..).into_iter().next()` and its all-links cost. Measure before adding a second function.
- [ ] Decide the `Embed` / `Dir` question for shelf's use: does a catalog item whose first link is an `@embed`
      count as its page target? `first_page_target` says no today; the answer belongs in shelf's doc comment.
- [ ] Re-export through the `tomet` facade (`tomet::links`), since shelf and desk depend on that, not the sub-crates.
- [ ] Tests: a link nested in a list item inside a section; a block kind with no elements (code block); header
      declarations are not visited.

## After it lands

- [ ] tomet-shelf: replace the hand-written walker with the new function and delete `first_block_link_target` /
      `first_element_link_target`; bump the tomet pin in Cargo.lock (`cargo update -p tomet`, never `[patch]`).

## Destination when done

The contract goes in the `//!` / `///` of the two new functions (what is visited, what is excluded). Nothing else is
worth keeping; delete this file.
