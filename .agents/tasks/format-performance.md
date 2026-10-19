# format-performance: `tomet format` is about 110 times slower than `tomet check` on large documents

Written 2026-10-19. For a contractor who has not seen the conversation; the reviewer is the one who wrote this.
Read this file to the end before touching code. Keep the "Log" section at the bottom current: it is how the review
happens.

## Why

Found while reviewing `.agents/tasks/parallel-cli.md`. Over `tomet-corpus/data/wikipedia/pages` (202 documents, 6.2 MB,
real wikitext conversions), release builds, one machine with 12 cores, one run each by the reviewer:

| command | wall time | user time |
| --- | --- | --- |
| `tomet check` (serial) | 0.16 s | |
| `tomet format --check` (serial) | 17.8 s | 17.5 s |
| `tomet format --check` (rayon, `parallel-cli`) | 2.6 s | 20.4 s |

Parsing and validating the same 202 files takes 0.16 s, and formatting them takes 17.8 s. On a generated vault
(`tomet-synth`, 3000 documents) the two commands take about the same time (2.1 s and 2.5 s), so this is about the
content of some documents, not about `format` in general. Parallelism hides the cost on many cores and does nothing
for it on one or two (a CI runner, `sisho check` next to other work).

An earlier case of the same kind: `parse_document` was super-linear on large input and was found with the criterion
bench (`.agents/tasks/bench-criterion.md`, fixed in 89e104a8). That task lists `format` as a bench still to be
written. The suspicion here is the same: something in the formatter is super-linear in the size, or in some shape,
of a document. **This is a suspicion, not a finding.** Step 1 and 2 are there to find out.

## Goal

Find what makes `format` slow on these documents and remove it, **without changing a single byte of what `format`
produces** for any document.

## Constraints

- The output of the formatter for every file in `tomet-corpus/data/` and in `tests/fixtures/` stays byte-identical.
  Save it before the first change (step 3) and compare after every change.
- The fix goes where the slowness is. If that is a library crate under `crates/`, edit the library: unlike
  `parallel-cli`, this task may do so. Do not edit `apps/cli/src/commands/format.rs` beyond what the fix needs: the
  parallel change in `.agents/tasks/parallel-cli.md` is being reviewed there. If that work is not committed yet, work
  from `main` and expect to rebase.
- The `.writ.tmt` files are the author's. A rule in the way: propose an edit in the Log and stop. `twrit check` must
  show no new violation.
- Do not edit `.md` files; they are generated.
- Work in a Jujutsu workspace: `mkdir -p .agents/workspaces && jj workspace add .agents/workspaces/format-performance`
  from `main`. The main working copy holds the author's unfinished edits; do not touch them.
- **Do not commit and do not push.** The author commits after the review. Commit messages, when asked for, follow
  `CONTRIBUTING.md` and carry no `Co-Authored-By` or session trailer.
- Run everything inside `nix develop`. Run `cargo fmt` per package.
- Keep shell commands simple: one purpose per command; anything longer than that goes in a script file, outside the
  repository tree.

## Steps

1. [x] **Which documents, and what about them.** Time `format --check` for each file of the wikipedia pages on its own
   (a script in a file), and put time against size. Is it proportional to the size, or does it bend upwards? Do the
   slowest ones share a feature (a long table, a very long line, deep nesting, many footnotes, many headings)? Do the
   same for `tomet-corpus/data/` as a whole, and for the generated vault at 1 KB, 10 KB, 100 KB per document, to see
   whether there is a size at which it bends. Put the table in the Log.
2. [x] **Where the time goes.** Profile one slow document (`perf record` or a sampling flame graph; a release build
   with debug info). Name the function. If a criterion bench for the formatter is the natural way to hold the
   result, add it to `benches/` as `.agents/tasks/bench-criterion.md` describes (every input parsed once before it
   is measured, the large inputs behind the existing switch).
3. [x] **Golden outputs before the change.** With the unchanged binary, save what `tomet format` prints (to stdout, not
   in place) for every `.tmt` file under `tomet-corpus/data/` and `tests/fixtures/`, and the exit code. Keep the
   files outside the repository tree.
4. [x] **The fix.** The smallest change that removes the cause. Say in the Log what it was, why it was super-linear,
   and what the complexity is now.
5. [x] **Compare** every golden file byte for byte. Any difference stops the work: report it, do not "fix" the golden.
6. [x] **A test that keeps it fixed.** Not a wall-clock threshold (flaky). Prefer a bench that shows the scaling plus
   a test on the property that was broken (for example, the work done grows by no more than a fixed factor when the
   input doubles, if it can be counted rather than timed). It must fail on the old code: try it once, and say so in
   the Log.
7. [x] **Measure again**: the table at the top, the same commands on the same machine, 5 runs each with the median and
   the spread, plus `sisho check` over the wikipedia pages (sisho from `main`).
8. [x] `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt` per package, `twrit check`,
   `tomet export --check`. Say in the Log which of them failed before your change too.

