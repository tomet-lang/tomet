# Codebase Review, Critical Analysis, and Proposed Action Plan (2026-08-12)

Comprehensive review of the Tomet repository, architectural analysis, critical evaluation, and proposed 1.0 strategic roadmap.

## 1. Overview

Tomet (`.tmt` / `.tmt`) is an ambitious markup language and Rust ecosystem designed to solve the ambiguity and lack of structural typing in CommonMark while overcoming the poor human-readability of JSON for text-heavy content. The repository exhibits a mature, highly structured multi-crate Rust workspace architecture (`Cargo.toml` edition 2024 with Nix packaging).

*Note on IDE / Binding Status*: Active IDE integration prototypes exist for **Zed** and **VS Code** (`apps/integrations/zed` and `apps/vscode-extension`), which were created as minimal proof-of-concept implementations to verify end-to-end functionality. Other editor and language binding members (`helix`, `neovim`, `java`, `js`, `python`) are currently reserved placeholder stubs.

---

## 2. Key Strengths

1. **Unified Element Model (`<T>(input)[area]{value}`)**
   - Cleanly unifies attributes `(input)`, raw/rendered content spans `[area]`, and structured data `{value}` into an orthogonal, order-independent element structure.
   - Concise reference inference for `@` elements (`@(url:...)`, `@(file:...)`, `@(ref:...)`) provides practical syntax shortcuts for common markup patterns.

2. **Embedded Format Interoperability (`format: json/yaml/toml`)**
   - `tomet-parser::embedded_format` natively allows embedded JSON, YAML, or TOML inside `{value}` blocks via `format: json|yaml|toml`. This provides a pragmatic escape hatch for complex schemas without overloading the markup language itself.

3. **Well-Structured Ecosystem Architecture**
   - Clear separation of concerns across dedicated workspace crates: `tomet-ast`, `tomet-lexar`, `tomet-parser`, `tomet-renderer`, `tomet-markdown`, `serde_tomet`, `tomet-formatter`, and `tree-sitter-tomet`.

4. **Grammar Drift Protection & Test Harness**
   - Explicit test harness (`*_fixture_has_only_known_error_cases`) in `tree-sitter-tomet` detects drift between the canonical hand-written parser (`tomet-parser`) and the tree-sitter grammar (`grammar.js`).

---

## 3. Critical Analysis & Potential Pitfalls

1. **Cognitive Load & Visual Noise**
   - High bracket density (`<>`, `()`, `[]`, `{}`) risks frustrating prose writers by increasing keystroke friction and visual clutter compared to plain Markdown.
   - Over-extending list items with complex inline structures (e.g., `-(!)[content]{tag:...}`) risks turning simple document writing into writing verbose DSL code.

2. **Identity & Scope Boundaries**
   - Pushing Tomet to simultaneously serve as a prose markup, a JSON replacement, and an in-parser dynamic template engine (e.g., `$add(a, b)` or `${a % b}` proposed in design notes) risks creating a "jack of all trades, master of none".
   - Dynamic evaluation inside `tomet-parser` would introduce security risks, parser performance degradation, and static analysis complexity.

3. **AST Position Tracking Constraints**
   - `tomet_ast::Document` currently does not carry source span/position info (`Line`, `Column`, byte offset).
   - *Impact*: `tomet-formatter` is limited to text-level whitespace normalization rather than true AST-driven lossless formatting. LSP capabilities (diagnostics ranges, Go-to-Definition) are also constrained.

---

## 4. Proposed Solution: The 4-Pillar Action Plan for Tomet 1.0

### Pillar 1: Syntax & Ergonomics Streamlining
- **Strict Disambiguation of `[]`**: Reserve `[]` strictly for Element Content-Spans (`[area]`) and Headings (`#[ Heading ]`).
- **Keep List Syntax Standard**: Maintain standard Markdown `- item` / `- [ ] item` lists. Attach metadata using standard inline attributes (e.g. `- item (id: t1){tag: dev}`) rather than introducing heavy `-[area]` syntax.

### Pillar 2: Identity & Scope Boundary Definition
- **Keep Parser Pure**: Keep `tomet-parser` strictly deterministic, fast, and static. Do not build an expression evaluator inside the core parser; handle templating in a separate preprocessing layer if needed.
- **Rely on Embedded Formats for Complex Schemas**: Leverage `format: json|yaml|toml` inside `{value}` rather than expanding `Value` syntax to compete with dedicated serialization formats.

### Pillar 3: AST Span Metadata Integration
- **Add `Span` to AST Nodes**: Attach byte offset and line/column ranges to all AST nodes in `tomet-ast`.
- **Lossless Formatter & Rich LSP**: Upgrade `tomet-formatter` to perform AST-driven lossless formatting and enable precise LSP diagnostic ranges.

### Pillar 4: Grammar Freeze & Targeted Toolchain Execution
- **Freeze Core Grammar**: Finalize the core grammar specification and synchronize `tomet-parser` and `tree-sitter-tomet`.
- **Focus Core Engineering**: Prioritize `apps/tomet` (CLI) and `apps/tomet-lsp`, keeping placeholder binding stubs paused until core 1.0 stabilization.
