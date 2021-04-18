use crate::metainfo::HashPiece;
use lazy_static::lazy_static;
use std::sync::RwLock;
use std::time::Duration;

lazy_static! {
    pub(crate) static ref DHT_CONFIG: RwLock<DhtConfig> = {
        let config = DhtConfig::default();
        RwLock::new(config)
    };
}

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
    /// How long bewtween clear bad node
    pub refresh_interval: Duration,
    /// Recursive query limit
    pub depth: usize,
    /// If true, the port argument should be ignored,
    /// and the source port of the UDP packet should be used
    pub implied_port: bool,
    /// Max transaction execution time
    pub max_transaction_time_out: Duration,
}
