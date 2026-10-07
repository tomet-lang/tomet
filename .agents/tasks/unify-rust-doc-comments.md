# Unify Rust Doc-Comments to Tomet Syntax

Task file for unifying and supplementing Rust documentation comments (`//!`, `///`)
across the workspace to adhere to Tomet native markup syntax.

## Context & Motivation

With `tomet api` (backed by `tomet-extract-rust`), documentation comments from Rust code
are extracted directly into Tomet documents under `tmtroot/generated/crates/*.tmt`.
Inspection of the initial output revealed two major issues:
1. **Markdown syntax leakage**: Many doc-comments contain CommonMark `#` headings, markdown tables (`|---|`), and markdown callouts instead of Tomet syntax.
2. **Missing/sparse crate docs**: Several crates have empty or minimal crate-level (`//!`) comments (e.g. `tomet-python`, `tomet-zed`, `tomet-cli`, `tomet-markdown`).

This task unifies all Rust doc-comments to conform to Tomet syntax and supplements missing docs.

## Decisions

1. **Markup Syntax: Strict Tomet Native Syntax**
   Doc-comments in Rust are parsed as Tomet blocks. Markdown formatting must be eliminated:
   - **Headings**:
     - Do NOT use `#` or `##`.
     - In `//!`, authors start top-level sections naturally with `=[ Heading ]`. The extract engine (`tomet-extract-rust`) automatically adjusts/shifts heading depth beneath the crate's root title (`=[ crate-name ]`). Subheadings use `==[ Subheading ]`, `===[ ... ]`.
   - **Multi-line list items**: Use `-| line 1\n | line 2` syntax (pipe continuation).
   - **Tables**: Use `@table\n|[ col1 ][ col2 ]` block syntax, never markdown `|---|`.
   - **Callouts**: Use `@callout(note)`, `@callout(tip)`, `@callout(warning)` blocks instead of `> [!NOTE]`.
   - **Code blocks**: Standard fenced code blocks ` ```rust ` are supported.

2. **Crate-Level Doc (`//!`) Standard Structure**
   Every crate's root (`lib.rs` or `main.rs`) must contain:
   - A 1-2 sentence concise summary of the crate's single responsibility.
   - `=[ Overview ]`: High-level explanation of role and boundaries.
   - `=[ Key Types ]` or `=[ Key Functions ]`: Summary of primary entry points with [`TypeName`] links.
   - `=[ Example ]`: Usage snippet if applicable.

3. **Item-Level Doc (`///`) Standard**
   - 1st line: Single concise summary sentence ending with a period.
   - Describe contracts, inputs, outputs, error conditions.
   - Internal algorithm/implementation details belong in internal `//` comments, not `///`.

4. **Language**
   - Strictly English only (`Write all code comments in English`).

## Areas Needing Attention

### 1. Markdown `#` Headings to Fix
Discovered in `tmtroot/generated/crates/`:
- `crates/tomet/src/lib.rs` (`# Reading a document`, `# Features`, markdown table)
- `crates/tomet-resolver/src/lib.rs` (`## Reading is here...`, `## @include...`)
- `crates/tomet-load/src/lib.rs` (`# Two levels...`, `# A fifth step...`, `## Who prepares...`)
- `crates/tomet-transform/src/lib.rs` (`# Why this is not...`, `# Why the rows arrive...`)
- `crates/tomet-vault/src/lib.rs` (`# wasm`, `# Preparing`)
- `crates/tomet-pandoc/src/lib.rs` (`# Why a JSON bridge`)
- `crates/tomet-stats/src/lib.rs` (`# Counting rules`)
- `crates/tove/src/lib.rs` (`# TOVE`)

### 2. Empty / Sparse Crates to Supplement
- `bindings/python/src/lib.rs` (`tomet-python`: currently empty)
- `editors/zed/` (`tomet-zed`: currently empty)
- `apps/cli/src/main.rs` (`tomet-cli`: only 1 short sentence, missing module/command overview)
- `crates/tomet-markdown/src/lib.rs`: supplement key types and bidirectional conversion overview

## Discrete Steps

- [ ] **Step 1: Replace Markdown syntax with Tomet syntax**
  - Search for `#` in doc comments across `crates/`, `apps/`, `bindings/`:
    `grep -rn "^ *//! *#" crates/ apps/ bindings/`
    `grep -rn "^ */// *#" crates/ apps/ bindings/`
  - Convert headings to `=[ ... ]` / `==[ ... ]`.
  - Convert markdown tables to `@table` blocks.
  - Convert multi-line lists to `-| ... |` style.

- [ ] **Step 2: Supplement missing and sparse crate-level `//!` docs**
  - Add standard structure (Summary, Overview, Key Types) to `tomet-python`, `tomet-zed`, `tomet-cli`, etc.
  - Ensure public items in these crates have meaningful 1-line doc comments.

- [ ] **Step 3: Regenerate and verify**
  - Run `cargo check --workspace` to ensure no syntax/compilation breakage.
  - Run `./target/debug/tomet api .` (or `cargo run -p tomet-cli -- api .`) to refresh `tmtroot/generated/crates/*.tmt`.
  - Run `./target/debug/tomet api --check .` to ensure zero drift.
  - Inspect git diff in `tmtroot/generated/crates/` to confirm all markdown artifacts (`#`, `|---|`) are gone and generated `.tmt` files are clean.

- [ ] **Step 4: Completion & Cleanup**
  - Verify `just docs-check` passes.
  - Delete this task file once done.
