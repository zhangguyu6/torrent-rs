use std::net::AddrParseError;
use std::path::StripPrefixError;
use std::{io, result};
use thiserror::Error;

pub type Result<T> = result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Address(#[from] AddrParseError),
    #[error("{0}")]
    StripPrefix(#[from] StripPrefixError),
    #[error("Path convert failed")]
    PathConvert,
    #[error("Root path is empty")]
    EmptyRootPath,
}
