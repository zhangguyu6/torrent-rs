use super::hashes::HashVec;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use smol::{fs, stream::StreamExt};
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Ord)]
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
    pub async fn walk<P: AsRef<Path>>(root: P) -> Result<(Vec<File>, Vec<PathBuf>)> {
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
                        deque.push_front(entry.path())
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Info {
    /// Name of the file in the single file case
    /// Or the name of the directory in the muliple file case
    name: String,
    /// The number of bytes in each piece the file is split into
    /// Almost always a power of two
    #[serde(rename = "piece length")]
    piece_length: u64,
    /// A string whose length is a multiple of 20
    /// It is to be subdivided into strings of length 20,
    /// Each of which is the SHA1 hash of the piece at the corresponding index
    pieces: HashVec,
    /// The length of the file in bytes in the single file case
    #[serde(skip_serializing_if = "Option::is_none")]
    length: Option<u64>,
    /// The list of all the files in the multi-file case
    #[serde(skip_serializing_if = "Vec::is_empty")]
    files: Vec<File>,
}

impl Info {
    pub fn is_multi(&self) -> bool {
        self.length.is_none()
    }
}
