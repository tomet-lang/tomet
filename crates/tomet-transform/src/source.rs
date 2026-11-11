//! Where the values a blueprint makes up come from: the clock and the random source.
//!
//! A type seat with `default: auto` (`<$datetime(default: auto)>`, `<$uuid(default: auto)>`) is
//! filled with the moment the document is made, or with an id drawn for it. The evaluator reads no
//! clock and draws nothing, so filling the seats is the one place that does, and it asks a
//! [`Source`] for both. Nothing else in blueprint instantiation reads the clock or draws.
//!
//! The trait is there so that a caller can fix both: a document made from a blueprint is then the
//! same every time, which is what a test needs. [`SystemSource`] is the real ones, and
//! [`FixedSource`] a stopped clock with the ids it was given.

use std::cell::RefCell;
use std::collections::VecDeque;

use time::{OffsetDateTime, UtcOffset};
use tomet_id::IdSpec;

/// The clock and the random source a blueprint's `default: auto` is made from.
pub trait Source {
    /// The moment it is, written in the offset the document is to carry: a `datetime` holds it
    /// with that offset, and a `date` holds the day it falls on there.
    fn now(&self) -> OffsetDateTime;

    /// A new id of the shape `spec` gives. Every call is a different id for a real source; a
    /// source that does not draw (a [`FixedSource`]) says what it answers.
    fn id(&self, spec: &IdSpec) -> String;
}

/// The real clock, written in one offset, and real random ids ([`IdSpec::generate`]).
///
/// The vault's settings say which offset a field wants (`offset: "+09:00"`), so a document made in
/// Tokyo says `+09:00` and not UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemSource {
    offset: UtcOffset,
}

impl SystemSource {
    /// The real clock in UTC.
    pub fn utc() -> Self {
        Self {
            offset: UtcOffset::UTC,
        }
    }

    /// The real clock in the offset written `+09:00`, `-05:30` or `Z`; `None` when the text is
    /// not an offset.
    pub fn in_offset(text: &str) -> Option<Self> {
        tomet_expr::parse_offset(text).map(|offset| Self { offset })
    }
}

impl Source for SystemSource {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc().to_offset(self.offset)
    }

    fn id(&self, spec: &IdSpec) -> String {
        spec.generate()
    }
}

/// A clock stopped at one moment, and the ids it is given to answer with, in order.
///
/// For a test, or for a run that has to come out the same every time. It draws nothing: asked for
/// an id, it answers the next one it was given, whatever shape is asked for -- so a test gives
/// the ids that fit the seats it writes -- and **panics** when it has none left, which is a
/// test asking for more ids than it set out.
#[derive(Debug, Clone)]
pub struct FixedSource {
    moment: OffsetDateTime,
    ids: RefCell<VecDeque<String>>,
}

impl FixedSource {
    /// A clock stopped at `moment`, written as a `datetime` is (`2026-11-11T09:30:00+09:00`); a
    /// text without an offset is UTC. `None` when the text is not a date or a datetime.
    pub fn at(moment: &str) -> Option<Self> {
        Some(Self {
            moment: tomet_expr::parse_datetime(moment)?,
            ids: RefCell::default(),
        })
    }

    /// The same source, answering with `ids`, in order, for the ids that are asked of it.
    pub fn with_ids<I, S>(self, ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            ids: RefCell::new(ids.into_iter().map(Into::into).collect()),
            ..self
        }
    }
}

impl Source for FixedSource {
    fn now(&self) -> OffsetDateTime {
        self.moment
    }

    fn id(&self, _spec: &IdSpec) -> String {
        self.ids
            .borrow_mut()
            .pop_front()
            .expect("a fixed source was asked for more ids than it was given")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_type::{Arg, Datum, Type};

    fn id_form(name: &str) -> IdSpec {
        IdSpec::new(&Type::build(name, &[], &[]).expect("a type")).expect("an id form")
    }

    #[test]
    fn the_real_clock_is_written_in_the_offset_asked_for() {
        assert_eq!(SystemSource::utc().now().offset(), UtcOffset::UTC);
        for (text, hours, minutes) in [("+09:00", 9, 0), ("-05:30", -5, -30), ("Z", 0, 0)] {
            let offset = SystemSource::in_offset(text)
                .expect("an offset")
                .now()
                .offset();
            assert_eq!(offset.whole_hours(), hours, "{text}");
            assert_eq!(offset.minutes_past_hour(), minutes, "{text}");
        }
        assert_eq!(SystemSource::in_offset("tomorrow"), None);
        assert_eq!(SystemSource::in_offset("+25:00"), None);
    }

    #[test]
    fn real_ids_have_the_shape_asked_for() {
        let named = |prefix: &str| {
            Type::build(
                "nanoid",
                &[],
                &[
                    ("length", Arg::Datum(Datum::Int(6))),
                    ("prefix", Arg::Datum(Datum::Text(prefix.to_string()))),
                ],
            )
            .expect("a type")
        };
        let ty = named("n-");
        let spec = IdSpec::new(&ty).expect("an id form");
        let id = SystemSource::utc().id(&spec);
        assert!(spec.accepts(&id), "{id}");
        assert!(id.starts_with("n-"));
        let uuid = id_form("uuid");
        assert!(uuid.accepts(&SystemSource::utc().id(&uuid)));
    }

    #[test]
    fn a_fixed_source_stands_still_and_answers_the_ids_it_was_given_in_order() {
        let source = FixedSource::at("2026-11-11T09:30:00+09:00")
            .expect("a datetime")
            .with_ids(["first", "second"]);
        assert_eq!(source.now(), source.now());
        assert_eq!(source.now().offset().whole_hours(), 9);
        assert_eq!(source.id(&id_form("uuid")), "first");
        assert_eq!(source.id(&id_form("nanoid")), "second");
        assert!(FixedSource::at("sometime").is_none());
        assert_eq!(
            FixedSource::at("2026-11-11")
                .expect("a date")
                .now()
                .offset(),
            UtcOffset::UTC
        );
    }

    #[test]
    #[should_panic(expected = "more ids than it was given")]
    fn a_fixed_source_asked_for_more_ids_than_it_has_says_so() {
        let source = FixedSource::at("2026-11-11").expect("a date");
        source.id(&id_form("uuid"));
    }
}
