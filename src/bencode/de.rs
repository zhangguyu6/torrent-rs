use super::error::{BencodeError, Result};
use serde::de;
use std::convert::{TryFrom, TryInto};
use std::io::Read;
use std::str::FromStr;

pub fn from_bytes<'de, T: de::Deserialize<'de>>(b: &[u8]) -> Result<T> {
    de::Deserialize::deserialize(&mut Deserializer::new(b))
}

pub fn from_str<'de, T: de::Deserialize<'de>>(s: &str) -> Result<T> {
    from_bytes(s.as_bytes())
}

pub struct Deserializer<R> {
    de: R,
    next: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseResult {
    Bytes,
    Int,
    List,
    Dict,
    End,
}

impl TryFrom<u8> for ParseResult {
    type Error = BencodeError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            b'i' => Ok(ParseResult::Int),
            b'0'..=b'9' => Ok(ParseResult::Bytes),
            b'l' => Ok(ParseResult::List),
            b'd' => Ok(ParseResult::Dict),
            b'e' => Ok(ParseResult::End),
            c => Err(BencodeError::UnexpectedValueType(format!(
                "Bencode only support int, bytes, list and dict,but get {}",
                c
            ))),
        }
    }
}

impl<R: Read> Deserializer<R> {
    fn new(reader: R) -> Self {
        Self {
            de: reader,
            next: None,
        }
    }
    fn peek(&mut self) -> Result<u8> {
        if self.next.is_none() {
            self.next = Some(self.next()?);
        }
        Ok(self.next.unwrap())
    }
    fn next(&mut self) -> Result<u8> {
        match self.next.take() {
            None => {
                let mut buf = [0];
                self.de.read_exact(&mut buf)?;
                Ok(buf[0])
            }
            Some(v) => Ok(v),
        }
    }
    fn set_next(&mut self, next: u8) {
        self.next = Some(next);
    }
    fn parse_int(&mut self) -> Result<i64> {
        let mut buf = Vec::new();
        loop {
            let c = self.next()?;
            match c {
                b'e' | b':' => break,
                b'0'..=b'9' | b'-' => buf.push(c),
                _ => {
                    return Err(BencodeError::Custom(
                        "except int but get string".to_string(),
                    ))
                }
            }
        }
        Ok(i64::from_str(&String::from_utf8(buf)?)?)
    }
    fn parse_bytes(&mut self) -> Result<Vec<u8>> {
        let bytes_len = self.parse_int()? as usize;
        let mut buf = vec![0; bytes_len];
        self.de.read_exact(&mut buf)?;
        Ok(buf)
    }
}

