# serde_tomet

Serde serializer and deserializer for Tomet's pure data subset (`tomet_ast::Value`).

## Responsibilities

1. **Serialization & Deserialization**:
   - `from_str`: Deserializes data-only `.tmt` text (maps, sequences, scalars) into Rust structs implementing `serde::Deserialize`.
   - `to_string`: Serializes Rust types implementing `serde::Serialize` into `.tmt` format.

2. **Integration with Serde Data Model**:
   - Maps 1:1 with standard serde types (numbers, booleans, strings, sequences, maps, structs, enums).
