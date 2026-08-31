# tomet-indexer

Directory scanning, file classification, ignore filtering, and metadata cataloging for Tomet workspaces.

## Architecture & Responsibilities

1. **Workspace Scanning & Classification Domain**:
   - `tomet-indexer` is responsible for discovering `.tm`/`.tmt` and `.md` files in a workspace.
   - Respects `.gitignore`, hidden files, and `ignore_files` rules configured in `default.config.tmt` / `tomet.config.tmt`.
   - Used by CLI commands (`check`, `export`, `format`, `lint`) and TUI components (Explorer, BatchMeta, Migration view).

2. **Core Capabilities**:
   - **Path Filtering & Discovery (`is_path_ignored`, `collect_tm_files`, `collect_all_paths_with_config`)**:
     - Fast directory walks using `ignore::WalkBuilder`.
     - Supports directory-prefix, exact-path, and relative-prefix ignore matching.
   - **Metadata Extraction (`extract_metadata`)**:
     - Extracts `@meta` and `@config` key-value pairs (`meta.author`, `config.lang`).
   - **Stateful In-Memory Workspace Index (`WorkspaceIndex`)**:
     - Flat in-memory catalog (`BTreeMap<PathBuf, EntryKind>`) with zero-I/O derived views.
     - Single-child directory chain compaction for explorer tree (`tree_nodes`).
     - Markdown-to-Tomet migration candidate listing (`migration_candidates`).
     - Incremental path updates (`refresh_path`, `remove_path`) and complete re-indexing (`rebuild`).
