use std::cmp::Ordering;

use super::*;

#[test]
fn test_codepoint_matching_str_cmp() {
    let collator = Collator::codepoint();
    assert!(collator.is_codepoint());
    assert!(CollationSpec::new().compile().unwrap().is_codepoint());
    assert!(
        CollationSpec::new()
            .rest()
            .compile()
            .unwrap()
            .is_codepoint()
    );
    assert!(
        !CollationSpec::new()
            .token("a")
            .compile()
            .unwrap()
            .is_codepoint()
    );
    assert!(
        !CollationSpec::new()
            .ignore(["-"])
            .compile()
            .unwrap()
            .is_codepoint()
    );

    let test_strings = [
        "", "a", "b", "aa", "ab", "ba", "foo", "foobar", "bar", "あ", "い", "う", "か", "が",
        "東京", "京都", "😀", "😁", "🎉", "🦀", "test", "testing", "tester", "\0", "\0a", "a\0",
        "a\0b",
    ];

    for &a in &test_strings {
        for &b in &test_strings {
            let expected = a.cmp(b);
            let actual = collator.cmp(a, b);
            assert_eq!(
                actual, expected,
                "Collator::codepoint cmp mismatch for {a:?} vs {b:?}"
            );

            let key_a = collator.sort_key(a);
            let key_b = collator.sort_key(b);
            assert_eq!(
                key_a.cmp(&key_b),
                expected,
                "Collator::codepoint sort_key cmp mismatch for {a:?} vs {b:?}"
            );
        }
    }
}

#[test]
fn test_multichar_longest_match_and_fallback() {
    // 1. Spanish/Welsh digraph "ch" between "c" and "d"
    let spec = CollationSpec::new().token("c").token("ch").token("d");
    let collator = spec.compile().unwrap();

    assert_eq!(collator.cmp("ca", "cz"), Ordering::Less);
    assert_eq!(collator.cmp("cz", "ch"), Ordering::Less);
    assert_eq!(collator.cmp("ch", "da"), Ordering::Less);

    // 2. Japanese digraph "きゃ" vs "きや"
    let spec_kana = CollationSpec::new().token("き").token("きゃ").token("や");
    let collator_kana = spec_kana.compile().unwrap();

    // "き" is slot 0, "きゃ" is slot 1, "や" is slot 2.
    // "きや" matches "き" (slot 0) then "や" (slot 2).
    // "きゃ" matches "きゃ" (slot 1).
    // At token 0, "きや" has slot 0 while "きゃ" has slot 1, so "きや" < "きゃ".
    assert_eq!(collator_kana.cmp("き", "きや"), Ordering::Less);
    assert_eq!(collator_kana.cmp("きや", "きゃ"), Ordering::Less);

    // 3. Partial prefix fallback: "abc" is mapped, "a"/"ab" are unmapped.
    // For "abd", it matches 'a', 'b', then fails on 'd'.
    // It must fall back to single character 'a', then 'b', then 'd'.
    let spec_prefix = CollationSpec::new().token("abc");
    let collator_prefix = spec_prefix.compile().unwrap();

    // "abc" is slot 0, unmapped characters are slot 1.
    assert_eq!(collator_prefix.cmp("abc", "abd"), Ordering::Less);
    assert_eq!(collator_prefix.cmp("abd", "abe"), Ordering::Less);

    // Multibyte characters safely fall back character by character
    assert_eq!(collator_prefix.cmp("あ", "い"), Ordering::Less);
    assert_eq!(collator_prefix.cmp("abdあ", "abdい"), Ordering::Less);
}

#[test]
fn test_two_level_equivalence_case_groups() {
    // Case groups ["A", "a"] .. ["Z", "z"]
    let mut spec = CollationSpec::new();
    for ch in 'a'..='z' {
        let upper = ch.to_ascii_uppercase().to_string();
        let lower = ch.to_string();
        spec = spec.group([upper, lower]);
    }
    let collator = spec.compile().unwrap();

    // Verify "aa" < "Apple" < "apple" < "Az"
    assert_eq!(collator.cmp("aa", "Apple"), Ordering::Less);
    assert_eq!(collator.cmp("Apple", "apple"), Ordering::Less);
    assert_eq!(collator.cmp("apple", "Az"), Ordering::Less);

    // Verify prefix dominance: "apple" < "Apples"
    assert_eq!(collator.cmp("apple", "Apples"), Ordering::Less);

    // Verify stable sort order
    let mut words = vec!["Az", "apple", "aa", "Apples", "Apple"];
    collator.sort(&mut words);
    assert_eq!(words, vec!["aa", "Apple", "apple", "Apples", "Az"]);
}

