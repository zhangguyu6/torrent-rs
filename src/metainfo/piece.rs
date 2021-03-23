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
use std::path::PathBuf;
use std::result::Result as StdResult;
use std::usize;

pub const PIECE_SIZE_256_KB: u64 = 1024 * 256;
pub const PIECE_SIZE_512_KB: u64 = 2 * PIECE_SIZE_256_KB;
pub const PIECE_SIZE_1M: u64 = 2 * PIECE_SIZE_512_KB;
pub const PIECE_SIZE_2M: u64 = 2 * PIECE_SIZE_1M;

#[derive(Debug, PartialEq, Eq)]
pub struct HashPiece(pub [u8; 20]);

#[derive(Debug, PartialEq, Eq)]
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
        struct HashVecVisitor;
        impl<'de> Visitor<'de> for HashVecVisitor {
            type Value = HashPieces;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`HashVec`")
            }
            fn visit_bytes<E>(self, v: &[u8]) -> StdResult<Self::Value, E>
            where
                E: Error,
            {
                if v.len() % 20 != 0 {
                    return Err(Error::custom("v.len not expected".to_string()));
                }
                let len = v.len() / 20;
                let mut hashv: Vec<HashPiece> = Vec::with_capacity(len);
                for i in 0..len {
                    let chunk = HashPiece(v[i * 20..i * 20 + 20].try_into().unwrap());
                    hashv.push(chunk)
                }
                Ok(HashPieces(hashv))
            }
        }
        deserializer.deserialize_bytes(HashVecVisitor)
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
