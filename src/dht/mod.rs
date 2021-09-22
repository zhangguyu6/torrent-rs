mod config;
pub use config::DhtConfig;

mod error;
pub use error::DhtError;
use error::Result;

mod protocol;
use protocol::DhtRsp;

mod routing_table;
use routing_table::RoutingTable;

mod token;
use token::TokenManager;

mod transaction;
use transaction::{Transaction, TransactionManager};

mod peer_store;
pub use peer_store::{MemPeerStore, PeerStore};

use crate::bencode::{from_bytes, to_bytes};
use crate::krpc::{
    CompactNodes, KrpcMessage, KrpcQuery, KrpcResponse, MessageType, QueryType,
    MAX_KRPC_MESSAGE_SIZE,
};
use crate::metainfo::{CompactAddresses, PeerAddress};
use async_std::{
    net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket},
    sync::RwLock,
    task::JoinHandle,
};
use std::sync::Arc;

/// Dht Sever Instance
struct Inner<S: PeerStore = MemPeerStore> {
    transaction_mgr: RwLock<TransactionManager>,
    token_mgr: TokenManager,
    routing_table: RwLock<RoutingTable>,
    peer_store: RwLock<S>,
    config: DhtConfig,
    addr: SocketAddr,
    socket: UdpSocket,
    support_ipv4: bool,
    support_ipv6: bool,
    rsp_handle: Option<JoinHandle<Result<()>>>,
    refresh_handle: Option<JoinHandle<Result<()>>>,
}

impl<S: PeerStore> Inner<S> {
    async fn new(config: DhtConfig, peer_store: S) -> Result<Self> {
        let addr = config
            .local_addr
            .to_socket_addrs()
            .await?
            .next()
            .ok_or(DhtError::DhtAddrBind)?;
        // Binding on :: will  listen on IPv4 and IPV6 (dual-stack).
        let dual_stack_ip: IpAddr = "::".parse()?;
        let mut support_ipv4 = false;
        let mut support_ipv6 = false;
        if addr.ip() == dual_stack_ip {
            support_ipv4 = true;
        } else if addr.is_ipv4() {
            support_ipv4 = true;
        } else {
            support_ipv6 = true;
        }
        let transaction_mgr = RwLock::new(TransactionManager::default());
        let token_mgr = TokenManager::new(
            config.secret.clone(),
            config.token_interval,
            config.max_token_interval_count,
        );
        let routing_table = RwLock::new(RoutingTable::new(
            config.id.clone(),
            config.k,
            config.questionable_interval,
        ));
        let peer_store = RwLock::new(peer_store);
        Ok(Self {
            transaction_mgr,
            token_mgr,
            routing_table,
            peer_store,
            config,
            addr,
            socket: UdpSocket::bind(addr).await?,
            support_ipv4,
            support_ipv6,
            rsp_handle: None,
            refresh_handle: None,
        })
    }

    async fn send_krpc_message<A: ToSocketAddrs>(
        &self,
        mut message: KrpcMessage,
        addr: A,
    ) -> Result<()> {
        match message.a.as_mut() {
            Some(query) => query.id = self.config.id.clone(),
            None => {
                message.a = Some(KrpcQuery {
                    id: self.config.id.clone(),
                    ..Default::default()
                })
            }
        }
        let buf = to_bytes(&message)?;
        self.socket.send_to(&buf[..], addr).await?;
        Ok(())
    }

    async fn handle_rsp(&self) -> Result<()> {
        let mut buf = [0; MAX_KRPC_MESSAGE_SIZE];
        loop {
            let (size, addr) = self.socket.recv_from(&mut buf).await?;
            let krpc_message = from_bytes(&buf[0..size])?;
        }
    }

