# tomet-resolver

Preprocessor and linker layer for external settings files, remote attribute connections, and `${id}` reference resolution.

## Architecture & Responsibilities

1. **Preprocessor / Linker Boundary (I/O Zone)**:
   - `tomet-resolver` is the preprocessor and linker layer of the Tomet toolchain.
   - Unlike `tomet-parser` (which strictly operates under the Deterministic Static Parser Boundary with zero I/O), `tomet-resolver` performs file reads and path resolution for referenced documents.
   - It reads and parses external files (e.g. `@settings(file:...)`) and extracts referenced data blocks.

2. **Core Capabilities**:
   - **Settings File Resolution (`settings.rs`)**:
     - `settings_file_ref`: Extracts project-root-relative file path from `@settings(file:...)` reference elements.
     - `resolve_settings_file`: Reads, parses, and extracts the top-level `@settings{...}` definition map from a file path.
     - `resolve_settings_ref`: Joins reference path with project root and resolves the settings block.
   - **Interpolation Reference Resolution (`interp.rs`)**:
     - `resolve_reference`: Resolves `${id}` and `${id.member}` interpolation expressions against same-document `{id:...}` / `(id:...)` nodes using `tomet-tree`.
     - Supports recursive member path navigation (e.g. `${node.meta.priority}`).
   - **Remote Attribute Connections (`connect.rs`)**:
     - `resolve_connect_targets`: Extracts `@references[ <id:target>:{...} ]` blocks and applies connected attributes across target elements in a `Document`, respecting direct attribute precedence (Direct > Connected).
