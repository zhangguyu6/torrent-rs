use super::DhtRsp;
use crate::error::Result;
use crate::krpc::QueryType;
use crate::metainfo::HashPiece;
use smol::channel::Sender;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Instant;

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
