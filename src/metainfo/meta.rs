use super::PeerAddress;
use crate::bencode::Value;
use crate::error::{Error, Result};
use serde::{
    de::{self, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::result::Result as StdResult;
use std::{collections::HashSet, fmt, str};
use url::Url;

#[derive(Debug, PartialEq, Eq)]
pub struct UrlList(Vec<Url>);

impl Serialize for UrlList {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.0.len() == 1 {
            serializer.serialize_str(&self.0[0].as_str())
        } else {
            let urls: Vec<&str> = self.0.iter().map(|url| url.as_str()).collect();
            urls.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for UrlList {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct URLListVisitor;
        impl<'de> Visitor<'de> for URLListVisitor {
            type Value = UrlList;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`String or Vec<String>`")
            }
            fn visit_bytes<E>(self, v: &[u8]) -> StdResult<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(UrlList(vec![
                    Url::parse(str::from_utf8(v).unwrap()).unwrap()
                ]))
            }
            fn visit_str<E>(self, v: &str) -> StdResult<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(UrlList(vec![Url::parse(v).unwrap()]))
            }

            fn visit_seq<A>(self, mut seq: A) -> StdResult<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut v = Vec::new();
                while let Some(elem) = seq.next_element::<String>()? {
                    v.push(Url::parse(&elem).unwrap());
                }
                Ok(UrlList(v))
            }
        }
        deserializer.deserialize_any(URLListVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
/// The MetaInfo represents the .torrent file.
pub struct MetaInfo {
    /// Info dictionary
    pub info: Value,

    /// The URL of the tracker single
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub announce: Option<String>,

    /// The URL of the tracker mutli
    #[serde(rename = "announce-list")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub announce_list: Vec<Vec<String>>,

    /// The list of dht nodes
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub nodes: Vec<PeerAddress>,

    /// The list of web addresses where torrent data can be retrieved
    #[serde(rename = "url-list")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub url_list: Option<UrlList>,

    /// The creation time of the torrent, UNIX epoch
    #[serde(rename = "creation date")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub creation_date: Option<u64>,

    /// The free-form textual comments of the author
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub comment: String,

    /// The name and version of the program used to create the .torrent
    #[serde(rename = "created by")]
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub created_by: String,

    /// The string encoding format used to generate the pieces
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub encoding: String,
}

impl MetaInfo {
    pub fn get_name(&self) -> Result<String> {
        match &self.info {
            Value::Dict(m) => {
                if let Some(v) = m.get("name") {
                    match v {
                        Value::Bytes(buf) => return Ok(String::from_utf8(buf.clone())?),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Err(Error::CustomErr("not find name".to_string()))
    }
    pub fn get_trackers(&self) -> Result<Vec<Url>> {
        if let Some(announce) = &self.announce {
            return Ok(vec![Url::parse(announce)?]);
        }
        let mut seen = HashSet::new();
        let urls = self
            .announce_list
            .iter()
            .flatten()
            .flat_map(|s| {
                if seen.contains(s) {
                    None
                } else {
                    let url = Url::parse(s).unwrap();
                    seen.insert(s);
                    Some(url)
                }
            })
            .collect();
        Ok(urls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::{from_bytes, from_str, to_bytes, to_str};

    #[test]
    fn test_url_list() {
        let a = "http://qq1.com/".to_string();
        assert_eq!(
            to_str(&UrlList(vec![Url::parse(&a).unwrap()])).unwrap(),
            "15:http://qq1.com/".to_string()
        );
        assert_eq!(
            from_str::<UrlList>("15:http://qq1.com/").unwrap(),
            UrlList(vec![Url::parse(&a).unwrap()])
        );
        let b = "http://qq2.com/".to_string();
        assert_eq!(
            to_str(&UrlList(vec![
                Url::parse(&a).unwrap(),
                Url::parse(&b).unwrap()
            ]))
            .unwrap(),
            "l15:http://qq1.com/15:http://qq2.com/e".to_string()
        );
        assert_eq!(
            from_str::<UrlList>("l15:http://qq1.com/15:http://qq2.com/e").unwrap(),
            UrlList(vec![Url::parse(&a).unwrap(), Url::parse(&b).unwrap()])
        )
    }

    #[test]
    fn test_meta_info() {
        let raw_torrent =
            include_bytes!("example/archlinux-2011.08.19-netinstall-i686.iso.torrent");
        // let raw_torrent = include_bytes!("example/1.txt.torrent");
        let res = from_bytes::<MetaInfo>(raw_torrent);
        assert!(res.is_ok());
        let meta_info_a = res.unwrap();
        let meta_info_b =
            from_bytes::<MetaInfo>(to_bytes(&meta_info_a).unwrap().as_slice()).unwrap();
        dbg!(&meta_info_a, &meta_info_b);
        assert_eq!(&meta_info_a, &meta_info_b);
        assert_eq!(to_bytes(&meta_info_b).unwrap().as_slice(), &raw_torrent[..]);
    }
}
