# tomet-links

Vault-wide broken-link detection, target resolution, and SQLite-backed mtime caching.

## Architecture & Responsibilities

1. **Vault-Wide Link Integrity & Resolution Layer**:
   - `tomet-links` scans workspaces and checks `File`, `Embed`, `Tm`, and `Ref` links for validity against real files on disk.
   - External URLs (`Url`) and intra-document identifiers (`Id`) are deliberately excluded (handled by network checkers or `tomet-validator`).
   - SQLite cache database (`.tomet/links-cache.sqlite3`) avoids re-parsing unchanged files across runs based on filesystem modification timestamps (`mtime`).

2. **Core Capabilities**:
   - **Link Collection (`collect_links`, `DocumentLink`, `LinkKind`)**:
     - Traverses AST using `tomet-walker` and identifies links via `tomet-semantics::link_target_of`.
   - **Link Target Resolution (`check_vault`, `resolve_file_target`, `resolve_ref_target`)**:
     - `./` or `../`: Relative to referencing file's directory.
     - `/`: OS-absolute path.
     - Bare string: Relative to project root.
     - `Ref`: Case/extension-insensitive matching against file names and stems (wikilink convention).
     - `Tm`: Project-root-relative path with `#fragment` stripped.
   - **SQLite Incremental Cache (`LinkCache`)**:
     - Stores file `mtime` and extracted links with source span coordinates.
     - Returns `CacheOutcome::Hit` or `CacheOutcome::Miss`.
