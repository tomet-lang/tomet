//! What a long line costs. The groups a bracket-less body ends in used to be found by trying the
//! grammar at each `:` and `{` in turn, each trial reading the rest of the line: a line of `a: a:
//! a: ...` took a time proportional to the square of its length, and list items nested in
//! each other doubled it again per level. Each case below is long enough to take minutes that way.

use super::*;
use std::time::{Duration, Instant};
use tomet_ast::Id;

/// The limit a case gets. A reading proportional to the input takes well under a second here, even
/// in a debug build; the quadratic one took many times this.
const LIMIT: Duration = Duration::from_secs(30);

fn within_the_limit(name: &str, src: &str) {
    let started = Instant::now();
    let _ = parse_file(src);
    let took = started.elapsed();
    assert!(took < LIMIT, "{name}: {took:?} for {} bytes", src.len());
}

#[test]
fn a_long_line_is_read_in_time_proportional_to_it() {
    let n = 30_000;
    let cases = [
        ("colons", format!("- {}\n", "a: ".repeat(n))),
        ("braces in prose", format!("- {}\n", "a {x} ".repeat(n))),
        ("ids in prose", format!("- {}\n", "a #(x) b ".repeat(n))),
        (
            "a chain of connects",
            format!("- a {}\n", ":a(1)".repeat(n)),
        ),
        (
            "a chain of bad connects",
            format!("- a {}\n", ":a({x})".repeat(n)),
        ),
        (
            "colons before braces",
            format!("- a {} z\n", ": {x}".repeat(n)),
        ),
        ("a heading", format!("= {}\n", "a: ".repeat(n))),
    ];
    for (name, src) in &cases {
        within_the_limit(name, src);
    }
}

#[test]
fn list_items_in_the_groups_of_list_items_cost_no_more_per_level() {
    for depth in [24, 48, 64] {
        let one_line = format!("- t {}x{}\n", "{k: [- t ".repeat(depth), "]}".repeat(depth));
        within_the_limit(&format!("{depth} on one line"), &one_line);
        let nested = format!("{}x{}\n", "- t {k: [".repeat(depth), "]}".repeat(depth));
        within_the_limit(&format!("{depth} over lines"), &nested);
    }
}

fn only_item(src: &str) -> tomet_ast::ListItem {
    let doc = parse_file(src).unwrap();
    match doc.body.into_iter().next() {
        Some(Block::List(mut list)) => list.items.remove(0),
        other => panic!("expected a list, got {other:?}"),
    }
}

#[test]
fn a_group_that_is_not_valid_stays_in_the_text() {
    // `{x}` is not a value group (it holds `key: value`), `(the manual)` is not args, and a `:`
    // with no group after it is a word with a colon.
    for src in ["- text {x}\n", "- note: see (the manual)\n", "- a: b\n"] {
        let item = only_item(src);
        assert_eq!(item.id, None, "{src:?}");
        assert_eq!(item.data.value, None, "{src:?}");
        assert_eq!(item.data.args, None, "{src:?}");
        assert!(item.connects.is_empty(), "{src:?}");
    }
}

#[test]
fn the_tail_starts_after_the_last_group_that_is_not_valid() {
    // Reading `{x} #(id)` as groups fails on the `{x}`; the `#(id)` after it is still the item's.
    let item = only_item("- a {x} #(id)\n");
    assert_eq!(item.id, Some(Id("id".into())));
    assert_eq!(item.data.value, None);
    assert_eq!(
        item.content,
        wrap(vec![Inline::Text("a {x}".into())]).unwrap()
    );
}

#[test]
fn a_line_of_quotes_parses_in_proportional_time() {
    let n = 100_000;
    within_the_limit("100k double quotes", &format!("- {}\n", "\"".repeat(n)));
    let mut mixed = String::with_capacity(n);
    for i in 0..n {
        mixed.push(if i % 2 == 0 { '"' } else { '\'' });
    }
    within_the_limit("100k mixed quotes", &format!("- {}\n", mixed));
}

#[test]
fn comment_shapes_parse_in_proportional_time() {
    let n = 40_000;
    within_the_limit("40k a/*x*/ on one line", &format!("{}\n", "a/*x*/ ".repeat(n)));
    within_the_limit("40k a //x on one line", &format!("{}\n", "a //x ".repeat(n)));
    within_the_limit("40k lines of a /* open", &"a /* open\n".repeat(n));
    within_the_limit("40k /* openers on one line", &format!("{}\n", "/*".repeat(n)));
    let mut s5 = String::with_capacity(n * 5);
    for i in 0..n {
        match i % 3 {
            0 => s5.push('"'),
            1 => s5.push_str("/*"),
            _ => s5.push_str("*/"),
        }
    }
    s5.push('\n');
    within_the_limit("40k quotes mixed with block comments", &s5);
}
