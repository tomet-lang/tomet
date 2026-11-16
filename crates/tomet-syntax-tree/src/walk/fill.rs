//! Filling what is not there yet: the interpolations and type seats of a value, replaced where they sit.

use super::mutate::{MutWalk, Sink, walk_mut};
use std::marker::PhantomData;
use tomet_ast::{Block, File, Inline, Interpolation, Paragraph, Text, TypeSeat, Value};
use tomet_span::Span;

/// A node that stands where a value, a block or running text does, and is replaced, where it
/// stands, by a value that fills it: an [`Interpolation`] and a [`TypeSeat`]. What the value
/// becomes depends on the place, which is what [`Filling`] knows and its two callers do not.
trait Fillable: Sized {
    fn inline(inline: &Inline) -> Option<&Self>;
    fn block(block: &Block) -> Option<&Self>;
    fn value(value: &Value) -> Option<&Self>;
    fn span(&self) -> Span;
}

impl Fillable for Interpolation {
    fn inline(inline: &Inline) -> Option<&Self> {
        match inline {
            Inline::Interp(i) => Some(i),
            _ => None,
        }
    }
    fn block(block: &Block) -> Option<&Self> {
        match block {
            Block::Interp(i) => Some(i),
            _ => None,
        }
    }
    fn value(value: &Value) -> Option<&Self> {
        match value {
            Value::Interp(i) => Some(i),
            _ => None,
        }
    }
    fn span(&self) -> Span {
        self.span
    }
}

impl Fillable for TypeSeat {
    fn inline(inline: &Inline) -> Option<&Self> {
        match inline {
            Inline::TypeSeat(t) => Some(t),
            _ => None,
        }
    }
    fn block(block: &Block) -> Option<&Self> {
        match block {
            Block::TypeSeat(t) => Some(t),
            _ => None,
        }
    }
    fn value(value: &Value) -> Option<&Self> {
        match value {
            Value::TypeSeat(t) => Some(t),
            _ => None,
        }
    }
    fn span(&self) -> Span {
        self.span
    }
}

/// The sink of [`fill_interpolations`] and [`fill_type_seats`]: what `fill` answers for a node of
/// kind `T`, as the place the node stands in takes it.
struct Filling<T, F> {
    fill: F,
    node: PhantomData<fn(&T)>,
}

impl<T: Fillable, F: FnMut(&T) -> Option<Value>> Filling<T, F> {
    /// What the node becomes in running text: the scalar text of its value, if it has any.
    fn text(&mut self, node: &T) -> Option<Text> {
        let filled = (self.fill)(node)?;
        let text = filled.as_scalar()?.text()?.into_owned();
        Some(Text::new(text, node.span()))
    }
}

impl<T: Fillable, F: FnMut(&T) -> Option<Value>> Sink for Filling<T, F> {
    fn inline(&mut self, inline: &mut Inline) {
        if let Some(node) = T::inline(inline)
            && let Some(text) = self.text(node)
        {
            *inline = Inline::Text(text);
        }
    }
    fn block(&mut self, block: &mut Block) {
        if let Some(node) = T::block(block)
            && let Some(text) = self.text(node)
        {
            let span = node.span();
            *block = Block::Paragraph(Paragraph::new(vec![Inline::Text(text)], span));
        }
    }
    fn value(&mut self, value: &mut Value) {
        if let Some(node) = T::value(value)
            && let Some(filled) = (self.fill)(node)
        {
            *value = filled;
        }
    }
}

fn fill_nodes<T: Fillable>(value: &mut Value, fill: impl FnMut(&T) -> Option<Value>) {
    MutWalk {
        sink: &mut Filling {
            fill,
            node: PhantomData,
        },
        connects: true,
    }
    .value(value);
}

fn fill_nodes_in_file<T: Fillable>(file: &mut File, fill: impl FnMut(&T) -> Option<Value>) {
    walk_mut(
        file,
        &mut Filling {
            fill,
            node: PhantomData,
        },
        true,
    );
}

/// Replaces, in place, each interpolation under `value` that `fill` has a value for. It is shown
/// them in the order [`for_each_interpolation_in_value`](crate::for_each_interpolation_in_value) shows them, and an interpolation it answers
/// `None` for is left as written.
///
/// What the value becomes depends on where the interpolation sits, which is the one thing this
/// function knows:
///
/// - as a value (`(args)`, an entry of `{value}`, a `[...]` value): the value itself, whatever type
///   it has;
/// - in running text: text, of the value's scalar text;
/// - as a block of its own: a paragraph of that text.
///
/// A value with no scalar text (a map, a list) has no place in text, so there the interpolation is
/// left as written too.
pub fn fill_interpolations(value: &mut Value, fill: impl FnMut(&Interpolation) -> Option<Value>) {
    fill_nodes(value, fill);
}

/// [`fill_interpolations`] for [`TypeSeat`]s: replaces, in place, each type seat under `value`
/// that `fill` has a value for, in the order [`for_each_type_seat_in_value`](crate::for_each_type_seat_in_value) shows them. A seat it
/// answers `None` for is left as written, and where the seat sits says what the value becomes: the
/// value itself as a value, its scalar text in running text and as a paragraph where the seat is
/// a block of its own, and no place in text for a value with no scalar text.
pub fn fill_type_seats(value: &mut Value, fill: impl FnMut(&TypeSeat) -> Option<Value>) {
    fill_nodes(value, fill);
}

/// [`fill_type_seats`] for a whole [`File`]: replaces, in place, each type seat of its header and
/// its body that `fill` has a value for, in the order [`for_each_type_seat`](crate::for_each_type_seat) shows them, `::connect`s
/// included. What the value becomes depends on where the seat sits, as it does there.
pub fn fill_type_seats_in_file(file: &mut File, fill: impl FnMut(&TypeSeat) -> Option<Value>) {
    fill_nodes_in_file(file, fill);
}