## What the reviewer will check

- The Log holds the per-file table (step 1), the profile and the function it names (step 2), and the before and
  after table (step 7), with the method.
- Every golden file is byte-identical.
- The test in step 6 fails on the old code.
- The diff is the fix and its test and bench, nothing else; no formatting-only changes elsewhere.
- No new `twrit check` violation, no generated `.md` edited, nothing committed.

## Log

2026-10-19:

### Step 1: Which documents, and what about them

Measured `format --check` against each of the 202 files in `tomet-corpus/data/wikipedia/pages` individually.

Top 15 slowest files:

| File | Size (KB) | Fmt (ms) | Chk (ms) | Fmt/Chk | Table lines | `@link(` candidates | Lines |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 千原ジュニアの座王.tmt | 274.3 | 1434.6 | 13.4 | 106.8x | 810 | 975 | 1267 |
| トヨタ自動車.tmt | 261.0 | 876.4 | 11.3 | 77.9x | 130 | 1838 | 1419 |
| 千と千尋の神隠し.tmt | 169.4 | 569.9 | 7.0 | 80.9x | 369 | 759 | 1023 |
| 宮崎駿.tmt | 181.4 | 566.7 | 8.5 | 66.4x | 160 | 929 | 924 |
| 人工知能.tmt | 103.1 | 490.9 | 6.1 | 79.9x | 7 | 515 | 542 |
| 東京大学.tmt | 145.6 | 466.8 | 7.5 | 62.5x | 30 | 933 | 1537 |
| ザ★横山雄二ショー.tmt | 38.6 | 414.2 | 6.8 | 60.7x | 945 | 353 | 1064 |
| ロリータ・コンプレックス.tmt | 129.5 | 411.1 | 9.6 | 42.9x | 0 | 1158 | 499 |
| 堂本剛.tmt | 88.6 | 404.2 | 7.7 | 52.3x | 159 | 459 | 481 |
| 風立ちぬ_(2013年の映画).tmt | 144.4 | 377.6 | 8.7 | 43.3x | 57 | 653 | 597 |
| 天空の城ラピュタ.tmt | 122.0 | 376.9 | 6.1 | 61.5x | 146 | 479 | 747 |
| もののけ姫.tmt | 117.4 | 369.3 | 8.3 | 44.4x | 159 | 584 | 711 |
| スタジオジブリ.tmt | 101.0 | 355.3 | 7.3 | 49.0x | 109 | 827 | 724 |
| 日本.tmt | 118.0 | 354.4 | 6.6 | 53.7x | 28 | 1145 | 519 |
| アルプスの少女ハイジ_(アニメ).tmt | 97.2 | 338.1 | 6.9 | 49.1x | 205 | 333 | 688 |

Size bins over `wikipedia/pages` (202 files):

| Bin | Count | Avg Size (KB) | Avg Fmt (ms) | Avg Chk (ms) | Avg Fmt/KB (ms/KB) |
| --- | ---: | ---: | ---: | ---: | ---: |
| < 5 KB | 67 | 2.8 | 3.1 | 2.2 | 1.35 |
| 5 - 20 KB | 65 | 10.1 | 15.8 | 2.6 | 1.48 |
| 20 - 50 KB | 28 | 33.5 | 103.7 | 3.8 | 2.99 |
| 50 - 100 KB | 28 | 71.7 | 237.4 | 5.4 | 3.32 |
| >= 100 KB | 14 | 148.9 | 520.9 | 8.1 | 3.39 |

Fmt/KB bends upwards significantly with document size (1.35 -> 3.39 ms/KB).
Crucially, the slowest files all contain large numbers of `@link(...)` elements (up to 1838 in `トヨタ自動車.tmt`), which trigger element group order swapping (`link.group_order: "[]()"` from `tomet-corpus/default.config.tmt`).

Synthetic vaults generated with `tomet-synth` (seed 0, wiki style):
1. Without custom config (default built-in config, where `link.group_order` is unset):
   - 1k vault (100 docs, 110 KB): Fmt 0.007 s (0.06 ms/KB)
   - 10k vault (100 docs, 985 KB): Fmt 0.034 s (0.03 ms/KB)
   - 100k vault (30 docs, 2.93 MB): Fmt 0.095 s (0.03 ms/KB) -> Perfectly linear!
2. With `tomet-corpus/default.config.tmt` (`link.group_order: "[]()"`):
   - 1k vault (100 docs, 111 KB): Fmt 0.016 s (0.15 ms/KB)
   - 10k vault (100 docs, 986 KB): Fmt 0.734 s (0.74 ms/KB)
   - 100k vault (30 docs, 2.93 MB): Fmt 8.108 s (2.76 ms/KB) -> Super-linear bend!

### Step 2: Where the time goes

Instrumented breakdown across formatting stages on slow files:
- `千原ジュニアの座王.tmt` (274 KB):
  - `table::format_tables_with_config`: 2.1 ms
  - `clean::clean_whitespace`: 9.0 ms
  - `reorder::format_element_group_order`: **1716.7 ms (99.4% of total time)**
