use super::config::{DhtConfig, DHT_CONFIG};
use crate::bencode::Value;
use crate::metainfo::{HashPiece, PeerAddress};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

#[derive(Debug)]
pub struct TokenManager {
    secret: String,
    interval: Duration,
    max_interval_count: usize,
    token_m: HashMap<HashPiece, Value>,
}

impl Default for TokenManager {
    fn default() -> Self {
        Self::new(DHT_CONFIG.read().as_ref().unwrap())
    }
}

impl TokenManager {
    pub fn new(config: &DhtConfig) -> Self {
        let secret = config.secret.clone();
        let interval = config.token_interval;
        let max_interval_count = config.max_token_interval_count;
        TokenManager {
            secret,
            interval,
            max_interval_count,
            token_m: HashMap::new(),
        }
    }

    pub fn get_token(&self, id: &HashPiece) -> Option<&Value> {
        self.token_m.get(id)
    }

    pub fn insert_token(&mut self, id: HashPiece, token: Value) -> Option<Value> {
        self.token_m.insert(id, token)
    }

    pub fn create_token(&self, now: Option<SystemTime>, node: &PeerAddress) -> Value {
        let mut hasher = Sha1::new();
        let ip_buf: Vec<u8> = node.into();
        hasher.update(ip_buf);
        let count = now
            .unwrap_or(SystemTime::now())
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            % self.interval.as_secs();
        hasher.update(count.to_be_bytes());
        hasher.update(self.secret.as_bytes());
        let v: [u8; 20] = hasher.finalize().into();
        (&v[..]).into()
    }

    pub fn valid_token(&self, token: Value, node: &PeerAddress) -> bool {
        let mut now = SystemTime::now();
        for _ in 0..self.max_interval_count + 1 {
            let value = self.create_token(Some(now), node);
            if value == token {
                return true;
            }
            now -= self.interval;
        }
        false
    }
}
