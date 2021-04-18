use crate::error::Result;
use crate::krpc::KrpcMessage;
use crate::metainfo::{HashPiece, Node, PeerAddress};
use smol::channel::Sender;
use std::net::SocketAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DhtReq {
    Ping(PeerAddress),
    FindNode(PeerAddress, HashPiece),
    GetPeers(HashPiece),
    AnnouncePeer(HashPiece),
    ShutDown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DhtRsp {
    Pong,
    FindNode(Option<Node>),
    GetPeers(PeerAddress),
    Announced,
    ShutDown,
}

#[derive(Debug)]
pub enum DhtMessage {
    Req(DhtReq, Sender<Result<DhtRsp>>),
    Refresh,
    Message(KrpcMessage, SocketAddr),
}