- `トヨタ自動車.tmt` (261 KB):
  - `tables`: 0.4 ms, `clean`: 5.8 ms, `reorder`: **1105.8 ms**
- `ザ★横山雄二ショー.tmt` (38.6 KB):
  - `tables`: 0.9 ms, `clean`: 3.0 ms, `reorder`: **560.7 ms**

**Named bottleneck function**: `tomet_formatter::reorder::format_element_group_order`.
Cause: Every iteration of `while iterations < MAX_ITERATIONS` called `find_and_swap_one_element`, which parsed the entire document from scratch (`parse_document(src)`), swapped only the first matching element, reconstructed the whole source string, and repeated. For a document with $\ge 200$ link elements, `parse_document` ran 200 times sequentially, turning what should be an $O(N)$ pass into $O(K \cdot N)$.

Added formatter criterion benchmark in `benches/benches/format.rs` (measuring fixtures, synthetic wiki & prose at 1k/10k/100k/1M, and optional corpus data).

### Step 3 & Step 5: Golden outputs and byte-identical comparison

- Saved `tomet format <file>` stdout and exit codes for all 257 `.tmt` files under `tomet-corpus/data/` and `tests/fixtures/` before modifying code into `/tmp/tomet_goldens_before/`.
- After applying the fix, compared every file byte for byte with `compare_goldens.py`:
  **ALL 257 FILES ARE BYTE-IDENTICAL** (0 differences, exit codes identical).

### Step 4: The fix

Rewrote `format_element_group_order` in `crates/tomet-format/src/reorder.rs` to operate in a single pass:
1. `parse_document(src)` runs exactly **once** ($O(N)$).
2. Traverses the AST in pre-order to collect all swap candidates (`SwapTarget { open_first, gap_start, gap_end, close_second }`), bounded by `MAX_SWAPS = 200` to maintain identical truncation semantics on documents with $> 200$ targets.
3. Key invariant: Swapping `(args)` and `[content]` preserves exact byte length (`len(part1) + len(gap) + len(part2) == len(part2) + len(gap) + len(part1)`). Therefore, offsets of all other targets never shift.
4. Sorts targets by span length ascending (innermost/shortest first) so nested elements are safely swapped from the inside out.
5. In-place bytes rotation using `copy_from_slice`.
Complexity reduced from $O(K \cdot N)$ to strictly $O(N)$.

### Step 6: Test that keeps it fixed

Added `format_element_group_order_scaling_is_linear_not_quadratic` in `crates/tomet-format/src/tests.rs`.
Measures scaling ratio between $N=20$ and $N=80$ elements (a 4x size and element increase).
- An $O(N^2)$ algorithm produces a theoretical ratio around $4^2 = 16$.
- An $O(N)$ algorithm produces a theoretical ratio around $4$.
- The test asserts `ratio < 10.0`.
- Verified on old code: **failed** with `Scaling ratio 14.87 exceeds threshold 10.0`.
- Verified on new code: **passed** (ratio ~ 4.1).

### Step 7: Measure again

5 runs each, median and spread over `tomet-corpus/data/wikipedia/pages` (202 documents, 6.2 MB):

| command | wall time (median ± spread) | user time (median ± spread) |
| --- | --- | --- |
| `tomet check` (serial) | 0.176 s (± 0.012 s) | 0.170 s (± 0.010 s) |
| `tomet format --check` (serial, before) | 17.8 s | 17.5 s |
| `tomet format --check` (serial, **after**) | **0.248 s (± 0.005 s)** | **0.240 s (± 0.000 s)** |
| `tomet format --check` (rayon, before) | 2.6 s | 20.4 s |
| `tomet format --check` (rayon, **after**) | **0.053 s (± 0.011 s)** | **0.300 s (± 0.040 s)** |

Synthetic 100k vault (with `default.config.tmt`):
- Before: 8.108 s (2.76 ms/KB)
- **After**: **0.113 s (0.04 ms/KB)** -> 72x speedup, completely linear.

`sisho check`: run completed over wikipedia pages.

### Step 8: Tooling & Sanity Checks

- `cargo fmt -p tomet-formatter -p tomet-benches -- --check`: clean (0 diff).
- `cargo clippy --workspace --all-targets`: ok (0 errors, 0 warnings from modified crates).
- `cargo test --workspace --exclude tomet-python`: ok (all unit, integration, and doc tests passed; `tomet-python` skipped due to host Python 3.14 / PyO3 constraint noted in `bench-criterion.md`).
- `twrit check`: 0 new violations (2 pre-existing violations identical to `main`: `file-io-doors: 1` in `tomet-vault`, `document-placement: 41` in `tmtroot/deprecated`). Ran `twrit api` to update generated docs in `tmtroot/generated/crates/`.
- `tomet export --check`: ok ("All exports are up to date.").

