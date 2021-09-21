use crate::bencode::BencodeError;
use crate::krpc::KrpcError;
use std::net::AddrParseError;
use std::path::StripPrefixError;
use std::{io, result};
use thiserror::Error;

pub type Result<T> = result::Result<T, MetaInfoError>;

#[derive(Error, Debug)]
pub enum MetaInfoError {
    #[error("Io {0}")]
    Io(#[from] io::Error),
    #[error("Address {0}")]
    Address(#[from] AddrParseError),
    #[error("StripPrefix {0}")]
    StripPrefix(#[from] StripPrefixError),
    #[error("PathConvert")]
    PathConvert,
    #[error("EmptyRootPath")]
    EmptyRootPath,
    #[error("Krpc {0}")]
    Krpc(#[from] KrpcError),
    #[error("Bencode {0}")]
    Bencode(#[from] BencodeError),
}
