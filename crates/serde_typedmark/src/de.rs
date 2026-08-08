use serde::de::{self, Deserialize, IntoDeserializer, Visitor};
use typedmark_ast::Value;

use crate::error::{Error, Result};

pub fn from_value<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T> {
    T::deserialize(ValueDeserializer(value))
}

/// A local newtype around `Value` -- `serde::Deserializer` is a foreign
/// trait and `Value` is a foreign type (defined in `typedmark-ast`), so
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
            Value::String(s) => visitor.visit_string(s),
            Value::Seq(items) => visitor.visit_seq(SeqAccess { iter: items.into_iter() }),
            Value::Map(entries) => visitor.visit_map(MapAccess { iter: entries.into_iter(), value: None }),
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

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple
        tuple_struct map struct identifier ignored_any
    }
}

struct SeqAccess {
    iter: std::vec::IntoIter<Value>,
}

impl<'de> de::SeqAccess<'de> for SeqAccess {
    type Error = Error;
    fn next_element_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        match self.iter.next() {
            Some(v) => seed.deserialize(ValueDeserializer(v)).map(Some),
            None => Ok(None),
        }
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct MapAccess {
    iter: std::vec::IntoIter<(String, Value)>,
    value: Option<Value>,
}

impl<'de> de::MapAccess<'de> for MapAccess {
    type Error = Error;
    fn next_key_seed<K: de::DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        match self.iter.next() {
            Some((k, v)) => {
                self.value = Some(v);
                seed.deserialize(ValueDeserializer(Value::String(k))).map(Some)
            }
            None => Ok(None),
        }
    }
    fn next_value_seed<V: de::DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        let v = self.value.take().ok_or_else(|| Error::msg("value requested before key"))?;
        seed.deserialize(ValueDeserializer(v))
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
    fn variant_seed<S: de::DeserializeSeed<'de>>(self, seed: S) -> Result<(S::Value, VariantAccess)> {
        let v = seed.deserialize(ValueDeserializer(Value::String(self.variant)))?;
        Ok((v, VariantAccess { value: self.value }))
    }
}

struct VariantAccess {
    value: Value,
}

impl<'de> de::VariantAccess<'de> for VariantAccess {
    type Error = Error;
    fn unit_variant(self) -> Result<()> {
        Ok(())
    }
    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(ValueDeserializer(self.value))
    }
    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        de::Deserializer::deserialize_seq(ValueDeserializer(self.value), visitor)
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        de::Deserializer::deserialize_map(ValueDeserializer(self.value), visitor)
    }
}
