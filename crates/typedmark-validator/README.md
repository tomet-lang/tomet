# typedmark-validator

Not implemented yet -- reserved in the workspace for future `.tm` schema and
lint validation. See `docs/develop/architecture.md` for how this crate fits
into the rest of the pipeline.

## Intended role

- **Schema validation**: check `<T>(args)[content]{value}` usages against a
  project's `@settings{}` type declarations (arg names/types/required-
  ness, allowed `content` kind, placement, singleton-ness, etc.).
- **Cross-document reference resolution**: resolve targets like `file:`,
  `path:`, `wiki:`, `ref:` (e.g. `wiki:file_name#id`) against the actual
  filesystem/project and report broken links, ambiguous `wiki:` matches, etc.

`typedmark-parser` is a pure, side-effect-free static parser (see its
"Deterministic Static Parser Boundary" note in `docs/develop/architecture.md`)
and never does I/O or external file resolution. Both of the above require
touching the filesystem, so they belong here, not in the parser -- this crate
is the only place in the pipeline meant to look beyond a single source
string.
