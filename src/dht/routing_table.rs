use crate::krpc::Node;
use crate::metainfo::{HashPiece, PeerAddress, ID_LEN};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct NodeExt {
    pub last_updated: Instant,
    pub node: Node,
    pub no_responding_times: usize,
}

impl NodeExt {
    fn new(node: Node, now: Option<Instant>) -> Self {
        let now = now.unwrap_or(Instant::now());
        Self {
            last_updated: now,
            node: node,
            no_responding_times: 0,
        }
    }

    fn is_questionable(&self, now: Option<Instant>, questionable_interval: Duration) -> bool {
        let now = now.unwrap_or(Instant::now());
        now - self.last_updated > questionable_interval
    }

    fn is_dead(&self) -> bool {
        self.no_responding_times >= 3
    }
}

#[derive(Debug)]
pub(crate) struct Bucket {
    nodes: HashMap<HashPiece, NodeExt>,
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

    /// If the map did not have this node present, return true
    fn insert(&mut self, node: Node) -> bool {
        self.last_updated = Instant::now();
        // node already exists in the routing bucket
        if let Some(node_ext) = self.nodes.get_mut(&node.id) {
            node_ext.no_responding_times = 0;
            // update address and clear trans
            if node_ext.node != node {
                node_ext.node = node;
            }
            // address is same, just update
            node_ext.last_updated = self.last_updated;
            return false;
        }
        self.nodes.retain(|_, node_ext| !node_ext.is_dead());
        // When a bucket is full of known good nodes, no more nodes may be added.
        if self.nodes.len() < self.k {
            self.nodes
                .insert(node.id.clone(), NodeExt::new(node, Some(self.last_updated)));
        }
        true
    }

    fn questionables(&mut self) -> Vec<PeerAddress> {
        let questionable_interval = self.questionable_interval;
        let mut addresses = Vec::default();
        for (_, node_ext) in self.nodes.iter_mut() {
            if node_ext.is_questionable(None, questionable_interval) {
                node_ext.no_responding_times += 1;
                addresses.push(node_ext.node.peer_address.clone())
            }
        }
        addresses
    }

    fn iter(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values().map(|node_ext| &node_ext.node)
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
        if node.id == self.id {
            return;
        }
        let bucket_index = (&node.id ^ &self.id).count_ones() - 1;
        let bucket = &mut self.buckets[bucket_index];
        bucket.insert(node);
    }

    /// find closest Nodes to the infohash of the target
    pub fn closest<P>(&self, target: &HashPiece, max: usize, f: P) -> Vec<Node>
    where
        P: Fn(&Node) -> bool,
    {
        let mut nodes = Vec::new();
        for bucket in self.buckets.iter() {
            for (_, node_ext) in bucket.nodes.iter() {
                if f(&node_ext.node) {
                    nodes.push(node_ext.node.clone());
                }
            }
        }
        nodes.sort_by_key(|node| (&node.id ^ target).count_ones());
        nodes.truncate(max);
        nodes
    }

    /// get all questionable nodes in table
    pub fn questionables(&mut self) -> Vec<PeerAddress> {
        let mut addresses = Vec::new();
        for bucket in self.buckets.iter_mut() {
            addresses.append(&mut bucket.questionables());
        }
        addresses
    }

    pub fn count(&self) -> usize {
        self.buckets
            .iter()
            .fold(0, |acc, bucket| acc + bucket.nodes.len())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Node> {
        self.buckets.iter().flat_map(|bucket| bucket.iter())
    }
}
