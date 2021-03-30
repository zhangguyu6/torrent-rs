use crate::metainfo::{CompactNodes, HashPiece};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Query,
    Response,
    Error,
}

impl Serialize for MessageType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use MessageType::*;
        match self {
            Query => "q".serialize(serializer),
            Response => "r".serialize(serializer),
            Error => "e".serialize(serializer),
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
                    "q" => Ok(MessageType::Query),
                    "r" => Ok(MessageType::Response),
                    "e" => Ok(MessageType::Error),
                    _ => Err(de::Error::custom(format!("receive unexpected {}", v))),
                }
            }
        }
        deserializer.deserialize_str(MessageTypeVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    Ping,
    FindNode,
    GetPeers,
    AnnouncePeer,
}

impl Serialize for QueryType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use QueryType::*;
        match self {
            Ping => "ping".serialize(serializer),
            FindNode => "find_node".serialize(serializer),
            GetPeers => "get_peers".serialize(serializer),
            AnnouncePeer => "announce_peer".serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for QueryType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct QueryTypeVisitor;
        impl<'de> Visitor<'de> for QueryTypeVisitor {
            type Value = QueryType;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`ping` or `find_node` or `get_peers` or `announce_peer`")
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match v {
                    "ping" => Ok(QueryType::Ping),
                    "find_node" => Ok(QueryType::FindNode),
                    "get_peers" => Ok(QueryType::GetPeers),
                    "announce_peer" => Ok(QueryType::AnnouncePeer),
                    _ => Err(de::Error::custom(format!("receive unexpected {}", v))),
                }
            }
        }
        deserializer.deserialize_str(QueryTypeVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct KrpcQuery {
    /// the querying node
    id: HashPiece,
    /// the node sought by the queryer
    /// find_node
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    target: Option<HashPiece>,
    /// the infohash of the torrent
    /// get_peers
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    info_hash: Option<HashPiece>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    port: Option<u16>,
    /// received one from an earlier "get_peers" query
    /// announce_peer
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    token: String,
    /// If it is present and non-zero, the port argument should be ignored
    /// and the source port of the UDP packet should be used as the peer's port instead
    /// announce_peer
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    implied_port: Option<bool>,
    /// Containing one or both of the strings "n4" or "n6"
    /// "n4": the node requests the presence of a "nodes" key
    /// "n6": the node requests the presence of a "nodes6" key
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    want: Vec<String>,
}

/// the results used by the RESPONSE message
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct KrpcResponse {
    /// indentify the queried node, that's, the response node
    id: HashPiece,
    /// found nodes ipv4
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    nodes: Option<CompactNodes>,
    /// found nodes ipv6
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    nodes6: Option<CompactNodes>,
    /// used for future "announce_peer"
    /// get_peers
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    token: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    /// A string value representing a transaction ID
    t: String,
    /// Type of the message: q for QUERY, r for RESPONSE, e for ERROR
    y: MessageType,
    /// Query method (one of 4: "ping", "find_node", "get_peers", "announce_peer")
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    q: Option<QueryType>,
    /// Named arguments sent with a query
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    a: Option<KrpcQuery>,
}
