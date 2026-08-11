# What is TypedMark?

- Human-readable and ergonomic.
- Friendly to parsers.
- Designed with extensibility in mind.
- Focused on \`key:value\` structures.
- Created as a research project.

## Why use TypedMark?

TypedMark uses an unambiguous syntax specialized for links and variables, with a unified notation.

\`\<T>\` can be followed by any combination of \`(input)\`, \`\[area\]\`, and \`{value}\`. The three groups may appear in any order, but each type of group can appear at most once per element.

- \`\<input>(name:email, type:email){required}\`

For built-in reference types, \`@\` is used instead of \`\<T>\`. The type is inferred from the key name inside \`()\` (\`url\`, \`file\`, \`ref\`, etc.). Since the name does not need to be written explicitly, the meaning of each key is kept unique within its type.

- \`@(url:https://example.com)\[Wiki\]\`
- \`@\[Wiki\](url:https://example.com)\`
- \`@(file:assets/image.png)\[Caption\]\`
- \`@(ref:anotation1)\`

**Rules for \`\[\]\`**

---

- \`\[\]\` (\`content-span\`) is valid only as a group immediately following \`\<T>\`, \`@\`, or \`#\` (heading). A standalone \`\[...\]\` at the beginning of a line is not a content-span; it is simply a string.

→ Therefore, do not write a bare word followed by \`\[\]\`, such as \`Caution \[ ... \]\`. Instead, explicitly specify the type: \`\<caution>\[ ... \]\`. → List markers \`-\` do not use \`\[\` or \`\]\` in the first place, so there is no conflict.

- Array literals such as \`tags: \[a, b\]\` are allowed inside \`{value}\`. The contents of \`{}\` form a separate nesting level and are therefore unrelated to the content-span rules.

**vs. JSON**

---

TypedMark can compete with JSON through its \`key:value\` structure.

TMT is human-first and is particularly suited to inserting long, line-break-containing text.

It is not well suited to deeply nested structures. On the other hand, I believe it may be particularly well suited to timeline-oriented formats.

# Other Concepts

**1**: Annotation
**anotation1**: Annotation

The \`(id)\[content\]\` notation inside \`@links{}\` is the only case where the type name may be omitted. Since the container itself establishes that these are annotation definitions, there is no need to specify the type for each individual element.

# Remaining Questions

- Whether to keep the name "TypedMark". It was originally intended to be compatible with Markdown, but the current syntax is now almost completely unrelated to Markdown.

- The list of inference keys for \`@\` (\`url\` / \`file\` / \`ref\`) is formally maintained as a registry in \`typedmark\_ast::INFERRED\_AT\_KEYS\` / \`infer\_at\_kind\` (decided and implemented). Bare \`@(key:...)\` inference was introduced specifically to reduce the number of characters needed when using \`url\`, \`file\`, and \`ref\` inline; it is not intended as a general convenience feature for omitting names.

\`meta\` is not included in this registry. \`@meta(tag){...}\` is always written explicitly at the block level, where there is little benefit to omitting the name. Therefore, the bare form \`@(meta:yaml){...}\` has been deprecated. If used, it will not be inferred as a \`meta\` element and will instead fall back to the generic \`at\` element.

- If \`(format)\` inside \`{...}\` contains a \`format\` key (\`json\` / \`yaml\` / \`toml\`), the contents are parsed directly as actual JSON/YAML/TOML source using the \`serde\_json\` / \`serde\_yaml\` / \`toml\` crates, rather than TypedMark's lightweight \`Value\` syntax (decided and implemented).

This is not special handling for \`@meta\`; it is a generic mechanism that works for any element. The behavior is determined solely by whether a \`(format:...)\` key is present.

If the \`format\` key is absent or contains an unknown value, parsing falls back to the lightweight syntax as before.

For \`@meta\`, the currently supported canonical form is the explicit key-based syntax:

\`@meta(format:json){...}\`

The former positional/tag-like syntax \`@meta(json){...}\` has been deprecated. It may be reintroduced as syntactic sugar in the future, but only after the explicit form has stabilized.

Note that the text passed as the contents of \`{...}\` excludes the outer \`{\` and \`}\` themselves. Therefore, when writing a JSON object, an additional pair of \`{}\` is required:

\`@meta(format:json){ {"key": "value"} }\`

YAML and TOML do not require the additional braces because \`key: value\` and \`key = "value"\` are already complete documents in their respective formats.

- Code blocks are called \`\<codeblock>\`, rather than \`\<pre>\`, to follow CommonMark terminology instead of borrowing an HTML tag name.

The code itself is placed in \`\[area\]\` rather than \`{value}\`:

\`\<codeblock>(lang:xxx)\[code\]\`

\`{}\` is reserved for metadata such as \`id\` and \`cssclass\`, just like for other elements:

\`\<codeblock>(lang:rust){id:snippet1}\[code\]\`

The \`\[...\]\` content of \`codeblock\` is the only exception that is treated as raw text and does not process any inline syntax, such as \`\*em\*\`, \`\` \`code\` \`\`, or element triggers.

This prevents characters such as \`\*\`, \`\<\`, and \`@\` appearing in actual source code from being interpreted as markup.

# Other

- Easy to write furigana (ruby annotations).

