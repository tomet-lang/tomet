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
const MIN_LOWERED: usize = 193;

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
    "@parent\n| ```rust\n| fn main() {}\n| ```\n| after\n",
    "@a\n| @b\n| | ```\n| | x\n| |  y\n| | ```\n",
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

// --- whole documents, spans included ------------------------------------------

use serde_json::Value as Json;

/// The first place two trees differ, as a path: `blocks[2].content[0].span.end`.
fn first_difference(a: &Json, b: &Json, path: String) -> Option<String> {
    match (a, b) {
        (Json::Object(x), Json::Object(y)) => {
            for (key, value) in x {
                match y.get(key) {
                    Some(other) => {
                        if let Some(d) = first_difference(value, other, format!("{path}.{key}")) {
                            return Some(d);
                        }
                    }
                    None => return Some(format!("{path}.{key} is only in the lowered tree")),
                }
            }
            y.keys()
                .find(|k| !x.contains_key(*k))
                .map(|k| format!("{path}.{k} is only in the AST parser's tree"))
        }
        (Json::Array(x), Json::Array(y)) => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                if let Some(d) = first_difference(p, q, format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
            (x.len() != y.len()).then(|| format!("{path} has {} items, the AST parser's {}", x.len(), y.len()))
        }
        _ => (a != b).then(|| format!("{path}: {a} against the AST parser's {b}")),
    }
}

/// Every `.tmt` under `<repo>/tmtroot` that is not generated, deprecated or built.
fn docs() -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !["dist", "generated", "deprecated"].contains(&name.as_str()) {
                    walk(&path, out);
                }
            } else if name.ends_with(".tmt") {
                out.push(path);
            }
        }
    }
    let root = tomet_tests::package_dir().join("..");
    let mut files = Vec::new();
    walk(&root.join("tmtroot"), &mut files);
    files.sort();
    files
        .into_iter()
        .filter_map(|p| {
            let label = p.strip_prefix(&root).ok()?.display().to_string();
            Some((label, std::fs::read_to_string(&p).ok()?))
        })
        .collect()
}

/// Lowering a whole document gives the AST parser's tree, positions and all.
fn check_whole(label: &str, src: &str, unsupported: &mut Vec<String>, wrong: &mut Vec<String>) -> bool {
    let Ok(parsed) = parse_document(src) else {
        return false;
    };
    match tomet_parser::lower_document(src, &parse_cst(src)) {
        Err(e) if e.message.starts_with(UNSUPPORTED) => {
            unsupported.push(format!("{label}: {}", e.message));
            false
        }
        Err(e) => {
            wrong.push(format!("{label}: {e}"));
            false
        }
        Ok(lowered) => {
            let (a, b) = (
                serde_json::to_value(&lowered).unwrap(),
                serde_json::to_value(&parsed).unwrap(),
            );
            if let Some(d) = first_difference(&a, &b, "document".into()) {
                wrong.push(format!("{label}: {d}"));
            }
            true
        }
    }
}

#[test]
fn every_fixture_lowers_as_a_whole_with_the_same_spans() {
    let (mut unsupported, mut wrong) = (Vec::new(), Vec::new());
    for rel in corpus() {
        check_whole(&rel.display().to_string(), &read_fixture(&rel), &mut unsupported, &mut wrong);
    }
    assert!(unsupported.is_empty(), "not lowered yet:\n  {}", unsupported.join("\n  "));
    assert!(wrong.is_empty(), "lowering differs:\n  {}", wrong.join("\n  "));
}

#[test]
fn the_documents_of_the_repository_lower_as_the_ast_parser_reads_them() {
    let (mut unsupported, mut wrong) = (Vec::new(), Vec::new());
    let (mut lowered, mut total) = (0usize, 0usize);
    for (label, src) in docs() {
        total += 1;
        if check_whole(&label, &src, &mut unsupported, &mut wrong) {
            lowered += 1;
        }
    }
    eprintln!("lowered {lowered} of {total} documents under tmtroot");
    assert!(wrong.is_empty(), "lowering differs:\n  {}", wrong.join("\n  "));
    assert!(
        unsupported.is_empty(),
        "not lowered yet:\n  {}",
        unsupported.join("\n  ")
    );
}
