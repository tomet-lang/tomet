# Give `@blueprint` a decided shape

`@blueprint` was implemented in `bb38aff` ("!feat(blueprint) add new
template and blueprint feature") with no record of why it took the form it
did. It appears nowhere in `docs/spec/`. The author does not recognise the
`@blueprint(target: ...)` spelling that the code accepts, and the code
accepts three of them.

Decided now, from `docs/why/root.tmt`'s "identifier, input, data":

    @kind(blueprint)
    @blueprint(writ){
      version: "1.0.0"
      description: "twrit"
    }

- **identifier** `blueprint` -- what this document is
- **input** `(writ)` -- what it targets, positional, normalised to `target`
- **data** `{...}` -- what the blueprint says about itself

`@kind(blueprint)` is written, not inferred from `@blueprint`'s presence.
`@kind` is the single answer to "what is this document"; inferring it from
an element's presence makes a second path to the same question, which is
the asymmetry removed three times already this week.

`target`, not `kind`, for the positional key: `@kind(blueprint)` sits two
lines above, and `kind:` there would mean the *output's* kind while `@kind`
means this file's. `embed`/`link` already normalise their positional to
`target`, so this adds no new vocabulary.

Promotion stays `@blueprint(X)` -> `@kind(X)`: positional to positional,
the same value moving.

## Order

Spec first, then the rule, then the code. The reason this task exists is an
implementation that arrived without either.

- [x] 1. `docs/spec/builtin-elements.tmt`: add `@blueprint` to the
      vocabulary listing. It is absent entirely today.
- [x] 2. `docs/spec/builtin-settings.tmt`: add a `blueprint` entry to the
      `elements` map -- args, values, placement, singleton.
- [x] 3. A writ rule fixing the shape, with the reasoning above as its
      "why" and the rejected spellings beside it.
- [x] 4. `positional.rs:88`: `"blueprint" => &["target"]`.
- [x] 5. `instantiate_blueprint` (`tomet-transform`): read the target
      through the normalised args only. Today it accepts a positional
      string, `target:`, and `kind:`.
- [x] 6. `extract_blueprint_schema` (`tomet-semantics-validator:34`): drop
      `|| kind == ElementKind::Kind`. A document is a blueprint because it
      says `@kind(blueprint)`, not because it has any kind at all.
- [x] 7. Rewrite the files that carry the old shapes:
      `docs/twrit/blueprint.writ.tmt` (has `@blueprint{}` *and*
      `@kind(writ)`), `docs/examples/templates/{daily-note,rfc}.template.tmt`
      (`@blueprint(daily-note)` used as a target declaration -- these
      predate the blueprint decision and were never revisited), and
      `tests/fixtures/templates/template.daily-note.tmt`.
- [x] 8. `.blueprint.tmt`, author's call. Directories renamed too
      (`docs/examples/blueprints/`, `tests/fixtures/blueprints/`), and the
      lookup narrowed from three spellings to one -- `find_template_path`
      tried `<name>.tmt`, `template.<name>.tmt` and `<name>.template.tmt`
      in turn, which is how `docs/docs.settings.tmt` pointed at a filename
      that does not exist and still resolved. `list_templates` had its own
      third directory and its own name-cleaning; the two disagreed about
      which spellings existed.
- [x] 9. Fixtures for the decided shape; regenerate `tests/ref/`.
- [x] 10. `cargo test --workspace`, `just docs-check`, `twrit check`.

## Found by running it

`tomet new` emitted a document with two kinds -- `@kind(blueprint)` from
the source and `@kind(daily-note)` from the promotion. The blueprint's own
kind describes a thing that stops existing at instantiation, so it has to
be dropped, not carried. Pinned by `instantiating_leaves_one_kind`.

Neither the spec nor the writ entry had said anything about it, because
nobody had run the shape before writing it down. Building it is what
found it.

## Watch

- Resolved. `docs/exports/` became `tmtroot/` in `f6a267b`, and the writ
  blueprint went to `.tomet/blueprints/writ.blueprint.tmt`, so `docs/twrit/`
  is gone and `document-placement` has its guard. The standing proposal --
  configuration and schema at the root of what they govern, which would
  have put the blueprint at the repository root -- was argued and rejected;
  the reasoning is the root writ's `dot-tomet-is-authored`. Nothing is left
  open here, so this file can go.
