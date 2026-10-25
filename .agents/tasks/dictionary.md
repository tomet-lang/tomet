# A dictionary of terms: `::dictionary` and `@term`

A term (a word the documents use, such as `vault.config.tmt`) is defined once, with a description, and everything else
mentions it by reference (`@term(...)`). Then a rename is one edit and `tomet` can rewrite the mentions. Delete this file when done.
Destination of what is worth keeping: `tmtroot/spec/` (a page for `::dictionary`, `@term`), the doc comments of the crates.
Related: `docs-config-rework` (waits for this, or uses `::macros` in the meantime), `import-declaration` (a possible way to
cut a dictionary out into a file), `id-generation-settings` (`aliases` is the same question as several names for an entry).

## Current shape (author, 2026-10-30, settled "for now"; read this first, the rounds below are the history and their old names are superseded)

```
::dictionary(tomet){
  "vault.config.tmt": @term[description]#(vault-config)
}
::dictionaries{ tomet: @file(dictionary/tomet.tmt) }     (registry, like ::vocabularies; layered, inner replaces by name)

@term(tomet#vault-config)               by identifier: the dictionary on the left of #, as <document>#<place>
@term(tomet#name("vault.config.tmt"))   by name: a step name("...") like item(2) / prop(tags)
@term("vault.config.tmt")               by name, no dictionary: searched in all registered dictionaries
```

- An identifier needs its dictionary (ids are unique per dictionary, and a bare `#` means this document): `@term(#id)` outside
  a dictionary is an error that says to name one. Inside a dictionary file a bare `#id` is that dictionary.
- The name form with no dictionary matching several dictionaries is an error that says to name one (assistant's lean, not yet
  confirmed by the author).
- The forms `@term("x", tomet)`, `@term(#id, tomet)` and `in:` of the third round are replaced by the above (one way to write).
- No address scheme (`def:` / `term:`); `@term` is unrelated to `@link`. No link to source code. Behaviour declarations: not now.
- New in the address grammar: the step `name("...")`, an entry by its name. Its own task: `address-name-step`.
- Comments in Rust: need a way to read and rewrite mentions in comments, which does not exist yet; a later task after the
  documents (`docs-config-rework`) have used the dictionary.

## Implementation plan (workspace `.agents/workspaces/dictionary`, from `main` `2e5598bb`; 2026-10-30)

One commit per feature, each green. Mark a step when it is done.

- [x] 1. The names (done in the workspace, uncommitted: `kind.rs` in `tomet-semantics-declaration` and `tomet-semantics`, `positional.rs`; whole workspace tests and `sisho check .` green). `DeclKind::Dictionary` (`::dictionary(name)`, a named container, like `::vocabulary(ns)`) and
  `DeclKind::Dictionaries` (the registry); `ElementKind::Term` (`term`, in `BODY_KINDS`) with one positional slot, `target`
  (the mention is one argument now: `tomet#id`, `tomet#name("...")` or `"..."`). `tomet check` accepts `::dictionary` / `@term`.
