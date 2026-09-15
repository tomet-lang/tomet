# `tomet-search`

Pure, in-memory search and query engines for Tomet documents.

## Scope

`tomet-search` sits in Layer 4 of the Tomet crate hierarchy. It performs
**pure, in-memory querying and matching** without any filesystem I/O:

1. **Structural Query (`structural`)**:
   AST-aware querying by element tag (kind), property keys, and value substrings.
2. **Metadata Filter Query (`filter`)**:
   Predicate evaluation (`contains`, `exists`, `eq`, `gt`, `and`, etc.) and
   ordering (`by(field, "asc"|"desc")`) across collections of document metadata rows
   (the engine underlying `${filter(...)}`).

Filesystem scanning, path resolution, and disk caching are intentionally left to
higher layers (`tomet-workspace-indexer` and `tomet-workspace` in Layer 5).
