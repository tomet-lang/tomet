<!-- Generated from tmtroot/contributing.tmt. Edit that, then `tomet export .`. -->

# Contributing to Tomet

## Documentation Policy

- All Markdown files (`README.md`, `AGENTS.md`, `CONTRIBUTING.md`, etc.) are generated artifacts.
Never edit `.md` files directly.
- Always edit the source `.tmt` files under `tmtroot/` (or crate-local `.tmt` files).
- Run `tomet export` to update Markdown files, and verify with `tomet export --check`.

## Commit Convention

All commits must follow the Conventional Commits format: `type(scope): description`.

### Types

- `feat`: New feature or capability.
- `fix`: Bug fix.
- `docs`: Documentation changes (`.tmt` or exported docs).
- `refactor`: Code restructuring without changing behavior.
- `perf`: Performance improvement.
- `test`: Adding or correcting tests.
- `chore`: Build scripts, dependencies, tooling, repo maintenance.

### Scopes

Use the affected component or crate name as the scope:

- **Apps**: `cli`, `lsp`, `tui`
- **Syntax & Semantics**: `parser`, `lexer`, `ast`, `syntax`, `semantics`, `validator`
- **Converters & Format**: `markdown`, `html`, `typst`, `pandoc`, `format`
- **Workspace & IO**: `workspace`, `load`, `vault`, `config`, `links`
- **Bindings**: `js`, `python`, `java`
- **Editors**: `vscode`, `zed`, `helix`, `neovim`
- **Infra / Meta**: `nix`, `ci`, `deps`, `rules` (for `.writ.tmt`)

> [!tip]
> If a change spans multiple components or the whole workspace,
> omit the scope or use a broad category (e.g. `refactor(crates): ...` or `chore: update dependencies`).

## Verification

Before committing or submitting changes, ensure all checks pass:

- `cargo check` / `cargo build` / `cargo test`
- `tomet export --check`: Ensure documentation artifacts are up to date.
- `tomet api --check`: Ensure Rust API documentation matches doc-comments.

