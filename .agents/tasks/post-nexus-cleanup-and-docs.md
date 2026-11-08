# Task: Post-Nexus Cleanup, Doc Regeneration, and Drift Elimination

Canonical tracking and execution instruction file for cleaning up pre-existing repository drift and regenerating all derived artifacts following the Nexus syntax unification.

- **Workspace:** Default root repository
- **Governing Rules:** `AGENTS.md`, `CONTRIBUTING.md`, root `.writ.tmt`

---

## 1. Core Philosophy & Non-Negotiable Rules

1. **Reject Ad-hoc Patches & Legacy Shims:**
   - All generated documents and legacy config files must cleanly obey the new unified Nexus & Data architecture.
   - Do NOT edit generated `.md` files directly. Edit `.tmt` sources or tooling and regenerate with `just docs` / `tomet export`.
2. **Read AGENTS.md First:**
   - Always run commands and builds in the Nix environment.
   - Always pass `--no-pager` to every `jj` command.
   - All commits require explicit user review and approval.

---

## 2. Issues & Scope of Work

### A. Regenerate Derived Documentation (`tmtroot/generated/**`)
- `tmtroot/generated/crates/*.tmt` still holds legacy `::meta{...}` blocks from previous versions of generator tooling.
- Run `tomet spec --all` to regenerate spec artifacts.
- Run `just docs` to regenerate exported `.md` documents, API docs, and formats across the repository.
- Verify `just docs-check` and `tomet export --check` pass.

### B. Migrate `catasto.ctmt` and `catasto.lock`
- `catasto.ctmt` currently fails `tomet check` due to unmigrated legacy syntax.
- `catasto.lock` has an unknown `mode` field reported by `sisho check` / `tomet check`.
- Align `catasto.ctmt` with the unified syntax and resolve lockfile drift.

### C. Resolve `.writ.tmt` Vocabulary Binding for `@crate(...)`
- In Commit 2, bare `(...)` became `Value::Seq`, causing `@crate(...)` elements inside sequences in `.writ.tmt` to be inspected by `tomet check`.
- Because `.writ.tmt` does not explicitly bind a vocabulary for `crate`, `tomet check` reports `unknown element crate`.
- Define the appropriate vocabulary binding (e.g. `::use(writ)` or crate vocabulary) in `.writ.tmt` so that `tomet check .` passes cleanly.

---

## 3. Verification Gates
- `cargo fmt --all --check`
- `cargo test --workspace`
- `cargo test -p tomet-tests`
- `just gen-treesitter-check`
- `twrit check .`
- `just writ-check`
- `tomet check .`
- `just docs-check`
