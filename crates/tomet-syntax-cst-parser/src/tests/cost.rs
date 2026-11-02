//! What a long line costs in the CST parser.

use std::time::{Duration, Instant};

const LIMIT: Duration = Duration::from_secs(30);

fn within_the_limit(name: &str, src: &str) {
    let started = Instant::now();
    let root = crate::parse_cst(src);
    assert_eq!(root.text().to_string(), src);
    let took = started.elapsed();
    assert!(took < LIMIT, "{name}: {took:?} for {} bytes", src.len());
}

#[test]
fn a_long_line_cst_is_read_in_time_proportional_to_it() {
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
