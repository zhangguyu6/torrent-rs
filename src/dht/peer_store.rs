use crate::error::Result;
use crate::krpc::Node;
use crate::metainfo::{HashPiece, PeerAddress};
use std::collections::{HashMap, HashSet};

/// Store Peer announced info hash before
pub trait PeerStore {
    /// store announced peer
    fn insert_info_hash(&mut self, info_hash: HashPiece, node: Node) -> Result<()>;
    /// get peers by info_hash
    fn get_peer_addresses<P>(&self, info_hash: &HashPiece, max: usize, f: P) -> Vec<PeerAddress>
    where
        P: Fn(&Node) -> bool;
}

#[derive(Debug, Default)]
pub struct MemPeerStore {
    info_to_node: HashMap<HashPiece, HashSet<Node>>,
}

impl PeerStore for MemPeerStore {
    fn insert_info_hash(&mut self, info_hash: HashPiece, node: Node) -> Result<()> {
        let nodes = match self.info_to_node.get_mut(&info_hash) {
            Some(nodes) => nodes,
            None => {
                self.info_to_node.insert(info_hash.clone(), HashSet::new());
                self.info_to_node.get_mut(&info_hash).unwrap()
            }
        };
        nodes.insert(node);
        Ok(())
    }

    fn get_peer_addresses<P>(&self, info_hash: &HashPiece, max: usize, f: P) -> Vec<PeerAddress>
    where
        P: Fn(&Node) -> bool,
    {
        let mut nodes = Vec::default();
        if let Some(ids) = self.info_to_node.get(info_hash) {
            for node in ids {
                if nodes.len() == max {
                    break;
                }
                if f(node) {
                    nodes.push(node.peer_address.clone());
                }
            }
        }
        nodes
    }
}
