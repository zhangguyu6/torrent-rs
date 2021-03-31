use crate::error::Error;
use serde::{
    de::{
        self, Deserialize, DeserializeSeed, Deserializer, EnumAccess, MapAccess, SeqAccess,
        VariantAccess, Visitor,
    },
    forward_to_deserialize_any,
    ser::{
        Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
        SerializeTuple, SerializeTupleStruct, SerializeTupleVariant, Serializer,
    },
};

use std::collections::{btree_map, BTreeMap};
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::vec;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Value {
    Bytes(Vec<u8>),
    Integer(i64),
    List(Vec<Value>),
    Dict(BTreeMap<String, Value>),
}

impl Serialize for Value {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Value::Bytes(v) => s.serialize_bytes(v),
            Value::Integer(num) => s.serialize_i64(*num),
            Value::List(vec) => {
                let mut seq = s.serialize_seq(Some(vec.len()))?;
                for e in vec {
                    seq.serialize_element(e)?;
                }
                seq.end()
            }
            Value::Dict(dict) => {
                let mut map = s.serialize_map(Some(dict.len()))?;
                for (k, v) in dict {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("any Bencode value")
            }

            fn visit_i64<E>(self, i: i64) -> Result<Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Integer(i.into()))
            }

            fn visit_u64<E>(self, u: u64) -> Result<Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Integer(u as i64))
            }

            fn visit_str<E>(self, s: &str) -> Result<Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Bytes(s.into()))
            }

            fn visit_string<E>(self, s: String) -> Result<Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Bytes(s.into()))
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Bytes(v.into()))
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut vec = Vec::new();

                while let Some(element) = visitor.next_element()? {
                    vec.push(element);
                }

                Ok(Value::List(vec))
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();

                while let Some((key, value)) = visitor.next_entry()? {
                    values.insert(key, value);
                }

                Ok(Value::Dict(values))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Integer(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Bytes(v.into())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Bytes(v.into())
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::List(v)
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(v: BTreeMap<String, Value>) -> Self {
        Value::Dict(v)
    }
}

pub fn to_value<T: Serialize>(value: T) -> Result<Value, Error> {
    value.serialize(ValueSerializer)
}

pub fn from_value<'de, T: Deserialize<'de>>(value: Value) -> Result<T, Error> {
    Deserialize::deserialize(value)
}

pub struct ValueSerializer;

pub struct ValueSerializeSeq(Vec<Value>);

impl SerializeSeq for ValueSerializeSeq {
    type Ok = Value;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.push(to_value(&value)?);
        Ok(())
    }

    fn end(self) -> Result<Value, Error> {
        Ok(Value::List(self.0))
    }
}

impl SerializeTuple for ValueSerializeSeq {
    type Ok = Value;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.push(to_value(&value)?);
        Ok(())
    }

    fn end(self) -> Result<Value, Error> {
        Ok(Value::List(self.0))
    }
}

impl SerializeTupleStruct for ValueSerializeSeq {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.0.push(to_value(&value)?);
        Ok(())
    }
    fn end(self) -> Result<Value, Error> {
        Ok(Value::List(self.0))
    }
}

impl SerializeTupleVariant for ValueSerializeSeq {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.0.push(to_value(&value)?);
        Ok(())
    }
    fn end(self) -> Result<Value, Error> {
        Ok(Value::List(self.0))
    }
}

pub struct ValueSerializeMap(BTreeMap<String, Value>, Option<String>);

impl SerializeMap for ValueSerializeMap {
    type Ok = Value;
    type Error = Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Error> {
        let key = to_value(key)?;
        let key_string = match key {
            Value::Bytes(buf) => String::from_utf8(buf)?,
            _ => return Err(Error::CustomErr("key is not string".to_string())),
        };
        self.1 = Some(key_string);
        Ok(())
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        let value = to_value(value)?;
        let key = self.1.take().unwrap();
        self.0.insert(key, value);
        Ok(())
    }
    fn end(self) -> Result<Value, Error> {
        Ok(Value::Dict(self.0))
    }
}

impl SerializeStruct for ValueSerializeMap {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        let value = to_value(value)?;
        self.0.insert(key.to_string(), value);
        Ok(())
    }

    fn end(self) -> Result<Value, Error> {
        Ok(Value::Dict(self.0))
    }
}

