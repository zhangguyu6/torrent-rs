use crate::error::Result;
use crate::krpc::KrpcMessage;
use crate::metainfo::{HashPiece, Node, PeerAddress};
use async_oneshot::{oneshot, Receiver, Sender};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DhtReq {
    Ping(PeerAddress),
    FindNode(PeerAddress, HashPiece),
    GetPeers(HashPiece),
    AnnouncePeer(HashPiece),
    ShutDown,
}

pub(crate) enum DhtRsp {
    Done,
    GetPeers(Vec<Node>),
}

pub(crate) enum DhtMessage {
    Req(DhtReq, Sender<Result<DhtRsp>>),
    Rsp(KrpcMessage),
}

pub(crate) struct Transaction {
    callback: Sender<Result<DhtRsp>>,
    depth: usize,
    ids: Option<HashSet<HashPiece>>,
}

impl Transaction {
    pub fn new(callback: Sender<Result<DhtRsp>>) -> Self {
        Self {
            callback,
            depth: 0,
            ids: None,
        }
    }

    pub fn insert_id(&mut self, id: HashPiece) -> bool {
        let ids = self.ids.get_or_insert(HashSet::new());
        ids.insert(id)
    }

    pub fn contain_id(&self, id: &HashPiece) -> bool {
        match &self.ids {
            None => false,
            Some(ids) => ids.contains(id),
        }
    }
}
