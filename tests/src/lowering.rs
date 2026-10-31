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
const MIN_LOWERED: usize = 5;

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
    "a\n\n----\n\nb\n",
    "```\ncode\n```\n",
    "```rust\nfn main() {}\n```\n",
    "```rust extra words\nx\n\ny\n```\n",
    "````\n```\n````\n",
    "```\nunterminated\n",
    "```\n```\n",
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
    for rel in corpus() {
        let src = read_fixture(&rel);
        let Ok(doc) = parse_document(&src) else {
            continue;
        };
        for (start, block) in lower_top_level_blocks(&src, &parse_cst(&src)) {
            total += 1;
            match block {
                Err(e) if e.message.starts_with(UNSUPPORTED) => {}
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
    eprintln!("lowered {lowered} of {total} top-level blocks");
    assert!(
        lowered >= MIN_LOWERED,
        "lowering produced {lowered} blocks, fewer than the {MIN_LOWERED} it did"
    );
}

fn short(block: &Block) -> String {
    format!("{block:?}").chars().take(160).collect()
}
