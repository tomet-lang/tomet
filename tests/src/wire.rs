//! The AST wire: what a document serializes to is the document, so a document handed to another
//! process and handed back is the one that was sent; and the plain-data projection of a value, which
//! is another thing and does not claim to be that.
//!
//! The wire is lossless and derived (see `tomet-ast`'s `Value`). A document read back is `==` to the
//! one sent, and `==` ignores spans, so each test also says that serializing the read-back
//! document again gives the same text: that holds a span, a key's kind and an entry's place too.

use tomet_ast::{File, Value};
use tomet_ast_parser::{parse_data_file, parse_file, parse_value};
use tomet_tests::parseable_corpus;

/// `value` written and read back: equal, and writing it again says the same thing. The value read
/// back is the result. (A macro, not a function: this package has no `serde` of its own to bound one by.)
macro_rules! round_trip {
    ($ty:ty, $value:expr) => {{
        let value = $value;
        let written = serde_json::to_string(value).expect("serializes");
        let read: $ty = serde_json::from_str(&written).expect("reads back");
        assert_eq!(&read, value);
        assert_eq!(
            serde_json::to_string(&read).expect("serializes again"),
            written,
            "writing what was read back says something else"
        );
        read
    }};
}

macro_rules! wire {
    ($value:expr) => {
        serde_json::to_value($value).expect("serializes")
    };
}
#[test]
fn every_document_of_the_corpus_reads_back_as_itself() {
    for (rel, src) in parseable_corpus() {
        let doc =
            parse_file(&src).unwrap_or_else(|e| panic!("{} does not parse: {e}", rel.display()));
        let written =
            serde_json::to_string(&doc).unwrap_or_else(|e| panic!("{}: {e}", rel.display()));
        let read: File = serde_json::from_str(&written)
            .unwrap_or_else(|e| panic!("{} does not read back: {e}", rel.display()));
        assert_eq!(
            read,
            doc,
            "{} reads back as another document",
            rel.display()
        );
        assert_eq!(
            serde_json::to_string(&read).unwrap(),
            written,
            "{} says something else when written again",
            rel.display()
        );
    }
}

#[test]
fn every_data_file_of_the_corpus_reads_back_as_itself() {
    let dir = tomet_tests::fixtures_dir().join("cotomet");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("cotomet fixtures").flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.starts_with("ok-") || path.extension().is_none_or(|e| e != "ctmt") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let doc = parse_data_file(&src).unwrap_or_else(|e| panic!("{name} does not parse: {e}"));
        round_trip!(File<Value>, &doc);
        checked += 1;
    }
    assert!(checked > 0, "no `ok-*.ctmt` fixture found in {dir:?}");
}

