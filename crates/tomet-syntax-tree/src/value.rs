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

    /// Returns the `i64` value if this value is a `Value::Int`.
    fn as_i64(&self) -> Option<i64>;

    /// Returns the `f64` value if this value is a `Value::Float`.
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
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
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