- [x] 2. Config (done in the workspace, uncommitted: `dictionary_name` / `document_dictionary` in `tomet-declaration`, `ConfigLayer`, `RegistryConfig.dictionaries`, `RegistryNameMismatch`; registry only; an inline `::dictionary(x){...}` written in a config layer is not in this task: it was the assistant's
  addition, not the author's decision, and a small registered file does the same. Ask the author before adding it). `RegistryConfig.dictionaries`, `ConfigLayer`, `Config::from_layers` / `from_merged`, `registry_entries`;
  `RegistryNameMismatch` for a file that names itself otherwise. Inline `::dictionary(x){...}` in any layer.
- [x] 3. Load (done in the workspace, uncommitted: `tomet-semantics/src/dictionary.rs` with `Term` / `Dictionary` / `LoadedDictionaries`, `tomet-load/src/dictionary.rs`, `Vault::with_dictionaries` / `dictionaries` / `setup_problems`, reported by `tomet check` and the LSP as warnings; a term written twice is already a parse error). A loaded set of dictionaries in `tomet-semantics` (entries: name, description, optional id), read from the registry
  paths in `tomet-load`, held by `Vault`; duplicate names inside a dictionary and duplicate ids are errors.
- [x] 4. Resolve (done in the workspace, uncommitted: `TermRef::parse`, `LoadedDictionaries::resolve`, `TermProblem` in `tomet-semantics/src/dictionary.rs`; `Diagnostic::UnresolvedTerm`; `Vault::diagnostics` reports them. The grammar is parsed in the dictionary module, not the address crate: the `name(...)` step stays in `address-name-step`. Known edge: a word that looks exactly like an address (`C#sharp`) must be written `tomet#name("...")`). Parse the `@term` target (`<dictionary>#<id>`, `<dictionary>#name("...")`, `"..."`) in the address crate, where
  the `name(...)` step belongs (task `address-name-step`); look up by id / by name / in all dictionaries; diagnostics:
  unknown term, ambiguous term (name form, no dictionary, several match), id without a dictionary, dictionary not registered.
- [x] 5. Render (done in the workspace, uncommitted: `Vault::prepare` calls `fill_terms`, which gives a resolved mention the term's name as its content unless the author wrote some; HTML `<span class="tm-element tm-term">`, Markdown `<span data-tm-kind="term">` (HTML passthrough, a possible follow-up is plain text), Typst plain text). `Vault::prepare` lowers `@term` to the entry's name as plain text, before any converter runs, so no converter
  learns the element; without a vault the name written in the mention stays. (The author has not said how a mention should look;
  plain text is the neutral default.)
- [x] 6. (done: `collect_ids` walks args, value groups and value slots) The duplicate-id check sees ids on elements in a declaration's value group (`collect_ids` in `tomet-validator-core`).
- [x] 7. (done as unit tests in each crate and checked with the real CLI; no fixture, since no grammar changed; whole workspace tests and `sisho check .` green) Spec and tests: fixture under `tests/fixtures/` (frozen corpus; regenerate refs), doc comments of every crate touched.
  The spec pages for `::dictionary`, `::dictionaries`, `@term` are part of `docs-config-rework`.
- [ ] 8. Not in this task: LSP (hover, go to definition, rename), the `name(...)` step in the general address grammar, comments
  in Rust.

**State (2026-10-30): implemented and pushed as `d08aeac9` (`feat(dictionary)`) on `main`; the workspace is gone and the default
workspace is rebased onto it. Next: use it (`docs-config-rework`: `tmtroot/dictionary/tomet.tmt`, `::dictionaries` in the repository's
`vault.config.tmt`, then the spec pages). Delete this file when the open points below are settled and folded into the spec.**

## Decided (author, 2026-10-30)

- A term has a name, a description and a reference.
- The name is the string itself, quoted (`"vault.config.tmt"`), not an identifier, so that words in other languages (Japanese)
  can be given. (An identifier is optional; see the second round.)
- It is addressed with a new address scheme, `def:` (the two forms are in the second round).
- It is written in text with an element, `@term("vault.config.tmt")`.
- A dictionary is declared with `::terms{ ... }`, all at once, and can be cut out into a file. It behaves almost like
  `::vocabularies`.

## Decided (author, 2026-10-30, second round)

- Naming splits by use. Someone who only declares a term and its description needs no identifier (the term does not change).
  Someone who creates a term that may be renamed can give it one. So the identifier is optional per entry.
- Two ways to address an entry: `def:#xxx` (the stable identifier, after `#`) and `def:"text"` (the term itself). The
  `def:#xxx` form is rename-proof; a rename of the text is done by the tool (`tandr`) over the `@term` references.
- `@term("x")` and `@link(def:"x")` stay apart, as two things of different meaning even though both refer (the way a
  specialised reference such as `@crate` is apart from a plain link).
- Direction, not designed yet: a term can declare its behaviour (how a mention is handled: a link or not, ...), so that a
  converter reads the declaration instead of knowing the term. It is meant to improve `@ftn` as well.

## Entry shape (accepted for now by the author, 2026-10-30: "the definition can be changed later"; checked against the parser)

```
::terms{
  "vault.config.tmt": @term[ description ]#(vault-config)
  "ボールト": @term[ 同じ語の日本語。 ]
}
```

It parses today with no change to the syntax (`Value::Element` in a value slot), and `tomet format` keeps it byte for byte.
The key is the name, the content is the description, and
`#(id)` is the optional identifier, so `def:#xxx` is the element identifier. Still open: the same element name `@term` would
mean a definition inside `::terms` and a mention in the body (`@term("x")`); and an element id is unique within a document
today, while `def:#xxx` needs a scope across the registered dictionaries.

## Decided (author, 2026-10-30, third round)

- The declaration is `::dictionary(name)`, a named container like `::vocabulary(ns)`; its entries are `"term": @term[description]#(id)`.
  The registry follows `::vocabularies`: `::dictionaries{ name: @file(path) }`; a dictionary may also be written inline.
- A mention names the dictionary in a second argument, not in the address: `@term("文字", in: tomet)`, or positionally
  `@term("文字", tomet)` (the second positional slot is called `in`), and by identifier `@term(#id, tomet)`; without a dictionary,
  `@term("文字")` searches all registered dictionaries. This keeps the address grammar out of it (the author: "I was too
  pulled by the address").
- Checked: all four forms parse as they stand; `tomet format` keeps the text; `#id` as a first argument reaches the parser as the
  string `"#id"` and is given its meaning later. Needs: a positional table entry for `term` (`crates/tomet-semantics/src/positional.rs`),
  an `ElementKind`, and the resolution.
- Residual concern (assistant): in `@term(#id, tomet)` the bare `#id` points into another document, against "a bare `#` is
  this document". The explicit dictionary makes it clear; a form without `#` would be `@term(id: vault-config, in: tomet)`.

## The name (settled: `term` for the element, `dictionary` for the declaration)

`def:` was provisional. Candidates for the address scheme, the declaration and the elements: `def`, `term`, `dictionary`, `dict`.
None of them is used as a name today (checked in `crates`, `apps`, `tmtroot/spec`, `tmtroot/schema`).

What decides it is consistency with how things are named now (assistant's reading):

- A table declaration is the plural of the entry noun: `::macros`, `::vars`, `::fields`, `::blueprints`, `::vocabularies`.
- An address scheme and an element share the name of what they point at: `file:` and `@file`, `dir:` and `@dir`. And `@file` is
  used both in prose and as the value of a registry entry (`::blueprints{ writ: @file(...) }`), so one element name for a
  definition and a mention has a precedent.
- `def` names a relation, and "definition" already means the entries of `::macros` / `::vars` / `::elements` in this
  project (`Diagnostic::InvalidDefinition`) and the definition list that is on hold.
- `dict` reads as the map type (the language's `{...}` groups are maps). `dictionary` names the container, not the entry.

Recommendation (not decided): `term` everywhere (`::terms`, `@term`, `term:"x"`, `term:#xxx`); `dictionary` for a kind of
document (`::kind(dictionary)`) and for prose.

### How it reads if `term` is used everywhere (same pattern as `@file` / `file:`)

```
@term("vault.config.tmt")          a mention, by the term itself (the element name stands for the scheme)
@term(#vault-config)               a mention, by the stable identifier
@link(term:"vault.config.tmt")     a generic link, the scheme spelled out
@link(term:#vault-config)
```

`@file(path)` takes no `file:` and `@link(file:path)` does, in the same way; so `term:` appears where an address is expected,
and never inside `@term(...)`.

## The meaning of a bare `#` (raised by the author, 2026-10-30)

A bare `#xxx` means "in this document" and must stay so: a reader who sees `#xxx` is sure it is somewhere in the same document.
So `@term(#vault-config)` / `term:#vault-config` break it. Options:

- A (assistant's lean): `<dictionary>#<id>`, the existing `<document>#<place>` shape with the dictionary as the "document":
  `@term(core#vault-config)`, `@link(term:core#vault-config)`. Inside the dictionary file a bare `#vault-config` still works (same
  document). The dictionary needs a name (the file names itself, as `::vocabulary(ns)` does); only the by-identifier form needs
  it, the by-text form `@term("...")` searches all registered dictionaries.
- B: the existing `tmt:path#id` (the dictionary is a document). No new rule; long and tied to the path.
- C: a marker other than `#` for the identifier of an element. Another sigil to learn; not recommended.

Found while checking (2026-10-30): an `#(id)` on an element that sits in a declaration's value group (`::fields{ "x": @file(a)#(entry) }`)
is seen as an anchor (`@link(#entry)` resolves, `@link(#nope)` is a dangling anchor), but the duplicate-id check does not see it
(`::fields{ "x": @file(a)#(dup) }` plus `=[ H ]#(dup)` passes `tomet check`). If the entry's `#(id)` is to be the stable identifier,
the check has to cover it. (`collect_ids` in `tomet-validator-core`.)

## The declaration: `::dictionary(name)` (the author's realization, 2026-10-30; to be confirmed)

```
::dictionary(tomet){
  "文字": @term[説明]#(id)
}
```

A named container, like `::vocabulary(ns)` / `::blueprint(x)`: a singular noun and a name. (The rule "a table is the plural of its
entry noun" is for tables with no name of their own: `::macros`, `::vars`.) The registry follows `::vocabularies`:
`::dictionaries{ tomet: @file(path) }`, and a dictionary may also be written inline in any layer. That answers how a dictionary
is cut out into a file.

Mentions that name the dictionary: `@term(tomet#id)`. For the text there is no rule that quotes separate two parts, so
`@term(tomet"文字")` is out. Candidates: `@term(tomet#("文字"))`, or no new grammar with `@term("文字", in: tomet)`.

Checked (tomet at `2e5598bb`): a quoted id is already valid where an id is declared (`=[ H ]#("文字 列")` reads as the id
`文字 列`; `tomet format` keeps it), but not where an address refers to one (`@link(#("文字 列"))` is a dangling anchor). So
`dictionary#("文字")` needs the reference side of `#(...)` defined, which is the same extension as the `#(key:value)`
selector that is only planned; design them together. A declaration `#(x)` (right after an element, nothing on its left) and a
reference `dictionary#(x)` (an address slot, something on its left) are told apart by position; a new sigil is not proposed yet.

## Notes (assistant, not decided)

- A `::terms` is a name table, the family of `::macros`, `::vars`, `::fields`, `::blueprints`: it is layered (vault, folder,
  document) and merged by name. A term contains `.` (`vault.config.tmt`): the merge of `::format` (`merge_entry`) splits a
  dotted key into nested maps and must not be used for it (`::fields` and the registries already avoid it).
- (Superseded: no address scheme.) `def:"x"` is like `ref:note`: a lookup by name. `#` is not needed unless the dictionary is a document whose entries are
  addressed inside it, which `tmt:dictionary.tmt#...` can already say.
- `@term("x")` is close to `@link(def:"x")` with the display text taken from the entry. Whether both are needed is open. A new
  standard element means a new `ElementKind` and a rendering in each converter (HTML, Markdown, Typst, Pandoc), and the
  language server (hover, go to definition, rename).
- A behaviour declared on a term is the same idea as the `display` / `args` an entry of `::elements` declares for a
  vocabulary element: metadata for tools, read from the declaration. A converter need not know a term if a step before
  rendering lowers `@term` to its concrete form, the way `Vault::prepare` resolves every `${...}` before any converter runs.
- An entry written as a map entry (`"x": { ... }`) has nowhere to put `#(id)` (that belongs to an element); an entry written as an
  element (`::term("x")#(id)[description]`) gets the identifier and block-content description for free, but is not a name table.
- A rename of the text needs `tandr`, which is not installed in this environment; `def:#xxx` does not need it.
- Missing mechanism it would use: a rename that rewrites references (`tomet refactor`), and a lint that finds a defined
  term written as a literal where a reference should be.

## Open

- [x] The scope of `#(id)` of an entry: the same as any id, unique in its document (the dictionary). A mention names the dictionary
  (`@term(#id, tomet)`), so there is no cross-dictionary scope. To do at implementation: fix the duplicate-id check for ids on
  value elements.
- [x] The shape of the description: `@term[description]` (author, 2026-10-30).
- [ ] (Not now; the author raises it.) The behaviours a term can declare, and where they are applied.
- [x] The address scheme: none. `@term` and `@link` are unrelated elements (author, 2026-10-30).
- [ ] Several names for one entry (English and Japanese forms of the same term), or one entry per name? (Same question as
  `aliases`.)
- [ ] Collisions: two dictionaries define the same term. Assistant's lean (author asked for a recommendation): a mention with no
  dictionary that matches several is an error that says to name one (`@term("x", tomet)`); a silent winner would change a
  document's meaning when a dictionary is added. Same-named dictionaries in different layers: the inner replaces by name, as
  `::vocabulary` does.
- [ ] Is this the "more general model" behind the definition-list element that was put on hold (`from-pandoc` drops a
  `DefinitionList` for lack of a target)? If so the element and this share a design.
