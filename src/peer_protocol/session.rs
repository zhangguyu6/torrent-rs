use async_std::io::{Read, Write};

use crate::metainfo;

/// Session represents a connection context to a peer.
pub struct Session<C: Read + Write> {
    /// inner connection to peer
    conn: C,
    /// hash of the torrent info
    info_hash: metainfo::HashPiece,
    /// id of the local peer
    id: metainfo::HashPiece,
    /// id of the remote peer we are connected to
    peer_id: metainfo::HashPiece,
    /// local peer is choked
    choked: bool,
    interested: bool,
    /// remote peer is choked
    peer_choked: bool,
    perr_interested: bool,
    /// has the handshake completed?
    handshake_done: bool,
}
