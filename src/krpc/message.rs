use serde::{
    de::{self, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    QUERY,
    RESPONSE,
    ERROR,
}

impl Serialize for MessageType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use MessageType::*;
        match self {
            QUERY => "q".serialize(serializer),
            RESPONSE => "r".serialize(serializer),
            ERROR => "e".serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for MessageType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MessageTypeVisitor;
        impl<'de> Visitor<'de> for MessageTypeVisitor {
            type Value = MessageType;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`q or r or e`")
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match v {
                    "q" => Ok(MessageType::QUERY),
                    "r" => Ok(MessageType::RESPONSE),
                    "e" => Ok(MessageType::ERROR),
                    _ => Err(de::Error::custom(format!("receive {}, not q or r or e", v))),
                }
            }
        }
        deserializer.deserialize_tuple_struct("MessageType", 1, MessageTypeVisitor)
    }
}

pub struct Message {
    /// a string value representing a transaction ID
    t: String,
    /// type of the message: q for QUERY, r for RESPONSE, e for ERROR
    y: MessageType,
}
