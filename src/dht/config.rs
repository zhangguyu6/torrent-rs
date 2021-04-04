use crate::metainfo::HashPiece;
use lazy_static::lazy_static;
use std::sync::RwLock;

lazy_static! {
    pub(crate) static ref DHT_CONFIG: RwLock<Config> = {
        let mut config = Config::default();
        RwLock::new(config)
    };
}

#[derive(Debug, Default)]
pub struct Config {
    /// The size of the bucket of the routing table
    pub k: usize,
    /// Id of the current DHT server node
    pub id: HashPiece,
}
