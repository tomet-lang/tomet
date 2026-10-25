//! Extension trait and helpers for [`tomet_ast::Value`].

use tomet_ast::Value;

/// Extension trait providing accessors and inspections on [`Value`].
pub trait ValueExt {
    /// Returns `true` if this value is `Value::Null`.
    fn is_null(&self) -> bool;

    /// Returns the string slice if this value is a `Value::String`.
    fn as_str(&self) -> Option<&str>;

    /// Returns the `bool` value if this value is a `Value::Bool`.
    fn as_bool(&self) -> Option<bool>;

    /// Returns the `i64` value if this value is a `Value::Int`, or a `Value::Number`
    /// that spells an integer an `i64` holds.
    fn as_i64(&self) -> Option<i64>;

    /// Returns the `f64` value if this value is a `Value::Float`, or a `Value::Number`
    /// that spells one (`1.10` is `1.1` here; the text it was written as is
    /// [`Value::scalar_text`]).
    fn as_f64(&self) -> Option<f64>;

    /// Returns a slice of elements if this value is a `Value::Seq`.
    fn as_seq(&self) -> Option<&[Value]>;

    /// Returns a slice of key-value pairs if this value is a `Value::Map`.
    fn as_map(&self) -> Option<&[(String, Value)]>;

    /// If this value is a `Value::Map`, returns the first value associated with `key`.
    fn get(&self, key: &str) -> Option<&Value>;

    /// If this value is a `Value::Map`, returns a mutable reference to the first value associated with `key`.
    fn get_mut(&mut self, key: &str) -> Option<&mut Value>;

    /// Returns `true` if a sequence or map is empty, or if string is empty, or if null.
    fn is_empty(&self) -> bool;
}

impl ValueExt for Value {
    fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            // `-0`: kept as written, and the integer it spells.
            Value::Number(text) => text.parse().ok(),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        self.as_number()
    }

    fn as_seq(&self) -> Option<&[Value]> {
        match self {
            Value::Seq(seq) => Some(seq.as_slice()),
            _ => None,
        }
    }

    fn as_map(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Map(map) => Some(map.as_slice()),
            _ => None,
        }
    }

    fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries
                .iter()
                .find_map(|(k, v)| if k == key { Some(v) } else { None }),
            _ => None,
        }
    }

    fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Value::Map(entries) => entries
                .iter_mut()
                .find_map(|(k, v)| if k == key { Some(v) } else { None }),
            _ => None,
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Value::Null => true,
            Value::String(s) => s.is_empty(),
            Value::Seq(seq) => seq.is_empty(),
            Value::Map(map) => map.is_empty(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_number_is_read_as_the_number_it_spells() {
        let one_ten = Value::Number("1.10".to_string());
        assert_eq!(one_ten.as_f64(), Some(1.1));
        assert_eq!(one_ten.as_i64(), None);
        assert_eq!(Value::Number("-0".to_string()).as_i64(), Some(0));
        let big = Value::Number("12345678901234567890".to_string());
        assert_eq!(big.as_i64(), None);
        assert_eq!(big.as_f64(), Some(1.2345678901234567e19));
        // too large for an f64: there is no number to read, only the text
        let huge = Value::Number("1e999".to_string());
        assert_eq!(huge.as_f64(), None);
        assert_eq!(huge.scalar_text().as_deref(), Some("1e999"));
        // and it is not a string
        assert_eq!(one_ten.as_str(), None);
    }
}
