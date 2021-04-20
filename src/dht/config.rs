use crate::metainfo::HashPiece;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DhtConfig {
    /// The size of the bucket of the routing table
    pub k: usize,
    /// Id of the current DHT server node
    pub id: HashPiece,
    /// Used to produce token
    pub secret: String,
    /// How long between token changes
    pub token_interval: Duration,
    /// How many intervals may pass between the current interval
    pub max_token_interval_count: usize,
    /// How long bewtween bad node clean
    pub refresh_interval: Duration,
    /// Recursive query limit
    pub depth: usize,
    /// If true, the port argument should be ignored,
    /// and the source port of the UDP packet should be used
    pub implied_port: bool,
    /// Max transaction execution time
    pub max_transaction_time_out: Duration,
}

impl Default for DhtConfig {
    fn default() -> Self {
        DhtConfig {
            k: 8,
            id: HashPiece::rand_new(),
            secret: "torrentisgreat".to_string(),
            token_interval: Duration::from_secs(30),
            max_token_interval_count: 2,
            refresh_interval: Duration::from_secs(30),
            depth: 4,
            implied_port: true,
            max_transaction_time_out: Duration::from_secs(5),
        }
    }
}
