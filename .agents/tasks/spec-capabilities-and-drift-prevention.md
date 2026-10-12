# Specification Capabilities, Drift Prevention & Feature Tracking

## Context & Motivation

Following the establishment of Layer 3 Validator (`tomet-validator-{core, rule, blueprint}`)
and the integration of blueprint with `:rule` constraints, development exposed a fundamental friction:
currently, one must read the entire codebase across multiple crates to know what Tomet can actually
do, what invariants are enforced, and what remains unimplemented.

Without a single source of truth for current language capabilities and future feature tracking,
development slows down and implementation drifts from specifications.

To make development smooth and future feature expansion rapid, we need a mechanism that:
1. Keeps specifications strictly aligned with actual implementation (executable specs).
2. Prevents undocumented features ("wild implementations") through bidirectional linking.
3. Explicitly records upcoming / proposed features right alongside their corresponding specs.

---

## Core Pillars & Architecture

### 1. Executable Specifications
- Code examples written in `tmtroot/docs/spec/` must be tested against the actual engine,
  guaranteeing that "what the spec says is 100% verified to work right now".
- Eliminates stale documentation, speculative fiction, or silent drifts.

### 2. Bidirectional Linking & Drift Prevention
- **Code to Spec**: Crates and feature modules must link to their specification
  in their root inner doc-comments (`/// See @file(tmtroot/docs/spec/feature/...)`).
- **Spec to Code**: Specifications link to their authoritative implementation and tests.
- **Drift Prevention**: Ensure newly introduced features cannot exist without an accompanying spec
  (mechanically enforceable via `twrit` checks or link validations).

### 3. Coexistence of "Present" and "Future" in Specs
Every feature specification document follows a standard structure:
- **1. Specification (What works right now)**: Syntax, Semantics, Validation rules, Diagnostics.
- **2. Executable Examples**: Tested examples representing current capabilities.
- **3. Roadmap / Future Work (What we want to add next)**: Concrete wishlist, open ideas, and planned extensions.

---

## Open Tasks

### Spec Template & Core Feature Specifications
- [ ] **Standardize Spec Template**:
  - Establish the 3-section structure (`Specification`, `Examples`, `Roadmap`) across `tmtroot/docs/spec/feature/*.tmt`.
- [ ] **Fully Specify `:rule` (`tmtroot/docs/spec/feature/rule.tmt`)**:
  - *Current*: `allow: list(...)`, `direct: @bool`, targets (Element, Section, List).
  - *Future Wishlist*:
    - `require: list(...)` (mandatory element enforcement in sections/blocks).
    - Multiplicity constraints (`count: n`, `min: n`, `max: n`).
    - Attribute / argument constraints (`args: { ... }`).
- [ ] **Fully Specify `blueprint` (`tmtroot/docs/spec/feature/blueprint.tmt`)**:
  - *Current*: `@blueprint(target)`, required `@meta` keys, required sections, section `:rule` propagation, `tomet check` integration.
  - *Future Wishlist*:
    - Pattern-matched / wildcard headings.
    - Mechanical enforcement of `@rule(blueprint-shape)` (no `@blueprint` without `@kind(blueprint)`, bare positional args only).
    - `twrit` blueprint guard (`guard: { twrit: blueprint }`).

### Feature Matrix & Linkage
- [ ] **Promote `features.index.tmt` to Feature Matrix**:
  - Transform `tmtroot/docs/spec/features.index.tmt` into an authoritative capabilities table detailing status (Supported / MVP / Proposed) and links to spec files.
- [ ] **Bidirectional Linking in Crates**:
  - Add `@file(...)` spec links to `tomet-validator-rule`, `tomet-validator-blueprint`, and core modules.
  - Track mechanism to warn or fail if code lacks a spec counterpart.
