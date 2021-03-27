use super::{HashPiece, PeerAddress};
use crate::{
    error::Error,
    metainfo::{meta::MetaInfo, Info},
};
use data_encoding::BASE32;
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
    /// xt
    info_hash: HashPiece,
    /// tr
    trackers: Vec<Url>,
    /// dn
    name: String,
    /// x.pe
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

// impl From<MetaInfo> for MagnetLink {
//     fn from(metainfo: MetaInfo) -> Self {
//         let trackers = metainfo.get_trackers().unwrap();
//         let name = metainfo.info.name.clone();
//         let info_hash = metainfo.info.into();
//         let peers = metainfo.nodes;
//         Self {
//             info_hash,
//             trackers,
//             name,
//             peers,
//         }
//     }
// }

impl TryFrom<Url> for MagnetLink {
    type Error = Error;
    fn try_from(value: Url) -> Result<Self, Self::Error> {
        if value.scheme() == MAGNET {
            let mut info_hash = HashPiece::default();
            let mut trackers = Vec::new();
            let mut name = String::new();
            let mut peers = Vec::new();
            for (key, val) in value.query_pairs() {
                match key.as_ref() {
                    "xt" => match &val[0..V1_PREFIX.len()] {
                        V1_PREFIX => {
                            let encoded = &val[V1_PREFIX.len()..];
                            if encoded.len() == 40 {
                                hex::decode_to_slice(encoded, info_hash.as_mut())?;
                            } else if encoded.len() == 32 {
                                match BASE32.decode_mut(encoded.as_bytes(), info_hash.as_mut()) {
                                    Ok(_) => {}
                                    Err(e) => return Err(Error::from(e.error)),
                                }
                            } else {
                                return Err(Error::BrokenMagnetLinkErr(value));
                            }
                        }
                        _ => return Err(Error::BrokenMagnetLinkErr(value)),
                    },
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
            Err(Error::BrokenMagnetLinkErr(value))
        }
    }
}

impl TryInto<Url> for MagnetLink {
    type Error = Error;
    fn try_into(mut self) -> Result<Url, Self::Error> {
        let mut link = Url::parse(format!("{}:", MAGNET).as_str())?;
        let mut query_pairs = link.query_pairs_mut();
        let hex_hash = hex::encode(self.info_hash.as_mut());
        query_pairs.append_pair("xt", &hex_hash);
        for track in self.trackers {
            query_pairs.append_pair("tr", track.as_str());
        }
        query_pairs.append_pair("dn", &self.name);
        for peer in self.peers {
            query_pairs.append_pair("x.pe", &peer.to_string());
        }
        drop(query_pairs);
        Ok(link)
    }
}

#[cfg(test)]
mod tests {
    use sha1::{Digest, Sha1};

    use super::*;
    use crate::{from_bytes, from_str, to_bytes};

    #[test]
    fn test_magnetlink() {
        let url = Url::parse("magnet:?xt=urn:btih:da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert!(url.is_ok());
        dbg!(MagnetLink::try_from(url.unwrap()));
        let info: Info = Info::default();
        dbg!(MagnetLink::from(info));
        // let link = MagnetLink::from(Infohash:;
        // assert_eq!(
        //     link.to_url().as_str(),
        //     "magnet:?xt=urn:btih:da39a3ee5e6b4b0d3255bfef95601890afd80709"
        // );
        // let raw_torrent = include_bytes!("example/1.txt.torrent");
        let raw_torrent = include_bytes!("example/bootstrap.dat.torrent");
        // let raw_torrent =
        // include_bytes!("example/archlinux-2011.08.19-netinstall-i686.iso.torrent");
        let res = from_bytes::<MetaInfo>(raw_torrent);

        assert!(res.is_ok());
        let meta_info_a = res.unwrap();
        // dbg!(&meta_info_a);

        let mut hasher = Sha1::new();
        hasher.update(to_bytes(&meta_info_a.info).unwrap());
        // let hash: HashPiece = meta_info_a.info.into();
        let hash: [u8; 20] = hasher.finalize().into();
        dbg!(&hash);
        let mut info_hash = [0; 40];
        dbg!(hex::encode(hash.as_ref()));
    }
}
