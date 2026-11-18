//! Filling the type seats of a blueprint: what a document made from it holds where it has one.
//!
//! A type seat (`<$uuid(default: auto)>`, `<$int(default: 3)>`) says what goes where it stands,
//! and where the value comes from. A document made from a blueprint has no seat left, so every
//! one is filled from its source, and a seat that has none stops the making:
//!
//! - `default: auto` is made up when the document is: an id drawn for the type
//!   (`uuid`, `nanoid`), or the moment (`datetime`, `date`), both from the [`Source`];
//! - a `default` written out is that value;
//! - a seat with no `default` has no value source.
//!
//! The value takes the place the seat is in: as a value (`::meta(id: <$uuid(default: auto)>)`) it
//! is the value itself, and in running text or as a block of its own it is its text.
//!
//! =[ Everything or nothing ]
//!
//! The seats are all read and every one that cannot be filled is reported ([`SeatProblem`]) before
//! the first is replaced, so a blueprint that cannot be made into a document is left as it was and
//! says everything that is wrong with it at once.

use std::cell::OnceCell;
use std::fmt;

use time::OffsetDateTime;
use tomet_ast::{File, TypeSeat, Value};
use tomet_expr::{EvalContext, EvalError, evaluate_type};
use tomet_id::IdSpec;
use tomet_scalar::Scalar;
use tomet_semantics_core::{Mismatch, admits_default, datum_value};
use tomet_span::Span;
use tomet_tree::{Node, fill_type_seats_in_file, for_each_node, for_each_type_seat};
use tomet_type::{AutoFill, Datum, Kind, Type};

use crate::source::Source;

/// A type seat that cannot be filled, and where it is in the blueprint.
#[derive(Debug, Clone, PartialEq)]
pub struct SeatProblem {
    /// The seat as written: `<$uuid(default: auto)>`.
    pub seat: String,
    /// Where it is in the document that was passed in.
    pub span: Span,
    /// Why it cannot be filled.
    pub reason: SeatReason,
}

/// Why a type seat cannot be filled.
#[derive(Debug, Clone, PartialEq)]
pub enum SeatReason {
    /// What is written in the seat is not a type, or is not a well formed one: an unknown name,
    /// an argument the type does not take, `default: auto` for a type that makes no value up.
    NotAType(EvalError),
    /// The seat's `default`, written out, is not of the seat's own type.
    DefaultNotOfType(Mismatch),
    /// `default: auto` of a type that is no form an id can have, so there is no id to draw. A
    /// `uuid` and a `nanoid` always have one.
    NoIdForm { ty: String },
    /// The seat has no `default`, and nothing else says what goes in it.
    NoValueSource,
    /// The value is one that has no place in running text -- a list -- where the seat stands in
    /// running text.
    NoPlaceInText { found: &'static str },
    /// The seat was still in the document after it was filled.
    Left,
}

impl fmt::Display for SeatReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeatReason::NotAType(error) => write!(f, "{error}"),
            SeatReason::DefaultNotOfType(mismatch) => write!(f, "`default`: {mismatch}"),
            SeatReason::NoIdForm { ty } => write!(
                f,
                "`{ty}` is not the type of an id, so `default: auto` has none to draw"
            ),
            SeatReason::NoValueSource => write!(
                f,
                "this seat has no value source: it has no `default`, so nothing says what goes in it"
            ),
            SeatReason::NoPlaceInText { found } => write!(
                f,
                "its value is {found}, which has no place in running text; \
                 write the seat where a value goes, such as `::meta(key: <$...>)`"
            ),
            SeatReason::Left => write!(f, "the seat is still in the document after it was filled"),
        }
    }
}

/// One line, without where the seat is: the seat as written, then why it cannot be filled.
impl fmt::Display for SeatProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "type seat `{}`: {}", self.seat, self.reason)
    }
}

impl std::error::Error for SeatProblem {}

