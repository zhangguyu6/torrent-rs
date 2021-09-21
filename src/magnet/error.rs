use crate::metainfo::MetaInfoError;
use data_encoding::DecodeError;
use hex::FromHexError;
use std::result;
use thiserror::Error;
use url::{ParseError as ParseUrlError, Url};

pub type Result<T> = result::Result<T, MagnetError>;

#[derive(Error, Debug)]
pub enum MagnetError {
    #[error("BrokenMagnetLink {0}")]
    BrokenMagnetLink(Url),
    #[error("BASE32 {0}")]
    BASE32(#[from] DecodeError),
    #[error("ParseUrl {0}")]
    FromParseUrl(#[from] ParseUrlError),
    #[error("FromHex {0}")]
    FromHex(#[from] FromHexError),
    #[error("MetaInfo {0}")]
    MetaInfo(#[from] MetaInfoError),
}
