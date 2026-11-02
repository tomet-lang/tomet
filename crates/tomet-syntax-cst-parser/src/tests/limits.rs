//! How deep constructs may nest. Past MAX_DEPTH, the parser keeps the text flat
//! in a `TOO_DEEP` node.
//! Each case runs on a 2 MB stack, the smallest a test thread gets: the limit is chosen so that the
//! whole CST path fits there in a debug build.

use crate::cst::MAX_DEPTH;
use tomet_cst::SyntaxKind as K;

fn shapes() -> Vec<(&'static str, usize, fn(usize) -> String)> {
    vec![
        ("[content]", 100000, |n| {
            format!("{}x{}\n", "@a[ ".repeat(n), " ]".repeat(n))
        }),
        ("(args)", 100000, |n| {
            format!("{}1{}\n", "@a(k: ".repeat(n), ")".repeat(n))
        }),
        ("{value}", 100000, |n| {
            format!("{}1{}\n", "@a{k: ".repeat(n), "}".repeat(n))
        }),
        ("emphasis", 100000, |n| {
            format!("{}x{}\n", "*a ".repeat(n), " a*".repeat(n))
        }),
        ("pipe runs", 2000, |n| {
            let mut s = String::from("@a\n");
            for i in 1..=n {
                s.push_str(&format!("{}| @a\n", "| ".repeat(i - 1)));
            }
            s
        }),
        ("lists", 2000, |n| {
            (0..n)
                .map(|i| format!("{}- item\n", "  ".repeat(i)))
                .collect()
        }),
        ("sections", 2000, |n| {
            (1..=n)
                .map(|i| format!("{} title\n", "=".repeat(i)))
                .collect()
        }),
    ]
}

fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn nesting_to_the_limit_is_read_losslessly() {
    for (name, _, shape) in shapes() {
        let src = shape(MAX_DEPTH);
        let same = on_small_stack(move || crate::parse_cst(&src).text().to_string() == src);
        assert!(same, "{name}: {MAX_DEPTH} levels should be lossless");
    }
}

#[test]
fn nesting_past_the_limit_has_too_deep_nodes() {
    for (name, big, shape) in shapes() {
        for n in [MAX_DEPTH + 2, big] {
            let src = shape(n);
            let cst_has_it = on_small_stack(move || {
                let root = crate::parse_cst(&src);
                root.descendants().any(|d| d.kind() == K::TOO_DEEP)
            });
            assert!(
                cst_has_it,
                "{name} x{n}: the tree should hold a TOO_DEEP node"
            );
        }
    }
}

#[test]
fn the_tree_stays_lossless_past_the_limit() {
    for (name, _, shape) in shapes() {
        let src = shape(MAX_DEPTH + 40);
        let same = on_small_stack(move || crate::parse_cst(&src).text().to_string() == src);
        assert!(same, "{name}: the tree must hold every character");
    }
}
