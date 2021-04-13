use crate::error::Result;
use crate::krpc::KrpcMessage;
use crate::metainfo::{HashPiece, Node, PeerAddress};
use smol::channel::Sender;
use std::net::SocketAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DhtReq {
    Ping(PeerAddress),
    FindNode(PeerAddress, HashPiece),
    GetPeers(HashPiece),
    AnnouncePeer(HashPiece),
    ShutDown,
}

pub(crate) enum DhtRsp {
    Pong,
    FindNode(Option<Node>),
    GetPeers(PeerAddress),
    Announced,
    ShutDown,
}

pub(crate) enum DhtMessage {
    Req(DhtReq, Sender<Result<DhtRsp>>),
    Message(KrpcMessage, SocketAddr),
}
