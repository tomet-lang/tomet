<!-- Generated from tmtroot/bindings/schema/readme.tmt. Edit that, then `tomet export .`. -->

# Tomet JSON Schemas

The shape of what the bindings hand out, as JSON Schema (draft 2020-12): one document per contract.

- **`ast.schema.json`**: The AST wire format, rooted at `File`. It is lossless: a document serialized and read back is the same

document, spans included. It is what `parseFile` returns and what `toHtml(file)`, `printFile(file)` and friends take back.

- **`plain.schema.json`**: Plain data, rooted at `PlainValue`: `null`, booleans, numbers, strings, arrays and objects. It is lossy by

design and does not read back into the AST. It is what `parseValue` (JavaScript), `loads` (Python), `parseValueJson` (Java) and the
`meta` of a processed file hand out.

They are two files because they are two contracts with nothing in common: a consumer of one has no use for the other.

## Generated, not written

Both files are generated from the Rust types by `tomet spec ast-schema`, and descriptions are the doc comments of those types.
Do not edit them: change the types or their doc comments, then regenerate.

```bash
just docs        # regenerates these files with the rest of the generated documents
tomet spec ast-schema --out bindings/schema
tomet spec ast-schema --check --out bindings/schema
```

`cargo test` holds them to the types too (`tomet-cli`'s `ast_schema` tests), and checks that every document of the test corpus
serializes to something valid against `ast.schema.json` (`tomet-tests`' `schema` test).

## Identity and versions

Each schema's `$id` carries the version of the workspace, which is the version of `tomet-ast`:
`https://github.com/tomet-lang/tomet/schemas/<version>/ast.schema.json`. An identifier is not a locator: nothing is served from it.
A change of the wire is a change of these files, and a release regenerates them under the new version.

## Reading the schema

- A tagged enum (`Block`, `Value`, `Scalar`, ...) is an object with one key, the variant's name, holding its payload; a variant with

no payload is its name as a string (`"Block"`, `"Auto"`).

- Every field is present; an absent one is `null`.
- A number kept as written is `{"Number": "1.10"}`, never a JSON number: the wire does not round it.

