//! =[ macros ]
//!
//! What a macro's template is made of: the places a call fills in.
//!
//! A template is a [`Value`]. A string is a text macro, and its places are written `$(1)` and
//! `$(name)` in the text. A `[ ... ]` is a block macro, and its places are the `$(1)` and
//! `$(name)` interpolations written in the blocks. Either way a place is not an expression: it is
//! where an argument goes, and the definition (`check_declarations`) and the call (the evaluator)
//! both ask this module what one is, so they cannot disagree.

use tomet_ast::{InterpExprKind, Interpolation, Literal, Value};
use tomet_tree::for_each_interpolation_in_value;

/// A place in a template that an argument fills: `$(1)`, the first positional argument, or
/// `$(name)`, the argument called `name`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Placeholder {
    Positional(usize),
    Named(String),
}

/// The placeholder an interpolation of a block macro's body is, or `None` if it is an expression.
pub fn placeholder_of(interp: &Interpolation) -> Option<Placeholder> {
    match &interp.expr.kind {
        InterpExprKind::Literal(Literal::Int(n)) if *n >= 1 => {
            Some(Placeholder::Positional(*n as usize))
        }
        InterpExprKind::Identifier(name) if is_ident_placeholder(name) => {
            Some(Placeholder::Named(name.clone()))
        }
        _ => None,
    }
}

/// Whether `s` can be a placeholder's name: `who`, `foo_bar`, `a-b`.
pub fn is_ident_placeholder(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// What a template takes: how many positional arguments, and which named ones.
///
/// `positional` is the highest `$(n)` written, so `$(2)` alone still takes two.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Arity {
    pub positional: usize,
    pub named: Vec<String>,
}

impl Arity {
    /// The arity of a text macro: the `$(n)` and `$(name)` written in `template`. Any other `$(...)`
    /// in it is plain text.
    pub fn of_text(template: &str) -> Self {
        let mut arity = Arity::default();
        let mut rest = template;
        while let Some(start) = rest.find("$(") {
            rest = &rest[start + 2..];
            if let Some(end) = rest.find(')') {
                let key = &rest[..end];
                if !key.is_empty() && key.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(n) = key.parse::<usize>() {
                        arity.take(&Placeholder::Positional(n));
                    }
                } else if is_ident_placeholder(key) {
                    arity.take(&Placeholder::Named(key.to_string()));
                }
                rest = &rest[end + 1..];
            }
        }
        arity
    }

    /// The arity of a block macro: the placeholders among the interpolations written anywhere in
    /// `body`, arguments of its elements included.
    pub fn of_body(body: &Value) -> Self {
        let mut arity = Arity::default();
        for_each_interpolation_in_value(body, |interp| {
            if let Some(placeholder) = placeholder_of(interp) {
                arity.take(&placeholder);
            }
        });
        arity
    }

    fn take(&mut self, placeholder: &Placeholder) {
        match placeholder {
            Placeholder::Positional(n) => self.positional = self.positional.max(*n),
            Placeholder::Named(name) => {
                if !self.named.contains(name) {
                    self.named.push(name.clone());
                }
            }
        }
    }

    /// Whether the template takes no arguments at all.
    pub fn is_empty(&self) -> bool {
        self.positional == 0 && self.named.is_empty()
    }
}

/// The interpolations written in a block macro's `body` that are not placeholders, each in its
/// canonical spelling. A block macro has none: `$(1)` and `$(name)` are where
/// arguments go, and anything else would have nothing to be evaluated against at the definition.
pub fn expressions_in_body(body: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for_each_interpolation_in_value(body, |interp| {
        if placeholder_of(interp).is_none() {
            found.push(interp.to_string());
        }
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(src: &str) -> Value {
        let file = tomet_ast_parser::parse_file(src).expect("parses");
        let Some(Value::Map(entries)) = &file.header[0].data.args else {
            panic!("expected `(name: template)` entries");
        };
        entries[0].1.clone()
    }

    #[test]
    fn a_text_template_takes_the_highest_position_and_every_name() {
        let arity = Arity::of_text("$(2) and $(who) and $(who) and $(1) and $(a.b)");
        assert_eq!(arity.positional, 2);
        assert_eq!(arity.named, ["who"]);
        assert!(Arity::of_text("no places, only $(a.b) and $1").is_empty());
    }

    #[test]
    fn a_block_body_takes_what_its_placeholders_say() {
        let arity = Arity::of_body(&body(
            "::macro(m: [\n@card(title: $(1))[ hi $(name) ]\n\n$(3)\n])\n",
        ));
        assert_eq!(arity.positional, 3);
        assert_eq!(arity.named, ["name"]);
    }

    #[test]
    fn a_zero_or_a_call_is_not_a_placeholder() {
        let value = body("::macro(m: [ $(0) $(a.b) $f(1) {$ \"s\" } $(1) $(n) ])\n");
        assert_eq!(
            expressions_in_body(&value),
            ["$(0)", "$(a.b)", "$f(1)", "{$ \"s\" }"]
        );
        assert_eq!(Arity::of_body(&value).positional, 1);
    }
}
