# Connect Syntax Specification (コネクト構文仕様) (2026-08-19)

## 1. Overview & Motivation

Tomet's Connect Syntax (`:(){}`) introduces a clean, explicit way to connect and merge `(args)` attributes and `{value}` data into elements -- either immediately after an inline element/block (Inline Connection) or from a distance using an explicit ID target (Remote Connection).

This specification resolves design notes in `docs/idea/syntax-connection.tmt` and defines the frozen mechanics for 0.1 -> 1.0.

---

## 2. Syntax Patterns

### 2.1 Inline Connection (`:(){}`)

Connects attributes or data immediately to the preceding block/inline element or list item using a colon `:` separator.

```tm
// Heading with inline connection
#[ Overview ]:{ id: intro, tag: main }

// List item with inline connection
- ( ) Task 1 :{ id: task1, priority: high }
```

### 2.2 Remote Connection inside `@references[...]`

Targets specific elements by ID using the explicit `<id:target_id>` sigil inside an `@references[...]` block.

```tm
// Document body
- ( ) Task A { id: taskA }
- ( ) Task B { id: taskB, priority: low }

// Remote connection container block
@references[
  <id:taskA>:{ priority: high, tag: dev }
  <id:[taskA, taskB]>:{ status: pending }
]
```

*Note on `:[]` (content connection)*: Connecting `[content]` is explicitly deferred/deferred for post-1.0 to preserve syntax stability. Connect syntax in v1 strictly operates on `(args)` and `{value}`.

---

## 3. Merging & Precedence Rules

When merging connected `(args)` or `{value}` data into a target element:

1. **Individual Direct Attributes Win (Direct > Connected)**:
   - For scalar and map properties, the value directly declared on the element itself takes precedence over values supplied via connect syntax.
   - Connected values serve as default / shared values when an individual explicit attribute is absent.
   - *Example*: If `Task B` explicitly specifies `{ priority: low }`, a remote `<id:taskB>:{ priority: high }` will not overwrite `low`.

2. **Sequence Merging (`Value::Seq`)**:
   - For array/sequence values (e.g. `tags: [a, b]`), items from the connected syntax are merged (concatenated) with the element's direct array.

---

## 4. Parser & AST Integration Strategy

- **Parser Layer (`tomet-parser`)**:
  - Recognizes `:<args>` / `:{value}` after headings, list items, and elements.
  - Recognizes `<id:name>` as a targeted connect element `Sigil::Type("id:name")` or specialized `Sigil::ConnectTarget(String)`.
- **Semantics & Resolution Layer (`tomet-semantics` / `tomet-resolve`)**:
  - Performs the attribute merge according to the precedence rules above.
