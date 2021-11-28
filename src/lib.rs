#![feature(associated_type_bounds)]
#![feature(btree_drain_filter)]
#![feature(hash_drain_filter)]

pub mod bencode;
pub mod dht;
pub mod error;
pub mod krpc;
pub mod magnet;
pub mod metainfo;
pub mod peer_protocol;

pub use bencode::{from_bytes, from_str, to_bytes, to_str, Deserializer, Serializer};
pub use error::Error;