impl SerializeStructVariant for ValueSerializeMap {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        let value = to_value(value)?;
        self.0.insert(key.to_string(), value);
        Ok(())
    }
    fn end(self) -> Result<Value, Error> {
        Ok(Value::Dict(self.0))
    }
}

impl Serializer for ValueSerializer {
    type Ok = Value;
    type Error = Error;
    type SerializeSeq = ValueSerializeSeq;
    type SerializeTuple = ValueSerializeSeq;
    type SerializeTupleStruct = ValueSerializeSeq;
    type SerializeTupleVariant = ValueSerializeSeq;
    type SerializeMap = ValueSerializeMap;
    type SerializeStruct = ValueSerializeMap;
    type SerializeStructVariant = ValueSerializeMap;

    fn serialize_bool(self, v: bool) -> Result<Value, Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_i8(self, v: i8) -> Result<Value, Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_i16(self, v: i16) -> Result<Value, Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_i32(self, v: i32) -> Result<Value, Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_i64(self, v: i64) -> Result<Value, Error> {
        Ok(Value::Integer(v))
    }

    fn serialize_u8(self, v: u8) -> Result<Value, Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_u16(self, v: u16) -> Result<Value, Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_u32(self, v: u32) -> Result<Value, Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_u64(self, v: u64) -> Result<Value, Error> {
        let v = i64::try_from(v)?;
        self.serialize_i64(v)
    }

    fn serialize_f32(self, _: f32) -> Result<Value, Error> {
        Err(Error::CustomErr("not support serialize float".to_string()))
    }

    fn serialize_f64(self, _: f64) -> Result<Value, Error> {
        Err(Error::CustomErr("not support serialize float".to_string()))
    }

    fn serialize_char(self, v: char) -> Result<Value, Error> {
        Ok(Value::Bytes(vec![v as u8]))
    }

    fn serialize_str(self, v: &str) -> Result<Value, Error> {
        Ok(Value::Bytes(v.as_bytes().to_vec()))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Value, Error> {
        Ok(Value::Bytes(v.to_vec()))
    }
    fn serialize_none(self) -> Result<Value, Error> {
        Err(Error::CustomErr("not support serialize none".to_string()))
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Value, Error> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Value, Error> {
        Err(Error::CustomErr("not support serialize none".to_string()))
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<Value, Error> {
        Err(Error::CustomErr("not support serialize none".to_string()))
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Value, Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<Value, Error> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        value: &T,
    ) -> Result<Value, Error> {
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        Ok(ValueSerializeSeq(Vec::with_capacity(
            len.unwrap_or_default(),
        )))
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Error> {
        Ok(ValueSerializeSeq(Vec::with_capacity(len)))
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Error> {
        Ok(ValueSerializeSeq(Vec::with_capacity(len)))
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Error> {
        Ok(ValueSerializeMap(BTreeMap::new(), None))
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeMap, Error> {
        Ok(ValueSerializeMap(BTreeMap::new(), None))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeMap, Error> {
        Ok(ValueSerializeMap(BTreeMap::new(), None))
    }
}

pub struct ValueDeserializeSeq(vec::IntoIter<Value>);

impl<'de> SeqAccess<'de> for ValueDeserializeSeq {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        if let Some(v) = self.0.next() {
            seed.deserialize(v).map(Some)
        } else {
            Ok(None)
        }
    }
}

pub struct ValueDeserializeMap(btree_map::IntoIter<String, Value>, Option<Value>);

pub struct StringDeserializer(String);

impl<'de> Deserializer<'de> for StringDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_string(self.0)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct identifier ignored_any enum
    }
}

impl<'de, 'a> MapAccess<'de> for ValueDeserializeMap {
    type Error = Error;
    fn next_key_seed<K: de::DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        if let Some((k, v)) = self.0.next() {
            self.1 = Some(v);
            seed.deserialize(StringDeserializer(k)).map(Some)
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        if let Some(v) = self.1.take() {
            seed.deserialize(v)
        } else {
            Err(Error::CustomErr("not find value".to_string()))
        }
    }
}

pub struct ValueDeserializeVariant(Value);

impl<'de> VariantAccess<'de> for ValueDeserializeVariant {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        Ok(())
    }

    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(self.0)
    }

    fn tuple_variant<V: de::Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::List(values) => visitor.visit_seq(ValueDeserializeSeq(values.into_iter())),
            _ => Err(Error::CustomErr("not a list".to_string())),
        }
    }
    fn struct_variant<V: de::Visitor<'de>>(
        self,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            Value::Dict(dict) => visitor.visit_map(ValueDeserializeMap(dict.into_iter(), None)),
            _ => Err(Error::CustomErr("not a dict".to_string())),
        }
    }
}

