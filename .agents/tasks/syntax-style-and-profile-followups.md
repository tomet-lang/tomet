# syntax-style-and-profile-followups

Pending decisions, measured facts, and future roadmap learned from the `tomet-spelling` and `tomet-syntax-style` work (committed on `main` up through `fe4b3fda`).

## 1. The restricted syntaxes (`oneline`, `standalone`)

A first slice of profile admission was prototyped and parked to focus on formatter/printer unification. What was measured and designed:

- **`oneline` rules**:
  - `tmtroot/dict/tomet.dict.tmt` is `::profile(oneline)`: a ONE-LINE `[ ]` (`;@term("tomet")[ Tomet ]`) is oneline; a `[ ]` holding a line break (`@x[ one\ntwo ]`) is not. A `|` run is (`;@term("v")\n| text`).
  - Read from the CST (which keeps `NEWLINE`): inside a form, every `NEWLINE` must be followed by a `|` (`PIPE` or `MARGIN`); `ROOT`, `DOCUMENT`, `SECTION`, `LIST` hold line breaks as lines of their own.
  - In the dictionary, only `BLOCK_FORM` and `ROOT` hold a `NEWLINE`; multi-line `[ ]`, multi-line `( )`, and multi-line paragraphs hold bare line breaks.
- **Equivalences (same `File`)**:
  - `@x[ one\ntwo ]` = `@x\n| one\n| two`
  - `;@term("v")[ one two ]` = `;@term("v")\n| one two`
  - `a\n\nb` = `a\n;b`
  - All three `ContentLayout`s = `[ one\ntwo ]`
  - Not equivalent: a bare multi-line paragraph has no `|` spelling that keeps the `File`.
  - Both paragraph separators are admitted by oneline (a blank line separates one-line paragraphs).
- **Architecture & open questions**:
  - Admission lives in a crate of its own at layer 3: `tomet-semantics-profile` (next to `tomet-validator-profile`); a rule over facts of a spelling, not a hand-maintained list per profile.
  - First consumer candidates:
    1. The `oneline` check in `tomet-validator-profile` (spec lists under 未実装; needs source text, which `tomet check` has).
    2. A `|` run layout in formatter and printer (`ContentLayout` gaining `Pipe`).
    3. Generated documentation of what a profile admits and removes.
  - See also: `.agents/tasks/oneline-profile-and-vault-separator.md`.

## 2. Architecture of `tomet-spelling`

Discussed with the author (2026-11-10). Decided later the same day: path 2 below -- the parsers read a name through
`tomet-spelling`'s rule, so there is one rule and not two kept equal by `tests/src/spelling.rs`. How: spelling gets a scan ("how
many bytes from here are a name"); `is_name` is that scan covering the whole text; the CST name rule
(`tomet-syntax-cst-parser` `name.rs`, `slot.rs`) calls it. Not read yet: what the scan must be given (the lexer reads a word of
any letters as one `IDENT`, the CST decides "ASCII only"). Strings wait for `escape.tmt` (below). The crate stays: `serde_cotomet`
(layer 2) cannot use anything above it.

- **Users**: `serde_cotomet`, `tomet-edit`, `tomet-printer` (all writers, no readers).
- **Two architectural paths**:
  1. The reader (the parser) exposes `is_name` and later key/value rules built on its token rules. Writers' answers match the parser by construction. `tomet-spelling` dissolves: key/string writers move into `tomet-syntax-style`, and name rules stay with the parser.
  2. The parser uses string rules kept in `tomet-spelling`. The crate stays a shared leaf without `tomet-ast`.
- **String escapes (`\r`)**:
  - Readers: `unescape_quoted_string` (`tomet-syntax-lowering/src/element.rs`) and `parse_quoted` (`.../interp.rs`).
  - Writers: `tomet-ast`'s `Display` `write_quoted` and `spelling::write_quoted`.
  - Currently, writers emit CR as `\r`, which the parser reads back as `r` (only `\"`, `\\`, `\n`, `\t` supported).
  - Waits for the escape specification being designed in `tmtroot/spec/syntax/escape.tmt`.

## 3. Smaller followups and pending alignments

- **`meta.<field>.always_newline`**:
  - List fields written as `tags:\n  - a` lines, and per-field `type` / `format` / `prefix` rules, remain printer-only (they alter how values are represented).
- **`link.no_space` option naming**:
  - Only ever concerned a `target` alone in an element's args (`@link(target: x)` vs `@link(target:x)`). Decide whether to rename the setting, generalize to all lone arguments, or deprecate.
- **`map.align_values`**:
  - Currently a plain `bool` (off = leave as written), whereas related options (`heading_brackets`, `link_target_colon`, `field_layout`) are three-state enums.
- **Crate directory renaming**: done (`crates/tomet-formatter`, `crates/tomet-printer`; layer 6 lists both by name).
- **Tree-sitter regeneration**:
  - Parser grammar was updated for ASCII name restrictions. Author will run `! just gen-treesitter` with pinned tree-sitter 0.26.12 to regenerate `parser.c`, `grammar.json`, `node-types.json`.
- **Bulk documentation passes**:
  - `tmtroot/generated/config.tmt` still describes `callout.style.content` as "block, quote" and references old crate names.
  - `tmtroot/spec/cli/format.tmt` references `format.group_order`.
  - To be updated in a later documentation pass.
- **Other Unicode name checks**:
  - `is_attribute_key` (`tomet-semantics/src/flatten.rs`), LSP completion prefixes, `tail_validity.rs` (`#(id)`). Align whether they follow the ASCII name rule.
