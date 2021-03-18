use serde::{Deserialize, Serialize};

const ID_LEN: usize = 20;
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Id(#[serde(with = "serde_bytes")] Vec<u8>);

impl From<&'static str> for Id {
    fn from(s: &'static str) -> Self {
        if s.len() != ID_LEN {
            panic!("overflow")
        }
        Self(s.into())
    }
}

impl From<String> for Id {
    fn from(s: String) -> Self {
        if s.len() != ID_LEN {
            panic!("overflow")
        }
        Self(s.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::to_str;
    #[test]
    fn test_id() {
        let id = Id::from("01234567890123456789");
        assert_eq!(to_str(&id).unwrap(), "20:01234567890123456789".to_string())
    }
}
