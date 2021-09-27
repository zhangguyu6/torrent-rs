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
    /// How long a node becomes questionable
    pub questionable_interval: Duration,
    /// How long ping questionable nodes
    pub refresh_interval: Duration,
    /// Recursive query limit
    pub depth: usize,
    /// If true, the port argument should be ignored,
    /// and the source port of the UDP packet should be used
    pub implied_port: bool,
    /// DHT service Listen addresss and port
    pub local_addr: String,
    /// DHT bootstrap server addr used to initialize
    pub bootstrap_addrs: Vec<String>,
}

impl Default for DhtConfig {
    fn default() -> Self {
        DhtConfig {
            k: 8,
            id: HashPiece::rand_new(),
            secret: "torrentisgreat".to_string(),
            token_interval: Duration::from_secs(30),
            max_token_interval_count: 2,
            questionable_interval: Duration::from_secs(60 * 15),
            refresh_interval: Duration::from_secs(60 * 1),
            depth: 4,
            implied_port: true,
            local_addr: "127.0.0.1:6881".to_string(),
            bootstrap_addrs: vec![
                "router.bittorrent.com:6881".to_string(),
                "router.utorrent.com:6881".to_string(),
                "router.bitcomet.com:6881".to_string(),
            ],
        }
    }
}
