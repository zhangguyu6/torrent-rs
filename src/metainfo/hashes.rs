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

#[derive(Debug, PartialEq, Eq)]
struct HashChunk([u8; 20]);

#[derive(Debug, PartialEq, Eq)]
pub struct HashVec(Vec<HashChunk>);

impl HashVec {
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
                        let hash_chunk = HashChunk(hasher.finalize().into());
                        hash_vec.push(hash_chunk);
                    }
                    break;
                }
                n => {
                    hasher.update(&buf[index..]);
                    if n == piece_length as usize {
                        index = 0;
                        let hash_chunk = HashChunk(hasher.finalize().into());
                        hash_vec.push(hash_chunk);
                        hasher = Sha1::new();
                    } else {
                        index += n;
                    }
                }
            }
        }

        Ok(HashVec(hash_vec))
    }
}

impl Serialize for HashVec {
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

impl<'de> Deserialize<'de> for HashVec {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct HashVecVisitor;
        impl<'de> Visitor<'de> for HashVecVisitor {
            type Value = HashVec;
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
                let mut hashv: Vec<HashChunk> = Vec::with_capacity(len);
                for i in 0..len {
                    let chunk = HashChunk(v[i..i + 20].try_into().unwrap());
                    hashv.push(chunk)
                }
                Ok(HashVec(hashv))
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
        let chunk = HashChunk([
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
        ]);
        let hashvec1 = HashVec(vec![chunk]);
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
            let hashes = HashVec::hashes(vec![path], 1024).await;
            dbg!(hashes);
        });
    }
}
