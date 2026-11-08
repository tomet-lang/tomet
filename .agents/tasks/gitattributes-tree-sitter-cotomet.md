# tomet's `.gitattributes` does not mark the generated files of `tree-sitter-cotomet`

Moved out of `shared-files-template.md` (2026-11-06; that file's cross-repository part is in `bearings/initiatives/commonfiles-rollout.tmt`).

tomet keeps its own `.gitattributes` and does not take it from commonfiles (decided 2026-10-24: its tree-sitter lines are tomet's only).
That file marks the generated files of `crates/tree-sitter-tomet` but not those of `crates/tree-sitter-cotomet`, added by the cotomet rename.
A regeneration shows up as tens of thousands of lines in review.

- [ ] Add `src/parser.c`, `grammar.json` and `node-types.json` of `crates/tree-sitter-cotomet` to tomet's own `.gitattributes`
      (the same marks the `tree-sitter-tomet` lines carry). The user commits.

Delete this file when that is done.