    async fn handle_query(&self, message: KrpcMessage, addr: SocketAddr) -> Result<()> {
        let query_t = message
            .q
            .ok_or(DhtError::Protocol("Not Found q".to_string()))?;
        let query = message
            .a
            .ok_or(DhtError::Protocol("Not Found a".to_string()))?;
        match query_t {
            QueryType::Ping => self.handle_ping_req(query, message.t, addr).await?,
            QueryType::FindNode => self.handle_find_node_req(query, message.t, addr).await?,
            QueryType::GetPeers => self.handle_get_peers_req(query, message.t, addr).await?,
            // QueryType::AnnouncePeer => self.on_announce_peer(query, message.t, addr).await?,
            _ => unimplemented!(),
        }
        Ok(())
    }

    async fn handle_ping_req(
        &self,
        req: KrpcQuery,
        tran_id: String,
        mut addr: SocketAddr,
    ) -> Result<()> {
        let id = self.config.id.clone();
        let rsp = KrpcResponse {
            id,
            ..Default::default()
        };
        let message = KrpcMessage {
            t: tran_id,
            y: MessageType::Response,
            r: Some(rsp),
            ..Default::default()
        };
        if req.implied_port.is_none() {
            if let Some(port) = req.port {
                addr.set_port(port);
            }
        }
        self.send_krpc_message(message, addr).await?;
        Ok(())
    }

    async fn handle_find_node_req(
        &self,
        mut req: KrpcQuery,
        tran_id: String,
        mut addr: SocketAddr,
    ) -> Result<()> {
        let target = req
            .target
            .take()
            .ok_or(DhtError::Protocol("Not Found target".to_string()))?;
        let mut rsp = KrpcResponse {
            id: self.config.id.clone(),
            ..Default::default()
        };
        let mut want_ipv4 = false;
        let mut want_ipv6 = false;
        for n in req.want {
            if n == "n4" {
                want_ipv4 = true;
            }
            if n == "n6" {
                want_ipv6 = true;
            }
        }
        let closest_nodes =
            self.routing_table
                .read()
                .await
                .closest(&target, self.config.k, |node| {
                    if !want_ipv4 && !want_ipv6 {
                        if addr.is_ipv4() {
                            node.peer_address.0.is_ipv4()
                        } else {
                            node.peer_address.0.is_ipv6()
                        }
                    } else if want_ipv4 && want_ipv6 {
                        true
                    } else if want_ipv4 {
                        node.peer_address.0.is_ipv4()
                    } else {
                        node.peer_address.0.is_ipv6()
                    }
                });
        rsp.nodes = Some(CompactNodes::from(closest_nodes));
        let message = KrpcMessage {
            t: tran_id,
            y: MessageType::Response,
            r: Some(rsp),
            ..Default::default()
        };
        if req.implied_port.is_none() {
            if let Some(port) = req.port {
                addr.set_port(port);
            }
        }
        self.send_krpc_message(message, addr).await?;
        Ok(())
    }

    async fn handle_get_peers_req(
        &self,
        mut req: KrpcQuery,
        tran_id: String,
        mut addr: SocketAddr,
    ) -> Result<()> {
        let info_hash = req
            .info_hash
            .take()
            .ok_or(DhtError::Protocol("Not Found info_hash".to_string()))?;
        let mut rsp = KrpcResponse {
            id: self.config.id.clone(),
            ..Default::default()
        };
        let mut want_ipv4 = false;
        let mut want_ipv6 = false;
        for n in req.want {
            if n == "n4" {
                want_ipv4 = true;
            }
            if n == "n6" {
                want_ipv6 = true;
            }
        }
        let peer_nodes =
            self.peer_store
                .read()
                .await
                .peer_addresses(&info_hash, self.config.k, |node| {
                    if !want_ipv4 && !want_ipv6 {
                        if addr.is_ipv4() {
                            node.peer_address.0.is_ipv4()
                        } else {
                            node.peer_address.0.is_ipv6()
                        }
                    } else if want_ipv4 && want_ipv6 {
                        true
                    } else if want_ipv4 {
                        node.peer_address.0.is_ipv4()
                    } else {
                        node.peer_address.0.is_ipv6()
                    }
                });
        rsp.values = Some(CompactAddresses::from(peer_nodes));
        rsp.token = Some(self.token_mgr.create_token(None, &PeerAddress(addr)));
        let message = KrpcMessage {
            t: tran_id,
            y: MessageType::Response,
            r: Some(rsp),
            ..Default::default()
        };
        if req.implied_port.is_none() {
            if let Some(port) = req.port {
                addr.set_port(port);
            }
        }
        self.send_krpc_message(message, addr).await?;
        Ok(())
    }

