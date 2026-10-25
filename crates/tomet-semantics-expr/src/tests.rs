use std::collections::HashMap;

use time::OffsetDateTime;
use tomet_ast::ElementValue;

use super::*;

/// A clock stopped at the epoch and a UUID that is always the same.
struct Fixed;

impl Generate for Fixed {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }
    fn uuid(&self) -> String {
        "00000000-0000-0000-0000-000000000000".to_string()
    }
}

fn int(i: i64) -> EvalValue {
    EvalValue::Int(i)
}

fn text(s: &str) -> EvalValue {
    EvalValue::String(s.to_string())
}

fn seq(items: Vec<EvalValue>) -> EvalValue {
    EvalValue::Seq(items)
}

fn map(entries: Vec<(&str, EvalValue)>) -> EvalValue {
    EvalValue::Map(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn parse(src: &str) -> tomet_ast::InterpExpr {
    let el = tomet_parser::parse_dollar_element_str(&format!("${{{src}}}"))
        .unwrap_or_else(|e| panic!("`{src}` does not parse: {e}"));
    match el.value {
        Some(ElementValue::Interp(expr)) => expr,
        other => panic!("expected an interpolation, got {other:?}"),
    }
}

fn run_with(
    src: &str,
    phase: Phase,
    var: &EvalValue,
    macros: &HashMap<String, String>,
) -> Result<EvalValue, EvalError> {
    let cx = EvalContext {
        phase,
        resolver: &NoResolver,
        generate: &Fixed,
        self_doc: DocRef("here.tmt".to_string()),
        macros,
        var,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    evaluate(&parse(src), &cx)
}

/// Evaluates when rendering, with nothing in `var`.
fn out(src: &str) -> Result<EvalValue, EvalError> {
    run_with(src, Phase::Output, &EvalValue::Null, &HashMap::new())
}

/// Evaluates when creating a document, with `var` set.
fn create(src: &str, vars: EvalValue) -> Result<EvalValue, EvalError> {
    run_with(src, Phase::Create, &vars, &HashMap::new())
}

#[test]
fn literals_evaluate_to_themselves() {
    assert_eq!(out("1").unwrap(), int(1));
    assert_eq!(out("1.5").unwrap(), EvalValue::Float(1.5));
    assert_eq!(out("-3").unwrap(), int(-3));
    assert_eq!(out("\"hi\"").unwrap(), text("hi"));
    assert_eq!(out("true").unwrap(), EvalValue::Bool(true));
    assert_eq!(out("null").unwrap(), EvalValue::Null);
    assert_eq!(out("(1)").unwrap(), int(1));
}

#[test]
fn an_unknown_name_is_an_error_and_never_falls_through() {
    assert_eq!(
        out("intro"),
        Err(EvalError::UnknownName("intro".to_string()))
    );
    assert_eq!(out("gh"), Err(EvalError::UnknownName("gh".to_string())));
}

#[test]
fn reserved_values_resolve() {
    assert_eq!(
        out("self").unwrap(),
        EvalValue::Document(DocRef("here.tmt".into()))
    );
    assert_eq!(
        out("vault").unwrap(),
        EvalValue::Directory(DirRef(String::new()))
    );
}

#[test]
fn generate_exists_only_when_creating_and_var_in_both() {
    assert_eq!(
        out("generate"),
        Err(EvalError::PhaseOnly("generate".to_string()))
    );
    // `var` is a value of the document, so it reads when rendering too.
    let var = map(vec![("title", text("T"))]);
    assert_eq!(
        run_with("var.title", Phase::Output, &var, &HashMap::new()).unwrap(),
        text("T")
    );
    assert_eq!(create("var.title", var).unwrap(), text("T"));
}

#[test]
fn a_declared_variable_with_no_value_says_how_to_give_it_one() {
    let cx = EvalContext {
        phase: Phase::Output,
        resolver: &NoResolver,
        generate: &Fixed,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &HashMap::new(),
        var: &map(vec![("other", text("O"))]),
        var_unset: &["title".to_string()],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    assert_eq!(
        evaluate(&parse("var.title"), &cx),
        Err(EvalError::NoValue("title".to_string()))
    );
    // a name that is not declared at all is still an unknown member
    assert_eq!(
        evaluate(&parse("var.nope"), &cx),
        Err(EvalError::UnknownMember {
            on: "map",
            member: "nope".to_string()
        })
    );
    assert_eq!(evaluate(&parse("var.other"), &cx).unwrap(), text("O"));
}

#[test]
fn generate_runs_through_the_injected_clock_and_random_source() {
    assert_eq!(
        create("generate.uuid()", EvalValue::Null).unwrap(),
        text("00000000-0000-0000-0000-000000000000")
    );
    assert_eq!(
        create("generate.now()", EvalValue::Null).unwrap(),
        EvalValue::DateTime(OffsetDateTime::UNIX_EPOCH)
    );
}

#[test]
fn generate_called_when_rendering_names_the_function() {
    assert_eq!(
        out("generate.uuid()"),
        Err(EvalError::PhaseOnly("generate".to_string()))
    );
}

#[test]
fn a_member_that_does_not_exist_is_named() {
    let vars = map(vec![("title", text("T"))]);
    assert_eq!(
        create("var.nope", vars),
        Err(EvalError::UnknownMember {
            on: "map",
            member: "nope".to_string()
        })
    );
    assert_eq!(
        out("true.foo"),
        Err(EvalError::UnknownMember {
            on: "bool",
            member: "foo".to_string()
        })
    );
}

#[test]
fn equality_compares_numbers_as_numbers_and_never_errors_across_types() {
    assert_eq!(out("1 == 1.0").unwrap(), EvalValue::Bool(true));
    assert_eq!(out("1 != 2").unwrap(), EvalValue::Bool(true));
    assert_eq!(out("\"a\" == \"a\"").unwrap(), EvalValue::Bool(true));
    assert_eq!(out("1 == \"1\"").unwrap(), EvalValue::Bool(false));
    assert_eq!(out("null == true").unwrap(), EvalValue::Bool(false));
    assert_eq!(out("null == null").unwrap(), EvalValue::Bool(true));
}

#[test]
fn ordering_takes_two_numbers_or_two_strings_and_nothing_else() {
    assert_eq!(out("1 < 2").unwrap(), EvalValue::Bool(true));
    assert_eq!(out("2 <= 2").unwrap(), EvalValue::Bool(true));
    assert_eq!(out("1.5 > 1").unwrap(), EvalValue::Bool(true));
    assert_eq!(
        out("\"2026-01-02\" >= \"2026-01-01\"").unwrap(),
        EvalValue::Bool(true)
    );
    assert_eq!(
        out("1 < \"a\""),
        Err(EvalError::NotComparable {
            left: "int",
            right: "string"
        })
    );
    assert_eq!(
        out("null < 1"),
        Err(EvalError::NotComparable {
            left: "null",
            right: "int"
        })
    );
}

#[test]
fn and_or_skip_the_right_side_once_the_left_decides() {
    // The right side names something that does not exist: it would fail if run.
    assert_eq!(out("false && nope").unwrap(), EvalValue::Bool(false));
    assert_eq!(out("true || nope").unwrap(), EvalValue::Bool(true));
    assert_eq!(out("true && false").unwrap(), EvalValue::Bool(false));
    assert_eq!(out("false || true").unwrap(), EvalValue::Bool(true));
    assert!(out("true && nope").is_err());
    assert!(out("false || nope").is_err());
}

#[test]
fn and_or_take_only_booleans() {
    assert!(matches!(
        out("1 && true"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
    assert!(matches!(
        out("null || true"),
        Err(EvalError::TypeMismatch { got: "null", .. })
    ));
    assert!(matches!(
        out("true && \"x\""),
        Err(EvalError::TypeMismatch { got: "string", .. })
    ));
}

#[test]
fn not_and_exists() {
    assert_eq!(out("not(true)").unwrap(), EvalValue::Bool(false));
    assert!(matches!(
        out("not(0)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
    assert_eq!(out("exists(null)").unwrap(), EvalValue::Bool(false));
    assert_eq!(out("exists(0)").unwrap(), EvalValue::Bool(true));
    assert_eq!(out("exists(\"\")").unwrap(), EvalValue::Bool(true));
}

#[test]
fn precedence_puts_comparison_before_and_before_or() {
    assert_eq!(
        out("1 < 2 && 3 > 2 || false").unwrap(),
        EvalValue::Bool(true)
    );
    assert_eq!(out("false && true || true").unwrap(), EvalValue::Bool(true));
    assert_eq!(
        out("false && (true || true)").unwrap(),
        EvalValue::Bool(false)
    );
}

#[test]
fn arithmetic_functions_keep_integers_and_promote_on_overflow() {
    assert_eq!(out("add(1, 2)").unwrap(), int(3));
    assert_eq!(out("sub(5, 7)").unwrap(), int(-2));
    assert_eq!(out("mul(3, 4)").unwrap(), int(12));
    assert_eq!(out("add(1, 1.5)").unwrap(), EvalValue::Float(2.5));
    assert_eq!(out("div(7, 2)").unwrap(), EvalValue::Float(3.5));
    assert_eq!(out("mod(7, 3)").unwrap(), int(1));
    assert!(matches!(
        out("add(9223372036854775807, 1)").unwrap(),
        EvalValue::Float(_)
    ));
    assert_eq!(out("div(1, 0)"), Err(EvalError::DivisionByZero));
    assert_eq!(out("mod(1, 0)"), Err(EvalError::DivisionByZero));
    assert!(matches!(
        out("add(1, \"a\")"),
        Err(EvalError::TypeMismatch { got: "string", .. })
    ));
}

#[test]
fn an_unknown_function_is_named_and_a_wrong_count_is_reported() {
    assert_eq!(
        out("frobnicate(1)"),
        Err(EvalError::UnknownFunction("frobnicate".to_string()))
    );
    assert!(matches!(
        out("add(1)"),
        Err(EvalError::WrongArgCount { got: 1, .. })
    ));
}

#[test]
fn the_old_comparison_and_logic_functions_are_gone() {
    for name in [
        "eq", "ne", "gt", "gte", "lt", "lte", "and", "or", "contains",
    ] {
        assert!(
            matches!(
                out(&format!("{name}(1, 2)")),
                Err(EvalError::UnknownFunction(_))
            ),
            "{name} should no longer be a function"
        );
    }
}

fn xs() -> EvalValue {
    map(vec![("xs", seq(vec![int(3), int(1), int(2)]))])
}

#[test]
fn a_lambda_is_a_value_and_can_be_applied_by_a_method() {
    assert_eq!(
        create("var.xs.map(x => add(x, 1))", xs()).unwrap(),
        seq(vec![int(4), int(2), int(3)])
    );
}

#[test]
fn filter_keeps_what_the_function_says_yes_to() {
    assert_eq!(
        create("var.xs.filter(x => x > 1)", xs()).unwrap(),
        seq(vec![int(3), int(2)])
    );
}

#[test]
fn filter_needs_a_bool_from_its_function() {
    assert!(matches!(
        create("var.xs.filter(x => x)", xs()),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn sort_orders_by_a_key_and_is_stable() {
    assert_eq!(
        create("var.xs.sort(x => x)", xs()).unwrap(),
        seq(vec![int(1), int(2), int(3)])
    );
    assert_eq!(
        create("var.xs.sort(x => x, \"desc\")", xs()).unwrap(),
        seq(vec![int(3), int(2), int(1)])
    );
    // Equal keys keep their original order.
    let pairs = map(vec![(
        "xs",
        seq(vec![
            map(vec![("k", int(1)), ("n", text("a"))]),
            map(vec![("k", int(0)), ("n", text("b"))]),
            map(vec![("k", int(1)), ("n", text("c"))]),
        ]),
    )]);
    assert_eq!(
        create("var.xs.sort(p => p.k).map(p => p.n)", pairs).unwrap(),
        seq(vec![text("b"), text("a"), text("c")])
    );
}

#[test]
fn sort_refuses_keys_that_have_no_common_order() {
    let mixed = map(vec![("xs", seq(vec![int(1), text("a")]))]);
    assert_eq!(
        create("var.xs.sort(x => x)", mixed),
        Err(EvalError::NotComparable {
            left: "int",
            right: "string"
        })
    );
    assert!(create("var.xs.sort(x => x, \"sideways\")", xs()).is_err());
}

#[test]
fn an_item_with_no_key_goes_last_whichever_way_the_rest_are_ordered() {
    let xs = map(vec![(
        "xs",
        seq(vec![
            map(vec![("k", int(2)), ("n", text("a"))]),
            map(vec![("k", EvalValue::Null), ("n", text("none1"))]),
            map(vec![("k", int(1)), ("n", text("b"))]),
            map(vec![("k", EvalValue::Null), ("n", text("none2"))]),
        ]),
    )]);
    assert_eq!(
        create("var.xs.sort(p => p.k).map(p => p.n)", xs.clone()).unwrap(),
        seq(vec![text("b"), text("a"), text("none1"), text("none2")])
    );
    assert_eq!(
        create("var.xs.sort(p => p.k, \"desc\").map(p => p.n)", xs).unwrap(),
        seq(vec![text("a"), text("b"), text("none1"), text("none2")])
    );
    // Only keys that exist have to agree: a number and a string still clash.
    let clash = map(vec![("xs", seq(vec![int(1), EvalValue::Null, text("a")]))]);
    assert!(create("var.xs.sort(x => x)", clash).is_err());
}

#[test]
fn join_len_contains_and_item_on_a_list() {
    assert_eq!(
        create("var.xs.join(\", \")", xs()).unwrap(),
        text("3, 1, 2")
    );
    assert_eq!(create("var.xs.len()", xs()).unwrap(), int(3));
    assert_eq!(
        create("var.xs.contains(2)", xs()).unwrap(),
        EvalValue::Bool(true)
    );
    assert_eq!(
        create("var.xs.contains(9)", xs()).unwrap(),
        EvalValue::Bool(false)
    );
    assert_eq!(create("var.xs.item(1)", xs()).unwrap(), int(1));
    assert!(create("var.xs.item(7)", xs()).is_err());
    assert!(create("var.xs.item(-1)", xs()).is_err());
}

#[test]
fn join_takes_scalars_only() {
    let nested = map(vec![("xs", seq(vec![seq(vec![int(1)])]))]);
    assert!(matches!(
        create("var.xs.join(\",\")", nested),
        Err(EvalError::TypeMismatch { got: "list", .. })
    ));
    let nulls = map(vec![("xs", seq(vec![EvalValue::Null]))]);
    assert!(create("var.xs.join(\",\")", nulls).is_err());
}

#[test]
fn an_empty_list_answers_contains_with_no_instead_of_failing() {
    // What a document with no tags gives for a `list` field.
    let empty = map(vec![("tags", seq(vec![]))]);
    assert_eq!(
        create("var.tags.contains(\"rust\")", empty).unwrap(),
        EvalValue::Bool(false)
    );
}

#[test]
fn maps_and_strings_have_their_own_contains_and_get() {
    let m = map(vec![("m", map(vec![("a", int(1))]))]);
    assert_eq!(
        create("var.m.contains(\"a\")", m.clone()).unwrap(),
        EvalValue::Bool(true)
    );
    assert_eq!(create("var.m.get(\"a\", 0)", m.clone()).unwrap(), int(1));
    assert_eq!(create("var.m.get(\"z\", 0)", m).unwrap(), int(0));
    assert_eq!(
        out("\"hello\".contains(\"ell\")").unwrap(),
        EvalValue::Bool(true)
    );
    assert!(matches!(
        out("\"hello\".contains(1)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn a_method_on_the_wrong_type_or_on_null_is_an_error() {
    assert_eq!(
        out("null.contains(1)"),
        Err(EvalError::UnknownMember {
            on: "null",
            member: "contains".to_string()
        })
    );
    assert_eq!(
        out("true.len()"),
        Err(EvalError::UnknownMember {
            on: "bool",
            member: "len".to_string()
        })
    );
}

#[test]
fn a_lambda_sees_the_names_around_it() {
    assert_eq!(
        create("var.xs.map(x => var.xs.filter(y => y > x).len())", xs()).unwrap(),
        seq(vec![int(0), int(2), int(1)])
    );
}

#[test]
fn a_lambda_cannot_take_a_reserved_name_as_a_parameter() {
    for name in RESERVED_VALUES {
        let err = out(&format!("{name} => 1")).unwrap_err();
        assert!(err.to_string().contains("reserved"), "{name}: {err}");
    }
}

#[test]
fn a_lambda_called_with_the_wrong_number_of_arguments_is_reported() {
    assert!(matches!(
        create("var.xs.sort((a, b) => a)", xs()),
        Err(EvalError::WrongArgCount { .. })
    ));
}

#[test]
fn a_function_is_not_data() {
    assert_eq!(out("x => x").unwrap().to_ast(), None);
}

#[test]
fn a_macro_fills_positional_and_named_placeholders() {
    let mut macros = HashMap::new();
    macros.insert("gh".to_string(), "https://x/issues/${1}".to_string());
    macros.insert("who".to_string(), "hi ${name}".to_string());
    let run = |src: &str| run_with(src, Phase::Output, &EvalValue::Null, &macros);
    assert_eq!(run("macro.gh(12)").unwrap(), text("https://x/issues/12"));
    assert_eq!(run("macro.who(name: \"ann\")").unwrap(), text("hi ann"));
    assert_eq!(
        run("macro.nope(1)"),
        Err(EvalError::UnknownFunction("macro.nope".to_string()))
    );
}

#[test]
fn a_macro_cannot_be_named_without_being_called_and_has_no_bare_form() {
    let mut macros = HashMap::new();
    macros.insert("gh".to_string(), "x".to_string());
    let run = |src: &str| run_with(src, Phase::Output, &EvalValue::Null, &macros);
    assert!(run("macro.gh").is_err());
    // The bare name is not a macro and not a function.
    assert_eq!(
        run("gh(12)"),
        Err(EvalError::UnknownFunction("gh".to_string()))
    );
}

#[test]
fn named_arguments_outside_a_call_are_rejected() {
    // `NamedArg` at the top level only parses; it means nothing there.
    assert!(out("heading: false").is_err());
}

// ---- documents, addresses, directories -------------------------------------

const HERE: &str = "::kind(note)
::meta{
  title: Here
  tags: list(a, b)
}

=[ Intro ]#(intro)

Some body.

@card(n: 1, tags: list(x, y, z)){ extra: yes }#(c1)
";

const DOC_A: &str = "::meta{
  title: A
}

@card(x: 7)#(top)
";

const DOC_B: &str = "::meta{
  title: B
}
";

fn vault() -> MemoryResolver {
    MemoryResolver::new()
        .with_source("here.tmt", HERE)
        .unwrap()
        .with_source("docs/a.tmt", DOC_A)
        .unwrap()
        .with_source("docs/sub/b.tmt", DOC_B)
        .unwrap()
}

/// Evaluates when rendering `here.tmt` in the small vault above.
fn in_vault(src: &str) -> Result<EvalValue, EvalError> {
    let resolver = vault();
    let macros = HashMap::new();
    let vars = EvalValue::Null;
    let cx = EvalContext {
        phase: Phase::Output,
        resolver: &resolver,
        generate: &Fixed,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        var: &vars,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    evaluate(&parse(src), &cx)
}

#[test]
fn a_leading_hash_picks_an_element_of_the_current_document() {
    assert_eq!(in_vault("#c1.prop.n").unwrap(), int(1));
    assert_eq!(in_vault("#c1.id").unwrap(), text("c1"));
    assert_eq!(in_vault("#c1.sigil").unwrap(), text("card"));
}

#[test]
fn the_leading_hash_is_self_hash() {
    assert_eq!(
        in_vault("self#c1.prop.n").unwrap(),
        in_vault("#c1.prop.n").unwrap()
    );
    assert_eq!(in_vault("#c1 == self#c1").unwrap(), EvalValue::Bool(true));
}

#[test]
fn prop_merges_args_and_value_and_item_walks_a_list() {
    assert_eq!(in_vault("#c1.prop.extra").unwrap(), text("yes"));
    assert_eq!(in_vault("#c1.prop.tags.item(2)").unwrap(), text("z"));
    assert_eq!(in_vault("#c1.prop.tags.len()").unwrap(), int(3));
    // The call form of `prop` takes the key as a string.
    assert_eq!(in_vault("#c1.prop(\"n\")").unwrap(), int(1));
}

#[test]
fn a_heading_is_found_by_its_id_and_is_a_section() {
    assert_eq!(in_vault("#intro.id").unwrap(), text("intro"));
    assert_eq!(in_vault("#intro.sigil").unwrap(), text("section"));
    // The element that follows the heading is under it.
    assert!(matches!(in_vault("#intro.content").unwrap(), EvalValue::Fragment(b) if b.len() == 2));
}

#[test]
fn a_document_on_the_left_picks_an_element_of_that_document() {
    assert_eq!(in_vault("tmt(\"docs/a.tmt\")#top.prop.x").unwrap(), int(7));
    assert_eq!(in_vault("ref(\"a\")#top.prop.x").unwrap(), int(7));
    // A leading `./` names the same document.
    assert_eq!(
        in_vault("tmt(\"./docs/a.tmt\")#top.prop.x").unwrap(),
        int(7)
    );
}

#[test]
fn a_missing_document_or_id_is_reported_with_what_was_asked_for() {
    let err = in_vault("tmt(\"nope.tmt\")#x").unwrap_err();
    assert!(
        matches!(&err, EvalError::Unresolved(m) if m.contains("nope.tmt")),
        "{err}"
    );
    let err = in_vault("#missing").unwrap_err();
    assert!(
        matches!(&err, EvalError::Unresolved(m) if m.contains("`missing`") && m.contains("here.tmt")),
        "{err}"
    );
    assert!(matches!(
        in_vault("ref(\"zzz\")"),
        Err(EvalError::Unresolved(_))
    ));
}

#[test]
fn a_hash_needs_a_document_on_its_left() {
    assert!(matches!(
        in_vault("1#x"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn document_members() {
    assert_eq!(in_vault("self.path").unwrap(), text("here.tmt"));
    assert_eq!(in_vault("self.filename").unwrap(), text("here.tmt"));
    assert_eq!(
        in_vault("tmt(\"docs/sub/b.tmt\").filename").unwrap(),
        text("b.tmt")
    );
    assert_eq!(
        in_vault("tmt(\"./docs/a.tmt\").path").unwrap(),
        text("docs/a.tmt")
    );
    assert_eq!(in_vault("self.meta.title").unwrap(), text("Here"));
    assert_eq!(in_vault("self.kind").unwrap(), text("note"));
    assert_eq!(
        in_vault("tmt(\"docs/a.tmt\").kind").unwrap(),
        EvalValue::Null
    );
    assert_eq!(
        in_vault("self.nope"),
        Err(EvalError::UnknownMember {
            on: "document",
            member: "nope".to_string()
        })
    );
}

#[test]
fn the_meta_a_resolver_gives_is_what_is_read() {
    // The in-memory resolver applies no settings, so a field the document
    // lacks is absent; the vault's resolver makes it `Null` or `[]`.
    assert!(matches!(
        in_vault("tmt(\"docs/sub/b.tmt\").meta.tags"),
        Err(EvalError::UnknownMember { on: "map", .. })
    ));
}

#[test]
fn root_items_are_numbered_from_zero_and_a_section_is_a_node() {
    assert_eq!(in_vault("#item(0).sigil").unwrap(), text("kind"));
    assert_eq!(in_vault("self#item(2).id").unwrap(), text("intro"));
    assert_eq!(in_vault("self.item(1).sigil").unwrap(), text("meta"));
    assert!(in_vault("#item(99)").is_err());
}

/// `item(n)` counts blocks -- a paragraph is `item(0)` here, so the element
/// after it is `item(1)` -- and not the blank lines between them
/// (`tmtroot/spec/syntax/addressable.tmt`, 位置による番地).
#[test]
fn item_on_a_node_walks_its_children() {
    assert!(matches!(
        in_vault("#intro.item(0)").unwrap(),
        EvalValue::Fragment(_)
    ));
    assert_eq!(in_vault("#intro.item(1).id").unwrap(), text("c1"));
}

#[test]
fn a_node_has_only_its_own_members() {
    assert_eq!(
        in_vault("#c1.tags"),
        Err(EvalError::UnknownMember {
            on: "element",
            member: "tags".to_string()
        })
    );
    // A property is reached through `.prop`, never directly.
    assert_eq!(in_vault("#c1.prop.n").unwrap(), int(1));
}

#[test]
fn a_directory_lists_the_documents_under_it() {
    assert_eq!(
        in_vault("vault.docs.map(d => d.path)").unwrap(),
        seq(vec![
            text("docs/a.tmt"),
            text("docs/sub/b.tmt"),
            text("here.tmt")
        ])
    );
    assert_eq!(in_vault("addr(dir:docs).docs.len()").unwrap(), int(2));
    assert_eq!(in_vault("addr(dir:./docs/sub).docs.len()").unwrap(), int(1));
}

#[test]
fn children_are_the_documents_directly_in_the_directory() {
    // `docs` goes down into `docs/sub`; `children` does not.
    assert_eq!(
        in_vault("addr(dir:docs).children.map(d => d.path)").unwrap(),
        seq(vec![text("docs/a.tmt")])
    );
    assert_eq!(
        in_vault("vault.children.map(d => d.path)").unwrap(),
        seq(vec![text("here.tmt")])
    );
    assert_eq!(
        in_vault("addr(dir:docs/sub).children.len()").unwrap(),
        int(1)
    );
}

#[test]
fn addr_reads_a_link_style_address() {
    assert_eq!(in_vault("addr(tmt:docs/a.tmt#top.prop.x)").unwrap(), int(7));
    assert_eq!(
        in_vault("addr(tmt:docs/a.tmt)").unwrap(),
        EvalValue::Document(DocRef("docs/a.tmt".to_string()))
    );
    assert_eq!(in_vault("addr(#c1).prop.n").unwrap(), int(1));
    assert_eq!(in_vault("addr(ref:a#top.prop.x)").unwrap(), int(7));
    assert_eq!(
        in_vault("addr(\"tmt:docs/a.tmt#top.prop.x\")").unwrap(),
        int(7)
    );
    // The steps after `#` go as far as an expression after `#` goes.
    assert_eq!(in_vault("addr(#c1.prop.tags.item(1))").unwrap(), text("y"));
}

#[test]
fn addr_rejects_what_is_not_an_address() {
    assert!(matches!(
        in_vault("addr(docs/a.tmt)"),
        Err(EvalError::BadAddress(_))
    ));
    assert!(matches!(
        in_vault("addr(wat:x)"),
        Err(EvalError::BadAddress(_))
    ));
    assert!(matches!(
        in_vault("addr(https://example.com/x)"),
        Err(EvalError::NotYetSupported(_))
    ));
    assert!(in_vault("addr(dir:docs#x)").is_err());
}

#[test]
fn a_string_known_only_at_run_time_is_read_with_dot_addr() {
    assert_eq!(
        in_vault("\"tmt:docs/a.tmt#top.prop.x\".addr()").unwrap(),
        int(7)
    );
    assert_eq!(in_vault("\"#c1.prop.n\".addr()").unwrap(), int(1));
    assert!(matches!(
        in_vault("\"not an address\".addr()"),
        Err(EvalError::BadAddress(_))
    ));
    assert!(matches!(
        in_vault("1.addr()"),
        Err(EvalError::UnknownMember { .. })
    ));
}

#[test]
fn an_address_inside_a_lambda_still_resolves_against_the_right_document() {
    // `#top` inside `addr(...)` is resolved against `docs/a.tmt`, not `self`,
    // even though the lambda runs while `self` is `here.tmt`.
    assert_eq!(
        in_vault("vault.docs.filter(d => d.path == \"docs/a.tmt\").map(d => d#top.prop.x)")
            .unwrap(),
        seq(vec![int(7)])
    );
}

// ---- datetimes --------------------------------------------------------------

fn when(text: &str) -> EvalValue {
    EvalValue::DateTime(parse_datetime(text).unwrap())
}

fn with_dates() -> EvalValue {
    map(vec![
        ("early", when("2026-01-01T00:00:00Z")),
        ("late", when("2026-06-01T12:30:00+09:00")),
    ])
}

#[test]
fn a_datetime_formats_with_the_shared_symbols() {
    assert_eq!(
        create("var.late.format(\"YYYY-MM-DD HH:mm\")", with_dates()).unwrap(),
        text("2026-06-01 12:30")
    );
    assert_eq!(
        create("var.late.format(\"YYYY年MM月DD日\")", with_dates()).unwrap(),
        text("2026年06月01日")
    );
}

#[test]
fn format_keeps_the_values_own_offset() {
    // 12:30 at +09:00 is 03:30 UTC; the value is written as it is, not converted.
    assert_eq!(
        create("var.late.format(\"HH\")", with_dates()).unwrap(),
        text("12")
    );
}

#[test]
fn datetimes_order_and_compare() {
    assert_eq!(
        create("var.early < var.late", with_dates()).unwrap(),
        EvalValue::Bool(true)
    );
    assert_eq!(
        create("var.late >= var.early", with_dates()).unwrap(),
        EvalValue::Bool(true)
    );
    assert_eq!(
        create("var.early == var.early", with_dates()).unwrap(),
        EvalValue::Bool(true)
    );
    // The same instant at two offsets is the same datetime.
    let same = map(vec![
        ("a", when("2026-01-01T09:00:00+09:00")),
        ("b", when("2026-01-01T00:00:00Z")),
    ]);
    assert_eq!(
        create("var.a == var.b", same).unwrap(),
        EvalValue::Bool(true)
    );
}

#[test]
fn a_datetime_does_not_order_against_a_string() {
    let mixed = map(vec![("d", when("2026-01-01T00:00:00Z"))]);
    assert_eq!(
        create("var.d < \"2026-02-01\"", mixed),
        Err(EvalError::NotComparable {
            left: "datetime",
            right: "string"
        })
    );
}

#[test]
fn datetimes_sort() {
    let list = map(vec![(
        "xs",
        seq(vec![
            when("2026-03-01T00:00:00Z"),
            when("2026-01-01T00:00:00Z"),
            when("2026-02-01T00:00:00Z"),
        ]),
    )]);
    assert_eq!(
        create(
            "var.xs.sort(d => d, \"desc\").map(d => d.format(\"MM\"))",
            list
        )
        .unwrap(),
        seq(vec![text("03"), text("02"), text("01")])
    );
}

#[test]
fn format_wants_a_string_and_a_datetime_has_no_other_members() {
    assert!(matches!(
        create("var.late.format(1)", with_dates()),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
    assert_eq!(
        create("var.late.century", with_dates()),
        Err(EvalError::UnknownMember {
            on: "datetime",
            member: "century".to_string()
        })
    );
}

#[test]
fn a_datetime_has_its_parts_in_its_own_offset() {
    // 12:30 at +09:00: the parts are the ones written, not the UTC ones.
    for (expr, want) in [
        ("year", 2026),
        ("month", 6),
        ("day", 1),
        ("hour", 12),
        ("minute", 30),
        ("second", 0),
    ] {
        assert_eq!(
            create(&format!("var.late.{expr}"), with_dates()).unwrap(),
            int(want),
            "{expr}"
        );
    }
    assert_eq!(
        create("var.late.offset", with_dates()).unwrap(),
        text("+09:00")
    );
    assert_eq!(
        create("var.early.offset", with_dates()).unwrap(),
        text("+00:00")
    );
    assert_eq!(
        create("var.late.year == 2026", with_dates()).unwrap(),
        EvalValue::Bool(true)
    );
}

#[test]
fn at_gives_the_same_instant_in_another_offset() {
    // 12:30 at +09:00 is 03:30 UTC.
    assert_eq!(
        create("var.late.at(\"Z\").format(\"HH:mm\")", with_dates()).unwrap(),
        text("03:30")
    );
    assert_eq!(
        create(
            "var.early.at(\"-05:30\").format(\"YYYY-MM-DD HH:mm\")",
            with_dates()
        )
        .unwrap(),
        text("2025-12-31 18:30")
    );
    assert_eq!(
        create("var.late.at(\"-05:00\").offset", with_dates()).unwrap(),
        text("-05:00")
    );
    // The instant is the same, so it compares equal.
    assert_eq!(
        create("var.late.at(\"Z\") == var.late", with_dates()).unwrap(),
        EvalValue::Bool(true)
    );
}

#[test]
fn at_wants_an_offset() {
    assert!(matches!(
        create("var.late.at(1)", with_dates()),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
    assert!(create("var.late.at(\"Tokyo\")", with_dates()).is_err());
}

#[test]
fn generate_now_formats_like_any_datetime() {
    assert_eq!(
        create("generate.now().format(\"YYYY-MM-DD\")", EvalValue::Null).unwrap(),
        text("1970-01-01")
    );
}

// ---- fragments, emoji, unicode ----------------------------------------------

/// The link element inside a one-paragraph fragment, as its `args` string.
fn link_target(value: &EvalValue) -> String {
    let EvalValue::Fragment(blocks) = value else {
        panic!("expected a fragment, got {value:?}");
    };
    let [tomet_ast::Block::Paragraph(p)] = blocks.as_slice() else {
        panic!("expected one paragraph, got {blocks:?}");
    };
    let [tomet_ast::Inline::Element(el)] = p.content.as_slice() else {
        panic!("expected one inline element, got {:?}", p.content);
    };
    assert_eq!(el.sigil, tomet_ast::Sigil::named("link"));
    match &el.args {
        Some(tomet_ast::Value::String(target)) => target.clone(),
        other => panic!("expected a string target, got {other:?}"),
    }
}

#[test]
fn a_document_links_to_itself_as_a_fragment() {
    assert_eq!(
        link_target(&in_vault("tmt(\"docs/a.tmt\").link()").unwrap()),
        "tmt:docs/a.tmt"
    );
    assert_eq!(
        link_target(&in_vault("link(self)").unwrap()),
        "tmt:here.tmt"
    );
}

#[test]
fn an_element_with_an_id_links_to_that_id() {
    assert_eq!(
        link_target(&in_vault("#c1.link()").unwrap()),
        "tmt:here.tmt#c1"
    );
    assert_eq!(
        link_target(&in_vault("#intro.link()").unwrap()),
        "tmt:here.tmt#intro"
    );
}

#[test]
fn an_element_without_an_id_cannot_be_linked() {
    let err = in_vault("self#item(1).link()").unwrap_err();
    assert!(err.to_string().contains("without an id"), "{err}");
}

#[test]
fn link_turns_a_list_of_documents_into_a_list_of_links() {
    let links = in_vault("addr(dir:docs).docs.map(d => d.link())").unwrap();
    let EvalValue::Seq(items) = links else {
        panic!("expected a list");
    };
    let targets: Vec<String> = items.iter().map(link_target).collect();
    assert_eq!(targets, ["tmt:docs/a.tmt", "tmt:docs/sub/b.tmt"]);
}

#[test]
fn link_wants_a_document_or_an_element() {
    assert!(matches!(
        in_vault("link(1)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn a_fragment_is_data_and_a_reference_is_not() {
    let link = in_vault("self.link()").unwrap();
    assert!(link.to_ast().is_some());
    assert_eq!(in_vault("self").unwrap().to_ast(), None);
}

#[test]
fn emoji_looks_a_name_up_and_refuses_one_it_does_not_know() {
    assert_eq!(out("emoji(\"rocket\")").unwrap(), text("🚀"));
    assert_eq!(out("emoji(\":tada:\")").unwrap(), text("🎉"));
    assert_eq!(out("emoji(\"party\")").unwrap(), text("🎉"));
    let err = out("emoji(\"nope\")").unwrap_err();
    assert!(err.to_string().contains("nope"), "{err}");
    assert!(matches!(
        out("emoji(1)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn unicode_takes_a_code_point_as_a_number_or_hex_text() {
    assert_eq!(out("unicode(65)").unwrap(), text("A"));
    assert_eq!(out("unicode(\"U+1F680\")").unwrap(), text("🚀"));
    assert_eq!(out("unicode(\"1F680\")").unwrap(), text("🚀"));
    assert_eq!(out("unicode(\"0x41\")").unwrap(), text("A"));
    assert!(out("unicode(\"zzz\")").is_err());
    assert!(out("unicode(-1)").is_err());
    // A surrogate is not a character.
    assert!(out("unicode(55296)").is_err());
    assert!(matches!(
        out("unicode(true)"),
        Err(EvalError::TypeMismatch { got: "bool", .. })
    ));
}

// ---- include ----------------------------------------------------------------

const GUIDE: &str = "::kind(note)
::meta{ title: Guide }

=[ Install ]#(install)

Run it.

==[ Linux ]#(linux)

Use apt.

==[ Mac ]#(mac)

Use brew.

=[ Usage ]#(usage)

See @link(./img/a.png) and @link(../shared/s.tmt#top) and @link(tmt:abs.tmt).

@card(n: 1)#(c1)
";

fn guide_vault() -> MemoryResolver {
    MemoryResolver::new()
        .with_source("docs/guide.tmt", GUIDE)
        .unwrap()
        .with_source("here.tmt", "::meta{ title: Here }\n")
        .unwrap()
        .with_source("docs/sub/host.tmt", "Host\n")
        .unwrap()
        .with_source("docs/other.tmt", "Other\n")
        .unwrap()
}

/// Evaluates `src` as written in `host`, under a heading of `level`, with
/// `includes` being included already.
fn include_in(
    host: &str,
    level: usize,
    includes: &[DocRef],
    src: &str,
) -> Result<EvalValue, EvalError> {
    let resolver = guide_vault();
    let macros = HashMap::new();
    let vars = EvalValue::Null;
    let cx = EvalContext {
        phase: Phase::Output,
        resolver: &resolver,
        generate: &Fixed,
        self_doc: DocRef(host.to_string()),
        macros: &macros,
        var: &vars,
        var_unset: &[],
        includes,
        heading_level: level,
        notes: Default::default(),
    };
    evaluate(&parse(src), &cx)
}

fn fragment(value: EvalValue) -> Vec<tomet_ast::Block> {
    match value {
        EvalValue::Fragment(blocks) => blocks,
        other => panic!("expected a fragment, got {other:?}"),
    }
}

/// `level:title` for each section and `p:text` for each plain paragraph, in
/// document order, nested sections after their parent.
fn outline(blocks: &[tomet_ast::Block]) -> Vec<String> {
    let text = |inlines: &[tomet_ast::Inline]| -> String {
        inlines
            .iter()
            .filter_map(|i| match i {
                tomet_ast::Inline::Text(t) => Some(t.value.as_str()),
                _ => None,
            })
            .collect()
    };
    let mut out = Vec::new();
    for block in blocks {
        match block {
            tomet_ast::Block::Section(s) => {
                out.push(format!("{}:{}", s.level, text(&s.title).trim()));
                out.extend(outline(&s.blocks));
            }
            tomet_ast::Block::Paragraph(p) => {
                let t = text(&p.content);
                if !t.trim().is_empty() && !t.starts_with("See ") {
                    out.push(format!("p:{}", t.trim()));
                }
            }
            tomet_ast::Block::Element(el) => out.push(format!(
                "e:{}",
                el.sigil.name().map(|n| n.to_string()).unwrap_or_default()
            )),
            _ => {}
        }
    }
    out
}

/// The target of every `@link` in the blocks, in order.
fn link_targets(blocks: &[tomet_ast::Block]) -> Vec<String> {
    let mut doc = tomet_ast::Document::new(blocks.to_vec(), tomet_ast::Span::default());
    let mut out = Vec::new();
    tomet_tree::for_each_element_mut(&mut doc, |el| {
        if let Some((_, target)) = tomet_semantics::link_target_of(el) {
            out.push(target);
        }
    });
    out
}

#[test]
fn a_document_is_included_without_its_own_declarations() {
    let blocks =
        fragment(include_in("here.tmt", 0, &[], "include(tmt(\"docs/guide.tmt\"))").unwrap());
    let all = outline(&blocks);
    assert_eq!(
        all,
        [
            "1:Install",
            "p:Run it.",
            "2:Linux",
            "p:Use apt.",
            "2:Mac",
            "p:Use brew.",
            "1:Usage",
            "e:card"
        ]
    );
    // `::kind` and `::meta` are the document's own business.
    assert!(!all.iter().any(|line| line == "e:kind" || line == "e:meta"));
}

#[test]
fn included_headings_go_one_below_the_heading_they_land_under() {
    let blocks =
        fragment(include_in("here.tmt", 2, &[], "include(tmt(\"docs/guide.tmt\"))").unwrap());
    let all = outline(&blocks);
    assert!(all.contains(&"3:Install".to_string()), "{all:?}");
    assert!(all.contains(&"4:Linux".to_string()), "{all:?}");
    assert!(all.contains(&"3:Usage".to_string()), "{all:?}");
}

#[test]
fn level_sets_the_level_of_the_top_headings_outright() {
    let blocks = fragment(
        include_in(
            "here.tmt",
            3,
            &[],
            "include(tmt(\"docs/guide.tmt\"), level: 2)",
        )
        .unwrap(),
    );
    let all = outline(&blocks);
    assert!(all.contains(&"2:Install".to_string()), "{all:?}");
    assert!(all.contains(&"3:Linux".to_string()), "{all:?}");
}

#[test]
fn an_id_takes_the_whole_section() {
    let blocks = fragment(
        include_in(
            "here.tmt",
            1,
            &[],
            "include(tmt(\"docs/guide.tmt\")#install)",
        )
        .unwrap(),
    );
    assert_eq!(
        outline(&blocks),
        [
            "2:Install",
            "p:Run it.",
            "3:Linux",
            "p:Use apt.",
            "3:Mac",
            "p:Use brew."
        ]
    );
}

#[test]
fn heading_false_takes_the_body_and_puts_the_subsections_directly_below() {
    let blocks = fragment(
        include_in(
            "here.tmt",
            1,
            &[],
            "include(tmt(\"docs/guide.tmt\")#install, heading: false)",
        )
        .unwrap(),
    );
    assert_eq!(
        outline(&blocks),
        ["p:Run it.", "2:Linux", "p:Use apt.", "2:Mac", "p:Use brew."]
    );
}

#[test]
fn an_element_without_a_section_is_just_that_element() {
    let blocks =
        fragment(include_in("here.tmt", 1, &[], "include(tmt(\"docs/guide.tmt\")#c1)").unwrap());
    assert_eq!(outline(&blocks), ["e:card"]);
}

#[test]
fn include_checks_what_it_is_given() {
    let err = include_in(
        "here.tmt",
        0,
        &[],
        "include(tmt(\"docs/guide.tmt\"), heading: false)",
    )
    .unwrap_err();
    assert!(err.to_string().contains("section"), "{err}");
    let err = include_in(
        "here.tmt",
        0,
        &[],
        "include(tmt(\"docs/guide.tmt\")#c1, heading: false)",
    )
    .unwrap_err();
    assert!(err.to_string().contains("section"), "{err}");
    assert!(matches!(
        include_in("here.tmt", 0, &[], "include(1)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
    assert!(
        include_in(
            "here.tmt",
            0,
            &[],
            "include(tmt(\"docs/guide.tmt\"), level: 0)"
        )
        .is_err()
    );
    assert!(
        include_in(
            "here.tmt",
            0,
            &[],
            "include(tmt(\"docs/guide.tmt\"), depth: 2)"
        )
        .is_err()
    );
    assert!(include_in("here.tmt", 0, &[], "include()").is_err());
    assert!(include_in("here.tmt", 0, &[], "include(tmt(\"nope.tmt\"))").is_err());
}

#[test]
fn relative_links_are_rewritten_to_still_reach_the_same_file() {
    // Host at the vault root: `docs/guide.tmt`'s `./img/a.png` is `docs/img/a.png`.
    let from_root =
        fragment(include_in("here.tmt", 0, &[], "include(tmt(\"docs/guide.tmt\"))").unwrap());
    assert_eq!(
        link_targets(&from_root),
        ["./docs/img/a.png", "./shared/s.tmt#top", "tmt:abs.tmt"]
    );
    // Host one level down from the guide.
    let from_sub = fragment(
        include_in(
            "docs/sub/host.tmt",
            0,
            &[],
            "include(tmt(\"docs/guide.tmt\"))",
        )
        .unwrap(),
    );
    assert_eq!(
        link_targets(&from_sub),
        ["../img/a.png", "../../shared/s.tmt#top", "tmt:abs.tmt"]
    );
    // Same directory: nothing to rewrite.
    let beside =
        fragment(include_in("docs/other.tmt", 0, &[], "include(tmt(\"docs/guide.tmt\"))").unwrap());
    assert_eq!(
        link_targets(&beside),
        ["./img/a.png", "../shared/s.tmt#top", "tmt:abs.tmt"]
    );
}

#[test]
fn a_document_already_being_included_is_a_cycle() {
    let chain = [
        DocRef("docs/other.tmt".into()),
        DocRef("docs/guide.tmt".into()),
    ];
    let err = include_in(
        "docs/other.tmt",
        0,
        &chain,
        "include(tmt(\"docs/guide.tmt\"))",
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("cycle"), "{message}");
    assert!(
        message.contains("docs/other.tmt -> docs/guide.tmt -> docs/guide.tmt"),
        "{message}"
    );
}

#[test]
fn includes_nested_too_deep_stop() {
    let chain: Vec<DocRef> = (0..MAX_INCLUDE_DEPTH)
        .map(|i| DocRef(format!("n{i}.tmt")))
        .collect();
    let err = include_in("here.tmt", 0, &chain, "include(tmt(\"docs/guide.tmt\"))").unwrap_err();
    assert!(err.to_string().contains("nested"), "{err}");
}

// ---- a list that carries an id -----------------------------------------------

/// A document whose list has a unique id.
///
/// No syntax writes one yet -- a trailing `#(id)` belongs to the last item --
/// so the id is set on the tree. The list is addressed by the rules the
/// element and the section already follow, which is what these pin down.
fn list_vault() -> MemoryResolver {
    let mut doc = tomet_parser::parse_document("Lead.\n\n- one #(i1)\n- two\n- three\n").unwrap();
    let mut named = 0;
    for block in &mut doc.blocks {
        if let tomet_ast::Block::List(list) = block {
            list.id = Some(tomet_ast::Id("shopping".to_string()));
            named += 1;
        }
    }
    assert_eq!(named, 1, "the document has one list");
    MemoryResolver::new().with_document("here.tmt", std::sync::Arc::new(doc))
}

fn in_list_vault(src: &str) -> Result<EvalValue, EvalError> {
    let resolver = list_vault();
    let macros = HashMap::new();
    let vars = EvalValue::Null;
    let cx = EvalContext {
        phase: Phase::Output,
        resolver: &resolver,
        generate: &Fixed,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        var: &vars,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    evaluate(&parse(src), &cx)
}

#[test]
fn a_list_is_found_by_its_id_like_an_element() {
    assert_eq!(in_list_vault("#shopping.id").unwrap(), text("shopping"));
    assert_eq!(in_list_vault("#shopping.sigil").unwrap(), text("list"));
    assert_eq!(in_list_vault("#shopping.connects").unwrap(), seq(vec![]));
    assert_eq!(in_list_vault("#shopping.prop").unwrap(), EvalValue::Null);
    // An item still has its own id, and the list's does not shadow it.
    assert_eq!(in_list_vault("#i1.id").unwrap(), text("i1"));
    assert!(in_list_vault("#nope.id").is_err());
}

#[test]
fn item_on_a_list_counts_its_items() {
    assert_eq!(in_list_vault("#shopping.item(0).id").unwrap(), text("i1"));
    assert_eq!(
        in_list_vault("#shopping.item(1).id").unwrap(),
        EvalValue::Null
    );
    assert!(in_list_vault("#shopping.item(3)").is_err());
}

/// The list is a node wherever a block is counted -- at the root of the
/// document, as the section already is -- so it can be asked for its id.
#[test]
fn a_list_among_the_blocks_is_a_node() {
    assert_eq!(in_list_vault("#item(1).id").unwrap(), text("shopping"));
    assert!(matches!(
        in_list_vault("#item(0)").unwrap(),
        EvalValue::Fragment(_)
    ));
}

#[test]
fn a_list_with_an_id_can_be_linked_and_included() {
    assert!(matches!(
        in_list_vault("#shopping.link()").unwrap(),
        EvalValue::Fragment(_)
    ));
    let included = match in_list_vault("include(#shopping)").unwrap() {
        EvalValue::Fragment(blocks) => blocks,
        other => panic!("expected a fragment, got {other:?}"),
    };
    assert!(
        matches!(included.as_slice(), [tomet_ast::Block::List(list)] if list.items.len() == 3),
        "{included:?}"
    );
    // `heading:` is about a section's heading, and a list has none.
    assert!(in_list_vault("include(#shopping, heading: false)").is_err());
}

// ---- the names an editor offers ----------------------------------------------

/// Every name in [`FUNCTIONS`] is one the evaluator knows. Called with no
/// arguments it fails for some reason of its own, never for not existing.
#[test]
fn every_listed_function_is_one_the_evaluator_answers() {
    for (name, detail) in FUNCTIONS {
        assert!(!detail.is_empty(), "{name} has no description");
        if *name == "addr" {
            // Read by the parser: `addr()` is a syntax error, not a call.
            continue;
        }
        if let Err(EvalError::UnknownFunction(unknown)) = in_vault(&format!("{name}()")) {
            panic!("`{unknown}` is listed in FUNCTIONS but the evaluator does not know it");
        }
    }
}

/// Every name in [`RESERVED_NAMES`] starts an expression the evaluator reads.
#[test]
fn every_reserved_name_is_one_the_evaluator_answers() {
    for (name, detail) in RESERVED_NAMES {
        assert!(!detail.is_empty(), "{name} has no description");
        if let Err(EvalError::UnknownName(unknown)) = in_vault(name) {
            panic!("`{unknown}` is listed in RESERVED_NAMES but the evaluator does not know it");
        }
    }
}

#[test]
fn no_function_is_listed_twice() {
    let mut names: Vec<&str> = FUNCTIONS.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len());
}
