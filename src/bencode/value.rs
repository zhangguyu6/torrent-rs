use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use std::{collections::BTreeMap, fmt};

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Value {
    Bytes(String),
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
            Value::Bytes(string) => s.serialize_str(string),
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
                Ok(Value::Bytes(s))
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Bytes(String::from_utf8_lossy(v).to_string()))
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
