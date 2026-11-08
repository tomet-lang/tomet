# Nexus syntax: `()` args, `[...]` prose sugar, `{}` / `#()` connect sugar, singular `::path(...)`, and `()` seq

Decided by the author on 2026-11-07 and unified on 2026-11-09 (see `tmtroot/dev/design/nexus-brackets-and-singular-declarations.tmt`).

## Decided

- **Nexus & Primary Group:**
  - A nexus (`@name`, `::path`) requires a primary group — `(...)` (arguments) or `[...]` (prose sugar) — or an explicit block body (`|` rows of `@table`).
  - `@name` alone, `@name{}` alone, and `@name#(id)` alone are invalid.
- **Two-Layer Group Architecture (Arguments vs. Connects):**
  1. **Argument Space (`()` and `[]`):**
     - `()` holds the nexus's named parameter map.
     - `[...]` is syntactic sugar for one key inside `()`: `@name(a: 1)[prose]` $\equiv$ `@name(a: 1, prose: [prose])`.
     - The default sugar key name is **`prose`**; a declaration or element definition may override it (e.g. `label` on `@link`, `about` on `::field` / `::var` / `::element`, `body` on `@callout`).
     - Writing both the sugar key inside `()` and `[...]` outside `()` is a `DuplicateKey` error; formatter normalization extracts the sugar key into `[...]`.
  2. **Connect Space (`::name(...)` and its `#()` / `{}` sugar):**
     - Every declaration and postfix connect is a child nexus connected via `::` (`Nexus::Nexus`).
     - At the start of a file (preamble), the left-hand `self` is omitted: `(self)::meta(...)` $\to$ `::meta(...)`.
     - Postfix on an element, section, or list item, connects are written `::name(...)` (unifying old `:name(...)` into `::name(...)` so single `:` is reserved strictly for `key: value`).
     - **`#(id)` is syntactic sugar for `::id(id)`** (`@name(...)#(x)` $\equiv$ `@name(...)::id(x)`).
     - **`{...}` (or `::{...}` after prose/headings/list items) is syntactic sugar for `::meta(...)`** (`@name(...){a: 1}` $\equiv$ `@name(...)::meta(a: 1)`).
- **Singular Root Declarations, Dot-Paths & Self-Slot Contraction (`X.X == X`):**
  - Built-in declaration roots are all singular: `id`, `kind`, `version`, `profile`, `use`, `tag`, `rule`, `meta`, `macro`, `var`, `format`, `field`, `workspace`, `export`, `migration`, `vocabulary`, `dictionary`, `blueprint`, `element`.
  - Target keys may be written either as a dot-path on the declaration head (`::field.author(type: string)[著者]`, `::macro.copyright("...")`, `::format.meta(always_newline: true)`) or grouped under the parent (`::macro(copyright: "...")`); both fold into the exact same tree.
  - **Self-slot contraction (`X.X == X`):** The 1st positional argument of any nexus `X` binds to its same-named slot `X.X`, which contracts to `X` itself (`::id(a)` $\equiv$ `::id(id: a)` $\implies$ `id.id == id == a`; `@typst.typst == @typst`; `::kind(config, 2)` $\equiv$ `::kind(kind: config, version: 2)` so `self::kind == "config"` and `self::kind.version == 2`).
- **Bare `(a, b, c)` Sequence Literal in Value Position:**
  - In value positions (`key: (a, b, c)` or nested sequence elements), bare parentheses `( ... )` represent `Value::Seq` (array/tuple), including 1-element `(x)` and empty `()`.
  - `list(...)` is removed from the data value space and reserved strictly for the type space (`type: list(string)`).
- **Dictionary & Vocabulary Document Forms:**
  - Dictionary documents declare `::dictionary(name)` in the preamble (no outer `{}` or `|` wrapper) and place `;@term("word")[description]#(id)` blocks directly in the document body.
  - Vocabulary documents declare `::vocabulary.<ns>(version: "1.0.0")` and `::element.<name>(...)[about]` / `::element.<name>.arg.<param>(...)[about]`.
- **Where facts live afterwards:**
  - Grammar and AST docs go to `tomet-syntax-cst-parser` and `tomet-syntax-ast-parser` `//!` comments, and user-facing specs go to `tmtroot/spec/`. Delete this file once all phases are folded.
