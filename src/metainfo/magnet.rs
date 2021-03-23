use super::{HashPiece, PeerAddress};
use crate::error::Error;
use data_encoding::BASE32;
use std::{convert::TryInto, str::FromStr};

const MAGNET_PREFIX: &'static str = "magnet:?";
const V1_PREFIX: &'static str = "urn:btih:";
const V2_PREFIX: &'static str = "urn:btmh:";

/// a link on a web page only containing enough information to join the swarm
/// see bep 9
pub struct MagnetLink {
    /// xt
    info_hash: HashPiece,
    /// tr
    trackers: Vec<String>,
    /// dn
    name: String,
    /// x.pe
    peers: Vec<PeerAddress>,
    /// btih or btmh
    multihash: bool,
}

impl FromStr for MagnetLink {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(remain) = s.strip_prefix(MAGNET_PREFIX) {
            let mut info_hash = HashPiece([0; 20]);
            let mut trackers = Vec::new();
            let mut name = String::new();
            let mut peers = Vec::new();
            let mut multihash = false;
            for pair in remain.split('&') {
                let pair_vec: Vec<&str> = pair.split('=').collect();
                if pair_vec.len() != 2 {
                    return Err(Error::BrokenMagnetLinkErr(s.to_string()));
                }
                let key = pair_vec[0];
                let val = pair_vec[1];
                match key {
                    "xt" => match &val[0..V1_PREFIX.len()] {
                        V1_PREFIX => {
                            let encoded = &val[V1_PREFIX.len()..];
                            if encoded.len() == 40 {
                                let buf: Vec<u8> = (0..40)
                                    .step_by(2)
                                    .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                                    .collect();
                                info_hash = HashPiece(buf.try_into().unwrap());
                            } else if encoded.len() == 32 {
                                match BASE32.decode_mut(encoded.as_bytes(), &mut info_hash.0) {
                                    Ok(_) => {}
                                    Err(e) => return Err(Error::from(e.error)),
                                }
                            } else {
                                return Err(Error::BrokenMagnetLinkErr(s.to_string()));
                            }
                        }
                        V2_PREFIX => {
                            multihash = true;
                        }
                        _ => return Err(Error::BrokenMagnetLinkErr(s.to_string())),
                    },
                    "tr" => {
                        trackers.push(val.to_string());
                    }
                    "dn" => {
                        name = val.to_string();
                    }
                    "x.pe" => {
                        let peer = PeerAddress::from_str(val)?;
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
                multihash,
            })
        } else {
            Err(Error::BrokenMagnetLinkErr(s.to_string()))
        }
    }
}
