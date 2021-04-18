use super::DHT_CONFIG;
use crate::metainfo::{HashPiece, Node, ID_LEN};
use std::cmp;
use std::collections::BTreeMap;
use std::mem::size_of;
use std::time::{Duration, Instant};

/// After 15 minutes of inactivity, a node becomes questionable
const QUESTIONABLE_TIMEOUT: u64 = 15 * 60;

const BAD_TIMEOUT: u64 = 20 * 60;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum NodeState {
    Good,
    Questionable,
    Bad,
}

#[derive(Debug, Clone)]
struct UpdatedNode {
    pub last_updated: Instant,
    pub node: Node,
}

impl UpdatedNode {
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
struct Bucket {
    nodes: BTreeMap<HashPiece, UpdatedNode>,
    cap: usize,
}

impl Bucket {
    fn new(k: usize) -> Self {
        Self {
            nodes: BTreeMap::new(),
            cap: k,
        }
    }
    fn insert(&mut self, node: UpdatedNode) -> Option<UpdatedNode> {
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

    pub fn insert(&mut self, node: Node) -> Option<Node> {
        let bucket_i = node.id.bits();
        let bucket = &mut self.buckets[bucket_i];
        bucket
            .insert(UpdatedNode {
                node,
                last_updated: Instant::now(),
            })
            .map(|n| n.node)
    }

    /// find closest Nodes to the infohash of the target
    pub fn closest(&self, target: &HashPiece, max: usize) -> Vec<Node> {
        let mut nodes: Vec<(&HashPiece, &UpdatedNode)> =
            self.buckets.iter().flat_map(|b| &b.nodes).collect();
        nodes.sort_by_key(|(k, _)| (*k) ^ target);
        nodes[0..cmp::min(max, nodes.len())]
            .iter()
            .map(|(_, v)| v.node.clone())
            .collect()
    }

    /// remove all bad nodes and return questionable nodes in table
    pub fn refresh(&mut self) -> Vec<Node> {
        self.buckets
            .iter_mut()
            .map(|b| b.refresh())
            .flatten()
            .map(|n| n.node)
            .collect()
    }
}
