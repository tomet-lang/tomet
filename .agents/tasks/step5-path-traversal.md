# Step 5: Path Traversal (`#id`, `.prop`, `.item(n)`, `.prop(k)`)

Source sketch: `tmtroot/docs/spec/syntax/addressable.tmt`
Parent task: `.agents/tasks/addressable.md`

## Overview
Implement Step 5 of the addressable system:
1. Support `#id` syntax inside `${...}` (e.g. `${#intro}`) as `InterpExprKind::Id(String)`.
2. Implement element projection and property traversal:
   - On `Element`:
     - `.prop`: Merged properties map (`Value::Map` from `args` and `value`)
     - `.id`: Element ID string (`Value::String`)
     - `.sigil`: Element name (`Value::String`)
     - `.connects`: Connected elements list
     - `.content`: Content blocks
     - `.prop(k)`: Lookup key `k` in properties
     - `.item(n)`: n-th content block
   - On `Value`:
     - `.key`: Member access on `Value::Map` (e.g. `${#intro.prop.tags}`)
     - `.item(n)`: n-th element of `Value::Seq` (e.g. `${#intro.prop.tags.item(0)}`)
     - `.prop(k)`: Function-call property access fallback
3. Integrate with `tomet-compute` so `#id` expressions cleanly resolve via `tomet-address`.

## Steps
- [x] 1. AST & Parser: Add `InterpExprKind::Id(String)`, parse `${#id...}` in `tomet-syntax-parser`
- [x] 2. Address resolution: Implement path traversal in `tomet-semantics-address`
- [x] 3. Compute integration: Connect `InterpExprKind::Id` in `tomet-semantics-compute`
- [x] 4. Verification & tests: Unit tests for parser, traversal, and full evaluation
