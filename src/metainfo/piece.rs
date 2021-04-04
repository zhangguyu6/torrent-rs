use super::Info;
use crate::bencode::{to_bytes, Value};
use crate::error::Result;
use crate::utils::Chains;
use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use sha1::{Digest, Sha1};
use smol::{
    fs::OpenOptions,
    io::{AsyncReadExt, BufReader},
};
use std::convert::TryInto;
use std::fmt;
use std::mem::size_of;
use std::ops::BitXor;
use std::path::PathBuf;
use std::result::Result as StdResult;
use std::usize;

pub const PIECE_SIZE_256_KB: u64 = 1024 * 256;
pub const PIECE_SIZE_512_KB: u64 = 2 * PIECE_SIZE_256_KB;
pub const PIECE_SIZE_1M: u64 = 2 * PIECE_SIZE_512_KB;
pub const PIECE_SIZE_2M: u64 = 2 * PIECE_SIZE_1M;
pub(crate) const ID_LEN: usize = 20;

#[derive(Debug, PartialEq, Eq, Default, Clone, PartialOrd, Ord)]
pub struct HashPiece([u8; ID_LEN]);

impl HashPiece {
    pub fn new(hash_val: [u8; ID_LEN]) -> Self {
        Self(hash_val)
    }

    pub(crate) fn leading_zeros(&self) -> usize {
        let mut zeros = 0;
        for v in self.0.iter() {
            zeros += v.leading_zeros() as usize;
            if *v != 0 {
                break;
            }
        }
        zeros
    }

    pub(crate) fn bits(&self) -> usize {
        ID_LEN * size_of::<u8>() - self.leading_zeros()
    }
}

impl BitXor for HashPiece {
    type Output = HashPiece;

    fn bitxor(self, rhs: Self) -> Self::Output {
        let mut pieces = HashPiece::default();
        for i in 0..ID_LEN {
            pieces.0[i] = self.0[i] ^ rhs.0[i];
        }
        pieces
    }
}

impl BitXor for &HashPiece {
    type Output = HashPiece;

    fn bitxor(self, rhs: Self) -> Self::Output {
        let mut pieces = HashPiece::default();
        for i in 0..ID_LEN {
            pieces.0[i] = self.0[i] ^ rhs.0[i];
        }
        pieces
    }
}

impl Serialize for HashPiece {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for HashPiece {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct HashPieceVisitor;
        impl<'de> Visitor<'de> for HashPieceVisitor {
            type Value = HashPiece;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`HashPiece`")
            }
            fn visit_bytes<E>(self, v: &[u8]) -> StdResult<Self::Value, E>
            where
                E: Error,
            {
                if v.len() != ID_LEN {
                    return Err(Error::custom("v.len not expected".to_string()));
                }

                let chunk = HashPiece(v[0..20].try_into().unwrap());

                Ok(chunk)
            }
        }
        deserializer.deserialize_bytes(HashPieceVisitor)
    }
}

impl From<Info> for HashPiece {
    fn from(info: Info) -> Self {
        let buf = to_bytes(&info).unwrap();
        let mut hasher = Sha1::new();
        hasher.update(&buf);
        let hash_val: [u8; 20] = hasher.finalize().into();
        HashPiece::new(hash_val)
    }
}

impl From<Value> for HashPiece {
    fn from(value: Value) -> Self {
        let buf = to_bytes(&value).unwrap();
        let mut hasher = Sha1::new();
        hasher.update(&buf);
        let hash_val: [u8; 20] = hasher.finalize().into();
        HashPiece::new(hash_val)
    }
}

impl AsRef<[u8]> for HashPiece {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsMut<[u8]> for HashPiece {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct HashPieces(pub Vec<HashPiece>);

impl HashPieces {
    pub async fn hashes(mut paths: Vec<PathBuf>, piece_length: u64) -> Result<Self> {
        assert!(paths.len() >= 1);
        let mut hasher = Sha1::new();
        let mut hash_vec = Vec::new();
        let mut buf: Vec<u8> = vec![0; piece_length as usize];
        let mut readers = Vec::new();
        for p in paths.drain(..) {
            let f = OpenOptions::new().read(true).open(p).await?;
            readers.push(f);
        }
        let mut reader_chains =
            BufReader::with_capacity(piece_length as usize, Chains::new(readers));
        let mut index = 0;
        loop {
            match reader_chains.read(&mut buf[index..]).await? {
                0 => {
                    if index != 0 {
                        let hash_chunk = HashPiece(hasher.finalize().into());
                        hash_vec.push(hash_chunk);
                    }
                    break;
                }
                n => {
                    hasher.update(&buf[index..index + n]);
                    if n == piece_length as usize {
                        index = 0;
                        let hash_chunk = HashPiece(hasher.finalize().into());
                        hash_vec.push(hash_chunk);
                        hasher = Sha1::new();
                    } else {
                        index += n;
                    }
                }
            }
        }

        Ok(HashPieces(hash_vec))
    }
}

impl Serialize for HashPieces {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut buf: Vec<u8> = Vec::with_capacity(self.0.len() * 20);
        for chunk in self.0.iter() {
            buf.extend_from_slice(&chunk.0);
        }
        serializer.serialize_bytes(&buf)
    }
}

impl<'de> Deserialize<'de> for HashPieces {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct HashPiecesVisitor;
        impl<'de> Visitor<'de> for HashPiecesVisitor {
            type Value = HashPieces;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`HashPieces`")
            }
            fn visit_bytes<E>(self, v: &[u8]) -> StdResult<Self::Value, E>
            where
                E: Error,
            {
                if v.len() % ID_LEN != 0 {
                    return Err(Error::custom("v.len not expected".to_string()));
                }
                let len = v.len() / ID_LEN;
                let mut hashv: Vec<HashPiece> = Vec::with_capacity(len);
                for i in 0..len {
                    let chunk = HashPiece(v[i * ID_LEN..i * ID_LEN + ID_LEN].try_into().unwrap());
                    hashv.push(chunk)
                }
                Ok(HashPieces(hashv))
            }
        }
        deserializer.deserialize_bytes(HashPiecesVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::{from_bytes, to_bytes};
    use smol::block_on;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_hashvec() {
        let chunk1 = HashPiece([
            255, 93, 150, 97, 206, 117, 188, 48, 195, 49, 19, 17, 246, 228, 209, 84, 108, 107, 32,
            165,
        ]);
        let chunk2 = HashPiece([
            93, 150, 97, 206, 117, 188, 48, 195, 49, 19, 17, 246, 228, 209, 84, 108, 107, 32, 165,
            240,
        ]);
        let hashvec1 = HashPieces(vec![chunk1, chunk2]);
        let s = to_bytes(&hashvec1);
        assert!(s.is_ok());
        let h = from_bytes(s.unwrap().as_slice());
        assert!(h.is_ok());
        assert_eq!(hashvec1, h.unwrap());
    }

    #[test]
    fn test_gen_hashes() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        write!(tmpfile, "Hello World!").unwrap();
        let p = tmpfile.into_temp_path();
        let path = p.to_path_buf();
        block_on(async {
            let hashes = HashPieces::hashes(vec![path], 1024).await;
            let mut hasher = Sha1::new();
            hasher.update("Hello World!".as_bytes());
            let v: [u8; 20] = hasher.finalize().into();
            assert_eq!(v, hashes.unwrap().0[0].0);
        });
    }
}
