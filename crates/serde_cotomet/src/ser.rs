use serde::ser::{self, Serialize};
use tomet_ast::{Tuple, Value};
use tomet_scalar::Scalar;

use crate::error::{Error, Result};

pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Result<Value> {
    value.serialize(ValueSerializer)
}

pub struct ValueSerializer;

impl ser::Serializer for ValueSerializer {
    type Ok = Value;
    type Error = Error;
    type SerializeSeq = SeqSerializer;
    type SerializeTuple = SeqSerializer;
    type SerializeTupleStruct = SeqSerializer;
    type SerializeTupleVariant = SeqSerializer;
    type SerializeMap = MapSerializer;
    type SerializeStruct = MapSerializer;
    type SerializeStructVariant = MapSerializer;

    fn serialize_bool(self, v: bool) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Bool(v)))
    }

    fn serialize_i8(self, v: i8) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Int(v as i64)))
    }
    fn serialize_i16(self, v: i16) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Int(v as i64)))
    }
    fn serialize_i32(self, v: i32) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Int(v as i64)))
    }
    fn serialize_i64(self, v: i64) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Int(v)))
    }
    fn serialize_u8(self, v: u8) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Int(v as i64)))
    }
    fn serialize_u16(self, v: u16) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Int(v as i64)))
    }
    fn serialize_u32(self, v: u32) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Int(v as i64)))
    }
    fn serialize_u64(self, v: u64) -> Result<Value> {
        // One that does not fit an `i64` is kept as the digits it is, not wrapped.
        Ok(i64::try_from(v).map_or_else(
            |_| Value::Scalar(Scalar::Number(v.to_string())),
            |n| Value::Scalar(Scalar::Int(n)),
        ))
    }
    fn serialize_i128(self, v: i128) -> Result<Value> {
        Ok(i64::try_from(v).map_or_else(
            |_| Value::Scalar(Scalar::Number(v.to_string())),
            |n| Value::Scalar(Scalar::Int(n)),
        ))
    }
    fn serialize_u128(self, v: u128) -> Result<Value> {
        Ok(i64::try_from(v).map_or_else(
            |_| Value::Scalar(Scalar::Number(v.to_string())),
            |n| Value::Scalar(Scalar::Int(n)),
        ))
    }
    fn serialize_f32(self, v: f32) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Float(v as f64)))
    }
    fn serialize_f64(self, v: f64) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Float(v)))
    }
    fn serialize_char(self, v: char) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Text(v.to_string())))
    }
    fn serialize_str(self, v: &str) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Text(v.to_string())))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Value> {
        Ok(Value::Tuple(Tuple::from_unnamed(
            v.iter().map(|b| Value::Scalar(Scalar::Int(*b as i64))),
        )))
    }
    fn serialize_none(self) -> Result<Value> {
        Ok(Value::Scalar(Scalar::None))
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Value> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<Value> {
        Ok(Value::Scalar(Scalar::None))
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<Value> {
        Ok(Value::Scalar(Scalar::None))
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Value> {
        Ok(Value::Scalar(Scalar::Text(variant.to_string())))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Value> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value> {
        Ok(Value::Tuple(Tuple::from_named(vec![(
            variant.to_string(),
            value.serialize(ValueSerializer)?,
        )])))
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<SeqSerializer> {
        Ok(SeqSerializer {
            items: Vec::with_capacity(len.unwrap_or(0)),
            variant: None,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<SeqSerializer> {
        Ok(SeqSerializer {
            items: Vec::with_capacity(len),
            variant: None,
        })
    }
    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<SeqSerializer> {
        Ok(SeqSerializer {
            items: Vec::with_capacity(len),
            variant: None,
        })
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<SeqSerializer> {
        Ok(SeqSerializer {
            items: Vec::with_capacity(len),
            variant: Some(variant.to_string()),
        })
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<MapSerializer> {
        Ok(MapSerializer {
            entries: Vec::new(),
            next_key: None,
            variant: None,
        })
    }
    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<MapSerializer> {
        Ok(MapSerializer {
            entries: Vec::with_capacity(len),
            next_key: None,
            variant: None,
        })
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<MapSerializer> {
        Ok(MapSerializer {
            entries: Vec::new(),
            next_key: None,
            variant: Some(variant.to_string()),
        })
    }
}

pub struct SeqSerializer {
    items: Vec<Value>,
    variant: Option<String>,
}

fn finish_seq(s: SeqSerializer) -> Result<Value> {
    let seq = Value::Tuple(Tuple::from_unnamed(s.items));
    match s.variant {
        Some(variant) => Ok(Value::Tuple(Tuple::from_named(vec![(variant, seq)]))),
        None => Ok(seq),
    }
}

impl ser::SerializeSeq for SeqSerializer {
    type Ok = Value;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.items.push(value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Value> {
        finish_seq(self)
    }
}

impl ser::SerializeTuple for SeqSerializer {
    type Ok = Value;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.items.push(value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Value> {
        finish_seq(self)
    }
}

impl ser::SerializeTupleStruct for SeqSerializer {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.items.push(value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Value> {
        finish_seq(self)
    }
}

impl ser::SerializeTupleVariant for SeqSerializer {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.items.push(value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Value> {
        finish_seq(self)
    }
}

pub struct MapSerializer {
    entries: Vec<(String, Value)>,
    next_key: Option<String>,
    variant: Option<String>,
}

fn finish_map(s: MapSerializer) -> Result<Value> {
    let map = Value::Tuple(Tuple::from_named(s.entries));
    match s.variant {
        Some(variant) => Ok(Value::Tuple(Tuple::from_named(vec![(variant, map)]))),
        None => Ok(map),
    }
}

/// A map key is the text of a scalar that has one (`1.0` stays `1.0`); `none` and `auto` are not keys.
fn value_to_key_string(v: Value) -> Result<String> {
    match v {
        Value::Scalar(ref s) if !matches!(s, Scalar::None | Scalar::Auto) => {
            Ok(s.text().map(|text| text.into_owned()).unwrap_or_default())
        }
        other => Err(Error::msg(format!(
            "map keys must be strings or simple scalars, got {other:?}"
        ))),
    }
}

impl ser::SerializeMap for MapSerializer {
    type Ok = Value;
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<()> {
        let k = key.serialize(ValueSerializer)?;
        self.next_key = Some(value_to_key_string(k)?);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        let key = self
            .next_key
            .take()
            .ok_or_else(|| Error::msg("serialize_value called before serialize_key"))?;
        self.entries.push((key, value.serialize(ValueSerializer)?));
        Ok(())
    }
    fn end(self) -> Result<Value> {
        finish_map(self)
    }
}

impl ser::SerializeStruct for MapSerializer {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.entries
            .push((key.to_string(), value.serialize(ValueSerializer)?));
        Ok(())
    }
    fn end(self) -> Result<Value> {
        finish_map(self)
    }
}

impl ser::SerializeStructVariant for MapSerializer {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.entries
            .push((key.to_string(), value.serialize(ValueSerializer)?));
        Ok(())
    }
    fn end(self) -> Result<Value> {
        finish_map(self)
    }
}
