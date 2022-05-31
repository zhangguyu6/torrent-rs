use serde::{
    de::{Error as DeError, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::fmt;
use std::net::{IpAddr, SocketAddr};

pub(crate) const ADDRESS_V4_LEN: usize = 6;
pub(crate) const ADDRESS_V6_LEN: usize = 18;

/// IPv6/v4 address information for a single peer
/// This type is used by  [`super::meta::MetaInfo`]
/// See bep_0005 & bep_0032
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct PeerAddress(pub SocketAddr);

impl Serialize for PeerAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let port = self.0.port();
        match self.0.ip() {
            IpAddr::V4(v4) => (v4.to_string(), port).serialize(serializer),
            IpAddr::V6(v6) => (v6.to_string(), port).serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for PeerAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PeerAddressVisitor;
        impl<'de> Visitor<'de> for PeerAddressVisitor {
            type Value = PeerAddress;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`ipv4+port` or `ipv6+port`")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let ip: String = seq
                    .next_element()?
                    .ok_or_else(|| DeError::invalid_length(0, &self))?;
                let port: u16 = seq
                    .next_element()?
                    .ok_or_else(|| DeError::invalid_length(0, &self))?;
                Ok(PeerAddress(SocketAddr::new(ip.parse().unwrap(), port)))
            }
        }
        deserializer.deserialize_seq(PeerAddressVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_bencode::de::from_str;
    use serde_bencode::ser::to_string;
    #[test]
    fn test_address_bencode() {
        let addr1 = PeerAddress("1.2.3.4:1234".parse().unwrap());
        assert_eq!(to_string(&addr1).unwrap(), "l7:1.2.3.4i1234ee".to_string());
        let addr2: PeerAddress = from_str("l7:1.2.3.4i1234ee").unwrap();
        assert_eq!(addr1, addr2);
    }
}
