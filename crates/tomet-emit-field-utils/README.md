# tomet-field-utils

Small pure utility helpers for `@meta` field values, driven by `FieldConfig`.

## Responsibilities

1. **ID Generation & Validation**:
   - `generate_id_for_field`: Generates unique IDs using `nanoid` or `uuid` with optional prefixes.
   - `is_valid_id_format`: Validates ID structure against configured length and prefix rules.

2. **Timestamp Handling**:
   - `is_iso8601`: Validates date and datetime strings.
   - `format_rfc3339`: Normalizes ISO8601 datetime strings to RFC3339 format with configurable timezone offsets.
