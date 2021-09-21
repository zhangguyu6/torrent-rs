//! This moduie implements magnet URI format defined in https://www.bittorrent.org/beps/bep_0009.html

mod error;

use crate::metainfo::{HashPiece, PeerAddress};
use crate::metainfo::{Info, MetaInfo};
use data_encoding::BASE32;
use error::{MagnetError, Result};
use hex;
use std::{
    convert::{TryFrom, TryInto},
    str::FromStr,
};
use url::Url;

const MAGNET: &'static str = "magnet";
const V1_PREFIX: &'static str = "urn:btih:";

/// a link on a web page only containing enough information to join the swarm
/// see bep 9
#[derive(Debug)]
pub struct MagnetLink {
    info_hash: HashPiece,
    /// The tracker url.
    trackers: Vec<Url>,
    /// The display name that may be used by the client to display while waiting for metadata.
    name: String,
    /// The peer address.
    peers: Vec<PeerAddress>,
}

impl From<Info> for MagnetLink {
    fn from(info: Info) -> Self {
        let trackers = Vec::new();
        let name = info.name.clone();
        let peers = Vec::new();
        let info_hash = info.into();
        Self {
            info_hash,
            trackers,
            name,
            peers,
        }
    }
}

impl From<MetaInfo> for MagnetLink {
    fn from(metainfo: MetaInfo) -> Self {
        let trackers = metainfo.get_trackers().unwrap();

        let name = metainfo.get_name().unwrap_or_else(|_| "".to_string());
        let info_hash = metainfo.info.into();
        let peers = metainfo.nodes;
        Self {
            info_hash,
            trackers,
            name,
            peers,
        }
    }
}

impl TryFrom<Url> for MagnetLink {
    type Error = MagnetError;
    fn try_from(value: Url) -> Result<Self> {
        if value.scheme() == MAGNET {
            let mut info_hash = HashPiece::default();
            let mut trackers = Vec::new();
            let mut name = String::new();
            let mut peers = Vec::new();
            for (key, val) in value.query_pairs() {
                match key.as_ref() {
                    "xt" => {
                        if val.len() < V1_PREFIX.len() {
                            return Err(MagnetError::BrokenMagnetLink(value));
                        }
                        match &val[0..V1_PREFIX.len()] {
                            V1_PREFIX => {
                                let encoded = &val[V1_PREFIX.len()..];
                                if encoded.len() == 40 {
                                    hex::decode_to_slice(encoded, info_hash.as_mut())?;
                                } else if encoded.len() == 32 {
                                    match BASE32.decode_mut(encoded.as_bytes(), info_hash.as_mut())
                                    {
                                        Ok(_) => {}
                                        Err(e) => return Err(MagnetError::from(e.error)),
                                    }
                                } else {
                                    return Err(MagnetError::BrokenMagnetLink(value));
                                }
                            }
                            _ => return Err(MagnetError::BrokenMagnetLink(value)),
                        }
                    }

                    "tr" => {
                        trackers.push(Url::from_str(val.as_ref())?);
                    }
                    "dn" => {
                        name = val.to_string();
                    }
                    "x.pe" => {
                        let peer = PeerAddress::from_str(val.as_ref())?;
                        peers.push(peer);
                    }
                    _ => {}
                }
            }
            Ok(MagnetLink {
                info_hash,
                trackers,
                name,
                peers,
            })
        } else {
            Err(MagnetError::BrokenMagnetLink(value))
        }
    }
}

impl TryInto<Url> for MagnetLink {
    type Error = MagnetError;
    fn try_into(mut self) -> Result<Url> {
        let mut link = Url::parse(format!("{}:", MAGNET).as_str())?;
        let hex_hash = hex::encode(self.info_hash.as_mut());
        link.set_query(Some(format!("xt=urn:btih:{}", hex_hash).as_str()));
        let mut query_pairs = link.query_pairs_mut();
        query_pairs.append_pair("dn", &self.name);
        for track in self.trackers {
            query_pairs.append_pair("tr", track.as_str());
        }
        for peer in self.peers {
            query_pairs.append_pair("x.pe", &peer.to_string());
        }
        drop(query_pairs);
        Ok(link)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::from_bytes;

    #[test]
    fn test_magnetlink() {
        let url = Url::parse("magnet:?xt=urn:btih:da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert!(url.is_ok());
        assert!(MagnetLink::try_from(url.unwrap()).is_ok());
        let raw_torrent = include_bytes!("example/debian-11.0.0-amd64-netinst.iso.torrent");
        let metainfo = from_bytes::<MetaInfo>(raw_torrent);
        assert!(metainfo.is_ok());
        let metainfo = metainfo.unwrap();
        let mgn = metainfo.try_into();
        assert!(mgn.is_ok());
        let mgn: MagnetLink = mgn.unwrap();
        let url = mgn.try_into();
        assert!(url.is_ok());
        let url: Url = url.unwrap();
        assert_eq!(url.as_str(),"magnet:?xt=urn:btih:3b4bd6f8296403dfebd41062f4658f5b61d2bc26&dn=debian-11.0.0-amd64-netinst.iso&tr=http%3A%2F%2Fbttracker.debian.org%3A6969%2Fannounce");
    }
}
