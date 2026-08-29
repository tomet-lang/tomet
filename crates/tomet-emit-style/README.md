# tomet-style

Per-node styling and value serialization for Tomet AST elements and metadata.

## Responsibilities

1. **Pure Value Rendering**:
   - `render_value` / `render_value_inner_with_config`: Renders `tomet_ast::Value` to `.tmt` source syntax.
   - Handles auto-quoting of strings containing delimiters (`:`, `,`, `[`, `]`, whitespace).
   - Formats scalars, sequences, and maps.

2. **Metadata & Directive Styling**:
   - `render_meta_element`: Formats `@meta` blocks according to `PrinterConfig` (single-line vs multi-line, format specification `yaml`/`json`/`toml`, per-field indentation).
   - `render_args_with_config`: Formats `(args)` attribute groups with optional spacing rules (e.g. `link_no_space`).