/// Fills every type seat of `doc` from its source, or leaves `doc` as it was and returns every
/// seat that has none (see the module documentation).
///
/// `cx` is what the written types are evaluated in, and `source` is the one place the clock and the
/// random source are read: the moment is read once, so every seat of a document agrees on it.
pub(crate) fn fill_seats(
    doc: &mut File,
    cx: &EvalContext,
    source: &dyn Source,
) -> Result<usize, Vec<SeatProblem>> {
    let filler = Filler {
        cx,
        source,
        moment: OnceCell::new(),
    };
    let mut values: Vec<(Span, Value)> = Vec::new();
    let mut problems: Vec<SeatProblem> = Vec::new();
    for_each_node(doc, |node| {
        let Some(seat) = node.type_seat() else { return };
        let in_text = matches!(node, Node::Block(_) | Node::Inline(_));
        match filler.value_of(seat, in_text) {
            Ok(value) => values.push((seat.span, value)),
            Err(reason) => problems.push(problem(seat, reason)),
        }
    });
    if !problems.is_empty() {
        return Err(problems);
    }

    let filled = values.len();
    let mut values = values.into_iter();
    fill_type_seats_in_file(doc, |seat| match values.next() {
        Some((span, value)) if span.exact_eq(&seat.span) => Some(value),
        _ => None,
    });
    Ok(filled)
}

/// Every type seat still in `doc`, as a problem each. A document made from a blueprint has none,
/// so any that is left means the filling missed it.
pub(crate) fn seats_left(doc: &File) -> Vec<SeatProblem> {
    let mut left = Vec::new();
    for_each_type_seat(doc, |seat| left.push(problem(seat, SeatReason::Left)));
    left
}

fn problem(seat: &TypeSeat, reason: SeatReason) -> SeatProblem {
    SeatProblem {
        seat: seat.to_string(),
        span: seat.span,
        reason,
    }
}

struct Filler<'a> {
    cx: &'a EvalContext<'a>,
    source: &'a dyn Source,
    /// The moment, read the first time a seat asks for it.
    moment: OnceCell<OffsetDateTime>,
}

impl Filler<'_> {
    /// The value `seat` is filled with. `in_text` is whether it stands in running text or as a
    /// block of its own, where only a value with a text can go.
    fn value_of(&self, seat: &TypeSeat, in_text: bool) -> Result<Value, SeatReason> {
        let ty = evaluate_type(&seat.expr, self.cx).map_err(SeatReason::NotAType)?;
        let value = self.source_of(&ty)?;
        if in_text && value.as_scalar().and_then(|s| s.text()).is_none() {
            return Err(SeatReason::NoPlaceInText {
                found: describe(&value),
            });
        }
        Ok(value)
    }

    /// What the type's `default` says goes in the seat.
    fn source_of(&self, ty: &Type) -> Result<Value, SeatReason> {
        match (ty.auto(), ty.default()) {
            (Some(AutoFill::Id), _) => {
                let spec =
                    IdSpec::new(ty).ok_or_else(|| SeatReason::NoIdForm { ty: ty.to_string() })?;
                Ok(Value::Scalar(Scalar::Text(self.source.id(&spec))))
            }
            (Some(AutoFill::Now), _) => {
                let moment = self.moment.get_or_init(|| self.source.now());
                Ok(Value::Scalar(Scalar::Text(match ty.kind() {
                    Kind::Date => date_text(moment),
                    _ => datetime_text(moment),
                })))
            }
            (None, Some(datum)) if *datum != Datum::Scalar(Scalar::Auto) => {
                admits_default(ty).map_err(SeatReason::DefaultNotOfType)?;
                Ok(datum_value(datum))
            }
            (None, _) => Err(SeatReason::NoValueSource),
        }
    }
}

/// What a value is, for a sentence about where it cannot go.
fn describe(value: &Value) -> &'static str {
    match value {
        Value::Tuple(tuple) => {
            if tuple.is_empty() {
                "an empty tuple"
            } else if tuple.has_no_named() {
                "a list"
            } else if tuple.has_no_unnamed() {
                "a map"
            } else {
                "a list with named entries"
            }
        }
        _ => "a value",
    }
}

/// The moment as a `datetime` holds it: RFC 3339, in the moment's own offset, `Z` for UTC.
fn datetime_text(moment: &OffsetDateTime) -> String {
    let offset = moment.offset();
    let offset = if offset.is_utc() {
        "Z".to_string()
    } else {
        let minutes = offset.whole_minutes();
        let sign = if minutes < 0 { '-' } else { '+' };
        format!("{sign}{:02}:{:02}", minutes.abs() / 60, minutes.abs() % 60)
    };
    format!(
        "{}{offset}",
        tomet_expr::format_datetime(moment, "YYYY-MM-DDTHH:mm:ss")
    )
}

