use super::error::{Error, Result};
use super::piece::HashPieces;
use async_std::{fs, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// File represents a file in a torrent.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Ord, Clone)]
pub struct File {
    /// Length of the file in bytes
    length: u64,
    /// A list of UTF-8 encoded strings corresponding to subdirectory names,
    /// The last of which is the actual file name
    #[serde(rename = "path")]
    paths: Vec<String>,
}

impl PartialOrd for File {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.paths.cmp(&other.paths))
    }
}

impl File {
    pub async fn generate_from_root<P: AsRef<Path>>(root: P) -> Result<(Vec<File>, Vec<PathBuf>)> {
        let mut files = Vec::new();
        let mut paths = Vec::new();
        let mut deque: VecDeque<PathBuf> = VecDeque::new();
        deque.push_back(root.as_ref().into());
        while !deque.is_empty() {
            let p = deque.pop_front().unwrap();
            if p.is_dir() {
                if let Ok(mut dir_entrys) = fs::read_dir(p).await {
                    while let Some(entry) = dir_entrys.next().await {
                        let entry = entry?;
                        deque.push_front(entry.path().into())
                    }
                }
            } else {
                if let Ok(meta_data) = fs::metadata(&p).await {
                    assert!(meta_data.file_type().is_file());
                    let path = p.strip_prefix(root.as_ref())?;
                    files.push(File {
                        length: meta_data.len(),
                        paths: path
                            .iter()
                            .map(|p| p.to_owned().into_string().unwrap())
                            .collect(),
                    });
                    paths.push(p);
                }
            }
        }
        Ok((files, paths))
    }
}

/// Info represents a dictionary that describes the file(s) of the torrent.
/// This type is used by  [`super::meta::MetaInfo`]
/// There are two possible forms:
/// one for the case of a 'single-file' torrent with no directory structure,
/// and one for the case of a 'multi-file' torrent (see https://wiki.theory.org/index.php/BitTorrentSpecification#Metainfo_File_Structure for details)
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Default, Clone)]
pub struct Info {
    /// Name of the file in the single file case
    /// Or the name of the directory in the muliple file case
    pub name: String,
    /// The number of bytes in each piece the file is split into
    /// Almost always a power of two
    #[serde(rename = "piece length")]
    pub piece_length: u64,
    /// A string whose length is a multiple of 20
    /// It is to be subdivided into strings of length 20,
    /// Each of which is the SHA1 hash of the piece at the corresponding index
    pub pieces: HashPieces,
    /// The length of the file in bytes in the single file case
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub length: Option<u64>,
    /// The list of all the files in the multi-file case
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub files: Vec<File>,
}

impl Info {
    pub async fn new<P: AsRef<Path>>(root_path: P, piece_length: u64) -> Result<Self> {
        let name = match root_path.as_ref().file_name() {
            Some(s) => s
                .to_str()
                .map_or(Err(Error::PathConvert), |v| Ok(v.to_string()))?,
            None => return Err(Error::EmptyRootPath),
        };
        let (files, paths) = File::generate_from_root(root_path).await?;
        let mut readers = Vec::with_capacity(paths.len());
        for path in paths {
            readers.push(fs::OpenOptions::new().read(true).open(path).await?);
        }
        let pieces = HashPieces::hash_pieces(readers, piece_length).await?;
        if files.len() == 1 {
            Ok(Self {
                name,
                piece_length,
                pieces,
                length: Some(files[0].length),
                files: Vec::new(),
            })
        } else {
            Ok(Self {
                name,
                piece_length,
                pieces,
                length: None,
                files,
            })
        }
    }
    pub fn is_multi(&self) -> bool {
        self.length.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metainfo::piece::PIECE_SIZE_256_KB;
    use async_std::task::block_on;
    use serde_bencode::{de::from_bytes, ser::to_bytes};
    use std::io::Write;
    use tempfile::{tempdir, NamedTempFile};

    #[test]
    fn test_info() {
        let dir = tempdir().unwrap();
        dbg!(dir.path());
        let mut tmpfile1 = NamedTempFile::new_in(dir.path()).unwrap();
        write!(tmpfile1, "Hello World!1").unwrap();

        let mut tmpfile2 = NamedTempFile::new_in(dir.path()).unwrap();
        write!(tmpfile2, "Hello World!2").unwrap();

        block_on(async {
            let info = Info::new(dir.path(), PIECE_SIZE_256_KB).await;
            dbg!(&info);
            assert!(info.is_ok());
            let info = info.unwrap();
            let value = to_bytes(&info);
            assert!(value.is_ok(), "{}", value.unwrap_err());
            let info1 = from_bytes(value.unwrap().as_ref());
            assert!(info1.is_ok());
            let info1 = info1.unwrap();
            assert_eq!(info, info1);
        });
    }
}
