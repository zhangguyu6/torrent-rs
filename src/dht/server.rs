use super::config::{DhtConfig, DHT_CONFIG};
use super::routing_table::RoutingTable;
use super::token::TokenManager;
use crate::metainfo::{HashPiece, Node, PeerAddress};
use crate::{
    error::{Error, Result},
    metainfo::CompactNodes,
};
use async_oneshot::{oneshot, Receiver as OneReceiver, Sender as OneSender};
use smol::{
    channel::{unbounded, Receiver, Sender},
    net::{AsyncToSocketAddrs, UdpSocket},
};
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
/// ip protocol stacks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetType {
    Ipv4,
    Ipv6,
    Dual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DhtReq {
    Ping(PeerAddress),
    FindNode(PeerAddress, HashPiece),
    GetPeers(HashPiece),
    AnnouncePeer(HashPiece),
}

pub enum DhtRsp {
    GetPeers(Vec<Node>),
}

pub struct DhtServer {
    addr: SocketAddr,
    socket: UdpSocket,
    net_type: NetType,
    routing_table4: RoutingTable,
    routing_table6: RoutingTable,
    token_manager: TokenManager,
    receiver: Receiver<(DhtReq, OneSender<DhtRsp>)>,
}

impl DhtServer {
    async fn new<A: AsyncToSocketAddrs>(addr: A, config: &DhtConfig) -> Result<Self> {
        let addr = addr
            .to_socket_addrs()
            .await?
            .next()
            .ok_or(Error::DhtAddrBindErr)?;
        // Binding on :: will  listen on IPv4 and IPV6 (dual-stack).
        let dual_addr = "::".parse().unwrap();
        let net_type;
        if addr == dual_addr {
            net_type = NetType::Dual;
        } else if addr.is_ipv4() {
            net_type = NetType::Ipv4;
        } else {
            net_type = NetType::Ipv6;
        }
        let routing_table4 = RoutingTable::new();
        let routing_table6 = RoutingTable::new();
        let token_manager = TokenManager::new(config);
        let (sender, receiver) = unbounded();
        Ok(Self {
            addr,
            socket: UdpSocket::bind(addr).await.unwrap(),
            net_type,
            routing_table4,
            routing_table6,
            token_manager,
            receiver,
        })
    }

    pub async fn bootstrap(&mut self) -> Result<()> {
        unimplemented!()
    }

    async fn handle(&mut self) -> Result<()> {
        unimplemented!()
    }
}

pub struct DhtClient {}

#[cfg(test)]
mod tests {
    use super::*;
    use smol::{block_on, net::resolve};

    #[test]
    fn test_resolve() {
        block_on(async {
            for addr in resolve("router.utorrent.com:6881").await.unwrap() {
                println!("{}", addr);
            }
        })
    }
}