impl<'de, 'a, R: Read> de::Deserializer<'de> for &'a mut Deserializer<R> {
    type Error = BencodeError;
    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_i64(self.parse_int()?),
            ParseResult::Bytes => {
                self.set_next(next);
                visitor.visit_bytes(&self.parse_bytes()?)
            }
            ParseResult::List => visitor.visit_seq(DeserializeSeq(self)),
            ParseResult::Dict => visitor.visit_map(DeserializeMap(self)),
            ParseResult::End => Err(BencodeError::Custom("unexpected End".to_string())),
        }
    }

    fn deserialize_bool<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => {
                if self.parse_int()? != 0 {
                    visitor.visit_bool(true)
                } else {
                    visitor.visit_bool(false)
                }
            }
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect int but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_i8<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_i8(self.parse_int()?.try_into()?),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect int but get {:?}",
                res,
            ))),
        }
    }

    fn deserialize_i16<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_i16(self.parse_int()?.try_into()?),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get int, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_i32<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_i32(self.parse_int()?.try_into()?),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get int, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_i64<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_i64(self.parse_int()?),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get int, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_char<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => {
                let i: u32 = self.parse_int()?.try_into()?;
                visitor.visit_char(i.try_into()?)
            }
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get char, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_u8<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_u8(self.parse_int()?.try_into()?),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get int, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_u16<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_u16(self.parse_int()?.try_into()?),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get int, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_u32<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_u32(self.parse_int()?.try_into()?),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get int, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_u64<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_u64(self.parse_int()?.try_into()?),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get int, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_f32<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_f32(self.parse_int()? as f32),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get int, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_f64<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Int => visitor.visit_f64(self.parse_int()? as f64),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get int, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_bytes<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Bytes => {
                self.set_next(next);
                visitor.visit_bytes(&self.parse_bytes()?)
            }
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get bytes, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_byte_buf<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Bytes => {
                self.set_next(next);
                visitor.visit_byte_buf(self.parse_bytes()?)
            }
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get bytes, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_string<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Bytes => {
                self.set_next(next);
                let bytes = self.parse_bytes()?;
                visitor.visit_string(String::from_utf8(bytes)?)
            }
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get bytes, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_str<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Bytes => {
                self.set_next(next);
                let bytes = self.parse_bytes()?;
                visitor.visit_str(String::from_utf8(bytes)?.as_str())
            }
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get bytes, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_seq<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::List => visitor.visit_seq(DeserializeSeq(self)),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get seq, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_unit<V: de::Visitor<'de>>(self, _: V) -> Result<V::Value> {
        Err(BencodeError::UnexpectedValueType(
            "Bencode only support int, bytes, list and dict,but get unit".to_string(),
        ))
    }

    fn deserialize_unit_struct<V: de::Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_map<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::Dict => visitor.visit_map(DeserializeMap(self)),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get dict, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_tuple<V: de::Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::List => visitor.visit_seq(DeserializeSeq(self)),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get list, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_tuple_struct<V: de::Visitor<'de>>(
        self,
        _: &'static str,
        _: usize,
        visitor: V,
    ) -> Result<V::Value> {
        let next = self.next()?;
        match ParseResult::try_from(next)? {
            ParseResult::List => visitor.visit_seq(DeserializeSeq(self)),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get list, but get {:?}",
                res
            ))),
        }
    }

    fn deserialize_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_map(visitor)
    }

    fn deserialize_identifier<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_any(visitor)
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_enum(DeserializeVariant(self))
    }
}

pub struct DeserializeSeq<'a, R>(&'a mut Deserializer<R>);

impl<'de, 'a, R: Read> de::SeqAccess<'de> for DeserializeSeq<'a, R> {
    type Error = BencodeError;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>> {
        match ParseResult::try_from(self.0.peek()?)? {
            ParseResult::End => {
                // consume end
                self.0.next()?;
                Ok(None)
            }
            _ => Ok(Some(seed.deserialize(&mut *self.0)?)),
        }
    }
}

pub struct DeserializeMap<'a, R>(&'a mut Deserializer<R>);

impl<'de, 'a, R: 'a + Read> de::MapAccess<'de> for DeserializeMap<'a, R> {
    type Error = BencodeError;
    fn next_key_seed<K: de::DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        match ParseResult::try_from(self.0.peek()?)? {
            ParseResult::End => {
                // consume end
                self.0.next()?;
                Ok(None)
            }
            _ => Ok(Some(seed.deserialize(&mut *self.0)?)),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(&mut *self.0)
    }
}

pub struct DeserializeVariant<'a, R>(&'a mut Deserializer<R>);

impl<'de, 'a, R: 'a + Read> de::VariantAccess<'de> for DeserializeVariant<'a, R> {
    type Error = BencodeError;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(&mut *self.0)
    }

    fn tuple_variant<V: de::Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value> {
        match ParseResult::try_from(self.0.next()?)? {
            ParseResult::List => visitor.visit_seq(DeserializeSeq(self.0)),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get list, but get {:?}",
                res
            ))),
        }
    }
    fn struct_variant<V: de::Visitor<'de>>(
        self,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        match ParseResult::try_from(self.0.next()?)? {
            ParseResult::Dict => visitor.visit_map(DeserializeMap(self.0)),
            res => Err(BencodeError::UnexpectedValueType(format!(
                "expect get dict, but get {:?}",
                res
            ))),
        }
    }
}

impl<'de, 'a, R: Read> de::EnumAccess<'de> for DeserializeVariant<'a, R> {
    type Error = BencodeError;
    type Variant = Self;

    fn variant_seed<V: de::DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant)> {
        let val = seed.deserialize(&mut *self.0)?;
        Ok((val, self))
    }
}
