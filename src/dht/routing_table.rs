use super::config::DHT_CONFIG;
use crate::metainfo::{HashPiece, Node, ID_LEN};
use std::mem::size_of;
use std::time::{Duration, Instant};
use std::{cmp, collections::BTreeMap};

/// After 15 minutes of inactivity, a node becomes questionable
pub const QUESTIONABLE_TIMEOUT: u64 = 15 * 60;

pub const BAD_TIMEOUT: u64 = 20 * 60;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum NodeState {
    Good,
    Questionable,
    Bad,
}

#[derive(Debug, Clone)]
pub struct UpdatedNode {
    pub last_updated: Instant,
    pub node: Node,
}

impl UpdatedNode {
    fn new(node: Node) -> Self {
        Self {
            last_updated: Instant::now(),
            node: node,
        }
    }

    fn state(&self, now: Option<Instant>) -> NodeState {
        let now = now.unwrap_or(Instant::now());
        if now - self.last_updated > Duration::new(BAD_TIMEOUT, 0) {
            NodeState::Bad
        } else if now - self.last_updated > Duration::new(QUESTIONABLE_TIMEOUT, 0) {
            NodeState::Questionable
        } else {
            NodeState::Good
        }
    }
}

#[derive(Debug)]
pub struct Bucket {
    nodes: BTreeMap<HashPiece, UpdatedNode>,
    cap: usize,
}

impl Bucket {
    pub fn new(k: usize) -> Self {
        Self {
            nodes: BTreeMap::new(),
            cap: k,
        }
    }
    pub fn insert(&mut self, node: UpdatedNode) -> Option<UpdatedNode> {
        if self.nodes.len() < self.cap {
            return self.nodes.insert(node.node.id.clone(), node);
        }
        // update old node
        if let Some(old_node) = self.nodes.remove(&node.node.id) {
            self.nodes.insert(node.node.id.clone(), node);
            return Some(old_node);
        }
        // try to remove bad node
        let mut del_k = None;
        for (k, v) in self.nodes.iter() {
            if v.state(None) == NodeState::Bad {
                del_k = Some(k.clone());
                break;
            }
        }
        if let Some(k) = del_k {
            let old_node = self.nodes.remove(&k);
            self.nodes.insert(node.node.id.clone(), node);
            return old_node;
        }
        // ignore
        None
    }

    /// remove bad nodes and return questionable nodes
    pub fn refresh(&mut self) -> Vec<UpdatedNode> {
        let mut questionables = Vec::new();
        self.nodes.drain_filter(|_, v| {
            match v.state(None) {
                NodeState::Bad => return true,
                NodeState::Questionable => questionables.push(v.clone()),
                _ => {}
            }
            false
        });
        questionables
    }
}

#[derive(Debug)]
pub struct RoutingTable {
    id: HashPiece,
    buckets: Vec<Bucket>,
}

impl RoutingTable {
    pub fn new() -> Self {
        let config = DHT_CONFIG.read().unwrap();
        let id = config.id.clone();
        let k = config.k;
        // The num of the bucket of the routing table
        let len = ID_LEN * (size_of::<u8>() as usize);
        drop(config);
        let mut buckets = Vec::with_capacity(len);
        for _ in 0..len {
            let bucket = Bucket::new(k);
            buckets.push(bucket);
        }
        Self { id, buckets }
    }

    pub fn insert(&mut self, node: UpdatedNode) -> Option<UpdatedNode> {
        let bucket_i = node.node.id.bits();
        let bucket = &mut self.buckets[bucket_i];
        bucket.insert(node)
    }

    pub fn closest(&self, target: HashPiece, max: usize) -> Vec<UpdatedNode> {
        let mut nodes: Vec<(&HashPiece, &UpdatedNode)> =
            self.buckets.iter().flat_map(|b| &b.nodes).collect();
        nodes.sort_by_key(|(k, _)| (*k) ^ &target);
        nodes[0..cmp::min(max, nodes.len())]
            .iter()
            .map(|(_, v)| (*v).clone())
            .collect()
    }

    /// remove all bad nodes and return questionable nodes in table
    pub fn refresh(&mut self) -> Vec<UpdatedNode> {
        self.buckets
            .iter_mut()
            .map(|b| b.refresh())
            .flatten()
            .collect()
    }
}
