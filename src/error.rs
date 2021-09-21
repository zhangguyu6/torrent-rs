use crate::bencode::BencodeError;
use crate::krpc::KrpcError;
use async_std::channel::RecvError;
use async_std::future::TimeoutError;
use data_encoding::DecodeError;
use hex::FromHexError;
use serde::{de, ser};
use std::net::AddrParseError;
use std::path::StripPrefixError;
use std::str::Utf8Error;
use std::{char, fmt::Display, io, num, result, string};
use thiserror::Error;
use url::{ParseError as ParseUrlError, Url};

pub type Result<T> = result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Io Error {0}")]
    IoErr(#[from] io::Error),
    #[error("FromUtf8 {0}")]
    FromUtf8Err(#[from] string::FromUtf8Error),
    #[error("ParserErr {0}")]
    ParserIntErr(#[from] num::ParseIntError),
    #[error("StripPrefixError {0}")]
    StripPrefixErr(#[from] StripPrefixError),
    #[error("ConvertErr {0}")]
    ConvertIntErr(#[from] num::TryFromIntError),
    #[error("CustomErr {0}")]
    CustomErr(String),
    #[error("ConvertCharErr {0}")]
    ConvertCharErr(#[from] char::CharTryFromError),

    #[error("BrokenMagnetLinkErr {0}")]
    BrokenMagnetLinkErr(Url),
    #[error("BASE32Err {0}")]
    BASE32Err(#[from] DecodeError),
    #[error("AddressErr {0}")]
    AddressErr(#[from] AddrParseError),
    #[error("FromHexErr {0}")]
    FromHexErr(#[from] FromHexError),
    #[error("ParseUrlError {0}")]
    FromParseUrlErr(#[from] ParseUrlError),
    #[error("Utf8Err {0}")]
    Utf8Err(#[from] Utf8Error),
    #[error("KrpcErr {0}")]
    KrpcErr(#[from] KrpcError),
    #[error("BencodeError {0}")]
    BencodeError(#[from] BencodeError),
    #[error("DhtAddrBindErr")]
    DhtAddrBindErr,
    #[error("ChannelClosed {0}")]
    ChannelRecvErr(#[from] RecvError),
    #[error("ProtocolErr")]
    ProtocolErr,
    #[error("Transaction {0} not found, maybe timeout")]
    TransactionNotFound(usize),
    #[error("TransactionTimeout")]
    TransactionTimeout,
    #[error("TransactionRelatedNodeIsRemoved")]
    TransactionRelatedNodeIsRemoved,
    #[error("DhtServerErr {0}")]
    DhtServerErr(String),
    #[error("CallBackErr")]
    DhtCallBackErr,
    #[error("TimeoutError")]
    TimeoutError(#[from] TimeoutError),
}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::CustomErr(msg.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::CustomErr(msg.to_string())
    }
}
