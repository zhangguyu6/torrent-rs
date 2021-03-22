use super::{Info, PeerAddress};
use serde::{
    de::{Error, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::fmt;
use std::result::Result as StdResult;

#[derive(Debug, PartialEq, Eq)]
pub struct URLList(Vec<String>);

impl Serialize for URLList {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.0.len() == 1 {
            serializer.serialize_str(&self.0[0])
        } else {
            self.0.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for URLList {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct URLListVisitor;
        impl<'de> Visitor<'de> for URLListVisitor {
            type Value = URLList;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`String or Vec<String>`")
            }
            fn visit_bytes<E>(self, v: &[u8]) -> StdResult<Self::Value, E>
            where
                E: Error,
            {
                Ok(URLList(vec![String::from_utf8(v.into()).unwrap()]))
            }
            fn visit_str<E>(self, v: &str) -> StdResult<Self::Value, E>
            where
                E: Error,
            {
                Ok(URLList(vec![v.to_string()]))
            }

            fn visit_seq<A>(self, mut seq: A) -> StdResult<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut v = Vec::new();
                while let Some(elem) = seq.next_element()? {
                    v.push(elem);
                }
                Ok(URLList(v))
            }
        }
        deserializer.deserialize_any(URLListVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
/// Represents the .torrent file
pub struct MetaInfo {
    /// Info dictionary
    info: Info,

    /// The URL of the tracker single
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    announce: Option<String>,

    /// The URL of the tracker mutli
    #[serde(rename = "announce-list")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    announce_list: Vec<Vec<String>>,

    /// The list of dht nodes
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    nodes: Vec<PeerAddress>,

    /// The list of web addresses where torrent data can be retrieved
    #[serde(rename = "url-list")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    url_list: Option<URLList>,

    /// The creation time of the torrent, UNIX epoch
    #[serde(rename = "creation date")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    creation_date: Option<u64>,

    /// The free-form textual comments of the author
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    comment: String,

    /// The name and version of the program used to create the .torrent
    #[serde(rename = "created by")]
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    created_by: String,

    /// The string encoding format used to generate the pieces
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    encoding: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::{from_bytes, from_str, to_bytes, to_str};

    #[test]
    fn test_URLList() {
        let a = "http://qq1.com".to_string();
        assert_eq!(
            to_str(&URLList(vec![a.clone()])).unwrap(),
            "14:http://qq1.com".to_string()
        );
        assert_eq!(
            from_str::<URLList>("14:http://qq1.com").unwrap(),
            URLList(vec![a.clone()])
        );
        let b = "http://qq2.com".to_string();
        assert_eq!(
            to_str(&URLList(vec![a.clone(), b.clone()])).unwrap(),
            "l14:http://qq1.com14:http://qq2.come".to_string()
        );
        assert_eq!(
            from_str::<URLList>("l14:http://qq1.com14:http://qq2.come").unwrap(),
            URLList(vec![a, b])
        )
    }

    #[test]
    fn test_meta_info() {
        let raw_torrent =
            include_bytes!("example/archlinux-2011.08.19-netinstall-i686.iso.torrent");
        let res = from_bytes::<MetaInfo>(raw_torrent);
        assert!(res.is_ok());
        let meta_info_a = res.unwrap();
        let meta_info_b =
            from_bytes::<MetaInfo>(to_bytes(&meta_info_a).unwrap().as_slice()).unwrap();
        assert_eq!(&meta_info_a, &meta_info_b);
        assert_eq!(to_bytes(&meta_info_b).unwrap().as_slice(), &raw_torrent[..]);
    }
}
