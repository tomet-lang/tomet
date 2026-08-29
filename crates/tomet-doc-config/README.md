# tomet-config

Configuration discovery, loading, and styling rule definitions (`PrinterConfig`, `FieldConfig`).

## Architecture & Responsibilities

1. **Centralized Configuration Domain**:
   - `tomet-config` owns the configuration data structures (`PrinterConfig`, `FieldConfig`) and discovery/loading functions (`find_config_file`, `load_config_from_file`, `load_config_from_str`).
   - Split out of `tomet-printer` because finding and loading project configuration is a shared concern needed across multiple crates (`tomet-style`, `tomet-field-utils`, `tomet-printer`, `tomet-formatter`, `tomet-indexer`, `tomet-edit`, `tomet-tui`, and `apps/cli`) without requiring full document printing dependencies.

2. **Core Capabilities**:
   - **Configuration Loading & Discovery (`find_config_file`, `load_config_from_file`, `load_config_from_str`)**:
     - Searches upward through parent directories for `default.config.tmt` or `tomet.config.tmt`.
   - **Printer Configuration (`PrinterConfig`)**:
     - `meta_format`: Serialization format for metadata (`yaml`, `json`, `toml`).
     - `meta_always_newline`: Always put `@meta` entries on newlines.
     - `heading_space_inside_brackets`: Normalizes `#[ Title ]` vs `#[Title]`.
     - `link_no_space`: Controls spacing around link arguments.
     - `callout_content_style`: Content block styling for callout elements.
     - `list_multiline_style_content`: Content styling for multi-line list elements.
     - `ignore_files`: Glob/path prefixes to exclude from vault walks.
   - **Per-Field Metadata Rules (`FieldConfig`)**:
     - `meta_fields`: Map of field names to validation/generation rules (`field_type`, `format`, `offset`, `length`, `prefix`, `force`, `overwrite`).