/// Each form that used to share a JSON shape with another: written, they are different, and read back
/// each is the variant it was.
#[test]
fn what_used_to_share_a_shape_is_told_apart() {
    let source = r#"@x(
  call: enum(1, 2),
  spelled: (call: enum, args: (1, 2)),
  interp: {$ y },
  seat: <$int>,
  hole: <?m>,
  element: @doc.icon("t"),
  blocks: [ a paragraph ],
  nothing: [ ],
  empty: (),
  <$int>: keyed,
  "<$int>": quoted,
)
"#;
    let doc = parse_file(source).expect("parses");
    let doc = round_trip!(File, &doc);
    let tomet_ast::Block::Element(el) = &doc.body[0] else {
        panic!("an element");
    };
    let args = el.data.args.as_ref().expect("args");
    let value = |key: &str| args.get(key).unwrap_or_else(|| panic!("no `{key}`"));

    assert!(matches!(value("call"), Value::Call(name, _) if name == "enum"));
    assert!(matches!(value("spelled"), Value::Tuple(_)));
    assert!(matches!(value("interp"), Value::Interp(_)));
    assert!(matches!(value("seat"), Value::TypeSeat(_)));
    assert!(matches!(value("hole"), Value::Hole(_)));
    assert!(matches!(value("element"), Value::Element(_)));
    assert!(matches!(value("blocks"), Value::Blocks(b) if b.len() == 1));
    assert!(matches!(value("nothing"), Value::Blocks(b) if b.is_empty()));
    assert!(matches!(value("empty"), Value::Tuple(t) if t.is_empty()));

    // They are told apart by what is written, not by guessing at its shape.
    let tags: Vec<String> = args
        .values()
        .map(|v| wire!(v).as_object().unwrap().keys().next().unwrap().clone())
        .collect();
    assert_eq!(
        tags,
        [
            "Call", "Tuple", "Interp", "TypeSeat", "Hole", "Element", "Blocks", "Blocks", "Tuple",
            "Scalar", "Scalar"
        ]
    );
    assert_ne!(wire!(value("call")), wire!(value("spelled")));
    assert_ne!(wire!(value("interp")), wire!(value("seat")));

    // A type seat key and a quoted key that spells it are two keys.
    assert_eq!(args.len(), 11);
    let keys: Vec<serde_json::Value> = wire!(args)["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["key"].clone())
        .collect();
    assert!(keys[9]["TypeSeat"].is_object(), "{:?}", keys[9]);
    assert_eq!(keys[10], serde_json::json!({"Name": "<$int>"}));
}

#[test]
fn auto_and_a_number_as_written_reach_the_wire_and_back() {
    let doc = parse_file("::meta(auto: auto, kept: 1.10, e: 1e3, big: 123456789012345678901234567890, i: 1, f: 1.5)\n")
        .expect("parses");
    let doc = round_trip!(File, &doc);
    let args = doc.header[0].data.args.as_ref().expect("args");
    let scalar = |key: &str| wire!(args.get(key).unwrap())["Scalar"].clone();
    assert_eq!(scalar("auto"), serde_json::json!("Auto"));
    assert_eq!(scalar("kept"), serde_json::json!({"Number": "1.10"}));
    assert_eq!(scalar("e"), serde_json::json!({"Number": "1e3"}));
    assert_eq!(
        scalar("big"),
        serde_json::json!({"Number": "123456789012345678901234567890"})
    );
    assert_eq!(scalar("i"), serde_json::json!({"Int": 1}));
    assert_eq!(scalar("f"), serde_json::json!({"Float": 1.5}));
}

#[test]
fn a_tuple_keeps_the_order_and_the_kind_of_its_keys() {
    let value = parse_value("t: (1, 2, b: 3, a: 4)\n").expect("parses");
    let value = round_trip!(Value, &value);
    let Value::Tuple(top) = &value else {
        panic!("a tuple")
    };
    let Some(Value::Tuple(t)) = top.get("t") else {
        panic!("a tuple")
    };
    let keys: Vec<String> = t.keyed().map(|(k, _)| k.into_owned()).collect();
    assert_eq!(keys, ["0", "1", "b", "a"]);
    let entries = wire!(t)["entries"].clone();
    let kinds: Vec<bool> = entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["key"].is_null())
        .collect();
    assert_eq!(kinds, [true, true, false, false]);
}

mod plain {
    use super::*;

    fn plain(src: &str) -> Result<String, tomet_ast::NotPlain> {
        let value = parse_value(src).expect("parses");
        value.to_plain().map(|p| serde_json::to_string(&p).unwrap())
    }

    #[test]
    fn data_is_the_json_it_looks_like() {
        assert_eq!(
            plain("title: \"T\"\ntags: (a, b)\nnest: (1, k: 2)\nn: none\nflag: true\nx: 1.10\nempty: ()\n").unwrap(),
            r#"{"title":"T","tags":["a","b"],"nest":{"0":1,"k":2},"n":null,"flag":true,"x":1.1,"empty":{}}"#
        );
    }

