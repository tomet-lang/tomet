<!-- Generated from tmtroot/readme.tmt. Edit that, then `tomet export .`. -->

# Tomet

[English](README.md) | [日本語](docs/readmes/README.ja.md) | [简体中文](docs/readmes/README.zh-CN.md)

Tome to me.

Tomet is a human-readable, structured markup language and Rust toolchain.
It combines clear syntax for typed data and cross-document links with multi-format export.

## Example

```tmt
::id(example-overview)

=[ heading ]

- list #(id)
  -. orderd

@table
|[ col1 ][ col2 ][ col3 ]

@callout(info)
| content
```

## Try it

### 1. Install

Install `sisho` and `tomet` together using the [Quick install](docs/install_tools.md#quick-install):

```bash
nix profile install github:tomet-lang/sisho#toolchain
```

### 2. Write hello.tmt

Create a document `hello.tmt`:

```tmt
=[ Hello, Tomet ]

Tomet is a *structured* markup language.

- a list item
  -. an ordered child

@callout(note)
| A callout.

@table
|[ name  ][ kind     ]
|[ tomet ][ language ]
```

### 3. Check it

Validate and format your document:

```bash
sisho check .
sisho fix .
```

Continue with the [Quick Start](docs/quick_start.md).

## Learn more

- [Quick Start](docs/quick_start.md): walkthrough for creating, checking, and exporting documents.
- [Install](docs/install_tools.md): complete installation instructions for CLI tools and libraries.
- [Why Tomet](tmtroot/docs/why/root.tmt): design motivations and comparison with other formats.
- [Specification](tmtroot/spec/tome.tmt): the language syntax and semantics specification.
- [Editors](docs/install_tools.md#editors): extensions and setup for VS Code, Zed, Helix, and Neovim.
- [Contributing](CONTRIBUTING.md): guidelines for contributing to Tomet.

## Contributing

Contributions are welcome.
Repository layout, building from source, running tests, and verification steps are documented in [CONTRIBUTING](CONTRIBUTING.md).