#[test]
fn test_japanese_kana_dictionary_groups() {
    let spec = CollationSpec::new()
        .group(["か", "が"])
        .group(["き", "ぎ"])
        .group(["さ", "ざ"]);
    let collator = spec.compile().unwrap();

    // In Japanese dictionary collation:
    // - "かがみ" has primary weights [P_ka, P_ka, P_mi].
    // - "かき" has primary weights [P_ka, P_ki].
    // - "がき" has primary weights [P_ka, P_ki].
    // - "かさ" has primary weights [P_ka, P_sa].
    //
    // Since "か/が" is slot 0 and "き/ぎ" is slot 1, P_ka < P_ki.
    // At token 1, "かがみ" has P_ka while "かき" has P_ki, so "かがみ" < "かき".
    // Between "かき" and "がき", both share [P_ka, P_ki], distinguished by secondary:
    // "か" (secondary 0) < "が" (secondary 1), so "かき" < "がき".
    // Finally, "かさ" has P_sa (slot 2), so "がき" < "かさ".
    assert_eq!(collator.cmp("かがみ", "かき"), Ordering::Less);
    assert_eq!(collator.cmp("かき", "がき"), Ordering::Less);
    assert_eq!(collator.cmp("がき", "かさ"), Ordering::Less);

    let mut words = vec!["かさ", "がき", "かがみ", "かき"];
    collator.sort(&mut words);
    assert_eq!(words, vec!["かがみ", "かき", "がき", "かさ"]);

    // If a later syllable such as "こ" (group 2) is used: "かき" < "がき" < "かこ" < "かさ"
    let spec_full = CollationSpec::new()
        .group(["か", "が"])
        .group(["き", "ぎ"])
        .group(["こ", "ご"])
        .group(["さ", "ざ"]);
    let collator_full = spec_full.compile().unwrap();
    assert_eq!(collator_full.cmp("かき", "がき"), Ordering::Less);
    assert_eq!(collator_full.cmp("がき", "かこ"), Ordering::Less);
    assert_eq!(collator_full.cmp("かこ", "かさ"), Ordering::Less);
}

#[test]
fn test_step_rest_placement() {
    // 1. Rest in middle: ["_"], Rest, ["~"]
    let spec_middle = CollationSpec::new().token("_").rest().token("~");
    let collator_middle = spec_middle.compile().unwrap();

    // Unmapped characters are at slot 1, ordered among themselves by char code point:
    // 'a' < 'b', so "alpha~" < "beta_"
    assert_eq!(collator_middle.cmp("alpha~", "beta_"), Ordering::Less);

    // "_" (slot 0) < unmapped chars (slot 1) < "~" (slot 2)
    assert_eq!(collator_middle.cmp("_", "a"), Ordering::Less);
    assert_eq!(collator_middle.cmp("z", "~"), Ordering::Less);
    assert_eq!(collator_middle.cmp("_", "~"), Ordering::Less);

    // 2. Rest at beginning: Rest, ["~"]
    let spec_begin = CollationSpec::new().rest().token("~");
    let collator_begin = spec_begin.compile().unwrap();
    assert_eq!(collator_begin.cmp("a", "b"), Ordering::Less);
    assert_eq!(collator_begin.cmp("z", "~"), Ordering::Less);

    // 3. Rest omitted (implicit end): ["a"], ["b"]
    let spec_omitted = CollationSpec::new().token("a").token("b");
    let collator_omitted = spec_omitted.compile().unwrap();
    // "a" (slot 0) < "b" (slot 1) < "c" (unmapped, slot 2)
    assert_eq!(collator_omitted.cmp("a", "b"), Ordering::Less);
    assert_eq!(collator_omitted.cmp("b", "c"), Ordering::Less);
}

#[test]
fn test_ignore_tokens_and_stable_sort() {
    let spec = CollationSpec::new().token("__").ignore(["_", "-"]);
    let collator = spec.compile().unwrap();

    // Ignored tokens do not contribute weights
    assert_eq!(collator.cmp("a_b", "ab"), Ordering::Equal);
    assert_eq!(collator.cmp("a-b", "ab"), Ordering::Equal);
    assert_eq!(collator.cmp("a_b", "a-b"), Ordering::Equal);

    // Stable sort preserves original relative order of equal elements
    let mut items = vec!["a_b", "ab", "a-b"];
    collator.sort(&mut items);
    assert_eq!(items, vec!["a_b", "ab", "a-b"]);

    // Longer mapped token "__" is matched as a mapped token, not as two ignored "_"
    assert_ne!(collator.cmp("__", "_"), Ordering::Equal);
    assert_eq!(collator.cmp("__", "a"), Ordering::Less);
}

