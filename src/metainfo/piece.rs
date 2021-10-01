use super::error::Result;
use super::Info;
use crate::bencode::{to_bytes, Value};
use async_std::{
    io::{self, Read, ReadExt},
    task::ready,
};
use rand::random;
use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use sha1::{Digest, Sha1};
use std::convert::TryInto;
use std::fmt;
use std::ops::BitXor;
use std::pin::Pin;
use std::result::Result as StdResult;
use std::task::{Context, Poll};
use std::usize;

pub const PIECE_SIZE_256_KB: u64 = 1024 * 256;
pub const PIECE_SIZE_512_KB: u64 = 2 * PIECE_SIZE_256_KB;
pub const PIECE_SIZE_1M: u64 = 2 * PIECE_SIZE_512_KB;
pub const PIECE_SIZE_2M: u64 = 2 * PIECE_SIZE_1M;
pub(crate) const ID_LEN: usize = 20;

/// HashPiece represents the SHA1 hash of the piece at the corresponding index.
#[derive(Debug, PartialEq, Eq, Default, Clone, PartialOrd, Ord, Hash)]
pub struct HashPiece([u8; ID_LEN]);

impl HashPiece {
    pub fn new(hash_val: [u8; ID_LEN]) -> Self {
        Self(hash_val)
    }

    pub fn rand_new() -> Self {
        let hash_val = random();
        Self(hash_val)
    }

    /// Returns the number of one in the binary representation of HashPiece.
    pub fn count_ones(&self) -> usize {
        self.0
            .iter()
            .fold(0, |res, byte| byte.count_ones() as usize + res)
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

impl From<&[u8]> for HashPiece {
    /// Create HashPieces by hashing the given bytes.
    fn from(bytes: &[u8]) -> Self {
        let mut hasher = Sha1::new();
        hasher.update(bytes);
        let hash_val: [u8; 20] = hasher.finalize().into();
        HashPiece::new(hash_val)
    }
}

impl From<Info> for HashPiece {
    /// Create HashPieces by hashing the Info dictionary.
    fn from(info: Info) -> Self {
        let buf = to_bytes(&info).unwrap();
        let mut hasher = Sha1::new();
        hasher.update(&buf);
        let hash_val: [u8; 20] = hasher.finalize().into();
        HashPiece::new(hash_val)
    }
}

impl From<Value> for HashPiece {
    /// Create HashPieces by hashing the Bencode Value.
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

/// HashPieces represents a concatenation of each piece's SHA-1 hash
#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct HashPieces(pub Vec<HashPiece>);

impl HashPieces {
    /// Create HashPieces by hashing the giving piece
    pub async fn hash_pieces<R: Read + Unpin>(
        piece_readers: Vec<R>,
        piece_length: u64,
    ) -> Result<Self> {
        assert!(piece_readers.len() >= 1);
        let mut hasher = Sha1::new();
        let mut hash_vec = Vec::new();
        let mut buf: Vec<u8> = vec![0; piece_length as usize];
        let mut readers = Chains::new(piece_readers);
        let mut index = 0;
        loop {
            match readers.read(&mut buf[index..]).await? {
                0 => {
                    if index != 0 {
                        let hash_chunk = HashPiece(hasher.finalize().into());
                        hash_vec.push(hash_chunk);
                    }
                    break;
                }
                n => {
                    hasher.update(&buf[index..index + n]);
                    if index == piece_length as usize {
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

struct Chains<R> {
    readers: Vec<R>,
    last_active: usize,
}

impl<R> Chains<R> {
    fn new(readers: Vec<R>) -> Self {
        assert!(!readers.is_empty());
        let last_active = 0;
        Self {
            readers,
            last_active,
        }
    }
}

impl<R: fmt::Debug> fmt::Debug for Chains<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chains").field("r", &self.readers).finish()
    }
}

impl<R: Read + Unpin> Read for Chains<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            let last_active = self.last_active;
            let max_last_active = self.readers.len() - 1;
            let readers: &mut R = self.readers.get_mut(last_active).unwrap();
            match ready!(Pin::new(readers).poll_read(cx, buf)) {
                Ok(0) if !buf.is_empty() => {
                    if last_active == max_last_active {
                        return Poll::Ready(Ok(0));
                    }
                    if last_active < max_last_active {
                        self.last_active += 1;
                    }
                }
                Ok(n) => return Poll::Ready(Ok(n)),
                Err(err) => return Poll::Ready(Err(err)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::{from_bytes, to_bytes};
    use async_std::fs::OpenOptions;
    use async_std::task::block_on;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_chains() {
        let input_a: &[u8] = b"hello";
        let input_b: &[u8] = b"world";
        let mut chains = Chains::new(vec![input_a, input_b]);
        let mut buf = Vec::new();
        block_on(async move {
            let result = chains.read_to_end(&mut buf).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), input_a.len() + input_b.len());
            assert_eq!(buf.as_slice(), &b"helloworld"[..]);
        })
    }

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
            let reader = OpenOptions::new().read(true).open(path).await.unwrap();
            let hashes = HashPieces::hash_pieces(vec![reader], 1024).await;
            let mut hasher = Sha1::new();
            hasher.update("Hello World!".as_bytes());
            let v: [u8; 20] = hasher.finalize().into();
            assert_eq!(v, hashes.unwrap().0[0].0);
        });
    }
}
