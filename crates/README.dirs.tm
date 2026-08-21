#[ crates/ directory layout ]

This directory is flat. Grouping is expressed through the crate name's
prefix family, not through subdirectories -- see
`docs/develop/architecture.md` for what each crate actually does and
how they depend on each other; this file is just the naming map.

Each package's Cargo.toml `name` is unprefixed by family (e.g.
`typedmark-ast`, unchanged since before this naming scheme) even though
its directory carries the family prefix (`typedmark-syntax-ast`) --
directory name and package name are allowed to differ, and this is
intentional: `use typedmark_ast::...` and every dependency reference
stays exactly as short as it always was. Only the directory name (and
therefore path-lookup) carries the family grouping.

<codeblock>(lang:text)[
typedmark-syntax-*      grammar/parsing: ast, lexar, parser.
                         Turns text into a Document/Value.

typedmark-semantics*     semantics (bare, no suffix -- the crate IS
                         the category), semantics-validator.
                         What a parsed Element means / whether it's
                         valid.

typedmark-doc-*          walker, resolver, compute, config, edit,
                         indexer.
                         Operations that take a whole Document (or a
                         directory of them) as their primary input.

typedmark-codegen-*      html, markdown.
                         Conversion to/from an *external* document
                         format.

typedmark-emit-*         field-utils, style, printer, formatter.
                         .tm text production, config-driven. Named
                         "emit" rather than "format" specifically so
                         `typedmark-formatter` doesn't become
                         `typedmark-format-formatter` -- "format"
                         is a substring of "formatter", so that
                         particular pairing reads as redundant in a
                         way `semantics`/`semantics-validator` (whose
                         member's name doesn't contain the category
                         word) doesn't.

serde_typedmark,          external-ecosystem adapters. Follow their
tree-sitter-typedmark     host ecosystem's own naming convention
                         (serde_*, tree-sitter-*) rather than
                         typedmark's -- that's what makes them
                         discoverable to serde/tree-sitter tooling.
]

`typedmark-tui` lives in `apps/tui`, not here -- it's consumed as a
subcommand of `apps/cli`, not depended on by any other crate, so it's
an application rather than a library.

Moving a crate only requires updating its path in the root
`Cargo.toml` (`members` list and `[workspace.dependencies]` `path =`
value) -- package names and `{ workspace = true }` dependency keys
never change, so no `use` statement anywhere needs touching. Watch out
for `include_str!`/`include_bytes!` literals using `../`-relative paths
reaching outside the crate's own directory (a few tests reach up to
`docs/tests/`), and for hardcoded `.parent().unwrap()` chains walking
up to the repo root -- both are sensitive to how many directories deep
the crate sits, which a rename alone doesn't change, but a nesting
depth change would.