impl<'de> EnumAccess<'de> for ValueDeserializeVariant {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Error> {
        let val = seed.deserialize(self.0.clone())?;
        Ok((val, self))
    }
}

impl<'de> Deserializer<'de> for Value {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Bytes(buf) => visitor.visit_byte_buf(buf),
            Value::List(values) => visitor.visit_seq(ValueDeserializeSeq(values.into_iter())),
            Value::Dict(dict) => visitor.visit_map(ValueDeserializeMap(dict.into_iter(), None)),
            Value::Integer(num) => visitor.visit_i64(num),
        }
    }
    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => {
                if num == 0 {
                    visitor.visit_bool(false)
                } else {
                    visitor.visit_bool(true)
                }
            }
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }
    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_i8(num.try_into()?),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_i16(num.try_into()?),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_i32(num.try_into()?),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_i64(num),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_char(num as u8 as char),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_u8(num.try_into()?),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_u16(num.try_into()?),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_u32(num.try_into()?),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_u64(num.try_into()?),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_u128(num.try_into()?),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_f32(num as f32),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Integer(num) => visitor.visit_f64(num as f64),
            _ => Err(Error::CustomErr("not an int".to_string())),
        }
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Bytes(buf) => visitor.visit_bytes(buf.as_slice()),
            _ => Err(Error::CustomErr("not a buf".to_string())),
        }
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Bytes(buf) => visitor.visit_byte_buf(buf),
            _ => Err(Error::CustomErr("not a buf".to_string())),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Bytes(buf) => visitor.visit_string(String::from_utf8(buf)?),
            _ => Err(Error::CustomErr("not a buf".to_string())),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Bytes(buf) => {
                let s = std::str::from_utf8(buf.as_slice())?;
                visitor.visit_str(s)
            }
            _ => Err(Error::CustomErr("not a buf".to_string())),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Error> {
        Err(Error::CustomErr("not support bencode to unit".to_string()))
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::List(values) => visitor.visit_seq(ValueDeserializeSeq(values.into_iter())),
            _ => Err(Error::CustomErr("not a list".to_string())),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::Dict(dict) => visitor.visit_map(ValueDeserializeMap(dict.into_iter(), None)),
            _ => Err(Error::CustomErr("not a list".to_string())),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value, Error> {
        match self {
            Value::List(values) => visitor.visit_seq(ValueDeserializeSeq(values.into_iter())),
            _ => Err(Error::CustomErr("not a list".to_string())),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_any(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self {
            Value::Dict(dict) => visitor.visit_map(ValueDeserializeMap(dict.into_iter(), None)),
            _ => Err(Error::CustomErr("not a list".to_string())),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_enum(ValueDeserializeVariant(self))
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct A(String);

    fn value_ser_de<'de, V: Into<Value>, T: Deserialize<'de> + Serialize + fmt::Debug>(v: V) {
        let src_value = v.into();
        let t = from_value::<T>(src_value.clone()).unwrap();
        let ser_value = to_value(t).unwrap();
        assert_eq!(src_value, ser_value);
    }

    fn value_de_ser<'de, T: Deserialize<'de> + Serialize + Eq + fmt::Debug + Clone>(t: T) {
        let v = to_value(t.clone()).unwrap();
        let de_v: T = from_value(v).unwrap();
        assert_eq!(t, de_v);
    }

    #[test]
    fn test_value_ser_de() {
        value_ser_de::<i64, i64>(1);
        value_ser_de::<String, A>("1".to_string());
    }

    #[test]
    fn test_value_de_ser() {
        value_de_ser::<String>("!".to_string());
    }
}
