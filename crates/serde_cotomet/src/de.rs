use serde::de::{self, Deserialize, IntoDeserializer, Visitor};
use tomet_ast::Value;

use crate::error::{Error, Result, Segment};

pub fn from_value<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T> {
    T::deserialize(ValueDeserializer(value))
}

/// An integer target reads a [`Value::Number`] that is a whole number (`-0`, or
/// one past an `i64`) as the integer it spells, to the width the target has.
macro_rules! deserialize_integer {
    ($($method:ident)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
            match self.0 {
                // The narrowest visit that holds it: a primitive's visitor takes its own
                // width and the next one up, not `i128` for every width.
                Value::Number(text) => {
                    if let Ok(n) = text.parse::<i64>() {
                        visitor.visit_i64(n)
                    } else if let Ok(n) = text.parse::<u64>() {
                        visitor.visit_u64(n)
                    } else if let Ok(n) = text.parse::<i128>() {
                        visitor.visit_i128(n)
                    } else if let Ok(n) = text.parse::<u128>() {
                        visitor.visit_u128(n)
                    } else {
                        Err(Error::msg(format!("`{text}` is not an integer")))
                    }
                }
                other => ValueDeserializer(other).deserialize_any(visitor),
            }
        }
    )*};
}

/// A float target reads a [`Value::Number`] as the number it spells.
macro_rules! deserialize_float {
    ($($method:ident)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
            match self.0 {
                Value::Number(text) => match text.parse::<f64>() {
                    Ok(f) if f.is_finite() => visitor.visit_f64(f),
                    _ => Err(Error::msg(format!("`{text}` is out of range for a float"))),
                },
                other => ValueDeserializer(other).deserialize_any(visitor),
            }
        }
    )*};
}

/// A local newtype around `Value` -- `serde::Deserializer` is a foreign
/// trait and `Value` is a foreign type (defined in `tomet-ast`), so
/// the orphan rules require this wrapper to implement it here.
struct ValueDeserializer(Value);

impl<'de> de::Deserializer<'de> for ValueDeserializer {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.0 {
            Value::Null => visitor.visit_unit(),
            Value::Bool(b) => visitor.visit_bool(b),
            Value::Int(i) => visitor.visit_i64(i),
            Value::Float(f) => visitor.visit_f64(f),
            // A target that does not say what it wants gets the number it is, as
            // `serde_json` gives an `f64`; one too large to be one gets its text.
            Value::Number(text) => match text.parse::<f64>() {
                Ok(f) if f.is_finite() => visitor.visit_f64(f),
                _ => visitor.visit_string(text),
            },
            Value::String(s) => visitor.visit_string(s),
            Value::Seq(items) => visitor.visit_seq(SeqAccess {
                iter: items.into_iter(),
                index: 0,
            }),
            Value::Map(entries) => visitor.visit_map(MapAccess {
                iter: entries.into_iter(),
                value: None,
            }),
            Value::Call(name, args) => Err(Error::msg(format!(
                "call syntax `{name}(...)` has no serde equivalent (got {} arg(s))",
                args.len()
            ))),
            Value::Element(el) => Err(Error::msg(format!(
                "an embedded element (`@{}(...)`) has no serde equivalent",
                el.sigil.name().map(|n| n.to_string()).unwrap_or_default()
            ))),
            Value::Blocks(blocks) => Err(Error::msg(format!(
                "block content (`[...]`, {} block(s)) has no serde equivalent",
                blocks.len()
            ))),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.0 {
            Value::Null => visitor.visit_none(),
            other => visitor.visit_some(ValueDeserializer(other)),
        }
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        match self.0 {
            Value::String(s) => visitor.visit_enum(s.into_deserializer()),
            Value::Map(mut entries) if entries.len() == 1 => {
                let (variant, value) = entries.remove(0);
                visitor.visit_enum(EnumAccess { variant, value })
            }
            other => Err(Error::msg(format!(
                "expected a string or single-entry map for an enum, got {other:?}"
            ))),
        }
    }

    /// A `String` target is given the text a scalar was written as: `2024`, `true`
    /// and `1.10` are each the text they were typed as. Nothing is lost on the
    /// way, which is what a number that is really a version or a revision needs.
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.0.scalar_text().map(std::borrow::Cow::into_owned) {
            Some(text) => visitor.visit_string(text),
            None => self.deserialize_any(visitor),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_string(visitor)
    }

    deserialize_integer!(
        deserialize_i8 deserialize_i16 deserialize_i32 deserialize_i64 deserialize_i128
        deserialize_u8 deserialize_u16 deserialize_u32 deserialize_u64 deserialize_u128
    );

    deserialize_float!(deserialize_f32 deserialize_f64);

    serde::forward_to_deserialize_any! {
        bool char bytes byte_buf unit unit_struct newtype_struct seq tuple
        tuple_struct map struct identifier ignored_any
    }
}

struct SeqAccess {
    iter: std::vec::IntoIter<Value>,
    index: usize,
}

impl<'de> de::SeqAccess<'de> for SeqAccess {
    type Error = Error;
    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>> {
        match self.iter.next() {
            Some(v) => {
                let index = self.index;
                self.index += 1;
                seed.deserialize(ValueDeserializer(v))
                    .map(Some)
                    .map_err(|e| e.within(Segment::Index(index)))
            }
            None => Ok(None),
        }
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct MapAccess {
    iter: std::vec::IntoIter<(String, Value)>,
    /// The entry whose key was just handed out, until its value is asked for.
    value: Option<(String, Value)>,
}

impl<'de> de::MapAccess<'de> for MapAccess {
    type Error = Error;
    fn next_key_seed<K: de::DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        match self.iter.next() {
            Some((k, v)) => {
                self.value = Some((k.clone(), v));
                seed.deserialize(ValueDeserializer(Value::String(k)))
                    .map(Some)
            }
            None => Ok(None),
        }
    }
    fn next_value_seed<V: de::DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        let (key, v) = self
            .value
            .take()
            .ok_or_else(|| Error::msg("value requested before key"))?;
        seed.deserialize(ValueDeserializer(v))
            .map_err(|e| e.within(Segment::Key(key)))
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct EnumAccess {
    variant: String,
    value: Value,
}

impl<'de> de::EnumAccess<'de> for EnumAccess {
    type Error = Error;
    type Variant = VariantAccess;
    fn variant_seed<S: de::DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> Result<(S::Value, VariantAccess)> {
        let v = seed.deserialize(ValueDeserializer(Value::String(self.variant.clone())))?;
        Ok((
            v,
            VariantAccess {
                variant: self.variant,
                value: self.value,
            },
        ))
    }
}

/// The payload of an externally tagged variant, written as the one-entry map
/// `{ Variant: payload }`, so its errors are under that key.
struct VariantAccess {
    variant: String,
    value: Value,
}

impl<'de> de::VariantAccess<'de> for VariantAccess {
    type Error = Error;
    fn unit_variant(self) -> Result<()> {
        Ok(())
    }
    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(ValueDeserializer(self.value))
            .map_err(|e| e.within(Segment::Key(self.variant)))
    }
    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        de::Deserializer::deserialize_seq(ValueDeserializer(self.value), visitor)
            .map_err(|e| e.within(Segment::Key(self.variant)))
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        de::Deserializer::deserialize_map(ValueDeserializer(self.value), visitor)
            .map_err(|e| e.within(Segment::Key(self.variant)))
    }
}
