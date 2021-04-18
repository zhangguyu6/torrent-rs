use super::DhtRsp;
use crate::error::{Error, Result};
use crate::krpc::QueryType;
use crate::metainfo::HashPiece;
use log::error;
use smol::channel::Sender;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;
use std::{cell::RefCell, time::Duration};

#[derive(Clone, Debug, Default)]
pub(crate) struct TransactionManager {
    /// tran_seq -> tran
    seq_transactions: HashMap<usize, Transaction>,
    /// target_id => tran_seq
    id_seqs: HashMap<HashPiece, HashSet<usize>>,
}

impl TransactionManager {
    pub fn insert(&mut self, seq: usize, tran: Transaction) {
        if let Some(id) = tran.target.as_ref() {
            if let Some(seqs) = self.id_seqs.get_mut(id) {
                seqs.insert(seq);
            }
        }
        self.seq_transactions
            .insert(seq, tran)
            .expect_none("tran is duplicate")
    }

    pub fn remove(&mut self, seq: &usize) -> Option<Transaction> {
        if let Some(tran) = self.seq_transactions.remove(&seq) {
            if let Some(id) = tran.target.as_ref() {
                if let Some(seqs) = self.id_seqs.get_mut(id) {
                    seqs.remove(&seq);
                }
            }
            return Some(tran);
        }
        None
    }

    pub fn remove_by_node(&mut self, id: &HashPiece) -> Vec<Transaction> {
        let mut removed_trans = Vec::new();
        if let Some(seqs) = self.id_seqs.remove(id) {
            for seq in seqs {
                if let Some(tran) = self.seq_transactions.remove(&seq) {
                    removed_trans.push(tran);
                }
            }
        }
        removed_trans
    }

    pub fn refresh(&mut self, time_out: Duration) -> Vec<Transaction> {
        let now = Instant::now();
        let mut removed_trans = Vec::new();
        let removed_tran_pairs = self
            .seq_transactions
            .drain_filter(|_, tran| tran.last_updated - now > time_out);
        for (seq, tran) in removed_tran_pairs {
            if let Some(id) = tran.target.as_ref() {
                if let Some(seqs) = self.id_seqs.get_mut(id) {
                    seqs.remove(&seq);
                }
            }
            removed_trans.push(tran);
        }
        removed_trans
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Transaction {
    pub callback: Option<Sender<Result<DhtRsp>>>,
    pub depth: usize,
    pub target: Option<HashPiece>,
    pub ids: Rc<RefCell<HashSet<HashPiece>>>,
    pub query_type: QueryType,
    pub last_updated: Instant,
}

impl Transaction {
    pub fn new(
        callback: Option<Sender<Result<DhtRsp>>>,
        depth: usize,
        target: Option<HashPiece>,
        query_type: QueryType,
    ) -> Self {
        Self {
            callback,
            depth,
            target,
            ids: Rc::new(RefCell::new(HashSet::new())),
            query_type,
            last_updated: Instant::now(),
        }
    }

    pub fn insert_id(&mut self, id: HashPiece) -> bool {
        self.ids.borrow_mut().insert(id)
    }

    pub fn contain_id(&self, id: &HashPiece) -> bool {
        self.ids.borrow().get(id).is_some()
    }

    pub async fn callback(&self, rsp: Result<DhtRsp>) -> Result<()> {
        if let Some(callback) = self.callback.as_ref() {
            match callback.send(rsp).await {
                Ok(_) => {}
                Err(e) => {
                    error!("callback failed, rsp {:?}", e.into_inner());
                    return Err(Error::DhtCallBackErr);
                }
            }
        }
        Ok(())
    }
}