/// The day the moment falls on, in its own offset, as a `date` holds it.
fn date_text(moment: &OffsetDateTime) -> String {
    tomet_expr::format_datetime(moment, "YYYY-MM-DD")
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::HashMap;

    use tomet_ast::{Block, Inline, Tuple, TupleEntry};
    use tomet_expr::{DocRef, EvalValue, NoResolver};

    use super::*;
    use crate::{Evaluator, FixedSource, Instantiated, instantiate_blueprint_with};

    struct StubEvaluator;
    impl Evaluator for StubEvaluator {
        fn evaluate(
            &self,
            _expr: &tomet_ast::InterpExpr,
            _cx: &EvalContext,
        ) -> Result<EvalValue, EvalError> {
            Err(EvalError::message("stub evaluator"))
        }
        fn evaluate_data(
            &self,
            _expr: &tomet_ast::InterpExpr,
            _cx: &EvalContext,
        ) -> Result<tomet_ast::Value, EvalError> {
            Err(EvalError::message("stub evaluator"))
        }
    }

    const NOW: &str = "2026-11-11T09:30:00+09:00";
    const UUID: &str = "00000000-0000-0000-0000-000000000000";

    fn parse(src: &str) -> File {
        tomet_ast_parser::parse_file(src).expect("valid source")
    }

    /// The blueprint `src` made into a document, with `vars` for `var`.
    fn made(
        src: &str,
        vars: EvalValue,
        source: &dyn Source,
    ) -> Result<(File, Instantiated), Vec<SeatProblem>> {
        let mut doc = parse(src);
        let macros = HashMap::new();
        let cx = EvalContext {
            resolver: &NoResolver,
            self_doc: DocRef("notes/plan.tmt".into()),
            macros: &macros,
            var: &vars,
            var_unset: &[],
            includes: &[],
            heading_level: 0,
            notes: Default::default(),
        };
        let report = instantiate_blueprint_with(&mut doc, &cx, source, &StubEvaluator)?;
        Ok((doc, report))
    }

    fn fixed(ids: &[&str]) -> FixedSource {
        FixedSource::at(NOW)
            .expect("a datetime")
            .with_ids(ids.iter().copied())
    }

    /// The `::meta` of the document, as the entries it holds.
    fn meta(doc: &File) -> Vec<(String, Value)> {
        let Some(tuple) = doc
            .header
            .iter()
            .find(|d| d.name.to_string() == "meta")
            .and_then(|d| d.data.args.clone())
        else {
            panic!("expected a ::meta with entries in {:?}", doc.header);
        };
        tuple.into_named().collect()
    }

    fn field(doc: &File, key: &str) -> Value {
        meta(doc)
            .into_iter()
            .find(|(k, _)| k == key)
            .unwrap_or_else(|| panic!("no `{key}` in the meta"))
            .1
    }

    fn text(value: &str) -> Value {
        Value::Scalar(Scalar::Text(value.to_string()))
    }

    fn problems(src: &str) -> Vec<SeatProblem> {
        made(
            src,
            EvalValue::Scalar(Scalar::None),
            &fixed(&[UUID, UUID, UUID]),
        )
        .expect_err("a stop")
    }

    #[test]
    fn a_seat_with_default_auto_is_filled_with_an_id_drawn_for_its_type() {
        let (doc, report) = made(
            "::blueprint(note)\n::id(<$uuid(default: auto)>)\n::meta(\n  short: <$nanoid(length: 8, prefix: \"note-\", default: auto)>\n)\n",
            EvalValue::Scalar(Scalar::None),
            &fixed(&[UUID, "note-ab12cd34"]),
        )
        .unwrap();
        assert!(report.changed);
        assert_eq!(field(&doc, "short"), text("note-ab12cd34"));
        let id = doc
            .header
            .iter()
            .find(|d| d.name.to_string() == "id")
            .unwrap();
        assert_eq!(id.data.args, Some(Tuple::from_unnamed([text(UUID)])));
    }

    #[test]
    fn a_seat_asks_the_source_for_the_shape_of_its_own_type() {
        struct Asking(Cell<usize>);
        impl Source for Asking {
            fn now(&self) -> OffsetDateTime {
                OffsetDateTime::UNIX_EPOCH
            }
            fn id(&self, spec: &IdSpec) -> String {
                self.0.set(self.0.get() + 1);
                // The id of the shape asked for: a nanoid of 6 after its prefix.
                assert!(!spec.accepts(UUID));
                format!("n-{}", "x".repeat(6))
            }
        }
        let asking = Asking(Cell::new(0));
        let (doc, _) = made(
            "::blueprint(note)\n::meta(a: <$nanoid(length: 6, prefix: \"n-\", default: auto)>)\n",
            EvalValue::Scalar(Scalar::None),
            &asking,
        )
        .unwrap();
        assert_eq!(asking.0.get(), 1);
        assert_eq!(field(&doc, "a"), text("n-xxxxxx"));
    }

    #[test]
    fn the_moment_is_a_datetime_in_the_offset_and_a_date_is_the_day_there() {
        let (doc, _) = made(
            "::blueprint(note)\n::meta(\n  created: <$datetime(default: auto)>\n  day: <$date(default: auto)>\n)\n",
            EvalValue::Scalar(Scalar::None),
            &fixed(&[]),
        )
        .unwrap();
        assert_eq!(field(&doc, "created"), text("2026-11-11T09:30:00+09:00"));
        assert_eq!(field(&doc, "day"), text("2026-11-11"));

        // The day is the one the moment falls on in its own offset, not in UTC.
        let late = FixedSource::at("2026-11-11T23:30:00-05:00").unwrap();
        let (doc, _) = made(
            "::blueprint(note)\n::meta(created: <$datetime(default: auto)>, day: <$date(default: auto)>)\n",
            EvalValue::Scalar(Scalar::None),
            &late,
        )
        .unwrap();
        assert_eq!(field(&doc, "created"), text("2026-11-11T23:30:00-05:00"));
        assert_eq!(field(&doc, "day"), text("2026-11-11"));

        // UTC is written `Z`.
        let utc = FixedSource::at("2026-01-05T07:08:09Z").unwrap();
        let (doc, _) = made(
            "::blueprint(note)\n::meta(created: <$datetime(default: auto)>)\n",
            EvalValue::Scalar(Scalar::None),
            &utc,
        )
        .unwrap();
        assert_eq!(field(&doc, "created"), text("2026-01-05T07:08:09Z"));
    }

    #[test]
    fn the_moment_is_read_once_so_the_seats_of_a_document_agree() {
        struct Counting(Cell<usize>);
        impl Source for Counting {
            fn now(&self) -> OffsetDateTime {
                self.0.set(self.0.get() + 1);
                OffsetDateTime::UNIX_EPOCH
            }
            fn id(&self, spec: &IdSpec) -> String {
                spec.generate()
            }
        }
        let counting = Counting(Cell::new(0));
        let (doc, _) = made(
            "::blueprint(note)\n::meta(a: <$datetime(default: auto)>, b: <$date(default: auto)>, c: <$datetime(default: auto)>)\n",
            EvalValue::Scalar(Scalar::None),
            &counting,
        )
        .unwrap();
        assert_eq!(counting.0.get(), 1);
        assert_eq!(field(&doc, "a"), field(&doc, "c"));
        assert_eq!(field(&doc, "a"), text("1970-01-01T00:00:00Z"));
        assert_eq!(field(&doc, "b"), text("1970-01-01"));
    }

    #[test]
    fn a_default_written_out_is_the_value_in_its_own_kind() {
        let (doc, _) = made(
            "::blueprint(note)\n::meta(\n  count: <$int(default: 3)>\n  done: <$bool(default: false)>\n  title: <$string(default: \"Untitled\")>\n  ratio: <$float(default: 1.5)>\n)\n",
            EvalValue::Scalar(Scalar::None),
            &fixed(&[]),
        )
        .unwrap();
        assert_eq!(field(&doc, "count"), Value::Scalar(Scalar::Int(3)));
        assert_eq!(field(&doc, "done"), Value::Scalar(Scalar::Bool(false)));
        assert_eq!(field(&doc, "title"), text("Untitled"));
        assert_eq!(field(&doc, "ratio"), Value::Scalar(Scalar::Float(1.5)));
    }

    #[test]
    fn a_list_default_is_a_list_where_a_value_goes() {
        // A list is not written in an expression; one a variable holds is a plain value.
        let vars = EvalValue::Tuple(
            Tuple::try_from_entries([TupleEntry::named(
                "tags".to_string(),
                EvalValue::Tuple(Tuple::from_unnamed([
                    EvalValue::Scalar(Scalar::Text("a".into())),
                    EvalValue::Scalar(Scalar::Text("b".into())),
                ])),
            )])
            .unwrap(),
        );
        let (doc, _) = made(
            "::blueprint(note)\n::meta(tags: <$list(string, length: 2, default: var.tags)>)\n",
            vars,
            &fixed(&[]),
        )
        .unwrap();
        assert_eq!(
            field(&doc, "tags"),
            Value::Tuple(Tuple::from_unnamed([text("a"), text("b")]))
        );
    }

    #[test]
    fn a_seat_in_prose_a_heading_and_on_a_line_of_its_own_is_its_text() {
        let (doc, _) = made(
            "::blueprint(note)\n\n=[ Notes of <$date(default: auto)> ]#(notes)\n\nWritten on <$date(default: auto)>, build <$int(default: 7)>.\n\n<$string(default: \"Fin\")>\n",
            EvalValue::Scalar(Scalar::None),
            &fixed(&[]),
        )
        .unwrap();
        let Block::Section(section) = &doc.body[0] else {
            panic!("expected a section, got {:?}", doc.body);
        };
        let title: String = section
            .title
            .iter()
            .map(|i| match i {
                Inline::Text(t) => t.value.clone(),
                other => panic!("expected text, got {other:?}"),
            })
            .collect();
        assert_eq!(title, "Notes of 2026-11-11");
        let paragraphs: Vec<String> = section
            .blocks
            .iter()
            .map(|b| match b {
                Block::Paragraph(p) => p
                    .content
                    .iter()
                    .map(|i| match i {
                        Inline::Text(t) => t.value.clone(),
                        other => panic!("expected text, got {other:?}"),
                    })
                    .collect(),
                other => panic!("expected a paragraph, got {other:?}"),
            })
            .collect();
        assert_eq!(paragraphs, ["Written on 2026-11-11, build 7.", "Fin"]);
    }

    #[test]
    fn a_seat_is_filled_wherever_it_sits_and_none_is_left() {
        let (doc, _) = made(
            "::blueprint(note)\n::meta(a: <$int(default: 1)>)\n::id(<$uuid(default: auto)>)\n\n=[ <$string(default: \"h\")> ]\n@card(n: <$int(default: 2)>)[ <$string(default: \"in\")> ]\n- item <$int(default: 3)>\n\n<$string(default: \"end\")>\n",
            EvalValue::Scalar(Scalar::None),
            &fixed(&[UUID]),
        )
        .unwrap();
        let mut left = Vec::new();
        tomet_tree::for_each_type_seat(&doc, |seat| left.push(seat.to_string()));
        assert!(left.is_empty(), "{left:?}");
        assert!(seats_left(&doc).is_empty());
    }

    #[test]
    fn a_seat_with_no_value_source_stops_the_making_and_says_where() {
        let src = "::blueprint(note)\n::meta(title: <$string>)\n";
        let found = problems(src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].seat, "<$string>");
        assert_eq!(found[0].reason, SeatReason::NoValueSource);
        assert!(src[found[0].span.start..].starts_with("<$string>"));
        assert_eq!(
            found[0].to_string(),
            "type seat `<$string>`: this seat has no value source: it has no `default`, \
             so nothing says what goes in it"
        );
    }

    #[test]
    fn every_seat_that_cannot_be_filled_is_reported_not_the_first() {
        let found = problems(
            "::blueprint(note)\n::meta(\n  a: <$string>\n  b: <$person>\n  c: <$int(default: \"three\")>\n  d: <$int(default: auto)>\n  e: <$uuid(default: auto)>\n)\n",
        );
        let reasons: Vec<&str> = found
            .iter()
            .map(|p| match &p.reason {
                SeatReason::NoValueSource => "no source",
                SeatReason::NotAType(_) => "not a type",
                SeatReason::DefaultNotOfType(_) => "default not of type",
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            reasons,
            [
                "no source",
                "not a type",
                "default not of type",
                "not a type"
            ]
        );
        assert!(
            found[1]
                .to_string()
                .contains("`person` is not a type; the types are")
        );
        assert_eq!(
            found[2].to_string(),
            "type seat `<$int(default: \"three\")>`: `default`: the text \"three\" is not an integer"
        );
        assert!(
            found[3]
                .to_string()
                .contains("`int` has no `default: auto`")
        );
    }

    #[test]
    fn a_list_has_no_place_in_running_text() {
        let vars = EvalValue::Tuple(
            Tuple::try_from_entries([TupleEntry::named(
                "tags".to_string(),
                EvalValue::Tuple(Tuple::from_unnamed([EvalValue::Scalar(Scalar::Text(
                    "a".into(),
                ))])),
            )])
            .unwrap(),
        );
        for src in [
            "::blueprint(note)\n\nTags: <$list(string, default: var.tags)>\n",
            "::blueprint(note)\n\n<$list(string, default: var.tags)>\n",
            "::blueprint(note)\n\n=[ <$list(string, default: var.tags)> ]\n",
        ] {
            let found = made(src, vars.clone(), &fixed(&[])).expect_err("a stop");
            assert_eq!(found.len(), 1, "{src}: {found:?}");
            assert_eq!(
                found[0].reason,
                SeatReason::NoPlaceInText { found: "a list" }
            );
            assert!(
                found[0]
                    .to_string()
                    .contains("a list, which has no place in running text"),
                "{}",
                found[0]
            );
        }
        // Where a value goes it is the value.
        assert!(
            made(
                "::blueprint(note)\n::meta(tags: <$list(string, default: var.tags)>)\n",
                vars,
                &fixed(&[])
            )
            .is_ok()
        );
    }

    #[test]
    fn a_blueprint_that_cannot_be_made_is_left_as_it_was() {
        let src = "::kind(blueprint)\n::blueprint(note)\n::meta(\n  id: <$uuid(default: auto)>\n  title: <$string>\n)\n\n=[ Day <$date(default: auto)> ]\n";
        let before = parse(src);
        let mut doc = parse(src);
        let macros = HashMap::new();
        let cx = EvalContext {
            resolver: &NoResolver,
            self_doc: DocRef("a.tmt".into()),
            macros: &macros,
            var: &EvalValue::Scalar(Scalar::None),
            var_unset: &[],
            includes: &[],
            heading_level: 0,
            notes: Default::default(),
        };
        let found = instantiate_blueprint_with(&mut doc, &cx, &fixed(&[UUID]), &StubEvaluator)
            .unwrap_err();
        assert_eq!(found.len(), 1);
        assert_eq!(doc, before);
    }



    #[test]
    fn a_seat_does_not_touch_the_expressions_around_it() {
        let (doc, report) = made(
            "::blueprint(note)\n::meta(by: $(var.author), day: <$date(default: auto)>)\n",
            EvalValue::Tuple(Tuple::new()),
            &fixed(&[]),
        )
        .unwrap();
        // The expression with no value is left and reported, as it always was; the seat is filled.
        assert_eq!(report.unresolved.len(), 1, "{:?}", report.unresolved);
        assert!(matches!(field(&doc, "by"), Value::Interp(_)));
        assert_eq!(field(&doc, "day"), text("2026-11-11"));
    }

    #[test]
    fn a_blueprint_with_no_seat_is_made_as_before() {
        let (doc, report) = made(
            "::blueprint(note)\n::meta(a: 1)\n",
            EvalValue::Scalar(Scalar::None),
            &fixed(&[]),
        )
        .unwrap();
        assert!(report.changed);
        assert_eq!(field(&doc, "a"), Value::Scalar(Scalar::Int(1)));
    }

    #[test]
    fn the_reasons_read_as_one_line_each() {
        let mismatch = tomet_semantics_core::admits(
            &"int".parse().unwrap(),
            &Value::Scalar(Scalar::Text("x".into())),
        )
        .unwrap_err();
        for (reason, said) in [
            (
                SeatReason::NotAType(EvalError::BadType(tomet_type::TypeError::Unknown(
                    "person".into(),
                ))),
                "`person` is not a type; the types are",
            ),
            (
                SeatReason::DefaultNotOfType(mismatch),
                "`default`: the text \"x\" is not an integer",
            ),
            (
                SeatReason::NoIdForm { ty: "int".into() },
                "`int` is not the type of an id, so `default: auto` has none to draw",
            ),
            (SeatReason::NoValueSource, "this seat has no value source"),
            (
                SeatReason::NoPlaceInText { found: "a list" },
                "its value is a list, which has no place in running text",
            ),
            (
                SeatReason::Left,
                "the seat is still in the document after it was filled",
            ),
        ] {
            let line = SeatProblem {
                seat: "<$x>".into(),
                span: Span::dummy(),
                reason,
            }
            .to_string();
            assert!(line.starts_with("type seat `<$x>`: "), "{line}");
            assert!(line.contains(said), "{line}");
            assert!(!line.contains('\n'), "{line}");
        }
    }
}
