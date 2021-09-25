use crate::krpc::Node;
use crate::metainfo::{HashPiece, PeerAddress};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DhtRsp {
    Pong(HashPiece),
    FindNode(Node),
    GetPeers(PeerAddress),
    Announced,
}
