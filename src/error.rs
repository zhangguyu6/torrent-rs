use crate::krpc::KrpcError;
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
    #[error("PathConvertErr")]
    PathConvertErr,
    #[error("EmptyRootPath")]
    EmptyRootPath,
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
    KrpcErr(KrpcError),
    #[error("DhtAddrBindErr")]
    DhtAddrBindErr,
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
