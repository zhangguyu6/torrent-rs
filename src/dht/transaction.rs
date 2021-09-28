use super::error::Result;
use super::DhtRsp;
use crate::krpc::QueryType;
use crate::metainfo::HashPiece;
use async_std::channel::Sender;
use rand::{thread_rng, Rng};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub(crate) struct Transaction {
    pub seq: usize,
    pub tx: Sender<Result<DhtRsp>>,
    pub depth: usize,
    pub target: Option<HashPiece>,
    pub ids: HashSet<HashPiece>,
    pub query_type: QueryType,
}

impl Transaction {
    pub fn new(
        tx: Sender<Result<DhtRsp>>,
        depth: usize,
        target: Option<HashPiece>,
        query_type: QueryType,
    ) -> Self {
        Self {
            seq: 0,
            tx,
            depth,
            target,
            ids: HashSet::new(),
            query_type,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TransactionManager {
    trans: HashMap<String, Transaction>,
    tran_seq: usize,
}

impl Default for TransactionManager {
    fn default() -> Self {
        let mut rng = thread_rng();
        let tran_seq: usize = rng.gen_range(0..usize::MAX / 2);
        let trans = HashMap::default();
        Self { trans, tran_seq }
    }
}

impl TransactionManager {
    pub fn insert(&mut self, mut tran: Transaction) -> usize {
        self.tran_seq += 1;
        tran.seq = self.tran_seq;
        self.trans.insert(self.tran_seq.to_string(), tran);
        self.tran_seq
    }
    pub fn remove(&mut self, tran_id: &String) -> Option<Transaction> {
        self.trans.remove(tran_id)
    }
}