#[test]
fn test_collation_error_variants() {
    // EmptyGroup
    let spec_empty_group = CollationSpec {
        steps: vec![Step::Group(vec!["a".into()]), Step::Group(vec![])],
        ignore: vec![],
    };
    assert_eq!(
        spec_empty_group.compile(),
        Err(CollationError::EmptyGroup { step: 1 })
    );

    // EmptyToken in group
    let spec_empty_token_group = CollationSpec::new().token("");
    assert_eq!(
        spec_empty_token_group.compile(),
        Err(CollationError::EmptyToken)
    );

    // EmptyToken in ignore
    let spec_empty_token_ignore = CollationSpec::new().ignore([""]);
    assert_eq!(
        spec_empty_token_ignore.compile(),
        Err(CollationError::EmptyToken)
    );

    // DuplicateToken in same group
    let spec_dup_same_group = CollationSpec::new().group(["a", "a"]);
    assert_eq!(
        spec_dup_same_group.compile(),
        Err(CollationError::DuplicateToken("a".into()))
    );

    // DuplicateToken across groups
    let spec_dup_across_groups = CollationSpec::new().token("a").token("a");
    assert_eq!(
        spec_dup_across_groups.compile(),
        Err(CollationError::DuplicateToken("a".into()))
    );

    // DuplicateToken across group and ignore
    let spec_dup_group_ignore = CollationSpec::new().token("a").ignore(["a"]);
    assert_eq!(
        spec_dup_group_ignore.compile(),
        Err(CollationError::DuplicateToken("a".into()))
    );

    // DuplicateToken in ignore list
    let spec_dup_ignore = CollationSpec::new().ignore(["_", "_"]);
    assert_eq!(
        spec_dup_ignore.compile(),
        Err(CollationError::DuplicateToken("_".into()))
    );

    // DuplicateRest
    let spec_dup_rest = CollationSpec::new().rest().rest();
    assert_eq!(spec_dup_rest.compile(), Err(CollationError::DuplicateRest));

    // Error formatting display checks
    assert_eq!(
        format!("{}", CollationError::EmptyGroup { step: 2 }),
        "collation step 2 has an empty group"
    );
    assert_eq!(
        format!("{}", CollationError::EmptyToken),
        "collation token cannot be empty"
    );
    assert_eq!(
        format!("{}", CollationError::DuplicateToken("x".into())),
        "duplicate collation token 'x'"
    );
    assert_eq!(
        format!("{}", CollationError::DuplicateRest),
        "duplicate Rest step in collation specification"
    );
    assert_eq!(
        format!("{}", CollationError::TooManyEntries),
        "collation specification contains too many entries"
    );
}

