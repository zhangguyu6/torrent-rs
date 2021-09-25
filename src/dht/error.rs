use crate::bencode::BencodeError;
use crate::krpc::KrpcError;
use async_std::channel::RecvError;
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
    #[error("Krpc {0}")]
    Krpc(#[from] KrpcError),
    #[error("Protocol {0}")]
    Protocol(String),
    #[error("InVaildToken")]
    InVaildToken,
    #[error("TransactionNotFound")]
    TransactionNotFound,
    #[error("ChannelClosed {0}")]
    ChannelClose(#[from] RecvError),
}
