# Link/Reference Target Unification via URI Schemes (2026-08-22)

## 1. Overview & Motivation

While building a broken-link checker (`typedmark-links`, this session), link-target
extraction turned out to be inconsistent across the codebase's existing consumers:

- HTML's `render_href_element`/`render_ref_element` and Markdown's
  `render_link`/`render_ref`/`render_wiki` each read `el.args` raw, so a
  bare-positional `<file>(readme.md)` silently yields no target in both.
- `<embed>`'s HTML and Markdown renderers each use a *different* fallback
  key chain (`src -> path -> file -> url -> wiki` vs `path -> file -> url -> wiki`).
  Neither `src` nor `path` is an officially supported key: `src` is a
  mistake (confirmed by this project's author), and `path` is documented
  in `docs/ja/builtins/args.tm` as having "no special meaning" even
  though the Markdown importer emits it (`![alt](dest)` -> `<embed>(path:dest)`)
  and both renderers special-case it anyway.

This drift is a symptom of the underlying design: the argument **key**
(`file:`/`url:`/`wiki:`/`ref:`) carries the "what kind of link is this"
information, and different consumers independently reinvented which keys
to check and in what order. This document records the resolution reached
in discussion: instead of the key carrying kind information, the **value**
does, via a URI-style scheme prefix -- a single, well-specified mechanism
(RFC 3986's relative-reference resolution) replaces several ad-hoc
per-consumer fallback chains.

## 2. Original key semantics (starting point)

Before this discussion, the intended (but inconsistently implemented)
semantics were:

- `path:` -- a literal filesystem path. `./...` is relative to the
  referencing file; `/...` is OS-absolute.
- `file:` -- a path relative to the **project root**. Introduced
  specifically because a bare `/...` alone doesn't visually distinguish
  "OS-absolute" from "project-root-absolute" -- `file:` exists to remove
  that ambiguity.
- `wiki:` -- a filename, resolved by searching within the project (the
  usual wikilink convention: readers don't type extensions or full paths).

These keys (`url`/`file`/`ref`/`wiki`) originate from how to interpret the
terse inline form `@[display](value)` (a Markdown-link-like shorthand):
`value` needs some way to self-classify as a URL, a file path, a wiki
name, or a same-document reference, and the explicit `key:value` forms
(`@(file:x)`, `@file(file:x)`, etc.) are the disambiguated form underlying
that shorthand.

## 3. Rejected direction: unify explicit forms under a generic `target:` key

An earlier idea in this discussion was to keep `file:`/`url:`/`wiki:`/`ref:`
for the *inferred*, unnamed `@(key:value)` shorthand (where the key must
carry the kind, since there's no element name to do it), but have every
*explicitly named* form (`<file>`, `<embed>`, `@file`, `@url`, `@wiki`,
`@ref`) use one uniform `target:` key instead, since the element's own
name already establishes the kind.

Rejected because:

- It doesn't compose with the terse inline form at all -- taken to its
  conclusion it produces nested absurdities like
  `@[display](target:@url(https://...))`.
- It was partly based on a misunderstanding: `@file`/`@url`/`<file>` etc.
  already exist as recognized element kinds (`ElementKind::File`/`Url`/...
  in `crates/typedmark-semantics/src/kind.rs`'s `BUILTIN_KINDS`) -- the
  real problem was never a lack of named forms, it was `<embed>`'s
  fallback-chain confusion specifically (`src`/`path` never being
  official keys in the first place).
- `<embed>` is not actually the same kind of problem as `@link`: `<embed>`
  already has an unambiguous name (it's definitely "an embed"), but that
  name alone doesn't say *what kind of target* it's embedding (file vs.
  url vs. wiki) -- collapsing its key to `target:` doesn't add
  information back, it just deletes the information the key used to carry.

## 4. Accepted direction: target values are URIs with mandatory schemes

Every link/reference target carries an explicit scheme. **No scheme-less
value is valid** -- a value without a recognized scheme is a plain error,
not something to guess at. This removes the entire "which key, in what
fallback order" class of bug (`<embed>`'s `src`/`path` mess), and lets
resolution be specified once, generically, per scheme, rather than
per-consumer.

**Verified against the actual grammar (`crates/typedmark-syntax-parser/src/value.rs`'s
`parse_map_body_or_scalar`) -- this splits into two structurally different
groups, not one uniform "string with embedded scheme" mechanism:**

**Group A -- identifier-like schemes (`file`, `tm`, `id`, `ref`) already
work exactly like this, no grammar change needed.** The parser's
map-vs-scalar rule is: read a leading identifier, and if a `:` follows,
it's *unconditionally* a map key (`parse_map_body_or_scalar`: `eat_ident`
then check for `:`). Confirmed by direct testing: `@(tm:foo/bar)` parses
as `Map([("tm", String("foo/bar"))])`, never as one string `"tm:foo/bar"`
-- and there is no way to make it parse as one string without quoting it.
So for this group, "value carries the scheme" and "today's existing
`key:value` map mechanism" are **the same mechanism**, just described two
ways. Nothing needs to change structurally; the only real change is
*vocabulary*: add `tm` and `id` to `INFERRED_AT_KEYS`
(`crates/typedmark-semantics/src/infer.rs`) and `BUILTIN_KINDS`
(`crates/typedmark-semantics/src/kind.rs`), and repoint `wiki`'s old
role onto the name `ref` (freed up once `id:` takes over `ref:`'s old
same-document-only meaning). `path:` as a standalone key is retired --
its old two meanings are absorbed into Group B below.

- `file:...` -- path relative to the *project root*. Targets any
  file/attachment (image, PDF, etc.), not specifically a TypedMark document.
- `tm:...` -- reference to another TypedMark (`.tm`/`.tmt`) *document*
  specifically, project-root-relative like `file:` but semantically
  distinct: targets a document, not an arbitrary attachment. Composable
  with a `#fragment` (`tm:path/to/doc#some-id`) to jump to a specific id
  inside that document -- **verified this needs zero grammar work**:
  `@(tm:foo/bar#some-id)` already parses today as
  `Map([("tm", String("foo/bar#some-id"))])`, `#` included, with no
  special-casing anywhere. Splitting `"foo/bar#some-id"` into path + id
  is pure string processing in the resolver, not a parser concern.
- `id:...` -- id-based reference. Takes over `ref:`'s old role
  (previously a same-document-only `@links{}` id lookup, see
  `crates/typedmark-doc-resolver/src/interp.rs`'s doc comment:
  "Same-document only for v1 -- no cross-file lookup yet") and is
  intended to generalize it to a project-wide/cross-file id lookup.
- `ref:...` (renamed from `wiki:`) -- resolved by searching the project
  for a file whose name/title matches the given string -- the old
  "wikilink" resolution.

**Group B -- symbol-prefixed forms (`/...`, `https://...`) do NOT work
today and need real grammar changes**, verified by direct parse testing:

- `./...`, `../...` (relative to the referencing document) already parse
  fine as bare scalar strings today (`@(./readme.md)` -> `String("./readme.md")`)
  -- no change needed here either.
- A bare `/readme.md` **fails to parse at all** ("expected a value") --
  the scalar-value grammar has no rule for a value starting with `/`.
- A bare `https://example.com` (no `url:` key wrapper) **also fails to
  parse**, for a compounding reason: `https` is read as an identifier,
  `:` follows so it's treated as a map-key introducer (per Group A's
  rule), and then the value after the colon (`//example.com`) fails for
  the same "can't start a scalar with `/`" reason as the previous case.
  Fixing only the leading-`/` scalar rule would make a *standalone*
  `/readme.md` parseable, but `https://example.com` would still become
  `Map([("https", String("//example.com"))])`, not one string -- getting
  a bare external URL to parse as a single value needs an additional
  disambiguation rule (e.g. "if `://` follows the identifier, treat the
  whole thing as one scalar, not a map-key introducer").

So the practical scope of "grammar work" for this whole redesign is much
narrower than it first appeared: only the `/`-absolute and bare-external-URL
cases need real parser changes. Everything else (`file`/`tm`/`id`/`ref`,
`./`/`../`) already works today or needs only a semantics-layer vocabulary
change.

## 5. Open items

- **Backward compatibility is a non-issue for Group A, confirmed moot.**
  Since `(file:docs/x)` already parses as `Map([("file", "docs/x")])` and
  always will (there's no alternative "unified string" form competing
  with it for identifier-like schemes, per section 4), no existing `.tm`
  content needs migrating for `file`/`ref`/`tm`/`id`. This resolves what
  was previously an open question.
- **Grammar changes for Group B are implemented** (a later session):
  `typedmark-parser`'s `value.rs::parse_map_body_or_scalar` now (1) treats
  a leading `/` as an unambiguous bare scalar (`starts_absolute_path`,
  skipping the map-key check entirely -- `/` was never a valid identifier
  character to begin with), and (2) disambiguates `identifier://...` from
  a real `key:value` map entry by peeking for a literal `//` immediately
  after the colon (`is_scheme_uri_colon`) before deciding the identifier
  is a map key. Confirmed safe against (2)'s "changes how *any* `word:`
  sequence is disambiguated" caveat: an unquoted value can never
  legitimately start with `//` anyway (`skip_ws_newlines_and_comments`
  always reads that as a comment), so no existing valid `key: value`
  content can be affected by the new check. `classify()`'s corresponding
  shape-based inference (see below) and end-to-end HTML rendering are
  covered by tests in `typedmark-parser`, `typedmark-semantics`,
  `typedmark-html`, and `typedmark-links`.
- **Cross-file `id:` implies project-wide id-uniqueness is now a real
  correctness question.** `typedmark-validator`'s duplicate-id check
  (`crates/typedmark-semantics-validator/src/id.rs`) is scoped to a
  single `Document` only -- there is no cross-document duplicate-id
  detection today. If the same id exists in two files, `id:x`'s
  resolution is ambiguous with no decided policy (error? first match?).
  Validator's scope would need to grow project-wide alongside `id:`
  becoming project-wide.
- **The relationship between bare `id:x` and `tm:path#x` isn't fully
  specified.** Presumably `id:x` means "search the whole project for this
  id" (no known target file) while `tm:path#x` means "go to this specific
  file, then this id within it" (target file already known, and a
  stronger check: the id must specifically be *inside that file*) --
  these seem complementary but this wasn't explicitly confirmed.
- **`classify()`'s bare-`@(...)` inference is implemented for both
  groups.** Group A: `infer_at_kind` (`crates/typedmark-semantics/src/infer.rs`)
  checks for a recognized key in a `Value::Map`, with `tm`/`id` in
  `INFERRED_AT_KEYS`. Group B: a new `infer_at_kind_from_scalar` handles
  the bare-`Value::String` shapes that never go through the map-key path
  -- `/`-leading -> `"file"`, `scheme://`-leading -> `"url"` (its own
  `is_scheme_uri` scheme-syntax check, independent of the parser's, since
  `typedmark-semantics` doesn't depend on `typedmark_parser`). `classify()`
  tries `infer_at_kind` first, falling back to `infer_at_kind_from_scalar`
  only when that returns `None` -- wired into both the `Sigil::At(None)`
  arm and `classify_named_at`'s inference fallback (so `@link(/foo)`
  benefits the same way `@link(url:...)` already did).

## 6. Relationship to `typedmark-links` (implemented this session)

The `typedmark-links` crate (`crates/typedmark-doc-links`) -- an
SQLite-cached, mtime-invalidated broken-link checker -- was built and
shipped in this session *before* this design discussion happened. It
implements extraction/resolution against the **old** key-based vocabulary
(`file:`/`url:`/`wiki:`/`ref:` as distinct argument keys, using ad-hoc
prefix-based resolution for `./`/`/`/bare-path cases in
`crates/typedmark-doc-links/src/check.rs`). Given section 4's finding
that Group A (`file`/`tm`/`id`/`ref`) stays key-based, updating that
crate is smaller than originally expected: `link_target`'s per-kind key
list gains `tm`/`id`, `wiki` support gets repointed onto `ref`'s name, and
`check.rs` gains a Group-B branch (sniffing a bare `/`-or-`scheme://`
string) once the corresponding grammar work lands. It isn't a full
rewrite from key-switching to scheme-string-parsing, as originally assumed.

The underlying architecture -- a flat, per-file, mtime-keyed SQLite cache
of extracted links (`LinkCache` in `crates/typedmark-doc-links/src/cache.rs`)
-- remains valid and reusable regardless of this vocabulary change; only
the extraction/resolution logic layered on top of it changes. The same
cache shape would also be the natural place to add the project-wide id
index that cross-file `id:` resolution (and the id-uniqueness validation
noted above) would need: a `(id, defining_file)` table, refreshed the same
mtime-keyed way links already are.
