//! How deep constructs may nest. A document nested past `MAX_DEPTH` used to abort the process with a
//! stack overflow; now the parser keeps the text flat in a `TOO_DEEP` node and lowering reports it.
//! Each case runs on a 2 MB stack, the smallest a test thread gets: the limit is chosen so that the
//! whole path (reading, lowering, dropping) fits there in a debug build.

use super::*;
use crate::cst::MAX_DEPTH;
use tomet_cst::SyntaxKind as K;

/// Every kind of nesting the parser recurses on, as `(name, how deep to push it, opens n levels deep)`.
/// A list, a section and a pipe run take input quadratic in their depth (each level is indented, or
/// has one more marker), so those are pushed less far.
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
        ("box content", 100000, |n| {
            format!("{}x{}\n", "<?(a)>[ ".repeat(n), " ]".repeat(n))
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

/// Runs `f` on a thread with the smallest stack a test thread gets, so that a recursion that would
/// overflow does so here and not only on a large `main` stack.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn nesting_to_the_limit_is_read() {
    for (name, _, shape) in shapes() {
        let src = shape(MAX_DEPTH);
        let ok = on_small_stack(move || parse_document(&src).is_ok());
        assert!(ok, "{name}: {MAX_DEPTH} levels should read");
    }
}

#[test]
fn nesting_past_the_limit_is_an_error_and_not_a_crash() {
    for (name, big, shape) in shapes() {
        for n in [MAX_DEPTH + 2, big] {
            let src = shape(n);
            let (cst_has_it, err) = on_small_stack(move || {
                let root = crate::parse_cst(&src);
                let has = root.descendants().any(|d| d.kind() == K::TOO_DEEP);
                (has, parse_document(&src).err())
            });
            assert!(
                cst_has_it,
                "{name} x{n}: the tree should hold a TOO_DEEP node"
            );
            let err = err.unwrap_or_else(|| panic!("{name} x{n}: should be an error"));
            assert!(
                err.message.contains("nested more than"),
                "{name} x{n}: {}",
                err.message
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
