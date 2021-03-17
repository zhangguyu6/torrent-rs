use serde::{de, ser};
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
    #[error("ConvertErr {0}")]
    ConvertIntErr(#[from] num::TryFromIntError),
    #[error("CustomErr {0}")]
    CustomErr(String),
    #[error("ConvertCharErr {0}")]
    ConvertCharErr(#[from] char::CharTryFromError),
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
