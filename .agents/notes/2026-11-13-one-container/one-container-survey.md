# Survey and AST Proposal: One-Container

## A. What exists today (facts only)

This section reports the exact representation of data containers, arguments, groups, literal constructors, and positional-to-named argument bindings in the current codebase. Every statement is substantiated with a `path:line` reference.

### 1. Value representations: `(a, b)`, `{k: v}`, `name(a, b)`

- **Sequence `(a, b)`**:
  - Represented in the AST as `Value::Seq(Vec<Value>)` ([crates/tomet-syntax-ast/src/lib.rs:83](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L83)).
  - Lowered from CST sequence nodes in `lower_single_value_from_children` ([crates/tomet-syntax-lowering/src/element.rs:671](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L671)) and in `lower_value` ([crates/tomet-syntax-lowering/src/value.rs:18](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/value.rs#L18)).
  - An empty sequence `()` produces `Value::Seq(Vec::new())`, and a single-item sequence `(x)` produces `Value::Seq(vec![x])` ([crates/tomet-syntax-ast/src/lib.rs:82-83](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L82-L83)).
- **Map `{k: v}`**:
  - Represented in the AST as `Value::Map(Vec<(String, Value)>)` ([crates/tomet-syntax-ast/src/lib.rs:86](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L86)), preserving insertion order.
  - Lowered from CST map nodes in `lower_single_value_from_children` ([crates/tomet-syntax-lowering/src/element.rs:667](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L667)) via `lower_map_entries` ([crates/tomet-syntax-lowering/src/element.rs:623-639](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L623-L639)) and in `lower_value` ([crates/tomet-syntax-lowering/src/value.rs:24](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/value.rs#L24)).
- **Call `name(a, b)`**:
  - Represented in the AST as `Value::Call(String, Vec<Value>)` ([crates/tomet-syntax-ast/src/lib.rs:105](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L105)).
  - Arguments are positional only: `Vec<Value>` ([crates/tomet-syntax-ast/src/lib.rs:88-89](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L88-L89)).
  - Lowered in `lower_single_value_from_children` ([crates/tomet-syntax-lowering/src/element.rs:608-609](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L608-L609)) via `resolve_call` ([crates/tomet-syntax-lowering/src/element.rs:927-943](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L927-L943)). When `name` is not a literal constructor, `resolve_call` constructs `Value::Call(name.to_string(), args)` ([crates/tomet-syntax-lowering/src/element.rs:932](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L932)).

### 2. A nexus's `(args)`: positional, named, and combined

- A nexus node (`Element`, `Declaration`, `Connect`, `Hole`, `Section`, `ListItem`) holds arguments in the `args` field of `Data`: `pub args: Option<Value>` ([crates/tomet-syntax-ast/src/lib.rs:916](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L916)).
- Arguments are parsed and lowered in `lower_paren_value` ([crates/tomet-syntax-lowering/src/element.rs:47-77](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L47-L77)):
  - **Empty `()`**: lowered to an empty map `Value::Map(Vec::new())` ([crates/tomet-syntax-lowering/src/element.rs:58](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L58)).
  - **Single positional argument `(x)`**: unboxed directly to the bare value `x` (e.g. `Value::String`, `Value::Int`) rather than wrapped in a collection ([crates/tomet-syntax-lowering/src/element.rs:74](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L74)).
  - **Named arguments only `(k: v, ...)`**: lowered to `Value::Map(entries)` ([crates/tomet-syntax-lowering/src/element.rs:76](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L76)).
  - **Multiple positional arguments `(x, y)`**: lowered to `Value::Map(entries)` where positional entries receive the empty string sentinel key `""` (`tomet_semantics_core::positional::POSITIONAL_ENTRY_KEY`, [crates/tomet-semantics-core/src/positional.rs:15](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-core/src/positional.rs#L15); [crates/tomet-syntax-lowering/src/element.rs:76](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L76)).
  - **Positional and named together `(x, y, k: v)`**: lowered to a single `Value::Map(entries)` containing both `("", x)` and `("k", v)` entries in their exact source order ([crates/tomet-syntax-lowering/src/element.rs:76](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L76)).

### 3. A nexus's `{…}` group and non-pair entries

- Stored in `Data.value: Option<Group>` ([crates/tomet-syntax-ast/src/lib.rs:917](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L917)).
- `Group` is a newtype around a vector of entries: `pub struct Group(pub Vec<Entry>)` ([crates/tomet-syntax-ast/src/lib.rs:1102](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1102)).
- The entries inside `{...}` are represented by the `Entry` enum ([crates/tomet-syntax-ast/src/lib.rs:1079-1087](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1079-L1087)):
  - `Entry::Pair(String, Value)` ([crates/tomet-syntax-ast/src/lib.rs:1080](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1080)): key-value attribute pair.
  - `Entry::Element(Element)` ([crates/tomet-syntax-ast/src/lib.rs:1082](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1082)): a named element `@name(...)` sitting as a keyless entry inside a group.
  - `Entry::Bare(Bare)` ([crates/tomet-syntax-ast/src/lib.rs:1084](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1084)): a nameless entry `(marker)[content]{value}` carrying groups but no name.
  - `Entry::Hole(Hole)` ([crates/tomet-syntax-ast/src/lib.rs:1086](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1086)): a `<?matcher(...)>` pattern hole sitting as an entry.
- Lowered in `lower_value_group` ([crates/tomet-syntax-lowering/src/element.rs:190-375](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L190-L375)), which iterates CST children sequentially:
  - Key-value pairs lower to `Entry::Pair` ([crates/tomet-syntax-lowering/src/element.rs:212, 248](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L212)).
  - CST element nodes lower to `Entry::Element` ([crates/tomet-syntax-lowering/src/element.rs:280](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L280)).
  - CST bare forms lower to `Entry::Bare` ([crates/tomet-syntax-lowering/src/element.rs:344](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L344)).
  - CST hole forms lower to `Entry::Hole` ([crates/tomet-syntax-lowering/src/element.rs:361](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L361)).

### 4. Literal constructors resolution and transformations

- Where resolved: In `crates/tomet-syntax-lowering` within `resolve_call` ([crates/tomet-syntax-lowering/src/element.rs:927-943](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L927-L943)), called directly by `lower_single_value_from_children` ([crates/tomet-syntax-lowering/src/element.rs:608-609](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L608-L609)).
- Exactly what each one does:
  - `string(arg)` ([crates/tomet-syntax-lowering/src/element.rs:945-958](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L945-L958)):
    - Requires 0 or 1 argument; if `> 1`, falls back to `Value::Call`.
    - If 0 args, returns `Value::String(String::new())`.
    - If 1 arg, tests whether the argument has a scalar representation via `arg.scalar_text()`. If so, converts to `Value::String(text.into_owned())`. If `arg` is non-scalar (`Seq`, `Map`, `Call`, `Element`, `Blocks`), refuses conversion and returns `Value::Call("string".to_string(), args)`.
  - `int(arg)` ([crates/tomet-syntax-lowering/src/element.rs:960-986](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L960-L986)):
    - Requires 0 or 1 argument; if `> 1`, falls back to `Value::Call`.
    - If 0 args, returns `Value::Int(0)`.
    - If 1 arg: converts `Value::Int(i)` directly; converts `Value::Float(f)` by truncating (`f.trunc() as i64`); converts `Value::Number(s)` or `Value::String(s)` by parsing signed decimal integers, or hex/octal/binary prefixes (`0x`, `0o`, `0b`). If string parse fails as integer, attempts float parse and truncation. If not parsable or if argument is composite, returns `Value::Call("int".to_string(), args)`.
  - `float(arg)` ([crates/tomet-syntax-lowering/src/element.rs:988-1010](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L988-L1010)):
    - Requires 0 or 1 argument; if `> 1`, falls back to `Value::Call`.
    - If 0 args, returns `Value::Float(0.0)`.
    - If 1 arg: converts `Value::Float(f)` directly; converts `Value::Int(i)` to `f64`; converts `Value::Number(s)` or `Value::String(s)` by parsing as `f64` (including `"nan"`, `"inf"`, `"+inf"`, `"-inf"`). If parse fails or argument is composite, returns `Value::Call("float".to_string(), args)`.

### 5. Positional argument naming and declaration binding

- **Where parameter signatures are defined**:
  - Built-in element signatures: defined in `tomet-semantics` at `builtin_positional_arg_keys(kind)` ([crates/tomet-semantics/src/positional.rs:15-45](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/positional.rs#L15-L45)). E.g., `Link` -> `["target"]`, `File`/`Dir` -> `["target"]`, `Raw` -> `["lang"]`, `Callout` -> `["variant"]`, `Task` -> `["status"]`.
  - Declaration signatures: defined in `tomet-semantics-declaration` at `DeclKind::positional_keys()` ([crates/tomet-semantics-declaration/src/kind.rs:135-155, 187-200, 412-425](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-declaration/src/kind.rs#L135-L155)). E.g., `Kind` -> `["name"]`, `Blueprint` -> `["target"]`, `Id` -> `["id"]`, `Use` -> `["namespace"]`, `Field` -> `["type"]`.
  - Vocabulary user-element signatures: defined via `@param` declarations inside `@element` vocabulary documents, marked with `positional: true` ([crates/tomet-semantics/src/vocabulary.rs:43, 130-136](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/vocabulary.rs#L43)).
- **Where binding is applied**:
  - Layer 3 crate `tomet-semantics-core`: `bind_positional` ([crates/tomet-semantics-core/src/positional.rs:30-56](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-core/src/positional.rs#L30-L56)).
  - It iterates the `entries` of `Value::Map`: for each entry where `k.is_empty()` (or matches `POSITIONAL_ENTRY_KEY`), it assigns the parameter name from `sig[pos_idx]`, advancing `pos_idx`.
  - Normalization entry points in `tomet-semantics`:
    - `normalized_element_args_in` ([crates/tomet-semantics/src/positional.rs:82-108](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/positional.rs#L82-L108)).
    - `normalized_declaration_args_in` ([crates/tomet-semantics/src/positional.rs:110-126](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/positional.rs#L110-L126)).
    - `element_data` and `declaration_data` ([crates/tomet-semantics/src/data.rs:14-45](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/data.rs#L14-L45)).

---

## B. Inventory of every reader and writer

Every non-test Rust site (crates, apps, bindings, benches, and `tests` package library code) that constructs or matches `Value::Seq`, `Value::Map`, `Value::Call`, `EvalValue::Seq`, `EvalValue::Map`, `EvalValue::Call`, `Group`, `Entry::*`, or reads `Data`.

Crates are grouped in Layer order according to `.writ.tmt` (Layers 2 through 8).

### Layer 2: Syntax & Ecosystem Adapters

| path:line | what the code does | what it asks of the value |
|---|---|---|
| [crates/tomet-syntax-ast/src/lib.rs:83](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L83) | Declares `Value::Seq(Vec<Value>)` variant | `is-it-a-list`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:86](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L86) | Declares `Value::Map(Vec<(String, Value)>)` variant | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:105](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L105) | Declares `Value::Call(String, Vec<Value>)` variant | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:203](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L203) | Serializes `Value::Seq` to JSON array | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-syntax-ast/src/lib.rs:211](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L211) | Serializes `Value::Map` to JSON map | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-syntax-ast/src/lib.rs:219](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L219) | Serializes `Value::Call` to JSON object `{"call": ..., "args": ...}` | `callee-name`, `walks-all-children` |
| [crates/tomet-syntax-ast/src/lib.rs:303](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L303) | Deserializes JSON sequence to `Value::Seq` | `is-it-a-list`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:314](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L314) | Deserializes JSON map to `Value::Map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:916](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L916) | Declares `Data` struct holding `args: Option<Value>` and `value: Option<Group>` | `by-name`, `walks-all-children` |
| [crates/tomet-syntax-ast/src/lib.rs:1080](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1080) | Declares `Entry::Pair(String, Value)` variant | `by-name`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:1082](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1082) | Declares `Entry::Element(Element)` variant | `walks-all-children`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:1084](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1084) | Declares `Entry::Bare(Bare)` variant | `walks-all-children`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:1086](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1086) | Declares `Entry::Hole(Hole)` variant | `walks-all-children`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:1102](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1102) | Declares `Group(pub Vec<Entry>)` tuple struct | `walks-all-children`, `in-written-order`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:1108](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1108) | `Group::from_map` matches `Value::Map` to convert to `Group` entries | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:1124](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1124) | `Group::as_data` filters pairs and constructs `Value::Map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:1138](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1138) | `Group::pairs` filters `Entry::Pair` in source order | `by-name`, `in-written-order` |
| [crates/tomet-syntax-ast/src/lib.rs:1147](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1147) | `Group::pairs_mut` yields mutable `Entry::Pair` items | `by-name`, `in-written-order` |
| [crates/tomet-syntax-ast/src/lib.rs:1155](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1155) | `Group::get` finds first pair matching key | `by-name` |
| [crates/tomet-syntax-ast/src/lib.rs:1160](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1160) | `Group::get_mut` finds mutable value by key | `by-name` |
| [crates/tomet-syntax-ast/src/lib.rs:1166](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1166) | `Group::push_pair` pushes `Entry::Pair` | `by-name`, `builds` |
| [crates/tomet-syntax-ast/src/lib.rs:1174](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1174) | `Group::remove` removes first pair matching key | `by-name` |
| [crates/tomet-syntax-lowering/src/element.rs:58](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L58) | `lower_paren_value` builds empty `Value::Map` for empty `()` | `is-it-a-map`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:76](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L76) | `lower_paren_value` builds `Value::Map` with positional and named entries | `is-it-a-map`, `by-name`, `by-position`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:212](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L212) | `lower_value_group` constructs `Entry::Pair` from CST pair | `by-name`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:248](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L248) | `lower_value_group` constructs `Entry::Pair` from quoted key CST pair | `by-name`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:280](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L280) | `lower_value_group` constructs `Entry::Element` | `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:344](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L344) | `lower_value_group` constructs `Entry::Bare` | `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:361](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L361) | `lower_value_group` constructs `Entry::Hole` | `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:374](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L374) | `lower_value_group` builds `Group(entries)` | `walks-all-children`, `in-written-order`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:608](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L608) | `lower_single_value_from_children` resolves call or builds `Value::Call` | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:667](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L667) | `lower_single_value_from_children` builds `Value::Map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:671](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L671) | `lower_single_value_from_children` builds `Value::Seq` | `is-it-a-list`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:932](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L932) | `resolve_call` constructs unresolved `Value::Call` | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:957](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L957) | `construct_string` constructs fallback `Value::Call` | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:985](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L985) | `construct_int` constructs fallback `Value::Call` | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/element.rs:1009](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L1009) | `construct_float` constructs fallback `Value::Call` | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/lower/connect.rs:408](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/lower/connect.rs#L408) | `merge_values_inner` merges `(Value::Map, Value::Map)` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-syntax-lowering/src/lower/connect.rs:442](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/lower/connect.rs#L442) | `merge_values` merges `Data.args` and `Data.value` | `by-name`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/lower/form.rs:188](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/lower/form.rs#L188) | `lower_element` populates `Element.data` | `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/lower/groups.rs:88](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/lower/groups.rs#L88) | `lower_groups` populates `Data` from CST groups | `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/value.rs:18](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/value.rs#L18) | `lower_value` matches CST sequence to `Value::Seq` | `is-it-a-list`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-lowering/src/value.rs:24](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/value.rs#L24) | `lower_value` matches CST map to `Value::Map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-syntax-lowering/src/value.rs:30](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/value.rs#L30) | `lower_value` matches CST call to `Value::Call` | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-syntax-tree/src/value.rs:33](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/value.rs#L33) | `ValueExt::as_seq` checks `Value::Seq` variant | `is-it-a-list` |
| [crates/tomet-syntax-tree/src/value.rs:38](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/value.rs#L38) | `ValueExt::as_map` checks `Value::Map` variant | `is-it-a-map` |
| [crates/tomet-syntax-tree/src/value.rs:43](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/value.rs#L43) | `ValueExt::get` looks up key in `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-syntax-tree/src/value.rs:49](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/value.rs#L49) | `ValueExt::get_mut` looks up mutable value in `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-syntax-tree/src/element.rs:22](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/element.rs#L22) | `DataExt::attrs_view` reads `Data.args` and `Data.value` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-syntax-tree/src/element.rs:42](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/element.rs#L42) | `DataExt::get_attr` finds attribute in `Data.args` or `Data.value` | `is-it-a-map`, `by-name` |
| [crates/tomet-syntax-tree/src/element.rs:47](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/element.rs#L47) | `DataExt::has_attr` tests attribute existence in `Data` | `is-it-a-map`, `by-name` |
| [crates/tomet-syntax-tree/src/element.rs:60](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/element.rs#L60) | `DataExt::set_prop` inserts or updates `Entry::Pair` in `Data.value` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-syntax-tree/src/element.rs:77](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/element.rs#L77) | `DataExt::rename_prop_key` renames key in `Data.args` or `Data.value` | `is-it-a-map`, `by-name` |
| [crates/tomet-syntax-tree/src/element.rs:104](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/element.rs#L104) | `DataExt::replace_prop_value` replaces value in `Data.args` or `Data.value` | `is-it-a-map`, `by-name` |
| [crates/tomet-syntax-tree/src/element.rs:125](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/element.rs#L125) | `DataExt::transform_prop` mutates value in `Data.args` or `Data.value` | `is-it-a-map`, `by-name` |
| [crates/tomet-syntax-tree/src/walk.rs:207](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/walk.rs#L207) | `children` visits items in `Node::Value(Value::Seq)` | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-syntax-tree/src/walk.rs:212](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/walk.rs#L212) | `children` visits entries in `Node::Value(Value::Map)` | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-syntax-tree/src/walk.rs:219](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/walk.rs#L219) | `children` visits arguments in `Node::Value(Value::Call)` | `callee-name`, `walks-all-children` |
| [crates/tomet-syntax-tree/src/walk.rs:238](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/walk.rs#L238) | `children` visits entries in `Node::Group(Group)` | `walks-all-children`, `in-written-order` |
| [crates/tomet-syntax-tree/src/walk.rs:242](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/walk.rs#L242) | `children` visits payload in `Node::Entry(Entry::*)` | `by-name`, `walks-all-children` |
| [crates/serde_cotomet/src/de.rs:73](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/de.rs#L73) | `deserialize_any` matches `Value::Seq` to drive `visit_seq` | `is-it-a-list`, `walks-all-children` |
| [crates/serde_cotomet/src/de.rs:77](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/de.rs#L77) | `deserialize_any` matches `Value::Map` to drive `visit_map` | `is-it-a-map`, `walks-all-children` |
| [crates/serde_cotomet/src/de.rs:81](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/de.rs#L81) | `deserialize_any` matches and refuses `Value::Call` | `callee-name` |
| [crates/serde_cotomet/src/de.rs:116](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/de.rs#L116) | `deserialize_enum` matches 1-entry `Value::Map` for enum variant | `is-it-a-map`, `by-name` |
| [crates/serde_cotomet/src/ser.rs:179](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/ser.rs#L179) | `finish_seq` builds `Value::Seq` | `is-it-a-list`, `builds` |
| [crates/serde_cotomet/src/ser.rs:181](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/ser.rs#L181) | `finish_seq` builds `Value::Map` for tuple variant | `is-it-a-map`, `by-name`, `builds` |
| [crates/serde_cotomet/src/ser.rs:238](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/ser.rs#L238) | `MapSerializer::end` builds `Value::Map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/serde_cotomet/src/print.rs:16](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/print.rs#L16) | `write_value` matches `Value::Map` to print map | `is-it-a-map`, `by-name`, `walks-all-children`, `in-written-order` |
| [crates/serde_cotomet/src/print.rs:17](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/print.rs#L17) | `write_value` matches `Value::Seq` to print sequence | `is-it-a-list`, `walks-all-children`, `in-written-order` |
| [crates/serde_cotomet/src/print.rs:33](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/print.rs#L33) | `write_value` matches `Value::Call` to print call syntax | `callee-name`, `walks-all-children`, `in-written-order` |

### Layer 3: Semantics

| path:line | what the code does | what it asks of the value |
|---|---|---|
| [crates/tomet-semantics-core/src/positional.rs:30](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-core/src/positional.rs#L30) | `bind_positional` rebinds empty-string entries by signature position | `by-position`, `in-written-order`, `builds` |
| [crates/tomet-semantics-declaration/src/kind.rs:135](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-declaration/src/kind.rs#L135) | `DeclKind::positional_keys` defines positional key mapping | `by-position` |
| [crates/tomet-semantics/src/positional.rs:15](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/positional.rs#L15) | `builtin_positional_arg_keys` defines element positional keys | `by-position` |
| [crates/tomet-semantics/src/positional.rs:90](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/positional.rs#L90) | `normalized_element_args_in` normalizes `Data.args` | `is-it-a-map`, `by-position`, `by-name`, `builds` |
| [crates/tomet-semantics/src/positional.rs:115](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/positional.rs#L115) | `normalized_declaration_args_in` normalizes `Declaration.data.args` | `is-it-a-map`, `by-position`, `by-name`, `builds` |
| [crates/tomet-semantics/src/data.rs:23](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/data.rs#L23) | `element_data` reads `Data.args` and `Data.value` into a merged map | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-semantics/src/data.rs:56](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/data.rs#L56) | `declaration_entries` reads `Declaration.data` | `is-it-a-map`, `by-name` |
| [crates/tomet-semantics/src/flatten.rs:50](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/flatten.rs#L50) | `flatten_element_data` flattens element `Data` | `is-it-a-map`, `by-name`, `by-position` |
| [crates/tomet-semantics/src/flatten.rs:88](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/flatten.rs#L88) | `flatten_data` inspects `Value::Map` or positional value | `is-it-a-map`, `by-name`, `by-position` |
| [crates/tomet-semantics/src/flatten.rs:141](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/flatten.rs#L141) | `value_to_json` matches `Value::Seq` to JSON array | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-semantics/src/flatten.rs:145](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/flatten.rs#L145) | `value_to_json` matches `Value::Map` to JSON object | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-semantics/src/flatten.rs:152](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/flatten.rs#L152) | `value_to_json` matches `Value::Call` to JSON `{"call": ..., "args": ...}` | `callee-name`, `walks-all-children` |
| [crates/tomet-semantics/src/vocabulary.rs:130](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/vocabulary.rs#L130) | `ElementDecl::positional_keys` extracts positional parameters from vocabulary | `by-position` |
| [crates/tomet-semantics/src/elements/config.rs:43](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/elements/config.rs#L43) | `process_export_type` matches `Value::Seq` | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-semantics/src/elements/config.rs:60](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/elements/config.rs#L60) | `process_export_path` matches `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-semantics/src/elements/tag.rs:23](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/elements/tag.rs#L23) | `extract_tags_from_value` matches `Value::Seq` | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-semantics/src/elements/tag.rs:32](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/elements/tag.rs#L32) | `extract_tags_from_value` matches `Value::Map` | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-semantics-eval/src/lib.rs:180](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-eval/src/lib.rs#L180) | `walk_value` matches `Value::Map` and recurses | `is-it-a-map`, `by-name`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-eval/src/lib.rs:188](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-eval/src/lib.rs#L188) | `walk_value` matches `Value::Seq` and recurses | `is-it-a-list`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-eval/src/lib.rs:195](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-eval/src/lib.rs#L195) | `walk_value` matches `Value::Call` and recurses | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-expr/src/lib.rs:113](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/lib.rs#L113) | `refers_to_vault` inspects `EvalValue::Seq` | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-semantics-expr/src/lib.rs:114](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/lib.rs#L114) | `refers_to_vault` inspects `EvalValue::Map` | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-semantics-expr/src/lib.rs:115](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/lib.rs#L115) | `refers_to_vault` inspects `EvalValue::Call` | `callee-name`, `walks-all-children` |
| [crates/tomet-semantics-expr/src/lib.rs:253](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/lib.rs#L253) | `member_value` looks up key on `EvalValue::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-semantics-expr/src/value.rs:233](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/value.rs#L233) | `from_ast` matches `Value::Seq` to construct `EvalValue::Seq` | `is-it-a-list`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-expr/src/value.rs:236](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/value.rs#L236) | `from_ast` matches `Value::Map` to construct `EvalValue::Map` | `is-it-a-map`, `by-name`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-expr/src/value.rs:242](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/value.rs#L242) | `from_ast` matches `Value::Call` to construct `EvalValue::Call` | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-expr/src/value.rs:272](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/value.rs#L272) | `to_ast` matches `EvalValue::Seq` to construct `Value::Seq` | `is-it-a-list`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-expr/src/value.rs:275](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/value.rs#L275) | `to_ast` matches `EvalValue::Map` to construct `Value::Map` | `is-it-a-map`, `by-name`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-expr/src/value.rs:281](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/value.rs#L281) | `to_ast` matches `EvalValue::Call` to construct `Value::Call` | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-expr/src/methods.rs:23](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/methods.rs#L23) | `call_method` matches `EvalValue::Seq` for `.filter` | `is-it-a-list`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-expr/src/methods.rs:41](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/methods.rs#L41) | `call_method` matches `EvalValue::Seq` for `.map` | `is-it-a-list`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-expr/src/methods.rs:49](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/methods.rs#L49) | `call_method` matches `EvalValue::Seq` for `.sort` | `is-it-a-list`, `walks-all-children`, `builds` |
| [crates/tomet-semantics-expr/src/methods.rs:50](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/methods.rs#L50) | `call_method` matches `EvalValue::Seq` for `.join` | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-semantics-expr/src/methods.rs:65](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/methods.rs#L65) | `call_method` matches `EvalValue::Seq` for `.contains` | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-semantics-expr/src/methods.rs:71](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/methods.rs#L71) | `call_method` matches `EvalValue::Seq` for `.len` | `is-it-a-list` |
| [crates/tomet-semantics-expr/src/methods.rs:75](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/methods.rs#L75) | `call_method` matches `EvalValue::Seq` for `.item` | `is-it-a-list`, `by-position` |
| [crates/tomet-semantics-expr/src/methods.rs:108](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/methods.rs#L108) | `call_method` matches `EvalValue::Map` for `.get` | `is-it-a-map`, `by-name` |
| [crates/tomet-semantics-expr/src/methods.rs:123](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/methods.rs#L123) | `call_method` matches `EvalValue::Map` for `.contains` | `is-it-a-map`, `by-name` |
| [crates/tomet-semantics-expr/src/types.rs:119](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/types.rs#L119) | `datum` matches `Value::Seq` to construct `Datum::Seq` | `is-it-a-list`, `walks-all-children`, `builds` |

### Layer 4: Validator

| path:line | what the code does | what it asks of the value |
|---|---|---|
| [crates/tomet-validator-core/src/lib.rs:350](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-core/src/lib.rs#L350) | `check_arguments_of` matches `args` to `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-validator-core/src/lib.rs:721](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-core/src/lib.rs#L721) | `check_field_types` matches declaration entries to `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-validator-core/src/lib.rs:800](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-core/src/lib.rs#L800) | `describe_value` matches `Value::Seq` to return `"a list"` | `is-it-a-list` |
| [crates/tomet-validator-core/src/lib.rs:801](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-core/src/lib.rs#L801) | `describe_value` matches `Value::Map` to return `"a map"` | `is-it-a-map` |
| [crates/tomet-validator-rule/src/decode.rs:21](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-rule/src/decode.rs#L21) | `decode_rule_args` matches `connect.data.args` to `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-validator-rule/src/decode.rs:27](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-rule/src/decode.rs#L27) | `decode_rule_args` matches `allow` value to `Value::Seq` | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-validator-rule/src/decode.rs:29](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-rule/src/decode.rs#L29) | `decode_rule_args` matches `allow` value to `Value::Call("enum", ...)` | `callee-name`, `walks-all-children` |
| [crates/tomet-validator-blueprint/src/lib.rs:80](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-blueprint/src/lib.rs#L80) | `extract_blueprint_schema` matches blueprint args to `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-validator-blueprint/src/lib.rs:102](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-blueprint/src/lib.rs#L102) | `extract_blueprint_schema` matches meta entries to `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-validator-blueprint/src/lib.rs:190](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-blueprint/src/lib.rs#L190) | `validate_against_schema` matches document meta entries to `Value::Map` | `is-it-a-map`, `by-name` |

### Layer 5: Config

| path:line | what the code does | what it asks of the value |
|---|---|---|
| [crates/tomet-config/src/tree.rs:18](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-config/src/tree.rs#L18) | `collect_paths` matches `Value::Seq` to collect path strings | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-config/src/tree.rs:43](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-config/src/tree.rs#L43) | `get_or_insert_map` creates or retrieves `Value::Map` | `is-it-a-map`, `builds` |
| [crates/tomet-config/src/tree.rs:60](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-config/src/tree.rs#L60) | `insert_or_merge_prop` merges `(Value::Map, Value::Map)` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-config/src/tree.rs:93](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-config/src/tree.rs#L93) | `merge_entry` recurses on incoming `Value::Map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-config/src/layer.rs:45](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-config/src/layer.rs#L45) | `ConfigLayer::from_doc` populates layers from declarations | `is-it-a-map`, `by-name` |

### Layer 6: Converters, Format, Transform, Search, Deps, Vault

| path:line | what the code does | what it asks of the value |
|---|---|---|
| [crates/tomet-printer/src/value.rs:18](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-printer/src/value.rs#L18) | `render_value` matches `Value::Map` to print `{k: v}` | `is-it-a-map`, `by-name`, `walks-all-children`, `in-written-order` |
| [crates/tomet-printer/src/value.rs:27](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-printer/src/value.rs#L27) | `render_value` matches `Value::Seq` to print `(a, b)` | `is-it-a-list`, `walks-all-children`, `in-written-order` |
| [crates/tomet-printer/src/value.rs:59](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-printer/src/value.rs#L59) | `render_value` matches `Value::Call` to print `name(args)` | `callee-name`, `walks-all-children`, `in-written-order` |
| [crates/tomet-printer/src/value.rs:322](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-printer/src/value.rs#L322) | `render_value_group_entries` matches and renders `Entry::Pair` | `by-name`, `in-written-order` |
| [crates/tomet-printer/src/value.rs:325](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-printer/src/value.rs#L325) | `render_value_group_entries` matches and renders `Entry::Element` | `walks-all-children`, `in-written-order` |
| [crates/tomet-printer/src/value.rs:334](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-printer/src/value.rs#L334) | `render_value_group_entries` matches and renders `Entry::Bare` | `walks-all-children`, `in-written-order` |
| [crates/tomet-printer/src/value.rs:343](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-printer/src/value.rs#L343) | `render_value_group_entries` matches and renders `Entry::Hole` | `walks-all-children`, `in-written-order` |
| [crates/tomet-convert-markdown/src/export/mod.rs:407](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/export/mod.rs#L407) | `render_callout` matches `Value::Map` in callout args | `is-it-a-map`, `by-name`, `by-position` |
| [crates/tomet-convert-markdown/src/export/mod.rs:426](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/export/mod.rs#L426) | `render_callout` matches `Value::Seq` in callout args | `is-it-a-list`, `by-position` |
| [crates/tomet-convert-markdown/src/export/mod.rs:626](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/export/mod.rs#L626) | `task_status` matches `Value::Map` for status key | `is-it-a-map`, `by-name` |
| [crates/tomet-convert-markdown/src/export/mod.rs:700](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/export/mod.rs#L700) | `conflict_sides` reads `a` and `b` keys from `as_map` | `is-it-a-map`, `by-name` |
| [crates/tomet-convert-markdown/src/export/mod.rs:711](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/export/mod.rs#L711) | `push_data_attrs` iterates map entries from `as_map` | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-convert-markdown/src/export/mod.rs:733](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/export/mod.rs#L733) | `value_to_plain` matches `Value::Seq` | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-convert-markdown/src/export/mod.rs:738](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/export/mod.rs#L738) | `value_to_plain` matches `Value::Map` to return empty string | `is-it-a-map` |
| [crates/tomet-convert-markdown/src/export/mod.rs:741](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/export/mod.rs#L741) | `value_to_plain` matches `Value::Call` to return empty string | `callee-name` |
| [crates/tomet-convert-markdown/src/export/mod.rs:753](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/export/mod.rs#L753) | `as_map` matches `Value::Map` | `is-it-a-map` |
| [crates/tomet-convert-markdown/src/export/mod.rs:758](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/export/mod.rs#L758) | `map_get` finds key in entry slice | `by-name` |
| [crates/tomet-convert-markdown/src/import/frontmatter.rs:87](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/import/frontmatter.rs#L87) | `yaml_to_value` constructs `Value::Seq` from YAML sequence | `is-it-a-list`, `builds` |
| [crates/tomet-convert-markdown/src/import/frontmatter.rs:89](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-markdown/src/import/frontmatter.rs#L89) | `yaml_to_value` constructs `Value::Map` from YAML mapping | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-convert-html/src/util.rs:67](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-html/src/util.rs#L67) | `split_attrs` extracts `cssclass` and data from `as_map` | `is-it-a-map`, `by-name` |
| [crates/tomet-convert-html/src/util.rs:133](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-html/src/util.rs#L133) | `push_value_data_attrs` calls `v.as_data()` on `held.value` | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-convert-html/src/util.rs:158](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-html/src/util.rs#L158) | `push_data_attrs` matches `Value::Map` vs positional args | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-convert-html/src/util.rs:190](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-html/src/util.rs#L190) | `as_map` matches `Value::Map` | `is-it-a-map` |
| [crates/tomet-convert-html/src/elements/mod.rs:77](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-html/src/elements/mod.rs#L77) | `render_task_element` reads `status` via `as_map` | `is-it-a-map`, `by-name` |
| [crates/tomet-convert-html/src/elements/mod.rs:139](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-html/src/elements/mod.rs#L139) | `conflict_sides` reads `a` and `b` via `as_map` | `is-it-a-map`, `by-name` |
| [crates/tomet-convert-html/src/elements/mod.rs:248](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-html/src/elements/mod.rs#L248) | `render_generic` inspects `body.data.value: Option<Group>` | `walks-all-children` |
| [crates/tomet-convert-html/src/elements/mod.rs:262](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-html/src/elements/mod.rs#L262) | `render_element_value` matches `Group(entries)` and branches on `Entry` | `walks-all-children`, `in-written-order` |
| [crates/tomet-convert-typst/src/render.rs:309](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-typst/src/render.rs#L309) | `task_status` matches `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-convert-typst/src/render.rs:390](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-typst/src/render.rs#L390) | `conflict_sides` reads `a` and `b` via `as_map` | `is-it-a-map`, `by-name` |
| [crates/tomet-convert-typst/src/render.rs:407](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-typst/src/render.rs#L407) | `callout_variant_and_title` matches `Value::Map` in args | `is-it-a-map`, `by-name`, `by-position` |
| [crates/tomet-convert-typst/src/render.rs:426](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-typst/src/render.rs#L426) | `callout_variant_and_title` matches `Value::Seq` in args | `is-it-a-list`, `by-position` |
| [crates/tomet-convert-typst/src/render.rs:447](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-typst/src/render.rs#L447) | `render_raw` reads `lang` via `as_map` | `is-it-a-map`, `by-name` |
| [crates/tomet-convert-typst/src/render.rs:766](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-typst/src/render.rs#L766) | `as_map` matches `Value::Map` | `is-it-a-map` |
| [crates/tomet-convert-pandoc/src/to_pandoc.rs:32](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/to_pandoc.rs#L32) | `to_pandoc` matches `document_meta` to `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-convert-pandoc/src/to_pandoc.rs:70](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/to_pandoc.rs#L70) | `meta_value` matches `Value::Seq` to `MetaValue::MetaList` | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-convert-pandoc/src/to_pandoc.rs:71](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/to_pandoc.rs#L71) | `meta_value` matches `Value::Map` to `MetaValue::MetaMap` | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-convert-pandoc/src/to_pandoc.rs:79](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/to_pandoc.rs#L79) | `meta_value` matches `Value::Call` to empty string | `callee-name` |
| [crates/tomet-convert-pandoc/src/from_pandoc.rs:136](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/from_pandoc.rs#L136) | `meta_declaration` constructs `Value::Map` for header | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-convert-pandoc/src/from_pandoc.rs:144](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/from_pandoc.rs#L144) | `meta_value_to_value` constructs `Value::Seq` | `is-it-a-list`, `builds` |
| [crates/tomet-convert-pandoc/src/from_pandoc.rs:145](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/from_pandoc.rs#L145) | `meta_value_to_value` constructs `Value::Map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-convert-pandoc/src/from_pandoc.rs:651](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/from_pandoc.rs#L651) | `push_bare_entries` inserts `Entry::Bare` into `data.value: Option<Group>` | `walks-all-children`, `in-written-order`, `builds` |
| [crates/tomet-convert-pandoc/src/from_pandoc.rs:673](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/from_pandoc.rs#L673) | `extract_data_from_attr` constructs `Group::from_map` from JSON | `is-it-a-map`, `builds` |
| [crates/tomet-convert-pandoc/src/from_pandoc.rs:685](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/from_pandoc.rs#L685) | `extract_data_from_attr` constructs `Value::Map` and `Group::from_map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-convert-pandoc/src/from_pandoc.rs:717](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/from_pandoc.rs#L717) | `merge_arg` updates `el.data.args` matching `Value::Map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-convert-pandoc/src/from_pandoc.rs:745](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/from_pandoc.rs#L745) | `json_to_value` builds `Value::Seq` from JSON array | `is-it-a-list`, `builds` |
| [crates/tomet-convert-pandoc/src/from_pandoc.rs:746](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-convert-pandoc/src/from_pandoc.rs#L746) | `json_to_value` builds `Value::Map` from JSON object | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-transform/src/expressions.rs:327](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-transform/src/expressions.rs#L327) | `data` resolves expressions in `Data.args` and `Data.value` | `walks-all-children`, `builds` |
| [crates/tomet-transform/src/expressions.rs:335](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-transform/src/expressions.rs#L335) | `element_value` matches `Group(entries)` and branches on `Entry` | `by-name`, `walks-all-children`, `builds` |
| [crates/tomet-transform/src/expressions.rs:362](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-transform/src/expressions.rs#L362) | `value` matches `Value::Seq` and recurses | `is-it-a-list`, `walks-all-children`, `builds` |
| [crates/tomet-transform/src/expressions.rs:363](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-transform/src/expressions.rs#L363) | `value` matches `Value::Map` and recurses | `is-it-a-map`, `by-name`, `walks-all-children`, `builds` |
| [crates/tomet-transform/src/expressions.rs:369](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-transform/src/expressions.rs#L369) | `value` matches `Value::Call` and recurses | `callee-name`, `walks-all-children`, `builds` |
| [crates/tomet-transform/src/structural.rs:45](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-transform/src/structural.rs#L45) | `apply_to` reads and modifies `Data` properties | `is-it-a-map`, `by-name` |
| [crates/tomet-transform/src/meta.rs:33](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-transform/src/meta.rs#L33) | `set_meta_in_doc` creates declaration with `Value::Map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-transform/src/macro_rewrite.rs:130](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-transform/src/macro_rewrite.rs#L130) | `MacroSet::from_map` extracts macros from map | `is-it-a-map`, `by-name` |
| [crates/tomet-search/src/structural.rs:61](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-search/src/structural.rs#L61) | `matches_parts` searches substring in `data.args` and `data.value` | `by-name`, `walks-all-children` |
| [crates/tomet-search/src/structural.rs:114](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-search/src/structural.rs#L114) | `value_contains_str` matches `Value::Call` | `callee-name`, `walks-all-children` |
| [crates/tomet-search/src/structural.rs:119](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-search/src/structural.rs#L119) | `value_contains_str` matches `Value::Map` | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-search/src/structural.rs:122](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-search/src/structural.rs#L122) | `value_contains_str` matches `Value::Seq` | `is-it-a-list`, `walks-all-children` |
| [crates/tomet-deps/src/extract.rs:54](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-deps/src/extract.rs#L54) | `extract_exports_hash` matches `declaration.data.args` to `Value::Map` | `is-it-a-map`, `by-name`, `in-written-order` |
| [crates/tomet-vault/src/resolve.rs:28](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-vault/src/resolve.rs#L28) | `typed_meta` matches `document_meta` to `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-vault/src/resolve.rs:46](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-vault/src/resolve.rs#L46) | `typed_meta` builds `EvalValue::Map` | `is-it-a-map`, `by-name`, `builds` |
| [crates/tomet-vault/src/resolve.rs:55](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-vault/src/resolve.rs#L55) | `typed` builds `EvalValue::Seq` for list type | `is-it-a-list`, `builds` |

### Layer 7: Workspace & Front Door

| path:line | what the code does | what it asks of the value |
|---|---|---|
| [crates/tomet-workspace-indexer/src/lib.rs:390](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-workspace-indexer/src/lib.rs#L390) | `document_fields` flattens `document_meta` | `is-it-a-map`, `by-name` |
| [crates/tomet-workspace-indexer/src/lib.rs:406](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-workspace-indexer/src/lib.rs#L406) | `flatten_into` matches `Value::Map` to recurse dotted keys | `is-it-a-map`, `by-name`, `walks-all-children` |
| [crates/tomet-workspace-indexer/src/lib.rs:424](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-workspace-indexer/src/lib.rs#L424) | `extract_metadata` matches `declaration_entries` to `Value::Map` | `is-it-a-map`, `by-name` |
| [crates/tomet-workspace-indexer/src/lib.rs:439](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-workspace-indexer/src/lib.rs#L439) | `collect_meta` reads `data.value` pairs | `is-it-a-map`, `by-name` |

### Layer 8: Applications, Bindings, and Tests

| path:line | what the code does | what it asks of the value |
|---|---|---|
| [apps/cli/src/commands/ast.rs:8](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/apps/cli/src/commands/ast.rs#L8) | `ast` parses and formats `Value` via Debug | `walks-all-children` |
| [apps/lsp/src/symbol.rs:146](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/apps/lsp/src/symbol.rs#L146) | `collect_element_symbol` reads `el.data.args` | `walks-all-children` |
| [apps/lsp/src/symbol.rs:176](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/apps/lsp/src/symbol.rs#L176) | `collect_declaration_symbol` reads `declaration.data.args` | `walks-all-children` |
| [apps/lsp/src/symbol.rs:198](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/apps/lsp/src/symbol.rs#L198) | `collect_connect_symbol` reads `conn.data.args` | `walks-all-children` |
| [bindings/python/src/lib.rs:41](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/bindings/python/src/lib.rs#L41) | `loads` serializes `Value` via `pythonize` to Python objects | `walks-all-children` |
| [bindings/python/src/lib.rs:48](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/bindings/python/src/lib.rs#L48) | `parse_file` serializes `File` via `pythonize` | `walks-all-children` |
| [bindings/js/src/lib.rs:20](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/bindings/js/src/lib.rs#L20) | `parse_file` serializes `File` via `serde_wasm_bindgen` | `walks-all-children` |
| [bindings/js/src/lib.rs:27](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/bindings/js/src/lib.rs#L27) | `parse_value` serializes `Value` via `serde_wasm_bindgen` | `walks-all-children` |
| [bindings/java/src/lib.rs:47](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/bindings/java/src/lib.rs#L47) | `Java_parseFileJson` serializes `File` via `serde_json` | `walks-all-children` |
| [bindings/java/src/lib.rs:68](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/bindings/java/src/lib.rs#L68) | `Java_parseValueJson` serializes `Value` via `serde_json` | `walks-all-children` |
| [tests/src/syntax_report.rs:1265](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/tests/src/syntax_report.rs#L1265) | `value_str` matches `Value::Seq` to format `(a, b)` | `is-it-a-list`, `walks-all-children` |
| [tests/src/syntax_report.rs:1269](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/tests/src/syntax_report.rs#L1269) | `value_str` matches `Value::Map` to format `{k: v}` and positional items | `is-it-a-map`, `by-name`, `by-position`, `walks-all-children` |
| [tests/src/syntax_report.rs:1283](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/tests/src/syntax_report.rs#L1283) | `value_str` matches `Value::Call` to format `name(args)` | `callee-name`, `walks-all-children` |

### Tag Frequency Counts

- `by-name`: 76
- `walks-all-children`: 73
- `is-it-a-map`: 65
- `builds`: 62
- `is-it-a-list`: 36
- `in-written-order`: 19
- `callee-name`: 18
- `by-position`: 15

### Sites Whose Behavior Depends on the Three Kinds Being Different

1. **`serde_cotomet` deserializer refusal of `Call`** ([crates/serde_cotomet/src/de.rs:81](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/serde_cotomet/src/de.rs#L81)):
   - *Current behavior*: Deserializing `Value::Seq` visits sequence; `Value::Map` visits map; `Value::Call` returns an explicit error ("call syntax has no serde equivalent").
   - *What the memo implies*: With `Value::Call` eliminated and call syntax `name(...)` represented as a 1-entry container `(name: (...))`, this site must deserialize a container. A 1-entry container maps directly to external enum representation (`{"Variant": payload}`), making call-like syntax deserialize cleanly into Rust enums instead of being rejected.
2. **`::rule(allow: ...)` handling of `enum` call vs sequence** ([crates/tomet-validator-rule/src/decode.rs:26-31](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-rule/src/decode.rs#L26-L31)):
   - *Current behavior*: Accepts both `Value::Seq(items)` and `Value::Call(callee, items) if callee == "enum"`.
   - *What the memo implies*: In the data space, `enum(...)` is no longer a call. It desugars to a container `(enum: (...))`. The memo does not state whether `allow:` should specifically unwrap a named `enum:` entry or whether `enum` syntax should be retired in favor of plain container entries `(card, item)`. (See Question 6 in Section F).
3. **`value_to_json` distinct serialization for Call** ([crates/tomet-semantics/src/flatten.rs:152](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics/src/flatten.rs#L152)):
   - *Current behavior*: `Seq` serializes as JSON array `[...]`, `Map` serializes as JSON object `{...}`, and `Call(name, args)` serializes as `{"call": name, "args": args}`.
   - *What the memo implies*: `Value::Call` ceases to exist. The container serializes uniformly. The special `{"call": ...}` JSON representation disappears.
4. **`Value::serialize` JSON encoding of Call** ([crates/tomet-syntax-ast/src/lib.rs:219-224](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L219-L224)):
   - *Current behavior*: Directly emits `{"call": name, "args": args}`.
   - *What the memo implies*: With one container, `Call` variant is gone. The container serializes according to the one container serialization rule.
5. **Printer distinguishing `Call` from `Map`** ([crates/tomet-printer/src/value.rs:18, 59](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-printer/src/value.rs#L18)):
   - *Current behavior*: `Value::Map` prints with colons `{k: v}`, while `Value::Call` prints without colon as `name(args)`.
   - *What the memo implies*: `name(...)` and `(name: (...))` are the exact same tree. The AST printer must either print one canonical form or inspect the spelling layer.
6. **`from_ast` / `to_ast` in `tomet-expr`** ([crates/tomet-semantics-expr/src/value.rs:242, 281](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-semantics-expr/src/value.rs#L242)):
   - *Current behavior*: Preserves `Value::Call` as `EvalValue::Call`.
   - *What the memo implies*: `EvalValue::Call` in the data domain disappears. In expression space `$`, function calls are evaluated expressions, not stored inert call data.
7. **`ValueExt::as_seq` and `as_map`** ([crates/tomet-syntax-tree/src/value.rs:33, 38](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-tree/src/value.rs#L33)):
   - *Current behavior*: `as_seq` returns `None` for a map or call; `as_map` returns `None` for a seq or call.
   - *What the memo implies*: The three mutually-exclusive variants become views over the single container type.
8. **Value merging in connect desugaring** ([crates/tomet-syntax-lowering/src/lower/connect.rs:408](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/lower/connect.rs#L408)):
   - *Current behavior*: `(Value::Map, Value::Map)` merges key-by-key, whereas `Value::Seq` completely overwrites the existing value.
   - *What the memo implies*: The memo does not specify how two containers merge when they contain both named and unnamed entries. (See Question 5 in Section F).

---

## C. Proposal for the one container type

### 1. Rust Sketch

The type and entry definitions, containing only fields and methods justified by the readers and writers identified in Section B:

```rust
use serde::{Deserialize, Serialize};

/// The single container type for data: an ordered sequence of entries,
/// each entry optionally bearing a name.
///
/// A list is a container whose entries all have no name.
/// A map is a container whose entries all have a name.
/// A mix of named and unnamed entries is legitimate and preserves source order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Container {
    entries: Vec<ContainerEntry>,
}

/// A single entry in a [`Container`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerEntry {
    /// The name of this entry, or `None` if it was written without a name.
    pub name: Option<String>,
    /// The value held in this slot.
    pub value: Value,
}

impl Container {
    /// Creates an empty container.
    pub const fn new() -> Self {
        Self { entries: Vec::new() }
    }

    /// Creates a container from an ordered sequence of entries.
    pub fn from_entries(entries: Vec<ContainerEntry>) -> Self {
        Self { entries }
    }

    // --- Access by written order (justified by walkers, printers, diff3) ---

    /// Number of entries in this container.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether this container holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterator over entries in their exact source order.
    pub fn iter(&self) -> impl Iterator<Item = &ContainerEntry> {
        self.entries.iter()
    }

    /// Mutable iterator over entries in their exact source order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ContainerEntry> {
        self.entries.iter_mut()
    }

    /// Returns the nth entry as written.
    pub fn nth_entry(&self, index: usize) -> Option<&ContainerEntry> {
        self.entries.get(index)
    }

    // --- Access by name (justified by map lookups, ::meta, settings, validators) ---

    /// Returns the value of the first entry with the given name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|e| e.name.as_deref() == Some(name))
            .map(|e| &e.value)
    }

    /// Mutable counterpart of [`get`].
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.entries
            .iter_mut()
            .find(|e| e.name.as_deref() == Some(name))
            .map(|e| &mut e.value)
    }

    /// Iterator over all named entries as `(&str, &Value)` pairs in source order.
    pub fn named(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries
            .iter()
            .filter_map(|e| e.name.as_deref().map(|n| (n, &e.value)))
    }

    // --- Access by position / unnamed entries (justified by seq index, args) ---

    /// Returns the nth unnamed entry.
    pub fn nth_unnamed(&self, n: usize) -> Option<&Value> {
        self.unnamed().nth(n)
    }

    /// Iterator over all unnamed values in source order.
    pub fn unnamed(&self) -> impl Iterator<Item = &Value> {
        self.entries
            .iter()
            .filter(|e| e.name.is_none())
            .map(|e| &e.value)
    }

    // --- Classification queries (justified by validators, converters, printers) ---

    /// True if every entry in this container has no name (pure list).
    pub fn is_list(&self) -> bool {
        self.entries.iter().all(|e| e.name.is_none())
    }

    /// True if every entry in this container has a name (pure map).
    pub fn is_map(&self) -> bool {
        self.entries.iter().all(|e| e.name.is_some())
    }

    // --- Mutation and building (justified by parser, merge, serde) ---

    /// Appends an unnamed entry.
    pub fn push_unnamed(&mut self, value: Value) {
        self.entries.push(ContainerEntry { name: None, value });
    }

    /// Appends a named entry.
    pub fn push_named(&mut self, name: impl Into<String>, value: Value) {
        self.entries.push(ContainerEntry {
            name: Some(name.into()),
            value,
        });
    }

    /// Removes the first entry with `name` and returns its value.
    pub fn remove(&mut self, name: &str) -> Option<Value> {
        let idx = self
            .entries
            .iter()
            .position(|e| e.name.as_deref() == Some(name))?;
        Some(self.entries.remove(idx).value)
    }
}
```

In `Value`, `Value::Seq`, `Value::Map`, and `Value::Call` are replaced by:

```rust
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Number(String),
    String(String),
    /// The single data container: an ordered run of entries, each with or without a name.
    Container(Container),
    Interp(Interpolation),
    Hole(Box<Hole>),
    TypeSeat(TypeSeat),
    Element(Box<Element>),
    Blocks(Vec<Block>),
}
```

### 2. Three candidate names

1. **`Container`**
   - *What it says*: Directly captures the concept of the memo: "one container". It expresses an open vessel that holds data entries without prejudging whether they act as a sequence, a dictionary, or a mix.
   - *What it gets wrong*: "Container" is extremely abstract in computer science and Rust (`std::collections`). It sounds like an implementation wrapper or an OS container rather than a concrete language primitive.
2. **`Tuple`**
   - *What it says*: Highlights that entries are ordered, heterogeneous, delimited by parentheses `(...)`, and optionally labeled (analogous to Python's namedtuple or TypeScript's labeled tuple elements).
   - *What it gets wrong*: In type theory and language design, a "tuple" strongly implies a fixed-length, immutable product type. Using "Tuple" for a variable-length list of 100 items or a map with 50 keys feels counterintuitive to users.
3. **`Series`**
   - *What it says*: Emphasizes that the structure is fundamentally a sequence/run of items in written order, where items may optionally carry labels. It conveys linear progression and order preservation.
   - *What it gets wrong*: In data science and programming, "Series" is associated with 1D labeled numeric arrays (like pandas Series). Furthermore, it fails to evoke the map/dictionary intuition when every entry is named.

### 3. Nexus args, `Group`, `Entry`, and `Data`

- **Nexus args and value container**:
  - In `Data`, `pub args: Option<Value>` becomes `pub args: Option<Container>`.
  - Empty `()` is `Container::new()`.
  - Single positional `(x)` is a `Container` with one unnamed entry `(None, x)`. (No more unboxing to bare scalar).
  - Multiple positional `(a, b)` is a `Container` with unnamed entries `(None, a)` and `(None, b)`.
  - Named arguments `(k: v)` is a `Container` with a named entry `(Some("k"), v)`.
  - Positional and named arguments together `(a, b, k: v)` naturally share this exact same structure.
- **What happens to `Group`, `Entry`, and `Data`**:
  - `Data` currently holds `args: Option<Value>` and `value: Option<Group>`.
  - `Group` currently holds `Vec<Entry>`, where `Entry` can be `Pair(String, Value)`, `Element(Element)`, `Bare(Bare)`, or `Hole(Hole)`.
  - **Where `Entry::Element`, `Entry::Bare`, and `Entry::Hole` go**:
    - `Entry::Element`: Already has a representation in `Value` as `Value::Element(Box<Element>)` ([crates/tomet-syntax-ast/src/lib.rs:133](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L133)). An entry in `{...}` that is a named element can be represented as an unnamed container entry holding `Value::Element`: `ContainerEntry { name: None, value: Value::Element(...) }`.
    - `Entry::Bare`: `(marker)[content]{value}` has no name and is not an element. It cannot be represented as a `Value` because `Value` has no variant for a bare block.
    - `Entry::Hole`: `<?matcher(...)>` sitting as an entry. While `Value::Hole` exists ([crates/tomet-syntax-ast/src/lib.rs:110](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L110)), an entry hole is structurally an entry.
    - **Crucial finding**: The memo is completely silent on `Bare` and non-pair entries in `{...}` groups. If `Group` is completely eliminated and merged into `Container`, `Bare` entries lose their AST representation unless `Value` or `ContainerEntry` is expanded to accommodate them. This is an unresolved design gap, listed under Section F, Question 2.

### 4. Written spelling: facts and implications

- **Facts from current codebase**:
  - **Formatter (`tomet-formatter`)**: Operates exclusively on CST nodes and source text byte ranges (`tomet-cst`) via whitespace hygiene passes ([crates/tomet-formatter/src/lib.rs:1-50](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-formatter/src/lib.rs#L1-L50)). It does not touch the AST and does not convert CST to AST. It preserves original source spelling by construction.
  - **Printer (`tomet-printer`)**: Operates exclusively on the AST ([crates/tomet-printer/src/lib.rs:1-30](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-printer/src/lib.rs#L1-L30)). It takes `&File` or `&Value` without any CST, tokens, or source text spans. It prints nodes canonically based solely on their AST enum variants.
- **Implication**:
  - The memo requires: `source: upstream(a, b)` and `source: (upstream: (a, b))` are the same tree, and the written spelling is kept as a matter of the spelling layer and not of the tree.
  - Because `tomet-printer` reconstructs `.tmt` text directly from the AST without access to the CST or source text, the AST printer **cannot** distinguish between `upstream(a, b)` and `(upstream: (a, b))` unless:
    1. The AST carries a spelling hint (which the memo explicitly rejects: "a matter of the spelling layer and not of the tree"); OR
    2. The printer always renders the canonical form `(upstream: (a, b))` when printing from AST alone, while format-preserving operations use the CST/formatter layer (`tomet-formatter` / `tomet-edit`).

---

## D. Serialization

### 1. The JSON the tree serializes to today

In `crates/tomet-syntax-ast/src/lib.rs:185-239`, `Value` has a hand-written `Serialize` implementation:
- `Value::Seq` serializes as a standard JSON array: `[item, ...]` ([crates/tomet-syntax-ast/src/lib.rs:203-210](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L203-L210)).
- `Value::Map` serializes as a JSON object: `{"key": val, ...}` ([crates/tomet-syntax-ast/src/lib.rs:211-218](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L211-L218)).
- `Value::Call` serializes as a 2-key JSON object: `{"call": name, "args": args}` ([crates/tomet-syntax-ast/src/lib.rs:219-225](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L219-L225)).

`Group` and `Entry` derive `Serialize` ([crates/tomet-syntax-ast/src/lib.rs:1078, 1101](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1078)):
- `Group(pub Vec<Entry>)` serializes as a JSON array of `Entry` objects.
- `Entry` serializes with external tagging:
  - `{"Pair": ["key", <Value>]}`
  - `{"Element": <Element>}`
  - `{"Bare": <Bare>}`
  - `{"Hole": <Hole>}`

### 2. Real reference file quotation

From committed reference file `tests/ref/syntax/sigils.pandoc.json` (lines 3-13), demonstrating how `Data.args` (here `id`) is serialized into Pandoc metadata:

```json
  "meta": {
    "id": {
      "t": "MetaInlines",
      "c": [
        {
          "t": "Str",
          "c": "doc-sigils"
        }
      ]
    }
  },
```

And in `tests/ref/syntax/groups.pandoc.json` (line 310), showing Pandoc `Attr` pairs flattened from element data:

```json
["", [], [["id", "sec-heading"], ["tag", "groups"]]]
```

### 3. Which reference files and bindings embed this shape

- **Bindings**:
  - `bindings/js/index.d.ts` (lines 32-55): explicitly defines the TypeScript types for the serialized wire format:
    ```typescript
    export type Value = null | boolean | number | string | Value[] | { [key: string]: Value };
    export interface Data { args: Value | null; value: Group | null; }
    export type Group = Entry[];
    export type Entry =
      | { Pair: [string, Value] }
      | { Element: Element }
      | { Bare: Bare }
      | { Hole: Hole };
    ```
  - `bindings/python/src/lib.rs` (lines 41-43, 47-50): `loads` and `parse_file` use `pythonize::pythonize` to convert `Value` and `File` into Python objects based on Serde serialization.
  - `bindings/java/src/lib.rs` (lines 47-50, 68): uses `serde_json::to_string(&doc)` to serialize the entire `File` AST to JSON for Java consumers.
- **Reference files**:
  - All Pandoc snapshot references under `tests/ref/**/*.pandoc.json` embed the flattened data attributes and metadata representations.

### 4. What would change

- `Value::Call`'s JSON representation (`{"call": ..., "args": ...}`) is eliminated.
- `Container` serialization:
  - When all entries are unnamed (`is_list()`), it serializes to a JSON array `[...]`.
  - When all entries are named (`is_map()`), it serializes to a JSON object `{...}`.
  - When entries are mixed, standard JSON has no native format: either a sequence of entry objects/tuples `[{"name": "...", "value": ...}, ...]` or an object with synthetic keys must be emitted.
- In `bindings/js/index.d.ts`, `Value`, `Data`, `Group`, and `Entry` definitions must be rewritten to match the unified `Container`.

---

## E. Order of work

The migration must follow the strict crate layering defined in `.writ.tmt`, moving from the bottom layer upward so that `cargo test --workspace` can remain green at every checkpoint.

### Step 1: Layer 2 -- AST Definition (`tomet-syntax-ast`)

- **Crates**: `crates/tomet-syntax-ast`
- **What changes**:
  - Add `Container` and `ContainerEntry` structs with basic iteration and lookup methods.
  - Replace `Value::Seq`, `Value::Map`, `Value::Call` with `Value::Container(Container)`.
  - Update `Value::serialize` and `Value::deserialize` to handle `Container`.
- **What proves it**:
  - Unit tests in `tomet-syntax-ast` pass.

### Step 2: Layer 2 -- Lowering & Tree Operations (`tomet-syntax-lowering`, `tomet-syntax-tree`, `serde_cotomet`)

- **Crates**: `crates/tomet-syntax-lowering`, `crates/tomet-syntax-tree`, `crates/serde_cotomet`
- **What changes**:
  - Remove `resolve_call`, `construct_string`, `construct_int`, `construct_float` from `tomet-syntax-lowering`.
  - Lower CST sequences, maps, and calls into `Container`. Lower single positional args without unboxing.
  - Update `ValueExt` and `DataExt` in `tomet-syntax-tree` to query `Container`.
  - Update `serde_cotomet`'s deserializer and serializer to work with `Container`.
- **What proves it**:
  - `cargo test -p tomet-syntax-lowering -p tomet-syntax-tree -p serde_cotomet` pass.

### Step 3: Layer 3 -- Semantics & Expression Language (`tomet-semantics-*`, `cotomet`)

- **Crates**: `crates/tomet-semantics-core`, `crates/tomet-semantics-declaration`, `crates/tomet-semantics`, `crates/tomet-semantics-expr`, `crates/tomet-semantics-eval`, `crates/cotomet`
- **What changes**:
  - Implement `string()`, `int()`, `float()` functions in `tomet-semantics-expr`'s `FUNCTIONS`.
  - Update `bind_positional`, `normalized_element_args`, and `flatten_data` to operate on `Container`.
  - Replace `EvalValue::Seq`, `Map`, `Call` with unified `EvalValue::Container` in `tomet-semantics-expr` and `cotomet-eval`.
- **What proves it**:
  - `cargo test -p tomet-semantics-core -p tomet-semantics-declaration -p tomet-semantics -p tomet-semantics-expr -p tomet-semantics-eval -p cotomet` pass.

### Step 4: Layer 4 & 5 -- Validators and Config (`tomet-validator-*`, `tomet-config`, `tomet-spec-derive`)

- **Crates**: `crates/tomet-validator-core`, `crates/tomet-validator-rule`, `crates/tomet-validator-blueprint`, `crates/tomet-validator`, `crates/tomet-config`, `crates/tomet-spec-derive`
- **What changes**:
  - Update argument validation in `tomet-validator-core` to read from `Container`.
  - Update `::rule(allow: ...)` in `tomet-validator-rule` to read allowed names from `Container`.
  - Update `get_path`, `collect_paths`, and `insert_or_merge_prop` in `tomet-config` to operate on `Container`.
- **What proves it**:
  - `cargo test -p tomet-validator-core -p tomet-validator-rule -p tomet-validator-blueprint -p tomet-config` pass.

### Step 5: Layer 6 -- Converters, Printer, Transform, Search, Deps, Vault (`tomet-printer`, `tomet-convert-*`, etc.)

- **Crates**: `crates/tomet-printer`, `crates/tomet-convert-markdown`, `crates/tomet-convert-html`, `crates/tomet-convert-typst`, `crates/tomet-convert-pandoc`, `crates/tomet-transform`, `crates/tomet-search`, `crates/tomet-deps`, `crates/tomet-vault`, `crates/tomet-tome`
- **What changes**:
  - Update `render_value` in `tomet-printer` to print `Container` entries.
  - Update `callout`, `task`, `conflict` handlers in HTML, Markdown, Typst, and Pandoc converters to read `Container`.
  - Update `expressions.rs` and `macro_rewrite.rs` in `tomet-transform`.
- **What proves it**:
  - All converter unit tests pass.

### Step 6: Layer 7 & 8 -- Workspace, CLI, LSP, Bindings, and Snapshot Suite

- **Crates**: `crates/tomet-workspace-indexer`, `apps/cli`, `apps/lsp`, `bindings/*`, `tests`
- **What changes**:
  - Update indexer metadata extraction in `tomet-workspace-indexer`.
  - Update `bindings/js/index.d.ts` and bindings serialization.
  - Update `tests/SYNTAX.md` via `cargo test -p tomet-tests --test syntax_report`.
  - Update snapshot tests in `tests/src/snapshot.rs`.
- **What proves it**:
  - Full workspace check: `cargo test --workspace` passes cleanly.

### Risks (Most Serious First)

1. **Serialization Breakage Across Language Bindings**
   - *Evidence*: `bindings/js/index.d.ts:32-55`, `bindings/python/src/lib.rs:41-50`, `bindings/java/src/lib.rs:47-54`.
   - *Risk*: Any change to the JSON representation of `Value` directly impacts WebAssembly, Python, and Java consumers. If mixed containers cannot serialize to plain JSON objects/arrays, external JS/Python code attempting to access properties will fail at runtime.
2. **Ambiguity and Lossiness in CST-Free AST Printing**
   - *Evidence*: `crates/tomet-printer/src/value.rs:18-65`.
   - *Risk*: The AST printer does not know whether an entry was written as `name(args)` or `(name: (args))` because both produce the identical tree. If round-trip tests require source fidelity, the AST alone cannot provide it without a spelling layer.
3. **Loss of Non-Pair Group Entries if `Group` is Merged into `Container`**
   - *Evidence*: `crates/tomet-syntax-ast/src/lib.rs:1079-1087`, `crates/tomet-convert-html/src/elements/mod.rs:262-278`.
   - *Risk*: Non-pair entries (`Entry::Bare`, `Entry::Element`, `Entry::Hole`) in `{...}` groups do not fit the `name: value` pair model. If `{...}` is forced into `Container`, `Bare` entries lose their home.
4. **Subtle Evaluation Semantic Divergence for Literal Constructors**
   - *Evidence*: `crates/tomet-syntax-lowering/src/element.rs:945-1010` vs `crates/tomet-semantics-expr/src/lib.rs:440-465`.
   - *Risk*: Moving `string()`, `int()`, `float()` from parser time to `$expression` space means unquoted `int("123")` in data files is no longer evaluated at parse time. In data contexts without `$`, `int("123")` becomes a 1-entry container `(int: ("123"))` rather than integer `123`.

---

## F. Questions the memo does not answer

1. **How should a mixed container (containing both named and unnamed entries) be serialized to JSON and Serde?**
   - *Raised at*: [crates/tomet-syntax-ast/src/lib.rs:185](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L185) (`impl Serialize for Value`).
   - *Context*: A pure list serializes as `[1, 2]`; a pure map serializes as `{"a": 1, "b": 2}`. JSON has no native type that represents an ordered sequence of optionally-named entries.
   - *Option A*: Serialize mixed containers as a JSON array of entry objects or tuples:
     - Example: `(1, key: 2)` -> `[{"name": null, "value": 1}, {"name": "key", "value": 2}]`.
   - *Option B*: Serialize mixed containers as a JSON object, generating numerical string keys for unnamed entries:
     - Example: `(1, key: 2)` -> `{"0": 1, "key": 2}`.
   - *Option C*: Fail serialization with an error when a container is mixed, requiring data intended for JSON export to be either pure list or pure map.

2. **Where do non-pair group entries (`Entry::Element`, `Entry::Bare`, `Entry::Hole`) live if `Group` and `{...}` become a `Container`?**
   - *Raised at*: [crates/tomet-syntax-ast/src/lib.rs:1079-1087](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1079-L1087) (`enum Entry`).
   - *Context*: A nexus's `{...}` group currently holds `Entry::Bare(Bare)` (e.g. `@memo(){ (1)[content] }`) and `Entry::Element(Element)` alongside `Entry::Pair(k, v)`. The memo states that data space has one container of entries, each with or without a name, but does not state whether `Group` is replaced by `Container` or retained for block bodies.
   - *Option A*: Keep `Group` and `Entry` separate from data containers; `Container` represents data `(...)` and pure data maps, while `Group` remains for `{...}` blocks holding elements, bare items, and holes.
     - Example: `Data.args` is `Option<Container>`, while `Data.value` remains `Option<Group>`.
   - *Option B*: Merge `Group` into `Container`, requiring `ContainerEntry` to be an enum that includes `Bare` and `Element`:
     - Example: `pub enum ContainerEntry { Field { name: Option<String>, value: Value }, Bare(Bare), Element(Element) }`.
   - *Option C*: Represent bare entries as special unnamed elements with a synthetic name (e.g. `bare`), so that all entries can be `Value::Element`.
     - Example: `ContainerEntry { name: None, value: Value::Element(BareElement) }`.

3. **Does the parser still desugar a single positional argument in `(args)` to a bare unboxed scalar?**
   - *Raised at*: [crates/tomet-syntax-lowering/src/element.rs:74](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/element.rs#L74) (`lower_paren_value`).
   - *Context*: Today, `@raw(rust)` produces `args: Some(Value::String("rust"))` (unboxed scalar), whereas `@memo(a: 1)` produces `args: Some(Value::Map(...))`. Under the one-container model, `(rust)` is a container with 1 unnamed entry.
   - *Option A*: Eliminate unboxing completely; every `(...)` is always stored as `Container`, so `@raw(rust)` has `args: Some(Container([None, "rust"]))`.
     - Example: `el.data.args.as_ref().unwrap().nth_unnamed(0) == Some("rust")`.
   - *Option B*: Retain scalar unboxing for single positional arguments when an element's parameter signature expects a single scalar.
     - Example: `el.data.args == Some(Value::String("rust"))`.

4. **How do two containers merge when evaluated under `merge_connect` / `merge_values`?**
   - *Raised at*: [crates/tomet-syntax-lowering/src/lower/connect.rs:408](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-lowering/src/lower/connect.rs#L408) (`merge_values_inner`).
   - *Context*: Today, merging two `Value::Map`s merges them key-by-key (latter overwriting former), while `Value::Seq` replaces the target outright. With containers holding both named and unnamed entries, merging rules must be defined.
   - *Option A*: Named entries merge key-by-key (overwriting duplicates in place), while unnamed entries are appended.
     - Example: Merging `(1, a: 10)` and `(2, a: 20)` yields `(1, 2, a: 20)`.
   - *Option B*: Full replacement if either container has unnamed entries; key-by-key merge only if both are pure maps.
     - Example: Merging `(1, a: 10)` and `(2, a: 20)` completely replaces with `(2, a: 20)`.

5. **How should `enum(...)` in `::rule(allow: ...)` be handled once call syntax is removed from data space?**
   - *Raised at*: [crates/tomet-validator-rule/src/decode.rs:29](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-validator-rule/src/decode.rs#L29) (`Value::Call(callee, items) if callee == "enum"`).
   - *Context*: Currently, `::rule(allow: enum(card, note))` parses as `Value::Call("enum", vec!["card", "note"])`. Under the one-container model, `enum(card, note)` becomes `(enum: (card, note))`.
   - *Option A*: `decode_rule_args` looks for a named entry `enum` inside `allow`, unwrapping its child container entries.
     - Example: `::rule(allow: (enum: (card, note)))` is decoded the same as `allow: (card, note)`.
   - *Option B*: Retire `enum(...)` syntax entirely from data space declarations, accepting only the plain container `allow: (card, note)`.
     - Example: `::rule(allow: (card, note))`.

6. **How should cotomet (`.ctmt`) root documents be typed?**
   - *Raised at*: [crates/tomet-syntax-ast/src/lib.rs:1537](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast/src/lib.rs#L1537) (`pub struct File<B = Vec<Block>>` where cotomet sets `B = Value`).
   - *Context*: A `.ctmt` data document currently requires top-level `key: value` pairs, producing `Value::Map`.
   - *Option A*: A cotomet document body is typed as `Container`, allowing top-level unnamed entries (a data list file) as well as named entries.
     - Example: A `.ctmt` file containing `(1, 2, 3)` parses to a root `Container` with 3 unnamed entries.
   - *Option B*: A cotomet document root is restricted to a `Container` whose entries must all be named (`is_map() == true`), rejecting bare unnamed items at root.
     - Example: A `.ctmt` file containing `a: 1, b: 2` parses to a root `Container`, while `(1, 2)` at the root is a syntax error.

7. **How should the AST printer format a single-named-entry container `(name: (args))` without CST access?**
   - *Raised at*: [crates/tomet-printer/src/value.rs:59](file:///home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-printer/src/value.rs#L59) (`render_value`).
   - *Context*: The memo states `name(args)` is the value `(name: (args))` and "the written spelling is kept, as a matter of the spelling layer and not of the tree." But `tomet-printer` rebuilds source text directly from the AST with no CST or token stream.
   - *Option A*: The AST printer always prints the canonical container spelling `(name: (args))`, accepting that printing from an AST does not preserve call-syntax sugar.
     - Example: Output is always `(upstream: (a, b))`.
   - *Option B*: The AST printer uses a heuristic: any 1-entry container `(name: (args))` where the child value is a container is printed using call syntax `name(args)`.
     - Example: `(upstream: (a, b))` prints as `upstream(a, b)`.

---

## G. What you did not do

- Did not run shell commands, cargo builds, or test suites, in strict compliance with the survey rules.
- Did not edit any existing code or documentation files.
- Did not inspect or modify other Jujutsu workspaces outside the current workspace directory.
- Did not read files under `editors/` (such as `editors/zed`), as they are editor configuration extensions rather than compiler pipeline readers.
- Did not read the internals of `crates/tomet-syntax-lexer` or `crates/tomet-syntax-cst-parser`, because the CST layer represents raw syntax tokens before AST value classification.
- Assumed that `tests/SYNTAX.md` and `tests/src/syntax_report.rs` accurately describe the complete set of grammar constructs currently accepted by the parser.
