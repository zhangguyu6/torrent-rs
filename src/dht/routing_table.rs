use crate::krpc::Node;
use crate::metainfo::{HashPiece, ID_LEN};
use std::cmp;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct NodeWithUpdateTime {
    pub last_updated: Instant,
    pub node: Node,
}

impl NodeWithUpdateTime {
    fn new(node: Node, now: Option<Instant>) -> Self {
        let now = now.unwrap_or(Instant::now());
        Self {
            last_updated: now,
            node: node,
        }
    }

    fn is_questionable(&self, now: Option<Instant>, questionable_interval: Duration) -> bool {
        let now = now.unwrap_or(Instant::now());
        now - self.last_updated > questionable_interval
    }
}

#[derive(Debug)]
pub(crate) struct Bucket {
    nodes: HashMap<HashPiece, NodeWithUpdateTime>,
    last_updated: Instant,
    k: usize,
    questionable_interval: Duration,
}

impl Bucket {
    fn new(k: usize, questionable_interval: Duration) -> Self {
        Self {
            nodes: HashMap::default(),
            last_updated: Instant::now(),
            k,
            questionable_interval,
        }
    }

    fn insert(&mut self, node: Node) {
        self.last_updated = Instant::now();
        // node already exists in the routing bucket
        if let Some(node_ext) = self.nodes.get_mut(&node.id) {
            // update address and clear trans
            if node_ext.node != node {
                node_ext.node = node;
            }
            // address is same, just update
            node_ext.last_updated = self.last_updated;
        }
        // When a bucket is full of known good nodes, no more nodes may be added.
        else if self.nodes.len() < self.k {
            self.nodes.insert(
                node.id.clone(),
                NodeWithUpdateTime::new(node, Some(self.last_updated)),
            );
        }
    }

    fn remove(&mut self, id: &HashPiece) -> Option<Node> {
        self.nodes.remove(&id).map(|node_ext| node_ext.node)
    }

    fn questionable_iter(&self) -> impl Iterator<Item = &Node> {
        let questionable_interval = self.questionable_interval;
        self.nodes.iter().filter_map(move |(_, node_ext)| {
            if node_ext.is_questionable(None, questionable_interval) {
                Some(&node_ext.node)
            } else {
                None
            }
        })
    }
}

#[derive(Debug)]
pub(crate) struct RoutingTable {
    id: HashPiece,
    buckets: Vec<Bucket>,
}

impl RoutingTable {
    pub fn new(id: HashPiece, k: usize, questionable_interval: Duration) -> Self {
        // The num of the bucket of the routing table
        let len = ID_LEN * 8;
        let mut buckets = Vec::with_capacity(len);
        for _ in 0..len {
            let bucket = Bucket::new(k, questionable_interval);
            buckets.push(bucket);
        }

        Self { id, buckets }
    }

    pub fn insert(&mut self, node: Node) {
        let bucket_index = (&node.id ^ &self.id).count_zeros();
        let bucket = &mut self.buckets[bucket_index];
        bucket.insert(node);
    }

    /// find closest Nodes to the infohash of the target
    pub fn closest<P>(&self, target: &HashPiece, max: usize, f: P) -> Vec<Node>
    where
        P: Fn(&Node) -> bool,
    {
        let mut nodes: Vec<(&HashPiece, &NodeWithUpdateTime)> = self
            .buckets
            .iter()
            .flat_map(|b| b.nodes.iter().filter(|(_, node_ext)| f(&node_ext.node)))
            .collect();
        nodes.sort_by_key(|(k, _)| (*k) ^ target);
        nodes[0..cmp::min(max, nodes.len())]
            .iter()
            .map(|(_, v)| v.node.clone())
            .collect()
    }

    /// get all questionable nodes in table
    pub fn questionables(&mut self) -> Vec<Node> {
        let mut nodes = Vec::new();
        for bucket in self.buckets.iter() {
            for node in bucket.questionable_iter() {
                nodes.push(node.clone());
            }
        }
        nodes
    }
}
