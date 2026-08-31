# tomet-compute

Expression evaluation for `${...}` interpolation expressions (`InterpExpr`) and built-in math functions.

## Architecture & Responsibilities

1. **Pure Compute Boundary (Zero I/O)**:
   - `tomet-compute` performs dynamic expression evaluation for Tomet interpolation expressions (`${...}`).
   - It performs ZERO I/O: identifier and member lookups (`${node.member}`) are delegated to `tomet-resolver::resolve_reference`.
   - Once values are resolved, `tomet-compute` applies functions and evaluates the final expression result.

2. **Core Capabilities**:
   - **Expression Evaluation (`evaluate`)**:
     - `Literal`: Evaluates directly to `Value::Int`, `Value::Float`, or `Value::String`.
     - `Identifier` / `Member`: Resolves against `Document` through `tomet_resolver::resolve_reference`.
     - `Call`: Recursively evaluates argument expressions from left to right, then dispatches to builtin functions.
   - **Builtin Functions (`add`, `sub`, `mul`, `div`, `mod`)**:
     - Binary numeric operations.
     - Type promotion: two integers produce an integer; mixing float/int produces a float.
     - Overflow safety: `i64` overflow automatically promotes to `f64` rather than panicking or wrapping.
     - True division: `div(7, 2)` produces `3.5` (`Value::Float`).
