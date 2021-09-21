use crate::bencode::BencodeError;
use std::io;
use std::net::AddrParseError;
use std::result;
use thiserror::Error;

pub type Result<T> = result::Result<T, DhtError>;

#[derive(Error, Debug)]
pub enum DhtError {
    #[error("Io {0}")]
    IoErr(#[from] io::Error),
    #[error("DhtAddrBind failed")]
    DhtAddrBind,
    #[error("Address {0}")]
    Address(#[from] AddrParseError),
    #[error("Bencode {0}")]
    Bencode(#[from] BencodeError),
}
