# Feasibility & Integration Assessment: Bringing oneline Techniques into tomet-ast-parser

Analysis of incorporating `oneline` parsing techniques (SIMD newline scanning, table-driven line classification, parallel ANSV tree linking, and GPU pipeline) into `tomet`'s parsing infrastructure.

Delete this file when done.
Destination of what is worth keeping: architecture notes and documentation in `crates/tomet-syntax-ast-parser-oneline` and `crates/tomet-syntax-ast-parser`.

---

## 1. Context & Current State

- **`oneline` stack**:
  - `oneline-simd`: AVX2/NEON vectorised newline scan (`count_newlines`, `scan_line_offsets`) + 2-pass allocation-free tokenisation.
  - Line classification: 256-entry `FIRST_BYTE` lookup table with run-length prefix scanner for `|` depth.
  - Tree linking: Parallel All Nearest Smaller Values (ANSV) in $O(n)$ work over block partitions (maps directly onto Rayon CPU or WGSL GPU).
  - Bridge to tomet (`oneline-tomet`): Converts `FlatTree` directly into `tomet_ast::Document` using parallel per-line block builds + bottom-up stitching. Differential testing (`compat.rs`) verifies identical AST output for the supported subset.
- **`tomet` stack**:
  - Reference parser (`tomet-ast-parser`): Combines lossless Rowan-based CST parsing (`tomet-cst-parser`) and semantic lowering (`tomet-lowering`).
  - `tomet-syntax-ast-parser-oneline`: Currently an empty crate directory reserved for fast oneline integration.
  - `bearings/plans/oneline.tmt`: Tracks the initiative ("Onelineのノウハウをtometのパーサに入れる").

---

## 2. Feasibility Findings & Boundaries

### A. What Can Be Brought Over (High Viability & Immediate Gain)

1. **SIMD Newline Scanning & Line Offsets**:
   - `count_newlines` and `scan_line_offsets` (AVX2 + scalar fallback) operate at memory bandwidth speeds (16–32 GB/s).
   - Can be dropped directly into `tomet-base-position` (`LineIndex`) or `tomet-syntax-lexer` without changing any parser semantics.
2. **Parallel ANSV Hierarchy Resolution**:
   - `tomet`'s structural hierarchy—heading levels (`=`) and pipe nesting (`|`)—is an ANSV problem.
   - Replacing sequential stack recursion with block-parallel ANSV (Rayon-based) enables multi-core tree construction for large `.tmt` documents.
3. **Line-local Subset Fast-Path (`tomet-syntax-ast-parser-oneline`)**:
   - For files conforming to the line-local subset (no multi-line brackets/fences), `oneline-tomet::to_tomet_ast` already demonstrates that `tomet_ast::Document` can be synthesized directly without constructing a CST or running lowering.

### B. What Cannot Be Brought Over (Hard Boundaries & Trade-offs)

1. **Full Replacement of `tomet-cst-parser` is Infeasible**:
   - `tomet-cst-parser` guarantees 100% lossless trivia preservation (whitespace, comments, error nodes) for formatter, LSP, and refactoring tools.
   - `oneline`'s `FlatNode` model discards intra-line trivia and cannot represent invalid/partial CST tokens needed for resilient IDE tooling.
2. **Multi-line Constructs Break Pure Line-Local Parallelism**:
   - Constructs such as `@form(\n  arg: 1\n)`, `/* ... */` block comments, and ` ``` ` raw fences span lines and require cross-line state tracking.
   - While code fences can be handled via prefix-scan XOR parity, multi-line nested bracket balancing breaks the simple independent line model.
3. **Inline Parsing Remains Sequential**:
   - Inside lines, syntax like `**bold**`, `//italic//`, `${interp}`, and link escaping requires context-sensitive scanning; SIMD acceleration applies primarily to line-level slicing and delimitation.
4. **AST Allocation Bottleneck**:
   - `oneline` achieves extreme throughput because `FlatTree` is allocation-free (index-based / borrowed).
   - Materialising standard `tomet_ast::Block` / `Vec` / `String` heap allocations reduces the real-world throughput benefit.

---

## 3. Recommended Architectural Integration (Tiered Strategy)

```
                       [ Input Source (.tmt) ]
                                  │
                  ┌───────────────┴───────────────┐
                  ▼                               ▼
       [ Fast Check: outside_subset ]   (SIMD newline scan for LineIndex)
                  │
        ┌─────────┴─────────┐
        ▼                   ▼
    [ True ]            [ False ]
(Line-Local Subset)    (Complex / Multi-line)
        │                   │
        ▼                   ▼
 [ oneline Fast-Path ]   [ tomet-cst-parser ]
 (SIMD + Parallel ANSV)      (Lossless CST)
        │                   │
        │                   ▼
        │            [ tomet-lowering ]
        └─────────┬─────────┘
                  ▼
         [ tomet_ast::Document ]
```

1. **Tier 1 (Core Utility Upgrades)**:
   - Introduce `oneline-simd` newline primitives into `tomet-base-position` and `tomet-syntax-lexer`.
2. **Tier 2 (2-Tier Fast-Path in `tomet-syntax-ast-parser-oneline`)**:
   - Implement `tomet-syntax-ast-parser-oneline` using the `oneline-tomet` logic.
   - Provide a fast speculative entry point in `tomet-ast-parser`: run `outside_subset()` / line pre-scan; if clean, execute fast parallel oneline path; otherwise, fall back to CST + Lowering.
3. **Tier 3 (Workspace & Indexing Offload)**:
   - Apply `oneline` scanning directly to `tomet-workspace-indexer` and `tomet-search`.
   - Indexing tags, headers, and element names across large repositories does not require a full AST and can leverage `FlatTree` directly at maximum speed.

---

## 4. Next Action Steps

- [ ] Port `oneline-simd::scan` routines into `crates/tomet-base-position` for SIMD-accelerated `LineIndex`.
- [ ] Populate `crates/tomet-syntax-ast-parser-oneline` with code from `oneline/crates/oneline-tomet`.
- [ ] Add `tomet-syntax-ast-parser-oneline` as an optional dependency/feature under `crates/tomet-syntax-ast-parser`.
- [ ] Benchmark differential throughput on the standard `tomet-corpus` suite.
