# Brief: one-container, stage 1 -- survey and AST proposal (NO CODE)

You are working in a jj workspace of the `tomet` repository. Your current directory is that workspace. This run is a
survey: you read code and write ONE report file. You change nothing else.

## Hard limits (a violation makes the run worthless)

- Do NOT run shell commands. You have no shell permission; an attempt ends the run. Use only your file reading and file
  searching tools, and file writing for the one report.
- Do NOT edit any existing file. No Rust, no `.tmt`, no `.md`, no `Cargo.toml`.
- Write exactly one new file: `.agents/tasks/one-container-survey.md` (inside the current directory).
- Do NOT read or touch anything outside the current directory. Other workspaces are other people's work.
- No commit, no `jj`, no `git`.
- No backward compatibility in anything you propose: no shims, no re-exports, no transition period.
- The existing implementation is never a reason for a design choice. "It is already like this" is not an argument in
  your report. The design memo is the target; where it is silent, list a question, do not invent an answer.

## Read first, in this order

1. `AGENTS.md`, `CONTRIBUTING.md`.
2. `tmtroot/dev/design/one-container.tmt` -- the decisions (Japanese). This is the target. Read all of it.
3. `tmtroot/dev/design/nexus-brackets-and-singular-declarations.tmt` -- the memo the above continues.
4. The root `.writ.tmt`, entry `crate-layering` -- how the crates depend on each other.
5. The `//!` of `crates/tomet-syntax-ast/src/lib.rs`, then its `Value`, `Group`, `Entry`, `Data`, `Element`, `Connect`,
   `Declaration`, `Bare`, `BodyView` definitions and their `impl`s, including `Serialize` / `Deserialize`.

## What the decision is, in one paragraph

The data space gets ONE container: an ordered run of entries, each entry with or without a name. A list is a container
whose entries all have no name; a map is one whose entries all have a name; a mix is legitimate. `name(…)` in the data
space is not a kind of value any more: it is the value `(name: (…))`, a container with one named entry. So `Value::Seq`,
`Value::Map` and `Value::Call` stop being three kinds. The literal constructors (`string(…)`, `int(…)`, `float(…)`)
leave the parser and become functions of the `$` expression space. A nexus's `( )` (the args of `@name(…)`,
`::name(…)`) is the same container as a value's `( )`.

## What to produce

Write `.agents/tasks/one-container-survey.md` with these sections, in this order.

### A. What exists today (facts only)

How each of these is represented in the AST today, with `path:line` for every statement:

- a value's `(a, b)`, `{k: v}`, `name(a, b)`;
- a nexus's `(args)`: positional arguments, named arguments, both together;
- a nexus's `{…}` group, and the non-pair entries it may hold (`Entry::Element`, `Entry::Bare`, `Entry::Hole`);
- where the parser resolves the literal constructors, and exactly what each one does to its argument;
- how a positional argument gets a name (where declarations with `positional` are applied, and in which crate).

### B. Inventory of every reader and writer

Every non-test Rust site (crates, apps, bindings, benches, the `tests` package's library code) that constructs or matches
`Value::Seq`, `Value::Map`, `Value::Call`, `EvalValue::Seq` / `Map` / `Call`, `Group`, `Entry::*`, or reads `Data`.
One row per site, grouped by crate, crates in layer order:

| path:line | what the code does | what it asks of the value |

The third column is the point of this section. Use these words so the rows can be counted: `by-name`, `by-position`,
`in-written-order`, `is-it-a-list`, `is-it-a-map`, `callee-name`, `walks-all-children`, `builds`. A site may have several.
Then a short count per word, and the list of sites whose behaviour DEPENDS on the three kinds being different (for
example: code that treats a `Seq` and a `Call("enum")` alike, code that reads a `Call` as a type, code that refuses a
`Call`, code that writes `{"call": …}`). For each of those, say what the memo implies it must become, or that the memo
does not say.

Do not sample. If you could not read a file, list it under "Not read" at the end. A row you are unsure of is marked
`(unsure)`.

### C. Proposal for the one container type

- The type, as a small Rust sketch (fields and the handful of methods section B shows the readers need: nth unnamed
  entry, entry by name, nth entry as written, iteration, "all unnamed", "all named", and whatever else B justifies --
  and nothing B does not justify).
- Three candidate names for it, each with what the name says to a reader of the language and what it gets wrong. Argue
  from the concept in the memo (one container; entries; name optional), not from names the code has now. Do not choose:
  the author chooses.
- How a nexus's args and a value's container become the same type. What happens to `Group`, `Entry` and `Data`. Say
  plainly where `Entry::Element`, `Entry::Bare` and `Entry::Hole` go: they are entries that are not `name: value` pairs,
  and the memo does not mention them. If they do not fit the one container, say so and list it under F instead of
  bending the proposal.
- What a "written spelling" needs: `source: upstream(a, b)` and `source: (upstream: (a, b))` are the same tree and the
  memo says the written spelling is kept, as a matter of the spelling layer and not of the tree. Report how the printer
  and the formatter decide spellings today (which crates, which inputs: CST or AST), and what that implies for where
  this spelling can live. Facts first, then the implication.

### D. Serialization

The JSON the tree serializes to today for these values and groups (quote a real reference file under `tests/ref/` with
its path), which reference files and which bindings (`bindings/*`, `packages/*`) embed that shape, and what would change.

### E. Order of work

The order in which crates can move so that `cargo test --workspace` can be green at the end of each step, following the
layers of `.writ.tmt`. For each step: the crates, what changes, what proves it (which existing tests or references must
stay byte-identical). Then the risks you see, most serious first, each with the evidence.

### F. Questions the memo does not answer

A numbered list. Each: the question, the `path:line` that raises it, and the options with one concrete example per
option. Do not answer them.

### G. What you did not do

Files not read, searches not run, anything you assumed. An honest short list is worth more than a clean one.

## How the report is judged

It will be checked against the code by someone who will re-run the searches. A missing reader in section B, a statement
without `path:line`, or a claim that everything fits when a site in B contradicts it, counts against the whole report.
Say "I do not know" where you do not.

When the file is written, end your turn with three lines: the path of the report, the number of rows in section B, and
the number of questions in section F.
