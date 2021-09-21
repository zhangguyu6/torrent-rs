use super::node::CompactNodes;
use super::KrpcError;
use crate::bencode::Value;
use crate::metainfo::{CompactAddresses, HashPiece};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::fmt;

pub const MAX_KRPC_MESSAGE_SIZE: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Query,
    Response,
    Error,
}

impl Default for MessageType {
    fn default() -> Self {
        Self::Error
    }
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Default, Clone)]
pub struct KrpcQuery {
    /// the querying node
    pub id: HashPiece,
    /// the node sought by the queryer
    /// find_node
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub target: Option<HashPiece>,
    /// the infohash of the torrent
    /// get_peers
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub info_hash: Option<HashPiece>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub port: Option<u16>,
    /// received one from an earlier "get_peers" query
    /// announce_peer
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub token: Option<Value>,
    /// If it is present and non-zero, the port argument should be ignored
    /// and the source port of the UDP packet should be used as the peer's port instead
    /// announce_peer
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub implied_port: Option<bool>,
    /// Containing one or both of the strings "n4" or "n6"
    /// "n4": the node requests the presence of a "nodes" key
    /// "n6": the node requests the presence of a "nodes6" key
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub want: Vec<String>,
}

/// the results used by the RESPONSE message
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KrpcResponse {
    /// indentify the queried node, that's, the response node
    pub id: HashPiece,
    /// found nodes ipv4
    /// find_node
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub nodes: Option<CompactNodes>,
    /// found nodes ipv6
    /// find_node
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub nodes6: Option<CompactNodes>,
    /// used for future "announce_peer"
    /// get_peers
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub token: Option<Value>,
    /// list of the torrent peers
    /// get_peers
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub values: Option<CompactAddresses>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KrpcMessage {
    /// A string value representing a transaction ID
    pub t: String,
    /// Type of the message: q for QUERY, r for RESPONSE, e for ERROR
    pub y: MessageType,
    /// Query method (one of 4: "ping", "find_node", "get_peers", "announce_peer")
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub q: Option<QueryType>,
    /// Named arguments sent with a query
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub a: Option<KrpcQuery>,
    /// Named return values
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub r: Option<KrpcResponse>,
    /// Return error list
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub e: Option<KrpcError>,
    /// ReadOnly
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub ro: Option<bool>,
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::bencode::{from_bytes, to_bytes};
    #[test]
    fn test_krpc_error() {
        let error = KrpcMessage {
            t: "aa".to_string(),
            y: MessageType::Error,
            e: Some(KrpcError::new(201, "A Generic Error Ocurred")),
            ..Default::default()
        };
        let buf = b"d1:eli201e23:A Generic Error Ocurrede1:t2:aa1:y1:ee";
        assert_eq!(to_bytes(&error).unwrap(), buf);
        assert_eq!(error, from_bytes(buf).unwrap());
    }

    #[test]
    fn test_krpc_ping() {
        let query = KrpcQuery {
            id: HashPiece::new(*b"abcdefghij0123456789"),
            ..Default::default()
        };
        let ping_req = KrpcMessage {
            t: "aa".to_string(),
            y: MessageType::Query,
            q: Some(QueryType::Ping),
            a: Some(query),
            ..Default::default()
        };
        let buf = b"d1:ad2:id20:abcdefghij0123456789e1:q4:ping1:t2:aa1:y1:qe";
        assert_eq!(to_bytes(&ping_req).unwrap(), buf);
        assert_eq!(ping_req, from_bytes(buf).unwrap());

        let rsp = KrpcResponse {
            id: HashPiece::new(*b"mnopqrstuvwxyz123456"),
            ..Default::default()
        };
        let ping_rsp = KrpcMessage {
            t: "aa".to_string(),
            y: MessageType::Response,
            q: None,
            r: Some(rsp),
            ..Default::default()
        };

        let buf = b"d1:rd2:id20:mnopqrstuvwxyz123456e1:t2:aa1:y1:re";
        assert_eq!(to_bytes(&ping_rsp).unwrap(), buf);
        assert_eq!(ping_rsp, from_bytes(buf).unwrap());
    }

    #[test]
    fn test_krpc_get_peer() {
        let query = KrpcQuery {
            id: HashPiece::new(*b"abcdefghij0123456789"),
            info_hash: Some(HashPiece::new(*b"mnopqrstuvwxyz123456")),
            ..Default::default()
        };
        let get_peer_req = KrpcMessage {
            t: "aa".to_string(),
            y: MessageType::Query,
            q: Some(QueryType::GetPeers),
            a: Some(query),
            ..Default::default()
        };
        let buf = b"d1:ad2:id20:abcdefghij01234567899:info_hash20:mnopqrstuvwxyz123456e1:q9:get_peers1:t2:aa1:y1:qe";
        assert_eq!(to_bytes(&get_peer_req).unwrap(), buf);
        assert_eq!(get_peer_req, from_bytes(buf).unwrap());

        let rsp = KrpcResponse {
            id: HashPiece::new(*b"abcdefghij0123456789"),
            token: Some("aoeusnth".into()),
            values: Some(vec![b"axje.u", b"idhtnm"].into()),
            ..Default::default()
        };
        let peers_rsp = KrpcMessage {
            t: "aa".to_string(),
            y: MessageType::Response,
            q: None,
            r: Some(rsp),
            ..Default::default()
        };

        let buf = b"d1:rd2:id20:abcdefghij01234567895:token8:aoeusnth6:valuesl6:axje.u6:idhtnmee1:t2:aa1:y1:re";
        assert_eq!(to_bytes(&peers_rsp).unwrap(), buf);
        assert_eq!(peers_rsp, from_bytes(buf).unwrap());

        let rsp = KrpcResponse {
            id: HashPiece::new(*b"abcdefghij0123456789"),
            token: Some("aoeusnth".into()),
            nodes: Some(vec![b"12345678901234567890123456"].into()),
            ..Default::default()
        };
        let node_rsp = KrpcMessage {
            t: "aa".to_string(),
            y: MessageType::Response,
            q: None,
            r: Some(rsp),
            ..Default::default()
        };
        let buf =
            b"d1:rd2:id20:abcdefghij01234567895:nodes26:123456789012345678901234565:token8:aoeusnthe1:t2:aa1:y1:re";
        assert_eq!(to_bytes(&node_rsp).unwrap(), buf);
        assert_eq!(node_rsp, from_bytes(buf).unwrap());
    }

    #[test]
    fn test_krpc_a() {
        let query = KrpcQuery {
            id: HashPiece::new(*b"abcdefghij0123456789"),
            target: Some(HashPiece::new(*b"mnopqrstuvwxyz123456")),
            ..Default::default()
        };
        let find_node_req = KrpcMessage {
            t: "aa".to_string(),
            y: MessageType::Query,
            q: Some(QueryType::FindNode),
            a: Some(query),
            ..Default::default()
        };
        let buf = b"d1:ad2:id20:abcdefghij01234567896:target20:mnopqrstuvwxyz123456e1:q9:find_node1:t2:aa1:y1:qe";
        assert_eq!(to_bytes(&find_node_req).unwrap(), buf);
        assert_eq!(find_node_req, from_bytes(buf).unwrap());

        let rsp = KrpcResponse {
            id: HashPiece::new(*b"0123456789abcdefghij"),
            nodes: Some(vec![b"12345678901234567890123456"].into()),
            ..Default::default()
        };
        let find_node_rsp = KrpcMessage {
            t: "aa".to_string(),
            y: MessageType::Response,
            q: None,
            r: Some(rsp),
            ..Default::default()
        };

        let buf =
            b"d1:rd2:id20:0123456789abcdefghij5:nodes26:12345678901234567890123456e1:t2:aa1:y1:re";
        assert_eq!(to_bytes(&find_node_rsp).unwrap(), buf);
        assert_eq!(find_node_rsp, from_bytes(buf).unwrap());
    }

    #[test]
    fn test_krpc_announce_peer() {
        let query = KrpcQuery {
            id: HashPiece::new(*b"abcdefghij0123456789"),
            implied_port: Some(true),
            info_hash: Some(HashPiece::new(*b"mnopqrstuvwxyz123456")),
            port: Some(6881),
            token: Some("aoeusnth".into()),
            ..Default::default()
        };
        let announce_peer_node_req = KrpcMessage {
            t: "aa".to_string(),
            y: MessageType::Query,
            q: Some(QueryType::AnnouncePeer),
            a: Some(query),
            ..Default::default()
        };
        let buf = b"d1:ad2:id20:abcdefghij012345678912:implied_porti1e9:info_hash20:mnopqrstuvwxyz1234564:porti6881e5:token8:aoeusnthe1:q13:announce_peer1:t2:aa1:y1:qe";
        assert_eq!(to_bytes(&announce_peer_node_req).unwrap(), buf);
        assert_eq!(announce_peer_node_req, from_bytes(buf).unwrap());

        let rsp = KrpcResponse {
            id: HashPiece::new(*b"mnopqrstuvwxyz123456"),
            ..Default::default()
        };
        let find_node_rsp = KrpcMessage {
            t: "aa".to_string(),
            y: MessageType::Response,
            q: None,
            r: Some(rsp),
            ..Default::default()
        };

        let buf = b"d1:rd2:id20:mnopqrstuvwxyz123456e1:t2:aa1:y1:re";
        assert_eq!(to_bytes(&find_node_rsp).unwrap(), buf);
        assert_eq!(find_node_rsp, from_bytes(buf).unwrap());
    }
}
