mod config;
use async_std::channel::unbounded;
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
    CompactNodes, KrpcMessage, KrpcQuery, KrpcResponse, MessageType, Node, QueryType,
    MAX_KRPC_MESSAGE_SIZE,
};
use crate::metainfo::{CompactAddresses, HashPiece, PeerAddress};
use async_std::{
    future,
    net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket},
    stream::{interval, Stream, StreamExt},
    sync::RwLock,
    task::{spawn, JoinHandle},
};
#[cfg(not(test))]
use log::{debug, error};
use std::sync::Arc;
use std::time::Duration;
#[cfg(test)]
use std::{println as debug, println as error};

/// Dht Sever Instance
pub struct Dht<S: PeerStore = MemPeerStore> {
    transaction_mgr: RwLock<TransactionManager>,
    token_mgr: RwLock<TokenManager>,
    routing_table: RwLock<RoutingTable>,
    peer_store: RwLock<S>,
    config: DhtConfig,
    addr: SocketAddr,
    socket: UdpSocket,
    support_ipv4: bool,
    support_ipv6: bool,
    rsp_handle: RwLock<Option<JoinHandle<Result<()>>>>,
    refresh_handle: RwLock<Option<JoinHandle<Result<()>>>>,
}

impl<S: PeerStore + Send + Sync + 'static> Dht<S> {
    pub async fn new(config: DhtConfig, peer_store: S) -> Result<Arc<Self>> {
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
        let token_mgr = RwLock::new(TokenManager::new(
            config.secret.clone(),
            config.token_interval,
            config.max_token_interval_count,
        ));
        let routing_table = RwLock::new(RoutingTable::new(
            config.id.clone(),
            config.k,
            config.questionable_interval,
        ));
        let peer_store = RwLock::new(peer_store);
        let inner = Self {
            transaction_mgr,
            token_mgr,
            routing_table,
            peer_store,
            config,
            addr,
            socket: UdpSocket::bind(addr).await?,
            support_ipv4,
            support_ipv6,
            rsp_handle: RwLock::new(None),
            refresh_handle: RwLock::new(None),
        };
        let arc_inner0 = Arc::new(inner);
        let arc_inner1 = arc_inner0.clone();
        let arc_inner2 = arc_inner0.clone();
        let rsp_handle = spawn(async move { arc_inner0.rsp_loop().await });
        let refresh_handle = spawn(async move { arc_inner1.refresh_loop().await });
        *arc_inner2.rsp_handle.write().await = Some(rsp_handle);
        *arc_inner2.refresh_handle.write().await = Some(refresh_handle);
        Ok(arc_inner2)
    }

    pub async fn close(&self) {
        if let Some(rsp_handle) = self.rsp_handle.write().await.take() {
            rsp_handle.cancel().await;
        }
        if let Some(refresh_handle) = self.refresh_handle.write().await.take() {
            refresh_handle.cancel().await;
        }
    }

    async fn rsp_loop(self: Arc<Self>) -> Result<()> {
        debug!("start listen on:{}", self.addr);
        let mut buf = [0; MAX_KRPC_MESSAGE_SIZE];
        loop {
            let res = self.socket.recv_from(&mut buf).await;
            match res {
                Ok((_, addr)) => {
                    let res = self.handle_frame(&buf, addr).await;
                    if let Err(e) = res {
                        error!("self.handle_frame failed, e={}", e);
                    }
                }
                Err(e) => {
                    error!("self.socket.recv_from failed, e={}", e);
                    break;
                }
            }
        }
        Ok(())
    }

    async fn refresh_loop(self: Arc<Self>) -> Result<()> {
        let mut refresh_timer = interval(self.config.refresh_interval);
        while let Some(_) = refresh_timer.next().await {
            let mut tasks = Vec::default();
            for address in self.routing_table.write().await.questionables() {
                let inner = self.clone();
                tasks.push(spawn(async move {
                    let res = future::timeout(Duration::from_secs(5), inner.ping(address)).await?;
                    res
                }));
            }
            for task in tasks {
                match task.await {
                    Ok(_) => {}
                    Err(e) => error!("refresh failed, e = {}", e),
                }
            }
        }
        Ok(())
    }

    async fn send_krpc_message<A: ToSocketAddrs>(
        &self,
        message: KrpcMessage,
        addr: A,
    ) -> Result<()> {
        let buf = to_bytes(&message)?;
        self.socket.send_to(&buf[..], addr).await?;
        Ok(())
    }

    async fn handle_frame(&self, buf: &[u8], addr: SocketAddr) -> Result<()> {
        let krpc_message: KrpcMessage = from_bytes(&buf)?;
        match krpc_message.y {
            MessageType::Query => self.handle_query(krpc_message, addr).await?,
            MessageType::Error => self.handle_error(krpc_message, addr).await?,
            MessageType::Response => self.handle_response(krpc_message, addr).await?,
        }
        Ok(())
    }

    async fn handle_query(&self, message: KrpcMessage, mut addr: SocketAddr) -> Result<()> {
        let query_t = message
            .q
            .ok_or(DhtError::Protocol("Not Found q".to_string()))?;
        let query = message
            .a
            .ok_or(DhtError::Protocol("Not Found a".to_string()))?;
        if query.implied_port.is_none() {
            if let Some(port) = query.port {
                addr.set_port(port);
            }
        }
        if message.ro != Some(true) {
            self.routing_table.write().await.insert(Node {
                id: query.id.clone(),
                peer_address: PeerAddress(addr.clone()),
            });
        }
        match query_t {
            QueryType::Ping => self.handle_ping_req(query, message.t, addr).await?,
            QueryType::FindNode => self.handle_find_node_req(query, message.t, addr).await?,
            QueryType::GetPeers => self.handle_get_peers_req(query, message.t, addr).await?,
            QueryType::AnnouncePeer => {
                self.handle_announce_peer_req(query, message.t, addr)
                    .await?
            }
        }
        Ok(())
    }

    async fn handle_ping_req(&self, _: KrpcQuery, tran_id: String, addr: SocketAddr) -> Result<()> {
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
        self.send_krpc_message(message, addr).await?;
        Ok(())
    }

    async fn handle_find_node_req(
        &self,
        mut req: KrpcQuery,
        tran_id: String,
        addr: SocketAddr,
    ) -> Result<()> {
        let target = req
            .target
            .take()
            .ok_or(DhtError::Protocol("Not Found target".to_string()))?;
        let mut rsp = KrpcResponse {
            id: self.config.id.clone(),
            ..Default::default()
        };
        // BEP32
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
        if !want_ipv4 && !want_ipv6 {
            if addr.is_ipv4() {
                let closest_nodes =
                    self.routing_table
                        .read()
                        .await
                        .closest(&target, self.config.k, |node| node.peer_address.0.is_ipv4());
                rsp.nodes = Some(CompactNodes::from(closest_nodes));
            } else {
                let closest_nodes =
                    self.routing_table
                        .read()
                        .await
                        .closest(&target, self.config.k, |node| node.peer_address.0.is_ipv6());
                rsp.nodes6 = Some(CompactNodes::from(closest_nodes));
            }
        } else {
            if want_ipv4 {
                let closest_nodes =
                    self.routing_table
                        .read()
                        .await
                        .closest(&target, self.config.k, |node| node.peer_address.0.is_ipv4());
                rsp.nodes = Some(CompactNodes::from(closest_nodes));
            } else {
                let closest_nodes =
                    self.routing_table
                        .read()
                        .await
                        .closest(&target, self.config.k, |node| node.peer_address.0.is_ipv6());
                rsp.nodes6 = Some(CompactNodes::from(closest_nodes));
            }
        }

        let message = KrpcMessage {
            t: tran_id,
            y: MessageType::Response,
            r: Some(rsp),
            ..Default::default()
        };
        self.send_krpc_message(message, addr).await?;
        Ok(())
    }

    async fn handle_get_peers_req(
        &self,
        mut req: KrpcQuery,
        tran_id: String,
        addr: SocketAddr,
    ) -> Result<()> {
        let info_hash = req
            .info_hash
            .take()
            .ok_or(DhtError::Protocol("Not Found info_hash".to_string()))?;
        let mut rsp = KrpcResponse {
            id: self.config.id.clone(),
            ..Default::default()
        };
        // BEP32
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
        rsp.token = Some(
            self.token_mgr
                .read()
                .await
                .create_token(None, &PeerAddress(addr)),
        );
        let message = KrpcMessage {
            t: tran_id,
            y: MessageType::Response,
            r: Some(rsp),
            ..Default::default()
        };
        self.send_krpc_message(message, addr).await?;
        Ok(())
    }

    async fn handle_announce_peer_req(
        &self,
        mut req: KrpcQuery,
        tran_id: String,
        mut addr: SocketAddr,
    ) -> Result<()> {
        let info_hash = req
            .info_hash
            .take()
            .ok_or(DhtError::Protocol("Not Found info_hash".to_string()))?;
        let token = req
            .token
            .take()
            .ok_or(DhtError::Protocol("Not Found token".to_string()))?;
        if !self
            .token_mgr
            .read()
            .await
            .valid_token(token, &PeerAddress(addr))
        {
            return Err(DhtError::InVaildToken);
        }
        let rsp = KrpcResponse {
            id: req.id.clone(),
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
        let _ = self.peer_store.write().await.insert(
            info_hash,
            Node {
                id: req.id.clone(),
                peer_address: PeerAddress(addr),
            },
        );
        Ok(())
    }

    async fn handle_error(&self, mut message: KrpcMessage, _: SocketAddr) -> Result<()> {
        let e = message
            .e
            .take()
            .ok_or(DhtError::Protocol("Not Found e".to_string()))?;
        let err = DhtError::Krpc(e);
        let tran = self
            .transaction_mgr
            .write()
            .await
            .remove(&message.t)
            .ok_or(DhtError::TransactionNotFound)?;
        // ignore if client dropped
        let _ = tran.tx.send(Err(err)).await;
        Ok(())
    }

    async fn handle_response(&self, mut message: KrpcMessage, addr: SocketAddr) -> Result<()> {
        let tran = self
            .transaction_mgr
            .write()
            .await
            .remove(&message.t)
            .ok_or(DhtError::TransactionNotFound)?;
        let mut rsp = message
            .r
            .take()
            .ok_or(DhtError::Protocol("Not Found r".to_string()))?;
        if message.ro != Some(true) {
            self.routing_table.write().await.insert(Node {
                id: rsp.id.clone(),
                peer_address: PeerAddress(addr.clone()),
            });
        }
        match tran.query_type {
            QueryType::Ping => self.on_ping_rsp(rsp, tran).await?,
            QueryType::FindNode => self.on_find_node_rsp(rsp, tran).await?,
            QueryType::GetPeers => {
                if let Some(token) = rsp.token.take() {
                    self.token_mgr
                        .write()
                        .await
                        .insert_token(rsp.id.clone(), token);
                }
                self.on_get_peers_rsp(rsp, tran).await?;
            }
            QueryType::AnnouncePeer => self.on_announce_rsp(rsp, tran).await?,
        }
        Ok(())
    }

    async fn on_ping_rsp(&self, rsp: KrpcResponse, tran: Transaction) -> Result<()> {
        let _ = tran.tx.send(Ok(DhtRsp::Pong(rsp.id))).await;
        Ok(())
    }

    async fn on_find_node_rsp(&self, mut rsp: KrpcResponse, mut tran: Transaction) -> Result<()> {
        tran.depth -= 1;
        let mut find_node = None;
        let mut nodes = rsp.nodes.take().map(|n| n.0).unwrap_or_default();
        let mut node6s = rsp.nodes6.take().map(|n| n.0).unwrap_or_default();
        for n in nodes.drain(..) {
            if tran.ids.contains(&n.id) {
                continue;
            }
            tran.ids.insert(n.id.clone());
            if &n.id == tran.target.as_ref().unwrap() {
                find_node = Some(n.clone());
            }
            if self.support_ipv4 && find_node.is_none() && tran.depth > 0 {
                self.send_find_node(n.peer_address, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        for n in node6s.drain(..) {
            if tran.ids.contains(&n.id) {
                continue;
            }
            tran.ids.insert(n.id.clone());
            if &n.id == tran.target.as_ref().unwrap() {
                find_node = Some(n.clone());
            }
            if self.support_ipv6 && find_node.is_none() && tran.depth > 0 {
                self.send_find_node(n.peer_address, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        if let Some(find_node) = find_node {
            let _ = tran.tx.send(Ok(DhtRsp::FindNode(find_node))).await;
        }
        Ok(())
    }

    async fn on_get_peers_rsp(&self, mut rsp: KrpcResponse, mut tran: Transaction) -> Result<()> {
        if let Some(addrs) = rsp.values.take() {
            for addr in addrs.0 {
                let _ = tran.tx.send(Ok(DhtRsp::GetPeers(addr))).await;
            }
            return Ok(());
        }
        tran.depth -= 1;
        let mut nodes = rsp.nodes.take().map(|n| n.0).unwrap_or_default();
        let mut node6s = rsp.nodes6.take().map(|n| n.0).unwrap_or_default();
        for n in nodes.drain(..) {
            if tran.ids.contains(&n.id) {
                continue;
            }
            tran.ids.insert(n.id.clone());
            if self.support_ipv4 && tran.depth > 0 {
                self.send_get_peers(n.peer_address, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        for n in node6s.drain(..) {
            if tran.ids.contains(&n.id) {
                continue;
            }
            tran.ids.insert(n.id.clone());
            if self.support_ipv6 && tran.depth > 0 {
                self.send_get_peers(n.peer_address, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        Ok(())
    }

    async fn on_announce_rsp(&self, _: KrpcResponse, tran: Transaction) -> Result<()> {
        let _ = tran.tx.send(Ok(DhtRsp::Announced)).await;
        Ok(())
    }

    async fn send_ping(&self, addr: PeerAddress, tran: Transaction) -> Result<()> {
        let query = KrpcQuery {
            id: self.config.id.clone(),
            ..Default::default()
        };
        let message = KrpcMessage {
            t: self.transaction_mgr.write().await.insert(tran).to_string(),
            y: MessageType::Query,
            q: Some(QueryType::Ping),
            a: Some(query),
            ..Default::default()
        };
        self.send_krpc_message(message, addr.0).await?;
        Ok(())
    }

    async fn send_find_node(
        &self,
        addr: PeerAddress,
        target: HashPiece,
        tran: Transaction,
    ) -> Result<()> {
        let mut want = Vec::new();
        if self.support_ipv4 {
            want.push("n4".to_string());
        }
        if self.support_ipv6 {
            want.push("n6".to_string());
        }
        let query = KrpcQuery {
            id: self.config.id.clone(),
            target: Some(target.clone()),
            want,
            ..Default::default()
        };

        let message = KrpcMessage {
            t: self.transaction_mgr.write().await.insert(tran).to_string(),
            y: MessageType::Query,
            q: Some(QueryType::FindNode),
            a: Some(query),
            ..Default::default()
        };
        self.send_krpc_message(message, addr.0).await?;
        Ok(())
    }

    async fn send_get_peers(
        &self,
        addr: PeerAddress,
        info_hash: HashPiece,
        tran: Transaction,
    ) -> Result<()> {
        let mut want = Vec::new();
        if self.support_ipv4 {
            want.push("n4".to_string());
        }
        if self.support_ipv6 {
            want.push("n6".to_string());
        }
        let query = KrpcQuery {
            id: self.config.id.clone(),
            info_hash: Some(info_hash),
            want,
            ..Default::default()
        };
        let message = KrpcMessage {
            t: self.transaction_mgr.write().await.insert(tran).to_string(),
            y: MessageType::Query,
            q: Some(QueryType::GetPeers),
            a: Some(query),
            ..Default::default()
        };
        self.send_krpc_message(message, addr.0).await?;
        Ok(())
    }

    async fn send_announce_peer(
        &self,
        node: Node,
        info_hash: HashPiece,
        tran: Transaction,
    ) -> Result<()> {
        let mut want = Vec::new();
        if self.support_ipv4 {
            want.push("n4".to_string());
        }
        if self.support_ipv6 {
            want.push("n6".to_string());
        }
        let mut query = KrpcQuery {
            id: self.config.id.clone(),
            info_hash: Some(info_hash),
            implied_port: if !self.config.implied_port {
                None
            } else {
                Some(self.config.implied_port)
            },
            port: Some(self.addr.port()),
            want,
            ..Default::default()
        };
        if let Some(token) = self.token_mgr.read().await.get_token(&node.id) {
            query.token = Some(token.clone());
            let message = KrpcMessage {
                t: self.transaction_mgr.write().await.insert(tran).to_string(),
                y: MessageType::Query,
                q: Some(QueryType::AnnouncePeer),
                a: Some(query),
                ..Default::default()
            };
            self.send_krpc_message(message, node.peer_address.0).await?;
            Ok(())
        } else {
            Err(DhtError::Protocol(format!(
                "Not Found token for {:?}",
                query.info_hash
            )))
        }
    }

    pub async fn ping(&self, addr: PeerAddress) -> Result<()> {
        let (tx, rx) = unbounded();
        let tran = Transaction::new(tx, 0, None, QueryType::Ping);
        self.send_ping(addr, tran).await?;
        match rx.recv().await? {
            Ok(DhtRsp::Pong(_)) => Ok(()),
            Ok(rsp) => {
                error!("expect receive ping, but receive {:?}", rsp);
                Err(DhtError::Protocol("receive unexpected rsp".to_string()))
            }
            Err(e) => Err(e),
        }
    }

    pub async fn find_node(&self, addr: PeerAddress, target: HashPiece) -> Result<Node> {
        let (tx, rx) = unbounded();
        let tran = Transaction::new(
            tx,
            self.config.depth,
            Some(target.clone()),
            QueryType::FindNode,
        );
        self.send_find_node(addr, target, tran).await?;
        match rx.recv().await? {
            Ok(DhtRsp::FindNode(node)) => Ok(node),
            Ok(rsp) => {
                error!("expect receive find_node, but receive {:?}", rsp);
                Err(DhtError::Protocol("receive unexpected rsp".to_string()))
            }
            Err(e) => Err(e),
        }
    }

    pub async fn get_peers(
        &self,
        addr: PeerAddress,
        info_hash: HashPiece,
    ) -> Result<impl Stream<Item = PeerAddress>> {
        let (tx, rx) = unbounded();
        let tran = Transaction::new(
            tx,
            self.config.depth,
            Some(info_hash.clone()),
            QueryType::GetPeers,
        );
        self.send_get_peers(addr, info_hash, tran).await?;
        Ok(rx.filter_map(|rsp| match rsp {
            Ok(DhtRsp::GetPeers(addr)) => Some(addr),
            _ => None,
        }))
    }

    pub async fn announce_peer(&self, node: Node, info_hash: HashPiece) -> Result<()> {
        let (tx, rx) = unbounded();
        let tran = Transaction::new(tx, 0, None, QueryType::AnnouncePeer);
        self.send_announce_peer(node, info_hash, tran).await?;
        match rx.recv().await? {
            Ok(DhtRsp::Announced) => Ok(()),
            Ok(rsp) => {
                error!("expect receive announce_peer, but receive {:?}", rsp);
                Err(DhtError::Protocol("receive unexpected rsp".to_string()))
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::task::{block_on, sleep};
    use std::time::Duration;

    #[test]
    fn test_dht_create() {
        block_on(async {
            let inner = Dht::new(DhtConfig::default(), MemPeerStore::default()).await;
            assert!(inner.is_ok());
            sleep(Duration::from_secs(1)).await;
            inner.unwrap().close().await;
        });
    }
}
