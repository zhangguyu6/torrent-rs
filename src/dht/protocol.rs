use crate::error::Result;
use crate::krpc::{KrpcMessage, QueryType};
use crate::metainfo::{HashPiece, Node, PeerAddress};
use smol::channel::Sender;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Instant;

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
    Rsp(KrpcMessage, SocketAddr),
}

#[derive(Clone)]
pub(crate) struct Transaction {
    pub callback: Sender<Result<DhtRsp>>,
    pub depth: usize,
    pub target: Option<HashPiece>,
    pub ids: Rc<HashSet<HashPiece>>,
    pub query_type: QueryType,
    pub last_updated: Instant,
}

impl Transaction {
    pub fn new(
        callback: Sender<Result<DhtRsp>>,
        depth: usize,
        query_type: QueryType,
        target: Option<HashPiece>,
    ) -> Self {
        Self {
            callback,
            depth,
            target,
            ids: Rc::new(HashSet::new()),
            query_type,
            last_updated: Instant::now(),
        }
    }

    pub fn insert_id(&mut self, id: HashPiece) -> bool {
        let ids = Rc::get_mut(&mut self.ids).unwrap();
        ids.insert(id)
    }

    pub fn contain_id(&self, id: &HashPiece) -> bool {
        self.ids.get(id).is_some()
    }

    pub fn pending(&self) -> usize {
        Rc::strong_count(&self.ids)
    }
}
