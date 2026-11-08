use crate::tail::TailScan;

#[test]
fn tail_scan_finds_candidates_and_chains() {
    let src = "- text ::name(arg) {key: val}\n";
    let mut scan = TailScan::new(src, 2);
    let cand = scan.next_candidate(2);
    assert_eq!(cand, Some(7)); // at ':'
    let end = scan.chain(7);
    assert_eq!(end, Some(src.len() - 1));
}

#[test]
fn tail_scan_ignores_non_tail_prose() {
    let src = "- plain text with nothing special\n";
    let scan = TailScan::new(src, 2);
    assert_eq!(scan.next_candidate(2), None);
}
