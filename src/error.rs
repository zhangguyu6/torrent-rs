use data_encoding::DecodeError;
use serde::{de, ser};
use std::net::AddrParseError;
use std::path::StripPrefixError;
use std::{char, fmt::Display, io, num, result, string};
use thiserror::Error;

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
    BrokenMagnetLinkErr(String),
    #[error("BASE32Err {0}")]
    BASE32Err(#[from] DecodeError),
    #[error("AddressErr {0}")]
    AddressErr(#[from] AddrParseError),
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
