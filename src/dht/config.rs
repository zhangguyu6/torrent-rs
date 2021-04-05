use crate::metainfo::HashPiece;
use lazy_static::lazy_static;
use std::sync::RwLock;
use std::time::Duration;

lazy_static! {
    pub(crate) static ref DHT_CONFIG: RwLock<Config> = {
        let config = Config::default();
        RwLock::new(config)
    };
}

#[derive(Debug, Default)]
pub struct Config {
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
}
