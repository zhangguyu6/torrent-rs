use crate::error::{Error, Result};
use serde::ser;
use std::convert::TryFrom;
use std::io::Write;

pub fn to_bytes<T: ser::Serialize>(v: &T) -> Result<Vec<u8>> {
    let mut ser: Serializer<Vec<u8>> = Serializer::default();
    v.serialize(&mut ser)?;
    Ok(ser.into())
}

pub fn to_str<T: ser::Serialize>(v: &T) -> Result<String> {
    let buf = to_bytes(v)?;
    Ok(String::from_utf8(buf)?)
}

#[derive(Default)]
pub struct Serializer<W> {
    inner: W,
}

impl<W: Write> Serializer<W> {
    pub fn into(self) -> W {
        self.inner
    }
}

impl<W> AsRef<W> for Serializer<W> {
    fn as_ref(&self) -> &W {
        &self.inner
    }
}

pub struct SerializeSeq<'a, W>(&'a mut Serializer<W>);

impl<'a, W: Write> SerializeSeq<'a, W> {
    pub fn end_seq(self) -> Result<()> {
        self.0.inner.write_all(&[b'e'])?;
        Ok(())
    }
}

impl<'a, W: Write> ser::SerializeSeq for SerializeSeq<'a, W> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.0)
    }

    fn end(self) -> Result<()> {
        self.end_seq()
    }
}

impl<'a, W: Write> ser::SerializeTuple for SerializeSeq<'a, W> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> {
        self.end_seq()
    }
}

impl<'a, W: Write> ser::SerializeTupleStruct for SerializeSeq<'a, W> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> {
        self.end_seq()
    }
}

impl<'a, W: Write> ser::SerializeTupleVariant for SerializeSeq<'a, W> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> {
        self.end_seq()
    }
}

pub struct SerializeMap<'a, W>(&'a mut Serializer<W>);

impl<'a, W: Write> SerializeMap<'a, W> {
    pub fn end_map(self) -> Result<()> {
        self.0.inner.write_all(&[b'e'])?;
        Ok(())
    }
}

impl<'a, W: Write> ser::SerializeMap for SerializeMap<'a, W> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: ?Sized + ser::Serialize>(&mut self, key: &T) -> Result<()> {
        key.serialize(&mut *self.0)
    }
    fn serialize_value<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> {
        self.end_map()
    }
}

pub struct SerializeStruct<'a, W>(
    &'a mut Serializer<W>,
    Vec<(&'static str, Serializer<Vec<u8>>)>,
);

impl<'a, W: Write> SerializeStruct<'a, W> {
    fn end_struct(mut self) -> Result<()> {
        self.1.sort_by(|(s0, _), (s1, _)| s0.cmp(s1));
        for (_, ser) in self.1 {
            self.0.inner.write_all(ser.as_ref())?;
        }
        self.0.inner.write_all(&[b'e'])?;
        Ok(())
    }
}

impl<'a, W: Write> ser::SerializeStruct for SerializeStruct<'a, W> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + ser::Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        let mut ser = Serializer::default();
        use ser::Serialize;
        key.serialize(&mut ser)?;
        value.serialize(&mut ser)?;
        self.1.push((key, ser));
        Ok(())
    }

    fn end(self) -> Result<()> {
        self.end_struct()
    }
}

impl<'a, W: Write> ser::SerializeStructVariant for SerializeStruct<'a, W> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + ser::Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        let mut ser = Serializer::default();
        use ser::Serialize;
        key.serialize(&mut ser)?;
        value.serialize(&mut ser)?;
        self.1.push((key, ser));
        Ok(())
    }
    fn end(self) -> Result<()> {
        self.end_struct()
    }
}

impl<'a, W: Write> ser::Serializer for &'a mut Serializer<W> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = SerializeSeq<'a, W>;
    type SerializeTuple = SerializeSeq<'a, W>;
    type SerializeTupleStruct = SerializeSeq<'a, W>;
    type SerializeTupleVariant = SerializeSeq<'a, W>;
    type SerializeMap = SerializeMap<'a, W>;
    type SerializeStruct = SerializeStruct<'a, W>;
    type SerializeStructVariant = SerializeStruct<'a, W>;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.serialize_i64(v.into())
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        self.serialize_i64(v.into())
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        self.serialize_i64(v.into())
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        self.serialize_i64(v.into())
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        self.inner.write_fmt(format_args!("i{:?}e", v))?;
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.serialize_i64(v.into())
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.serialize_i64(v.into())
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.serialize_i64(v.into())
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        let v = i64::try_from(v)?;
        self.serialize_i64(v)
    }

    fn serialize_f32(self, _: f32) -> Result<()> {
        Err(Error::CustomErr("not support serialize float".to_string()))
    }

    fn serialize_f64(self, _: f64) -> Result<()> {
        Err(Error::CustomErr("not support serialize float".to_string()))
    }

    fn serialize_char(self, v: char) -> Result<()> {
        self.inner.write_fmt(format_args!("1:{}", v))?;
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        self.inner
            .write_fmt(format_args!("{}:{}", v.as_bytes().len(), v))?;
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.inner.write_fmt(format_args!("{}:", v.len()))?;
        self.inner.write_all(v)?;
        Ok(())
    }

    fn serialize_none(self) -> Result<()> {
        Ok(())
    }

    fn serialize_some<T: ?Sized + ser::Serialize>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> Result<()> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T: ?Sized + ser::Serialize>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + ser::Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()> {
        self.serialize_str(variant)?;
        value.serialize(self)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq> {
        self.inner.write_all(&[b'l'])?;
        Ok(SerializeSeq(self))
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.serialize_str(variant)?;
        self.serialize_seq(Some(len))
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap> {
        self.inner.write_all(&[b'd'])?;
        Ok(SerializeMap(self))
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct> {
        self.inner.write_all(&[b'd'])?;
        Ok(SerializeStruct(self, Vec::new()))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.serialize_str(variant)?;
        self.inner.write_all(&[b'd'])?;
        Ok(SerializeStruct(self, Vec::new()))
    }
}
