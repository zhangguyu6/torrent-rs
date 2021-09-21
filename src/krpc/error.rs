use serde::{
    de::{self, SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::fmt::{self, Display, Formatter};
use std::mem::transmute;
use thiserror::Error;

/// Errors, or KRPC message dictionaries with a "y" value of "e",
/// contain one additional key "e". The value of "e" is a list.
/// The first element is an integer representing the error code.
/// The second element is a string containing the error message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum KrpcErrorCode {
    Generic = 201,
    Server = 202,
    Protocol = 203,
    Method = 204,
    MessageValueFieldTooBig = 205,
    InvalidSignature = 206,
    SaltFieldTooBig = 207,
    CasHashMismatched = 301,
    SequenceNumberLessThanCurrent = 302,
}

impl From<i64> for KrpcErrorCode {
    fn from(code: i64) -> Self {
        unsafe { transmute(code) }
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub struct KrpcError {
    code: KrpcErrorCode,
    desc: String,
}

impl KrpcError {
    pub fn new<C: Into<KrpcErrorCode>, S: Into<String>>(code: C, desc: S) -> Self {
        let code = code.into();
        let desc = desc.into();
        Self { code, desc }
    }
}

impl Display for KrpcError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}:{:?}", self.code, self.desc)
    }
}

impl Serialize for KrpcError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(&(self.code as i64))?;
        seq.serialize_element(&self.desc)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for KrpcError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KrpcErrorVisitor;
        impl<'de> Visitor<'de> for KrpcErrorVisitor {
            type Value = KrpcError;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`q or r or e`")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let code: i64 = seq
                    .next_element()?
                    .ok_or(de::Error::custom("expect an i64"))?;
                let code = KrpcErrorCode::from(code);
                let desc: String = seq
                    .next_element()?
                    .ok_or(de::Error::custom("expect a string"))?;
                if seq.next_element::<i64>()?.is_some() {
                    return Err(de::Error::custom("only two"));
                }
                Ok(KrpcError { code, desc })
            }
        }
        deserializer.deserialize_seq(KrpcErrorVisitor)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::bencode::{from_bytes, to_bytes};
    #[test]
    fn test_krpc_error() {
        let e = KrpcError::new(201, "not a error");
        let buf = to_bytes(&e);
        assert!(buf.is_ok());
        let e1 = from_bytes(&buf.unwrap());
        assert!(e1.is_ok());
        assert_eq!(e, e1.unwrap());
    }
}
