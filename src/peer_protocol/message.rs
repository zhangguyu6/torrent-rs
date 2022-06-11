use crate::metainfo::HashPiece;
use crate::peer_protocol::error::Error;
use asynchronous_codec::{Decoder, Encoder};
use bitvec::prelude::{BitVec, Lsb0};
use bytes::{Buf, BufMut, Bytes};

/// Meesage is the message used by the peer protocol.
/// All of the remaining messages in the protocol take the form of <length prefix><message ID><payload>.
/// The length prefix is a four byte big-endian value.
/// The message ID is a single decimal byte.
/// The payload is message dependent.
#[derive(Debug, PartialEq, Eq)]
pub enum Message {
    KeepAlive,
    Choke,
    UnChoke,
    Intersted,
    NotInterested,
    Have {
        piece_index: usize,
    },
    BitField {
        bitfield: BitVec<Lsb0, u8>,
    },
    Request {
        /// integer specifying the zero-based piece index
        piece_index: usize,
        /// integer specifying the zero-based byte offset within the piece
        block_begin: usize,
        /// integer specifying the length of the block to be requested
        block_length: usize,
    },
    Piece {
        piece_index: usize,
        block_begin: usize,
        block_data: Bytes,
    },
    Cancel {
        piece_index: usize,
        block_begin: usize,
        block_length: usize,
    },
    Port {
        port: u16,
    },
}

pub(crate) struct MessageCodec;

impl Encoder for MessageCodec {
    type Item = Message;
    type Error = Error;

    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut asynchronous_codec::BytesMut,
    ) -> Result<(), Self::Error> {
        todo!()
    }
}

impl Decoder for MessageCodec {
    type Item = Message;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut asynchronous_codec::BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        todo!()
    }
}

/// HandshakeMessage represents the handshake message used by the peer protocol.
pub struct HandshakeMessage {
    /// string identifier of the protocol, must be "BitTorrent protocol"
    pub pstr: [u8; 19],
    /// eight (8) reserved bytes. All current implementations use all zeroes
    pub reserved: [u8; 8],
    /// 20-byte SHA1 hash of the info key in the metainfo file
    pub info_hash: HashPiece,
    /// 20-byte string used as a unique ID for the client.
    pub peer_id: HashPiece,
}

impl HandshakeMessage {
    pub fn new(info_hash: HashPiece, peer_id: HashPiece) -> Self {
        HandshakeMessage {
            pstr: *b"BitTorrent protocol",
            reserved: [0; 8],
            info_hash,
            peer_id,
        }
    }
}

pub(crate) struct HandshakeMessageCodec;

impl Encoder for HandshakeMessageCodec {
    type Item = HandshakeMessage;
    type Error = Error;

    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut asynchronous_codec::BytesMut,
    ) -> Result<(), Self::Error> {
        todo!()
    }
}

impl Decoder for HandshakeMessageCodec {
    type Item = HandshakeMessage;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut asynchronous_codec::BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        todo!()
    }
}
