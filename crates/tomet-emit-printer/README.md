# tomet-printer

Serializes a Tomet AST `Document` into formatted `.tmt` source text.

## Responsibilities

1. **AST to Source Text Serialization**:
   - `document_to_tm` / `document_to_tm_with_config`: Rebuilds full `.tmt` document source from a `Document` AST.
   - Used for converting from other formats (like Markdown) or when generating documents programmatically.

2. **Configuration & Formatting Rules**:
   - Applies `PrinterConfig` for heading spacing, list styles, and meta formats.
   - Automatically invokes `ensure_document_id_with_config` to guarantee required metadata ID attributes.
