use crate::error::Result;
use crate::metainfo::{HashPiece, Node};
use std::collections::{HashMap, HashSet};

pub trait PeerStore {
    /// store announced peer
    fn insert(&mut self, info_hash: HashPiece, node: Node) -> Result<()>;
    /// remove bad peer
    fn remove(&mut self, id: &HashPiece) -> Result<Option<Node>>;
    /// get peers by info_hash
    fn get(&self, info_hash: &HashPiece, max: usize) -> Result<Vec<Node>>;
}

pub struct MemPeerStore {
    node_to_info: HashMap<HashPiece, (Node, HashSet<HashPiece>)>,
    info_to_node: HashMap<HashPiece, HashSet<HashPiece>>,
}

impl PeerStore for MemPeerStore {
    fn insert(&mut self, info_hash: HashPiece, node: Node) -> Result<()> {
        let infos = match self.node_to_info.get_mut(&node.id) {
            Some((_, infos)) => infos,
            None => {
                self.node_to_info
                    .insert(node.id.clone(), (node.clone(), HashSet::new()));
                self.node_to_info
                    .get_mut(&node.id)
                    .map(|(_, infos)| infos)
                    .unwrap()
            }
        };
        if infos.contains(&info_hash) {
            return Ok(());
        }
        infos.insert(info_hash.clone());
        let nodes = match self.info_to_node.get_mut(&info_hash) {
            Some(nodes) => nodes,
            None => {
                self.info_to_node.insert(info_hash.clone(), HashSet::new());
                self.info_to_node.get_mut(&info_hash).unwrap()
            }
        };
        nodes.insert(node.id);
        Ok(())
    }

    fn remove(&mut self, id: &HashPiece) -> Result<Option<Node>> {
        if let Some((node, mut infos)) = self.node_to_info.remove(id) {
            for info in infos.drain() {
                if let Some(nodes) = self.info_to_node.get_mut(&info) {
                    nodes.remove(&node.id);
                }
            }
            return Ok(Some(node));
        }
        Ok(None)
    }

    fn get(&self, info_hash: &HashPiece, max: usize) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();
        if let Some(ids) = self.info_to_node.get(info_hash) {
            for id in ids {
                if let Some((node, _)) = self.node_to_info.get(id) {
                    nodes.push(node.clone());
                }
            }
        }
        Ok(nodes)
    }
}
