# Task: Fenced Raw Block Line Marker (``)

Inside a ` ``` ` fenced code block, a body line beginning with ` `` ` (exactly two backticks) is a marked line: the ` `` ` is a line marker, not literal text. The marked form is the canonical spelling of fenced code blocks.

## Decisions from Manager Review
1. **CST Shape**: `LINE_MARKER` composite node (new `SyntaxKind::LINE_MARKER`). Replace `FENCE` in standalone line raw with `LINE_MARKER` too. `CstCodeLine::line_marker(&self) -> Option<CstLineMarker>`. No backward compatibility shims.
2. **Marker Definition**: Exactly 2 backticks at line start (after indentation / `MARGIN`). A line starting with 3 backticks followed by text (e.g. ` ```rust `) is an unmarked verbatim line, not marked. Closing fence detection stays as-is.
3. **Shared Line Processing**: Shared function between standalone line raw and fenced code block marked lines for stripping marker and exactly 1 following separator space. No duplicate logic.
4. **Formatter Fallback**: No runtime parse-and-rollback. Construct correct output and verify with tests.
5. **Bare (Legacy) Formatting**: Do not widen fence. Keep ` `` ` marker only on lines where stripping would change meaning:
   - Line content starts with exactly 2 backticks (` `` `).
   - Line content would be parsed as a closing fence (run of backticks >= fence length followed only by whitespace/newline).
6. **Naming**: `enum FenceLines { Marked, Bare }` in `tomet-syntax-style`, config key `code.fence.lines` (values `"marked"` / `"bare"`).
7. **Printer Default**: `FenceLines::Marked` when unset. Snapshot updates in `tests/ref/` are expected.
8. **Scope**:
   - Parser, CST, Lowering
   - Style, Config, Formatter, Printer
   - `tomet-syntax-highlight`
   - `tomet-edit` (`import.rs`, `doc.rs`, `node.rs`)
   - `tomet-merge`
   - `tomet-convert-markdown` (use `code.fence.lines` / `Marked`)
   - `tomet-convert-pandoc`
   - `tomet-spec/util.rs`
   - `apps/cli/src/stats.rs`
   - `tree-sitter-tomet` (`grammar.js`, `queries/injections.scm`, `queries/folds.scm`, `queries/highlights.scm`)
9. **Files to migrate**:
   - `tmtroot/docs/spec/syntax.tmt`: update line raw sample to ` `` `` `, rewrite `raw fence` section.
   - Do NOT touch `tmtroot/dev/dropped/1000-cotomet-header-syntax.adr.tmt` (mention indentation shift in final report).
   - Frozen fixtures (`tests/fixtures`): do not edit, add new fixture `tests/fixtures/syntax/fenced-raw-line-marker.tmt`.

## Destination of Decisions (Folding at completion)
Fold into crate `//!` and function doc comments across modified crates, `tmtroot/docs/spec/syntax.tmt`, `tmtroot/docs/spec/sigils.tmt`, and git commit messages.

---

## Steps

- [x] **Step 1: Preparation & Investigation (PHASE 1)**
- [x] **Step 2: Additional Investigation of Scope Crates**
  - Checked `tomet-syntax-highlight`, `tomet-edit`, `tomet-merge`, `tomet-convert-markdown`, `tomet-convert-pandoc`, `tomet-spec/util.rs`, `apps/cli/src/commands/stats.rs`, `tree-sitter-tomet/queries/`.
- [x] **Step 3: CST & Parser (`tomet-syntax-cst`, `tomet-syntax-cst-parser`)**
  - Added `LINE_MARKER` to `SyntaxKind`, defined `CstLineMarker`, added `CstCodeLine::line_marker(&self)`.
  - Defined `CstCodeLine::parts(&self, req_markers)` and `tomet_cst::pipe_depth(&node)` centrally in `tomet-cst`.
  - Updated `parse_code_block` and `parse_line_raw` to wrap exactly 2 backticks in `LINE_MARKER`.
  - Marked lines do not close fences.
- [x] **Step 4: Lowering (`tomet-syntax-lowering`)**
  - Uses `CstCodeLine::parts` and `cst.pipe_depth()`.
  - Unified `lower_code_block` and `lower_line_raw` with no duplicate logic.
  - Added comprehensive tests in `tomet-syntax-lowering/src/tests.rs`.
- [x] **Step 5: Style & Config (`tomet-syntax-style`, `tomet-config`)**
  - Implemented `FenceLines { Marked, Bare }` in `tomet-syntax-style`.
  - Implemented `fence_line_requires_marker` centrally in `tomet-syntax-spelling`.
  - Added `code.fence.lines` to `FormatConfig` in `tomet-config/src/groups/format.rs` with tests.
- [x] **Step 6: Formatter & Printer (`tomet-formatter`, `tomet-printer`, `convert-markdown`)**
  - Implemented `format_fence_lines` in `tomet-formatter` using `line.parts` and `fence_line_requires_marker`.
  - Updated `tomet-printer` (default `Marked`, supports `Bare` using `fence_line_requires_marker`).
  - Added unit tests in `tomet-formatter` and `tomet-printer`.
- [x] **Step 7: Other Consumer Crates & Tree-sitter**
  - Updated `tomet-highlight` to handle `K::LINE_MARKER`.
  - Updated `tree-sitter-tomet` `grammar.js` (canonical marked lines, bare lines) and `highlights.scm`.
  - Regenerated tree-sitter parser (`npx -y tree-sitter-cli@0.26.12 generate`).
- [x] **Step 8: Docs, Fixtures, and Snapshots**
  - Updated `tmtroot/docs/spec/syntax.tmt` (rewrote raw fence section, updated sample to ` `` `` `).
  - Updated `tmtroot/docs/spec/sigils.tmt`.
  - Added fixture `tests/fixtures/syntax/fenced-raw-line-marker.tmt`.
  - Updated snapshot references with `TOMET_UPDATE_REF=1`.
- [ ] **Step 9: Full Verification & Final Report**
  - `cargo test --workspace` (PASSED)
  - `cargo test -p tomet-tests -p tree-sitter-tomet` (PASSED)
  - `twrit check` (PASSED)
  - Final report to manager.
