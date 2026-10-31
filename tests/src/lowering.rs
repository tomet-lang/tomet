//! Differential test for lowering: the CST read as an AST (`tomet_parser::lower_document`)
//! against the AST parser, while the first is built up to replace the second.
//!
//! Two checks. Every top-level block that lowering does produce must equal the
//! AST parser's block that starts at the same place -- `Span`'s `PartialEq` is
//! unconditionally true, so this compares shape and content, not positions. And
//! the number of blocks it produces may only go up: `MIN_LOWERED` is raised in
//! the commit that teaches lowering another construct, and a drop below it is a
//! regression.
//!
//! Nothing in the AST parser is deleted until every block of every fixture is
//! lowered and equal.

use tomet_ast::Block;
use tomet_parser::{UNSUPPORTED, lower_top_level_blocks, parse_cst, parse_document};
use tomet_tests::{corpus, read_fixture};

/// How many top-level blocks of the fixtures lowering produces today.
const MIN_LOWERED: usize = 186;

/// Small documents, each of which lowering must give exactly as `parse_document` does.
const SMALL: &[&str] = &[
    "a\n",
    "a\nb\n",
    "a  b\n",
    "  indented\n",
    "a\n\nb\n",
    "a\n;b\n",
    "one\n\n\ntwo\n",
    "日本語\nの文\n",
    "a\n\\;b\n",
    "\\;a\n",
    "a;b\nc\n",
    "---\n",
    "---[ Title ]---\n",
    "---[ A *title* ]---\n\ntext\n",
    "a\n\n----\n\nb\n",
    "```\ncode\n```\n",
    "```rust\nfn main() {}\n```\n",
    "```rust extra words\nx\n\ny\n```\n",
    "````\n```\n````\n",
    "```\nunterminated\n",
    "```\n```\n",
    "@note\n",
    "@note(type: warn)\n",
    "@note{ k: v }\n",
    "@note[ hello ]\n",
    "@note(a: 1)[ hello\nworld ]{ k: v }#(x)\n",
    "@a\n\n@b(1, 2)\n",
    "@a(x: 1)\n[ on the next line ]\n",
    "@a:b(c: 1)\n",
    "@a#(id)\n",
    "a *em* b\n",
    "a **strong** and ~~gone~~ and _em_ and __strong__\n",
    "**a *b* c**\n",
    "a `code` b\n",
    "a ``co`de`` b\n",
    "see https://a.b/c, ok\n",
    "${x} and ${a.b}\n",
    "a @link(ref: y)[text] b\n",
    "a @em[x]\nb @mark[y]\n",
    "a // comment\nb\n",
    "text /* inline */ more\n",
    "@a()\n@b()\n",
    "@a() text\n",
    "- a\n",
    "- a\n- b\n",
    "-. one\n-. two\n",
    "- a\n  - b\n  - c\n- d\n",
    "- a\n-. b\n",
    "- (x) check\n- ( ) none\n",
    "- *a* and `b`\n",
    "-@task()\n-@task()[content] text\n",
    "- [ body ] tail\n",
    "- [ body ]\n",
    "- text #(i)\n",
    "- text :{a: 1}\n",
    "- (m) text :rule(r)\n",
    "== title :rule(allow: x) {k: v}\n",
    "== title ==\n",
    "== T #(id)\n",
    "- content\n  :(x: 1){y: 2}\n",
    "-@task():(x: 1){y: 2}\n",
    "-@task()[content]:rule(allow:list(card))\n",
    "::kind(config)\n",
    "::meta{ id: x, title: T }\n\n= T\n",
    "::kind(config, 2)\n::id(doc-a)\n",
    "=\n",
    "= T\n",
    "== sub heading\n",
    "=[ T ]\n\nbody\n",
    "= a\n\ntext\n\n== b\n\nmore\n\n= c\n",
    "=[ T ]{ k: v }\n",
    "---\n=[ T ]\n---\n",
];

#[test]
fn a_small_document_lowers_to_what_the_ast_parser_reads() {
    for src in SMALL {
        let cst = parse_cst(src);
        let lowered = tomet_parser::lower_document(src, &cst)
            .unwrap_or_else(|e| panic!("{src:?} did not lower: {e}"));
        let parsed = parse_document(src).unwrap();
        assert_eq!(lowered, parsed, "{src:?}");
    }
}

#[test]
fn what_lowering_produces_for_the_fixtures_is_what_the_ast_parser_reads() {
    let (mut lowered, mut total) = (0usize, 0usize);
    let mut wrong = Vec::new();
    let mut why: std::collections::BTreeMap<String, usize> = Default::default();
    for rel in corpus() {
        let src = read_fixture(&rel);
        let Ok(doc) = parse_document(&src) else {
            continue;
        };
        for (start, block) in lower_top_level_blocks(&src, &parse_cst(&src)) {
            total += 1;
            match block {
                Err(e) if e.message.starts_with(UNSUPPORTED) => {
                    *why.entry(e.message.clone()).or_default() += 1;
                }
                Err(e) => wrong.push(format!("{}: {e}", rel.display())),
                Ok(None) => lowered += 1,
                Ok(Some(block)) => {
                    lowered += 1;
                    let there = doc.blocks.iter().find(|b| b.span().start == start);
                    if there != Some(&block) {
                        wrong.push(format!(
                            "{}: the block at byte {start} is {:?}\n    but the AST parser has {:?}",
                            rel.display(),
                            short(&block),
                            there.map(short),
                        ));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "lowering differs:\n  {}", wrong.join("\n  "));
    eprintln!("lowered {lowered} of {total} top-level blocks; what is left:");
    let mut left: Vec<_> = why.into_iter().collect();
    left.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (reason, n) in left {
        eprintln!("  {n:>3}  {reason}");
    }
    assert!(
        lowered >= MIN_LOWERED,
        "lowering produced {lowered} blocks, fewer than the {MIN_LOWERED} it did"
    );
}

fn short(block: &Block) -> String {
    format!("{block:?}").chars().take(160).collect()
}
