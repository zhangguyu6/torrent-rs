use bitvec::ptr::BitSpanError;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("Message length read failed")]
    WrongMessageLength,
    #[error("buf is less than Message length")]
    MessageEndUnexpected,
    #[error("Message Type Num {0} Not Supposrt")]
    MessageTypeNotSupport(u8),
    #[error("Create BitField from &[u8] failed, err:{0:?}")]
    BitSpanError(#[from] BitSpanError<u8>),
    #[error("Receive info_hash that not currently serving")]
    InvaildInfoHash,
}
