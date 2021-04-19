use crate::metainfo::HashPiece;
use std::time::Duration;

#[derive(Debug, Default)]
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
