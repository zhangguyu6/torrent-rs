use std::fmt::{Display, Formatter, Result};

/// Errors, or KRPC message dictionaries with a "y" value of "e",
/// contain one additional key "e". The value of "e" is a list.
/// The first element is an integer representing the error code.
/// The second element is a string containing the error message.
#[derive(Debug, Clone, Copy)]
#[repr(i64)]
pub enum KrpcErrorCode {
    Generic = 201,
    Server = 202,
    Protocol = 203,
    Method = 204,
    MessageValueFieldTooBig = 205,
    InvalidSignature = 206,
    SaltFieldTooBig = 207,
    CasHashMismatched = 301,
    SequenceNumberLessThanCurrent = 302,
}

#[derive(Debug, Clone)]
pub struct KrpcError {
    code: KrpcErrorCode,
    desc: String,
}

impl KrpcError {
    pub fn new<S: Into<String>>(code: KrpcErrorCode, desc: S) -> Self {
        let desc = desc.into();
        Self { code, desc }
    }
}

impl Display for KrpcError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{:?}:{:?}", self.code, self.desc)
    }
}