    // async fn ping(&self, addr: PeerAddress) -> Result<HashPiece, Error> {
    //     let rx = self.inner.send_ping(addr).await?;
    //     let rsp = timeout(
    //         self.inner.as_ref().config.max_transaction_time_out,
    //         rx.recv(),
    //     )
    //     .await???;
    //     match rsp {
    //         DhtRsp::Pong(id) => Ok(id),
    //         _ => Err(Error::ProtocolErr),
    //     }
    // }

    // pub async fn find_node(&self, id: HashPiece) -> Result<Option<Node>, Error> {
    //     unimplemented!()
    // }

    // pub async fn get_peers(&self, info_hash: HashPiece) -> Result<Vec<PeerAddress>, Error> {
    //     unimplemented!()
    // }

    // pub async fn announce_peer(&self, info_hash: HashPiece) -> Result<(), Error> {
    //     unimplemented!()
    // }
}

// struct Inner<S: PeerStore> {
//     socket: UdpSocket,
//     config: DhtConfig,
//     routing_table: Mutex<RoutingTable>,
//     token_mgr: Mutex<TokenManager>,
//     transaction_mgr: Mutex<TransactionManager>,
//     peer_store: S,
// }

// impl<S: PeerStore> Inner<S> {
//     fn new(dht_config: DhtConfig) -> Result<Self, Error> {
//         unimplemented!()
//     }

//     fn bootstrap(&mut self) -> Result<(), Error> {
//         unimplemented!()
//     }

//     async fn send_krpc_message<A: ToSocketAddrs>(
//         &self,
//         addr: A,
//         mut message: KrpcMessage,
//     ) -> Result<(), Error> {
//         let id = self.config.id.clone();
//         match message.a.as_mut() {
//             Some(query) => query.id = id.clone(),
//             None => {
//                 message.a = Some(KrpcQuery {
//                     id: id.clone(),
//                     ..Default::default()
//                 })
//             }
//         }
//         let buf = to_bytes(&message)?;
//         self.socket.send_to(&buf[..], addr).await?;
//         Ok(())
//     }

//     async fn send_ping(&self, addr: PeerAddress) -> Result<Receiver<Result<DhtRsp, Error>>, Error> {
//         let id = self.config.id.clone();
//         let query = KrpcQuery {
//             id,
//             ..Default::default()
//         };
//         let (tx, rx) = bounded(1);
//         let tran = Transaction::new(tx, 1, None, QueryType::Ping);
//         let seq = self.transaction_mgr.lock().await.insert(tran);
//         let message = KrpcMessage {
//             t: seq.to_string(),
//             y: MessageType::Query,
//             q: Some(QueryType::Ping),
//             a: Some(query),
//             ..Default::default()
//         };
//         self.send_krpc_message(addr.0, message).await?;
//         Ok(rx)
//     }

//     async fn handle_rsp(&self, mut message: KrpcMessage, addr: SocketAddr) -> Result<(), Error> {
//         let tran_id: usize = message.t.parse()?;
//         let tran = self
//             .transaction_mgr
//             .lock()
//             .await
//             .remove(tran_id)
//             .ok_or(Error::TransactionNotFound(tran_id))?;
//         unimplemented!()
//     }

//     async fn handle_ping_rsp(&self, rsp: KrpcResponse, tran: Transaction) -> Result<(), Error> {
//         let _ = tran.tx.send(Ok(DhtRsp::Pong(rsp.id))).await;
//         Ok(())
//     }
// }