#[test]
fn test_exhaustive_pairwise_and_transitivity_laws() {
    let test_strings = [
        "",
        "a",
        "b",
        "A",
        "B",
        "z",
        "Z",
        "0",
        "9",
        "apple",
        "Apple",
        "Apples",
        "apples",
        "aa",
        "Az",
        "az",
        "ch",
        "c",
        "d",
        "cz",
        "da",
        "_",
        "__",
        "~",
        "-",
        "a_b",
        "ab",
        "a-b",
        "か",
        "が",
        "き",
        "ぎ",
        "かき",
        "がき",
        "かがみ",
        "かさ",
        "きゃ",
        "きや",
        "😀",
        "😁",
        "\0",
        "\0a",
        "a\0",
    ];

    let collators = [
        Collator::codepoint(),
        CollationSpec::new()
            .group(["A", "a"])
            .group(["B", "b"])
            .compile()
            .unwrap(),
        CollationSpec::new()
            .token("c")
            .token("ch")
            .token("d")
            .compile()
            .unwrap(),
        CollationSpec::new()
            .token("__")
            .ignore(["_", "-"])
            .compile()
            .unwrap(),
        CollationSpec::new()
            .group(["か", "が"])
            .group(["き", "ぎ"])
            .group(["さ", "ざ"])
            .compile()
            .unwrap(),
        CollationSpec::new()
            .token("_")
            .rest()
            .token("~")
            .compile()
            .unwrap(),
    ];

    for (c_idx, collator) in collators.iter().enumerate() {
        // 1. Pairwise checks
        for &a in &test_strings {
            let key_a = collator.sort_key(a);

            // Reflexivity: cmp(a, a) == Equal
            assert_eq!(
                collator.cmp(a, a),
                Ordering::Equal,
                "Reflexivity failed for collator {c_idx} on {a:?}"
            );
            assert_eq!(
                key_a.cmp(&key_a),
                Ordering::Equal,
                "Key reflexivity failed for collator {c_idx} on {a:?}"
            );

            for &b in &test_strings {
                let cmp_ab = collator.cmp(a, b);
                let key_b = collator.sort_key(b);
                let cmp_key = key_a.cmp(&key_b);

                // collator.cmp(a, b) == sort_key(a).cmp(&sort_key(b))
                assert_eq!(
                    cmp_ab, cmp_key,
                    "Key equivalence failed for collator {c_idx} on ({a:?}, {b:?})"
                );

                // Antisymmetry: cmp(a, b) == cmp(b, a).reverse()
                let cmp_ba = collator.cmp(b, a);
                assert_eq!(
                    cmp_ab,
                    cmp_ba.reverse(),
                    "Antisymmetry failed for collator {c_idx} on ({a:?}, {b:?})"
                );
            }
        }

        // 2. Transitivity checks over a subset to keep test runtime snappy
        let subset = &test_strings[..20];
        for &a in subset {
            for &b in subset {
                let ab = collator.cmp(a, b);
                for &c in subset {
                    let bc = collator.cmp(b, c);
                    let ac = collator.cmp(a, c);

                    if ab == Ordering::Less && bc == Ordering::Less {
                        assert_eq!(
                            ac,
                            Ordering::Less,
                            "Transitivity Less failed for collator {c_idx} on ({a:?}, {b:?}, {c:?})"
                        );
                    }
                    if ab == Ordering::Equal && bc == Ordering::Equal {
                        assert_eq!(
                            ac,
                            Ordering::Equal,
                            "Transitivity Equal failed for collator {c_idx} on ({a:?}, {b:?}, {c:?})"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn test_direction_and_sort_by_key() {
    assert_eq!(Direction::from_name("asc"), Some(Direction::Asc));
    assert_eq!(Direction::from_name("desc"), Some(Direction::Desc));
    assert_eq!(Direction::from_name("other"), None);

    assert_eq!(Direction::Asc.as_str(), "asc");
    assert_eq!(Direction::Desc.as_str(), "desc");
    assert_eq!(Direction::Asc.reverse(), Direction::Desc);
    assert_eq!(Direction::Desc.reverse(), Direction::Asc);
    assert_eq!(format!("{}", Direction::Asc), "asc");
    assert_eq!(format!("{}", Direction::Desc), "desc");

    let collator = Collator::codepoint();
    assert_eq!(
        collator.cmp_with_direction("a", "b", Direction::Asc),
        Ordering::Less
    );
    assert_eq!(
        collator.cmp_with_direction("a", "b", Direction::Desc),
        Ordering::Greater
    );
    assert_eq!(
        collator.cmp_with_direction("a", "a", Direction::Desc),
        Ordering::Equal
    );

    // sort_with_direction
    let mut words = vec!["banana", "apple", "cherry"];
    collator.sort_with_direction(&mut words, Direction::Desc);
    assert_eq!(words, vec!["cherry", "banana", "apple"]);

    // Stability verification under Direction::Desc
    // Equal items differing only in ignored characters must preserve their relative order.
    let spec_ignore = CollationSpec::new().ignore(["_", "-"]);
    let collator_ignore = spec_ignore.compile().unwrap();
    let mut equal_items = vec!["a_b", "ab", "a-b"];
    collator_ignore.sort_with_direction(&mut equal_items, Direction::Desc);
    assert_eq!(equal_items, vec!["a_b", "ab", "a-b"]);

    // sort_by_key and sort_by_key_with_direction with custom struct
    struct Item {
        name: &'static str,
        _value: i32,
    }

    let mut items = [
        Item {
            name: "banana",
            _value: 1,
        },
        Item {
            name: "apple",
            _value: 2,
        },
        Item {
            name: "cherry",
            _value: 3,
        },
    ];
    collator.sort_by_key(&mut items, |item| item.name);
    assert_eq!(items[0].name, "apple");
    assert_eq!(items[1].name, "banana");
    assert_eq!(items[2].name, "cherry");

    collator.sort_by_key_with_direction(&mut items, Direction::Desc, |item| item.name);
    assert_eq!(items[0].name, "cherry");
    assert_eq!(items[1].name, "banana");
    assert_eq!(items[2].name, "apple");
}
