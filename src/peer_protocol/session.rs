use super::error::{Error, Result};
use super::message::{HandshakeMessage, HandshakeMessageCodec, Message, MessageCodec};
use crate::metainfo;
use async_std::channel::Receiver;
use async_std::io::{Read, Write};
use asynchronous_codec::Framed;
use futures::{SinkExt, StreamExt};

pub enum Command {}

/// Session represents a connection context to a peer.
pub struct Session<C> {
    /// inner connection to peer
    conn: C,
    /// communication channel to send command to seesion
    cmd_rx: Receiver<Command>,
    /// hash of the torrent info
    info_hash: metainfo::HashPiece,
    /// id of the local peer
    id: metainfo::HashPiece,
    /// id of the remote peer we are connected to
    peer_id: metainfo::HashPiece,
    /// local peer is choked, local doesn't allow to download pieces from remote
    am_choking: bool,
    /// local is interested in pieces, remote has any of them
    am_interested: bool,
    /// remote peer is choked, remote doesn't allow to download pieces from local
    peer_choking: bool,
    /// remote peer is interested in pieces, local has any of them
    peer_interested: bool,
    /// has the handshake completed?
    handshake_done: bool,
    /// maximum number of pieces that can be requested at once
    max_request_queue_len: usize,
}

impl<C: Read + Write + Unpin> Session<C> {
    /// Create a new session.
    pub fn new(
        conn: C,
        cmd_rx: Receiver<Command>,
        info_hash: metainfo::HashPiece,
        id: metainfo::HashPiece,
    ) -> Self {
        Session {
            conn,
            cmd_rx,
            info_hash,
            id,
            peer_id: metainfo::HashPiece::default(),
            am_choking: true,
            am_interested: false,
            peer_choking: true,
            peer_interested: false,
            handshake_done: false,
            max_request_queue_len: 16,
        }
    }

    /// Initiate the handshake to the remote peer and handle message.
    pub async fn initiate_loop(&mut self) -> Result<()> {
        unimplemented!()
    }

    /// Accept the handshake from the remote peer and handle message.
    pub async fn accept_loop(&mut self) -> Result<()> {
        unimplemented!()
    }

    /// The initiator of a connection is expected to send handshake.
    /// The recipient may wait for the initiator's handshake.
    async fn handshake(&mut self, is_initated: bool) -> Result<()> {
        let mut framed = Framed::new(&mut self.conn, HandshakeMessageCodec);
        if is_initated {
            let message = HandshakeMessage::new(self.info_hash.clone(), self.id.clone());
            framed.send(message).await?;
        }
        if let Some(message) = framed.next().await {
            let message = message?;
            if message.info_hash != self.info_hash {
                return Err(Error::InvaildInfoHash);
            }
            self.peer_id = message.peer_id.clone();
        } else {
            return Err(Error::MessageEndUnexpected);
        }

        if !is_initated {
            let message = HandshakeMessage::new(self.info_hash.clone(), self.id.clone());
            framed.send(message).await?;
        }
        return Ok(());
    }
}
