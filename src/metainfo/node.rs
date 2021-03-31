use super::{PeerAddress, ADDRESS_V4_LEN, ADDRESS_V6_LEN, ID_LEN};
use crate::metainfo::HashPiece;
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::convert::TryInto;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

pub const NODE_V4_LEN: usize = ADDRESS_V4_LEN + ID_LEN;
pub const NODE_V6_LEN: usize = ADDRESS_V6_LEN + ID_LEN;

#[derive(Debug, PartialEq, Eq)]
/// Node Info
pub struct Node {
    id: HashPiece,
    peer_address: PeerAddress,
}

impl Into<Vec<u8>> for &Node {
    fn into(self) -> Vec<u8> {
        let port = self.peer_address.0.port().to_be_bytes();
        match self.peer_address.0.ip() {
            IpAddr::V4(v4) => {
                let mut buf = Vec::with_capacity(NODE_V4_LEN);
                buf.extend_from_slice(self.id.as_ref());
                buf.extend_from_slice(&v4.octets());
                buf.extend_from_slice(&port);
                buf
            }
            IpAddr::V6(v6) => {
                let mut buf = Vec::with_capacity(NODE_V4_LEN);
                buf.extend_from_slice(self.id.as_ref());
                buf.extend_from_slice(&v6.octets());
                buf.extend_from_slice(&port);
                buf
            }
        }
    }
}

impl From<&[u8]> for Node {
    fn from(v: &[u8]) -> Self {
        let id = HashPiece::new(v[0..ID_LEN].try_into().unwrap());
        let peer_address;
        if v.len() == NODE_V4_LEN {
            let ip_buf: [u8; ADDRESS_V4_LEN - 2] = v[0..ADDRESS_V4_LEN - 2].try_into().unwrap();
            let ip = Ipv4Addr::from(ip_buf);
            let port = u16::from_be_bytes([v[ADDRESS_V4_LEN - 2], v[ADDRESS_V4_LEN - 1]]);
            peer_address = PeerAddress(SocketAddr::new(IpAddr::V4(ip), port))
        } else if v.len() == NODE_V6_LEN {
            let ip_buf: [u8; ADDRESS_V6_LEN - 2] = v[0..ADDRESS_V6_LEN - 2].try_into().unwrap();
            let ip = Ipv6Addr::from(ip_buf);
            let port = u16::from_be_bytes([v[ADDRESS_V6_LEN - 2], v[ADDRESS_V6_LEN - 1]]);
            peer_address = PeerAddress(SocketAddr::new(IpAddr::V6(ip), port))
        } else {
            unreachable!()
        }
        Self { id, peer_address }
    }
}

/// Compacted ID/IP-address/port info
#[derive(Debug, PartialEq, Eq)]
pub struct CompactNodes(Vec<Node>);

impl Serialize for CompactNodes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut buf = Vec::new();
        for node in self.0.iter() {
            let node_buf: Vec<u8> = node.into();
            buf.extend(node_buf);
        }
        serializer.serialize_bytes(&buf)
    }
}

impl<'de> Deserialize<'de> for CompactNodes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct CompactNodesVisitor;
        impl<'de> Visitor<'de> for CompactNodesVisitor {
            type Value = CompactNodes;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`ipv4+port` or `ipv6+port`")
            }
            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v.len() % NODE_V4_LEN != 0 || v.len() % NODE_V6_LEN != 0 {
                    return Err(de::Error::custom("v.len not expected".to_string()));
                }
                if v.len() % NODE_V4_LEN == 0 {
                    let len = v.len() / NODE_V4_LEN;
                    let mut nodes = Vec::new();
                    for i in 0..len {
                        let node = Node::from(&v[i * NODE_V4_LEN..(i + 1) * NODE_V4_LEN]);
                        nodes.push(node);
                    }
                    Ok(CompactNodes(nodes))
                } else {
                    let len = v.len() / NODE_V6_LEN;
                    let mut nodes = Vec::new();
                    for i in 0..len {
                        let node = Node::from(&v[i * NODE_V6_LEN..(i + 1) * NODE_V6_LEN]);
                        nodes.push(node);
                    }
                    Ok(CompactNodes(nodes))
                }
            }
        }
        deserializer.deserialize_byte_buf(CompactNodesVisitor)
    }
}