    #[test]
    fn a_node_that_is_not_data_is_refused_where_it_stands() {
        for (src, reason, at) in [
            ("a: auto\n", tomet_ast::NotPlainReason::Auto, "a"),
            (
                "a: (1, enum(1, 2))\n",
                tomet_ast::NotPlainReason::Call,
                "a.1",
            ),
            ("a: {$ y }\n", tomet_ast::NotPlainReason::Interp, "a"),
            ("a: <$int>\n", tomet_ast::NotPlainReason::TypeSeat, "a"),
            ("a: <?m>\n", tomet_ast::NotPlainReason::Hole, "a"),
            (
                "a: @doc.icon(\"t\")\n",
                tomet_ast::NotPlainReason::Element,
                "a",
            ),
            ("a: [ text ]\n", tomet_ast::NotPlainReason::Blocks, "a"),
            (
                "a: (<$int>: 1)\n",
                tomet_ast::NotPlainReason::TypeSeatKey,
                "a",
            ),
        ] {
            let err = plain(src).expect_err(src);
            assert_eq!(err.reason, reason, "{src}");
            assert_eq!(err.path.join("."), at, "{src}");
        }
    }

    #[test]
    fn it_is_a_view_of_the_value_not_a_replacement_for_the_wire() {
        // The wire holds what the plain form cannot, and the plain form does not read back into it.
        let value = parse_value("a: enum(1, 2)\n").expect("parses");
        assert!(value.to_plain().is_err());
        let Value::Tuple(t) = round_trip!(Value, &value) else {
            panic!("a tuple")
        };
        assert!(matches!(t.get("a"), Some(Value::Call(..))));
    }
}

/// A tuple is written somewhere or nowhere, and the wire says which: a real span, or `null`. It
/// never says `{"start":0,"end":0}` for a tuple that was not written.
#[test]
fn a_tuple_has_the_span_it_is_written_at_or_none() {
    let source = "@x(1, a: (2, 3), c: enum(4))\n\nhttps://example.com\n";
    let doc = parse_file(source).expect("parses");
    let doc = round_trip!(File, &doc);
    let tomet_ast::Block::Element(el) = &doc.body[0] else {
        panic!("an element");
    };
    let args = el.data.args.as_ref().expect("args");
    let text_of = |t: &tomet_ast::Tuple| {
        let span = t.span().expect("a written tuple has a span");
        source[span.start..span.end].to_string()
    };
    assert_eq!(text_of(args), "(1, a: (2, 3), c: enum(4))");
    let Some(Value::Tuple(inner)) = args.get("a") else {
        panic!("a tuple")
    };
    assert_eq!(text_of(inner), "(2, 3)");
    // The arguments of a call carry the span of the whole call.
    let Some(Value::Call(_, call)) = args.get("c") else {
        panic!("a call")
    };
    assert_eq!(text_of(call), "enum(4)");
    assert_eq!(
        wire!(inner)["span"],
        serde_json::json!({"start": 9, "end": 15})
    );

    // `()` is written too.
    let doc = parse_file("@x()\n").expect("parses");
    let tomet_ast::Block::Element(el) = &doc.body[0] else {
        panic!("an element");
    };
    assert!(
        el.data.args.as_ref().is_none_or(|t| t.span().is_some()),
        "an empty tuple that is written has its span"
    );
}

#[test]
fn a_tuple_nobody_wrote_has_no_span_on_the_wire() {
    let doc = parse_file("see https://example.com now\n").expect("parses");
    let doc = round_trip!(File, &doc);
    let tomet_ast::Block::Paragraph(p) = &doc.body[0] else {
        panic!("a paragraph")
    };
    let link = p
        .content
        .iter()
        .find_map(|i| match i {
            tomet_ast::Inline::Element(e) => Some(e),
            _ => None,
        })
        .expect("a link element");
    let args = link.data.args.as_ref().expect("target");
    assert!(args.span().is_none());
    assert_eq!(wire!(args)["span"], serde_json::Value::Null);
}
