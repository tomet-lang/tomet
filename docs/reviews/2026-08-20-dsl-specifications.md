# Config DSL and Value DSL Specification Decision (2026-08-20)

## Overview

This document records the finalized specifications for TypedMark's **Value DSL** (used in `(args)` attribute groups, `{value}` data bodies, and `.tm`/`.tmt` data documents) and **Config DSL** (used in `@config` directive blocks, `@settings` schema definitions, and `.settings.tm` files).

---

## 1. Value DSL Specification

Value DSL is the data modeling language for TypedMark.

### 1.1 Key-Value Pairs and Colon-Less Blocks
- **Key-Value Separator**: `key: value` (unified standard).
- **Colon-Less Object Blocks**: `key { ... }` is sugar for `key: { ... }`.
- **Separators**: Entries can be separated by commas `,` or newlines. Trailing commas are allowed.

```tm
server {
  host: "localhost"
  port: 8080
  features: [ "ssl", "http2" ]
}
```

### 1.2 Dotted Key Path Expansion
Keys containing dots (`.`) automatically expand into nested map structures, merging consecutive entries sharing prefixes:
```tm
server.http.host: "localhost"
server.http.port: 8080
```

### 1.3 Literals & Scalars
- **Booleans & Null**: `true`, `false`, `null` (`nil`, `none` as aliases)
- **Numbers**: Integers (`42`), Hex (`0xFF`), Floats (`3.14`)
- **Strings**:
  - **Bare Scalars**: Unquoted identifiers/paths/enums (`format: json`, `placement: block`).
  - **Double-Quoted (`"..."`)**: Escaped string (`\n`, `\t`, `\"`, `\\`).
  - **Single-Quoted (`'...'`)**: Raw string without backslash escaping.
  - **Multi-line Triple Quotes**: `"""..."""` (escaped) and `'''...'''` (raw).
- **Comments**: Single-line (`//` and `#`), Multi-line (`/* ... */`).

---

## 2. Config DSL & Schema Specification (`@settings`)

### 2.1 Embedded Format Support
`@settings` supports embedded formats (`format: json` / `format: yaml` / `format: toml`) alongside native TypedMark syntax via `embedded_format.rs`.

```tm
@settings(format: json){
  {
    "elements": {
      "bookmark": {
        "args": {
          "title": { "type": "string" },
          "id": { "type": "string" },
          "count": { "type": "uint", "default": 0 }
        },
        "required": ["title", "id"],
        "positional": ["title"],
        "content": "block",
        "placement": "block"
      }
    }
  }
}
```

### 2.2 Schema Structure (`args` and `required` list)
Required fields are declared at the element schema level via a top-level `required: [...]` array (aligned with JSON Schema / OpenAPI standards).

In native TypedMark syntax:
```tm
@settings {
  elements {
    bookmark {
      args {
        title: { type: string },
        id: { type: string },
        count: { type: uint, default: 0 }
      },
      required: [title, id],
      positional: [title],
      content: block,
      placement: block
    }
  }
}
```
