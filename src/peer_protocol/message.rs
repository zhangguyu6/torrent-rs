use crate::peer_protocol::error::PeerProtocolError;
use bitvec::prelude::{BitVec, Lsb0};
use bytes::{Buf, BufMut};
use std::convert::TryFrom;
use std::mem::transmute;

/// The handshake starts with character ninteen (decimal) followed by the string 'BitTorrent protocol'
pub const ProtocolHeader: &'static str = "\x13BitTorrent protocol";

/// Message Type of Peer Messages
#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MessageType {
    /// BEP 3
    Choke = 0,
    UnChoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    BitFiled = 5,
    Request = 6,
    Piece = 7,
    Cancel = 8,
}

impl TryFrom<u8> for MessageType {
    type Error = PeerProtocolError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value >= MessageType::Choke as u8 && value <= MessageType::Cancel as u8 {
            Ok(unsafe { transmute(value) })
        } else {
            Err(PeerProtocolError::MessageTypeNotSupport(value))
        }
    }
}

impl Default for MessageType {
    fn default() -> Self {
        Self::Choke
    }
}

/// Meesage is the message used by the peer protocol.
#[derive(Debug, PartialEq, Eq, Default)]
pub struct Message {
    is_keepalive: bool,
    msg_type: MessageType,
    /// Represents the index which that downloader just completed and checked the hash of.
    /// Used by Have, Request, Piece, Cancel
    index: usize,
    /// Represents the start of byte offsets.
    /// Used by Reqeust, Cancel, Piece
    begin: usize,
    /// Represents the length of byte offsets.
    /// Used by Reqeust, Cancel
    length: usize,
    /// Represents the data correlated with request messages implicitly.
    /// Used by Piece
    piece: Vec<u8>,
    /// Represents a bitfield with each index that downloader has sent set to one and the rest set to zero.
    /// Used by BitFiled
    bit_field: BitVec<Lsb0, u8>,
}

impl Into<Vec<u8>> for &Message {
    fn into(self) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut buf_mut = &mut buf[..];
        // The 4-bytes is the placeholder of the length.
        buf_mut.put_bytes(b'0', 4);
        if !self.is_keepalive {
            buf_mut.put_u8(self.msg_type as u8);
            use MessageType::*;
            match self.msg_type {
                Choke | UnChoke | Interested | NotInterested => {}
                Request | Cancel => {
                    buf_mut.put_u32(self.index as u32);
                    buf_mut.put_u32(self.begin as u32);
                    buf_mut.put_u32(self.length as u32);
                }
                Have => {
                    buf_mut.put_u32(self.index as u32);
                }
                BitFiled => buf_mut.put(self.bit_field.as_raw_slice()),
                Piece => {
                    buf_mut.put_u32(self.index as u32);
                    buf_mut.put_u32(self.begin as u32);
                    buf_mut.put(self.piece.as_slice());
                }
            }
            // Reset the length of the message body
            let len = buf.len() - 4;
            let mut buf_mut = &mut buf[..];
            buf_mut.put_u32(len as u32);
        }
        buf
    }
}

impl TryFrom<&[u8]> for Message {
    type Error = PeerProtocolError;
    fn try_from(mut value: &[u8]) -> Result<Self, Self::Error> {
        // read length
        if value.len() < 4 {
            return Err(PeerProtocolError::WrongMessageLength);
        }
        let msg_len = value.get_u32() as usize;
        if msg_len > value.len() - 4 {
            return Err(PeerProtocolError::MessageEndUnexpected);
        }
        let mut message = Message::default();
        if msg_len == 0 {
            message.is_keepalive = true;
            return Ok(message);
        }
        message.msg_type = MessageType::try_from(value.get_u8())?;
        use MessageType::*;
        match message.msg_type {
            Choke | UnChoke | Interested | NotInterested => {}
            Request | Cancel => {
                message.index = value.get_u32() as usize;
                message.begin = value.get_u32() as usize;
                message.length = value.get_u32() as usize;
            }
            Have => {
                message.index = value.get_u32() as usize;
            }
            BitFiled => {
                message.bit_field = BitVec::from_slice(value.get(0..msg_len - 1).unwrap())?;
            }
            Piece => {
                message.index = value.get_u32() as usize;
                message.begin = value.get_u32() as usize;
                message.piece = value.get(0..msg_len - 1 - 4 - 4).unwrap().to_vec();
            }
        }
        Ok(message)
    }
}
