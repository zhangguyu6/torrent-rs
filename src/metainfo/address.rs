use crate::Error;
use serde::{
    de::{self, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::convert::TryInto;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::str::FromStr;

pub(crate) const ADDRESS_V4_LEN: usize = 6;
pub(crate) const ADDRESS_V6_LEN: usize = 18;

/// IPv6/v4 contact information for a single peer,  see bep_0005 & bep_0032
#[derive(Debug, Eq, PartialEq)]
pub struct PeerAddress(pub(crate) SocketAddr);

impl FromStr for PeerAddress {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

impl ToString for PeerAddress {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

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
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let port: u16 = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                Ok(PeerAddress(SocketAddr::new(ip.parse().unwrap(), port)))
            }
        }
        deserializer.deserialize_tuple_struct("NodeAddress", 1, PeerAddressVisitor)
    }
}

impl Into<Vec<u8>> for &PeerAddress {
    fn into(self) -> Vec<u8> {
        let port = self.0.port().to_be_bytes();
        match self.0.ip() {
            IpAddr::V4(v4) => {
                let mut buf = Vec::with_capacity(ADDRESS_V4_LEN);
                buf.extend_from_slice(&v4.octets());
                buf.extend_from_slice(&port);
                buf
            }
            IpAddr::V6(v6) => {
                let mut buf = Vec::with_capacity(ADDRESS_V6_LEN);
                buf.extend_from_slice(&v6.octets());
                buf.extend_from_slice(&port);
                buf
            }
        }
    }
}

impl From<&[u8]> for PeerAddress {
    fn from(v: &[u8]) -> Self {
        if v.len() == ADDRESS_V4_LEN {
            let ip_buf: [u8; 4] = v[0..4].try_into().unwrap();
            let ip = Ipv4Addr::from(ip_buf);
            let port = u16::from_be_bytes([v[4], v[5]]);
            PeerAddress(SocketAddr::new(IpAddr::V4(ip), port))
        } else if v.len() == ADDRESS_V6_LEN {
            let ip_buf: [u8; 16] = v[0..16].try_into().unwrap();
            let ip = Ipv6Addr::from(ip_buf);
            let port = u16::from_be_bytes([v[4], v[5]]);
            PeerAddress(SocketAddr::new(IpAddr::V6(ip), port))
        } else {
            unreachable!()
        }
    }
}

/// Compacted IP-address/port info
#[derive(Debug, PartialEq, Eq)]
pub struct CompactAddresses(Vec<PeerAddress>);

impl Serialize for CompactAddresses {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut buf = Vec::new();
        for addr in self.0.iter() {
            let node_buf: Vec<u8> = addr.into();
            buf.extend(node_buf);
        }
        serializer.serialize_bytes(&buf)
    }
}

impl<'de> Deserialize<'de> for CompactAddresses {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct CompactAddressesVisitor;
        impl<'de> Visitor<'de> for CompactAddressesVisitor {
            type Value = CompactAddresses;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`ipv4+port` or `ipv6+port`")
            }
            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v.len() % ADDRESS_V4_LEN != 0 || v.len() % ADDRESS_V6_LEN != 0 {
                    return Err(de::Error::custom("v.len not expected".to_string()));
                }
                if v.len() % ADDRESS_V4_LEN == 0 {
                    let len = v.len() / ADDRESS_V4_LEN;
                    let mut addresses = Vec::new();
                    for i in 0..len {
                        let addr =
                            PeerAddress::from(&v[i * ADDRESS_V4_LEN..(i + 1) * ADDRESS_V4_LEN]);
                        addresses.push(addr);
                    }
                    Ok(CompactAddresses(addresses))
                } else {
                    let len = v.len() / ADDRESS_V6_LEN;
                    let mut addresses = Vec::new();
                    for i in 0..len {
                        let addr =
                            PeerAddress::from(&v[i * ADDRESS_V6_LEN..(i + 1) * ADDRESS_V6_LEN]);
                        addresses.push(addr);
                    }
                    Ok(CompactAddresses(addresses))
                }
            }
        }
        deserializer.deserialize_byte_buf(CompactAddressesVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::{from_str, to_str};

    #[test]
    fn test_address_bencode() {
        let addr1 = PeerAddress("1.2.3.4:1234".parse().unwrap());
        assert_eq!(to_str(&addr1).unwrap(), "l7:1.2.3.4i1234ee".to_string());
        let addr2: PeerAddress = from_str("l7:1.2.3.4i1234ee").unwrap();
        assert_eq!(addr1, addr2);
    }

    #[test]
    fn test_address_bin() {
        let addr1 = PeerAddress("1.2.3.4:1234".parse().unwrap());
        let buf: Vec<u8> = (&addr1).into();
        assert_eq!(buf, b"\x01\x02\x03\x04\x04\xd2");
        let addr2: PeerAddress = PeerAddress::from(&b"\x01\x02\x03\x04\x04\xd2"[..]);
        assert_eq!(addr1, addr2);
    }
}
