use serde::{de, ser};
use std::char::CharTryFromError;
use std::num::{ParseIntError, TryFromIntError};
use std::str::Utf8Error;
use std::string::FromUtf8Error;
use std::{fmt::Display, io, result};
use thiserror::Error;

pub type Result<T> = result::Result<T, BencodeError>;

#[derive(Error, Debug)]
pub enum BencodeError {
    #[error("Io {0}")]
    Io(#[from] io::Error),
    #[error("failed to convert string from utf8, err:{0}")]
    StringFromUtf8(#[from] FromUtf8Error),
    #[error("failed to convert str from utf8,err:{0}")]
    StrFromUtf8(#[from] Utf8Error),
    #[error("failed to parse int, err:{0}")]
    ParserInt(#[from] ParseIntError),
    #[error("failed to convert int, err:{0}")]
    ConvertInt(#[from] TryFromIntError),
    #[error("failed to convert char, err:{0}")]
    ConvertChar(#[from] CharTryFromError),
    #[error("UnexpectedValueType {0}")]
    UnexpectedValueType(String),
    #[error("Custom {0}")]
    Custom(String),
}

impl ser::Error for BencodeError {
    fn custom<T: Display>(msg: T) -> Self {
        BencodeError::Custom(msg.to_string())
    }
}

impl de::Error for BencodeError {
    fn custom<T: Display>(msg: T) -> Self {
        BencodeError::Custom(msg.to_string())
    }
}
