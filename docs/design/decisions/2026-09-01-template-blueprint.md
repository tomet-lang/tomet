# Template & Blueprint Architecture Decision (2026-09-01)

## Overview

This document defines the architecture and specification for Tomet's template and archetype system powered by the `@blueprint` directive.

A Blueprint in Tomet serves two integrated purposes:
1. **Scaffolding / Generation (Creation Time)**: Used by `tomet new` to instantiate new documents by expanding `${...}` expressions (`${uuid()}`, `${date()}`, `${vars.*}`) and converting `@blueprint` into `@kind`.
2. **Structural Validation (Lifetime / Linting)**: Used by `tomet-validator` and `tomet-lsp` to ensure documents declaring a `@kind` adhere to the structural requirements defined in the corresponding `@blueprint`.

---

## 1. Syntax & Declaration

A blueprint is a `.tmt` document declaring the `@blueprint` directive at the top.

### 1.1 Basic Blueprint

```tmt
@blueprint(daily-note)
@meta {
  id: ${uuid()}
  date: ${date("YYYY-MM-DD")}
  tags: [ daily ]
}

#[ Plan ] {id: plan}
- ( ) 

#[ Review ] {id: review}
```

### 1.2 Extended Blueprint with Variables Schema (`{value}` block)

```tmt
@blueprint(rfc){
  description: "RFC Document Template"
  vars: {
    title: { type: string, prompt: "RFC Title" }
    author: { type: string, default: "Anonymous" }
  }
}
@meta {
  id: ${uuid()}
  title: ${vars.title}
  author: ${vars.author}
  status: draft
}

#[ Summary ] {id: summary}

#[ Motivation ] {id: motivation}
```

---

## 2. Scaffolding & Instantiation Lifecycle (`tomet new`)

When a document is created from a blueprint (e.g. `tomet new docs/daily/2026-09-01.tmt --template daily-note`):

1. **Directive Transformation**:
   - The `@blueprint(target)` element is transformed into `@kind(target)`.
   - Any `{value}` schema block attached to `@blueprint` is stripped from the output document.
2. **Interpolation & Evaluation**:
   - Dynamic `${...}` expressions throughout the AST (and within string literals / attributes) are evaluated using `tomet-compute`:
     - `${uuid()}`: Generates a new UUID v4.
     - `${date()}` / `${date(fmt)}`: Evaluates current local date.
     - `${time()}`: Evaluates current local time.
     - `${filename}`: Current output file name.
     - `${title}`: Explicit or inferred title.
     - `${vars.<name>}`: Explicitly passed `--var <name>=<value>` or default values.
3. **Output**:
   - The resulting AST is formatted and saved to disk via `tomet-printer` / `tomet-formatter`.

---

## 3. Blueprint Structural Validation (`tomet-validator`)

For documents declaring `@kind(T)`:

1. **Discovery**: The validator resolves the corresponding `@blueprint(T)` from project templates (configured in `@config.templates` or under `templates/`).
2. **Structural Rules**:
   - **`@meta` fields**: All keys defined in the blueprint's `@meta` block are required to exist in the document.
   - **Identified Headings**: Headings bearing `{id: ...}` attributes in the blueprint (e.g., `#[ Plan ] {id: plan}`) are considered required structural sections.
   - **Unidentified Headings & Body Elements**: Unidentified headings, list items (`- ( )`), and code blocks are considered optional starter content and may be modified or deleted without validation errors.
3. **LSP Integration**:
   - Diagnostic warnings for missing required sections or metadata keys.
   - Quick Fix actions to insert missing blueprint sections.

---

## 4. Pipeline & Crate Architecture

- **`tomet-semantics`**: Defines `ElementKind::Blueprint` and classifies `@blueprint` elements.
- **`tomet-compute`**: Provides evaluation support for `uuid()`, `date()`, `time()`, and arbitrary context lookup.
- **`tomet-transform`**: Provides AST transforms for blueprint instantiation (`instantiate_blueprint`).
- **`tomet-workspace`**: Template discovery and file instantiation (`instantiate_file`).
- **`tomet-validator`**: Implements `validate_blueprint(doc, blueprint)`.
- **`apps/cli`**: Exposes `tomet new <path> [--template <name>] [--var key=value]`.
