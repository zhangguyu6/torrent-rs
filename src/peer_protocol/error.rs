use bitvec::ptr::BitSpanError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PeerProtocolError {
    #[error("Message length read failed")]
    WrongMessageLength,
    #[error("buf is less than Message length")]
    MessageEndUnexpected,
    #[error("Message Type Num {0} Not Supposrt")]
    MessageTypeNotSupport(u8),
    #[error("Create BitField from &[u8] failed, err:{0:?}")]
    BitSpanError(#[from] BitSpanError<u8>),
}
