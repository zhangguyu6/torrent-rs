use super::{
    DhtConfig, DhtMessage, DhtReq, DhtRsp, MemPeerStore, PeerStore, RoutingTable, TokenManager,
    Transaction,
};
use crate::bencode::{from_bytes, to_bytes};
use crate::error::{Error, Result};
use crate::krpc::{
    KrpcMessage, KrpcQuery, KrpcResponse, MessageType, QueryType, MAX_KRPC_MESSAGE_SIZE,
};
use crate::metainfo::{CompactAddresses, CompactNodes, HashPiece, Node, PeerAddress};
use log::{debug, error};
use rand::{thread_rng, Rng};
use smol::{
    channel::{bounded, unbounded, Receiver, Sender},
    future::{or, race},
    net::{AsyncToSocketAddrs, UdpSocket},
    Timer,
};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Dht Sever Instance
pub struct DhtServer<S = MemPeerStore> {
    addr: SocketAddr,
    support_ipv4: bool,
    support_ipv6: bool,
    socket: UdpSocket,
    routing_table: RoutingTable,
    token_manager: TokenManager,
    receiver: Receiver<DhtMessage>,
    peer_store: S,
    config: Arc<RwLock<DhtConfig>>,
    tran_seq: usize,
    trans: HashMap<usize, Transaction>,
}

impl<S: PeerStore> DhtServer<S> {
    pub async fn new<A: AsyncToSocketAddrs>(
        addr: A,
        config: Arc<RwLock<DhtConfig>>,
        peer_store: S,
    ) -> Result<(Self, DhtClient)> {
        let addr = addr
            .to_socket_addrs()
            .await?
            .next()
            .ok_or(Error::DhtAddrBindErr)?;
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
        let config_guard = config.read().unwrap();
        let routing_table = RoutingTable::new(&config_guard);
        let token_manager = TokenManager::new(
            config_guard.secret.clone(),
            config_guard.token_interval,
            config_guard.max_token_interval_count,
        );
        let (sender, receiver) = unbounded();
        drop(config_guard);
        let mut rng = thread_rng();
        let tran_seq: usize = rng.gen_range(0..usize::MAX / 2);
        let trans = HashMap::default();
        Ok((
            Self {
                addr,
                socket: UdpSocket::bind(addr).await.unwrap(),
                support_ipv4,
                support_ipv6,
                routing_table,
                token_manager,
                receiver,
                peer_store,
                config,
                tran_seq,
                trans,
            },
            DhtClient { sender },
        ))
    }

    /// Initializes the routing table
    pub async fn bootstrap<A: AsyncToSocketAddrs>(&mut self, addresses: A) -> Result<()> {
        let (sender, _) = unbounded();
        let depth;
        let id;
        {
            let config_guard = self.config.read().unwrap();
            depth = config_guard.depth;
            id = config_guard.id.clone();
        }
        for addr in addresses.to_socket_addrs().await? {
            match self
                .send_find_node(
                    PeerAddress(addr),
                    id.clone(),
                    Transaction::new(sender.clone(), depth, Some(id.clone()), QueryType::FindNode),
                )
                .await
            {
                Ok(_) => {}
                Err(e) => error!("send_find_node failed, addr {}, err {}", addr, e),
            }
        }
        Ok(())
    }

    fn get_closest_nodes(&self, hash_piece: &HashPiece) -> Vec<Node> {
        let k = self.config.read().unwrap().k;
        let nodes = Vec::with_capacity(k);
        self.routing_table.closest(hash_piece, k, |node| {
            if self.support_ipv4 {
                return node.peer_address.0.is_ipv4();
            }
            if self.support_ipv6 {
                return node.peer_address.0.is_ipv6();
            }
            false
        });
        nodes
    }

    async fn receiver_req(&self, refresh_timer: &mut Timer) -> Result<DhtMessage> {
        Ok(or(self.receiver.recv(), async {
            refresh_timer.await;
            Ok(DhtMessage::Refresh)
        })
        .await?)
    }

    async fn receiver_rsp(&self, buf: &mut [u8]) -> Result<DhtMessage> {
        let (size, addr) = self.socket.recv_from(buf).await?;
        let krpc_message = from_bytes(&buf[0..size])?;
        Ok(DhtMessage::Message(krpc_message, addr))
    }

    async fn send_krpc_message<A: AsyncToSocketAddrs>(
        &mut self,
        mut message: KrpcMessage,
        addr: A,
    ) -> Result<()> {
        let id = self.config.read().unwrap().id.clone();
        match message.a.as_mut() {
            Some(query) => query.id = id.clone(),
            None => {
                message.a = Some(KrpcQuery {
                    id: id.clone(),
                    ..Default::default()
                })
            }
        }
        let buf = to_bytes(&message)?;
        self.socket.send_to(&buf[..], addr).await?;
        Ok(())
    }

    async fn send_ping(&mut self, addr: PeerAddress, tran: Transaction) -> Result<()> {
        let id = self.config.read().unwrap().id.clone();
        let query = KrpcQuery {
            id,
            ..Default::default()
        };
        self.tran_seq += 1;
        let message = KrpcMessage {
            t: self.tran_seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::Ping),
            a: Some(query),
            ..Default::default()
        };
        self.send_krpc_message(message, addr.0).await?;
        self.trans.insert(self.tran_seq, tran);
        Ok(())
    }

    async fn send_find_node(
        &mut self,
        addr: PeerAddress,
        target: HashPiece,
        tran: Transaction,
    ) -> Result<()> {
        let id = self.config.read().unwrap().id.clone();
        let mut want = Vec::new();
        if self.support_ipv4 {
            want.push("n4".to_string());
        }
        if self.support_ipv6 {
            want.push("n6".to_string());
        }
        let query = KrpcQuery {
            id,
            target: Some(target.clone()),
            want,
            ..Default::default()
        };
        let message = KrpcMessage {
            t: self.tran_seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::FindNode),
            a: Some(query),
            ..Default::default()
        };
        self.send_krpc_message(message, addr.0).await?;
        self.trans.insert(self.tran_seq, tran);
        Ok(())
    }

    async fn send_get_peers(
        &mut self,
        node: Node,
        info_hash: HashPiece,
        tran: Transaction,
    ) -> Result<()> {
        let id = self.config.read().unwrap().id.clone();
        let mut want = Vec::new();
        if self.support_ipv4 {
            want.push("n4".to_string());
        }
        if self.support_ipv6 {
            want.push("n6".to_string());
        }
        let query = KrpcQuery {
            id,
            info_hash: Some(info_hash),
            want,
            ..Default::default()
        };
        self.tran_seq += 1;
        let message = KrpcMessage {
            t: self.tran_seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::GetPeers),
            a: Some(query),
            ..Default::default()
        };
        self.send_krpc_message(message, node.peer_address.0).await?;
        self.trans.insert(self.tran_seq, tran.clone());
        Ok(())
    }

    async fn send_announce_peer(
        &mut self,
        node: Node,
        info_hash: HashPiece,
        tran: Transaction,
    ) -> Result<()> {
        let id;
        let implied_port;
        {
            let config_guard = self.config.read().unwrap();
            id = config_guard.id.clone();
            implied_port = config_guard.implied_port;
        }
        let mut want = Vec::new();
        if self.support_ipv4 {
            want.push("n4".to_string());
        }
        if self.support_ipv6 {
            want.push("n6".to_string());
        }
        let mut query = KrpcQuery {
            id,
            info_hash: Some(info_hash),
            implied_port: if !implied_port {
                None
            } else {
                Some(implied_port)
            },
            port: Some(self.addr.port()),
            want,
            ..Default::default()
        };
        if let Some(token) = self.token_manager.get_token(&node.id) {
            self.tran_seq += 1;
            query.token = Some(token.clone());
            let message = KrpcMessage {
                t: self.tran_seq.to_string(),
                y: MessageType::Query,
                q: Some(QueryType::AnnouncePeer),
                a: Some(query),
                ..Default::default()
            };
            self.trans.insert(self.tran_seq, tran.clone());
            self.send_krpc_message(message, node.peer_address.0).await?;
        }
        Ok(())
    }

    async fn on_error(&mut self, mut message: KrpcMessage) -> Result<()> {
        let e = message.e.take().ok_or(Error::ProtocolErr)?;
        let err = Error::KrpcErr(e);
        let seq: usize = message.t.parse()?;
        let tran = self
            .trans
            .remove(&seq)
            .ok_or(Error::TransactionNotFound(seq))?;
        // ignore if client dropped
        let _ = tran.callback(Err(err)).await;
        Ok(())
    }

    async fn on_response(&mut self, mut message: KrpcMessage, addr: SocketAddr) -> Result<()> {
        let tran_id: usize = message.t.parse()?;
        let tran = self
            .trans
            .remove(&tran_id)
            .ok_or(Error::TransactionNotFound(tran_id))?;
        let mut rsp = message.r.take().ok_or(Error::ProtocolErr)?;
        if message.ro != Some(true) {
            self.routing_table.insert(Node {
                id: rsp.id.clone(),
                peer_address: PeerAddress(addr),
            });
        }
        match tran.query_type {
            QueryType::Ping => self.on_ping_rsp(rsp, tran).await?,
            QueryType::FindNode => self.on_find_node_rsp(rsp, tran).await?,
            QueryType::GetPeers => {
                if let Some(token) = rsp.token.take() {
                    self.token_manager.insert_token(rsp.id.clone(), token);
                }
                self.on_get_peers_rsp(rsp, tran).await?;
            }
            QueryType::AnnouncePeer => self.on_announce_rsp(rsp, tran).await?,
        }
        Ok(())
    }

    async fn on_ping_rsp(&mut self, rsp: KrpcResponse, tran: Transaction) -> Result<()> {
        let _ = tran.callback(Ok(DhtRsp::Pong(rsp.id))).await;
        Ok(())
    }

    async fn on_find_node_rsp(
        &mut self,
        mut rsp: KrpcResponse,
        mut tran: Transaction,
    ) -> Result<()> {
        tran.depth -= 1;
        let mut find_node = None;
        let mut nodes = rsp.nodes.take().map(|n| n.0).unwrap_or_default();
        let mut node6s = rsp.nodes6.take().map(|n| n.0).unwrap_or_default();
        for n in nodes.drain(..) {
            if tran.contain_id(&n.id) {
                continue;
            }
            tran.insert_id(n.id.clone());
            if &n.id == tran.target.as_ref().unwrap() {
                find_node = Some(n.clone());
            }
            if self.support_ipv4 && find_node.is_none() && tran.depth > 0 {
                self.send_find_node(n.peer_address, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        for n in node6s.drain(..) {
            if tran.contain_id(&n.id) {
                continue;
            }
            tran.insert_id(n.id.clone());
            if &n.id == tran.target.as_ref().unwrap() {
                find_node = Some(n.clone());
            }
            if self.support_ipv6 && find_node.is_none() && tran.depth > 0 {
                self.send_find_node(n.peer_address, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        if find_node.is_some() {
            let _ = tran.callback(Ok(DhtRsp::FindNode(find_node))).await;
        }
        Ok(())
    }

    async fn on_get_peers_rsp(
        &mut self,
        mut rsp: KrpcResponse,
        mut tran: Transaction,
    ) -> Result<()> {
        if let Some(addrs) = rsp.values.take() {
            let _ = tran.callback(Ok(DhtRsp::GetPeers(addrs.0))).await;
            return Ok(());
        }
        tran.depth -= 1;
        let mut nodes = rsp.nodes.take().map(|n| n.0).unwrap_or_default();
        let mut node6s = rsp.nodes6.take().map(|n| n.0).unwrap_or_default();
        for n in nodes.drain(..) {
            if tran.contain_id(&n.id) {
                continue;
            }
            tran.insert_id(n.id.clone());
            if self.support_ipv4 && tran.depth > 0 {
                self.send_get_peers(n, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        for n in node6s.drain(..) {
            if tran.contain_id(&n.id) {
                continue;
            }
            tran.insert_id(n.id.clone());
            if self.support_ipv6 && tran.depth > 0 {
                self.send_get_peers(n, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        Ok(())
    }

    async fn on_announce_rsp(&mut self, _: KrpcResponse, tran: Transaction) -> Result<()> {
        let _ = tran.callback(Ok(DhtRsp::Announced)).await;
        Ok(())
    }

    async fn on_query(&mut self, message: KrpcMessage, addr: SocketAddr) -> Result<()> {
        let query_t = message.q.ok_or(Error::ProtocolErr)?;
        let query = message.a.ok_or(Error::ProtocolErr)?;
        match query_t {
            QueryType::Ping => self.on_ping_query(query, message.t, addr).await?,
            QueryType::FindNode => self.on_find_node_query(query, message.t, addr).await?,
            QueryType::GetPeers => self.on_get_peers_query(query, message.t, addr).await?,
            QueryType::AnnouncePeer => self.on_announce_peer(query, message.t, addr).await?,
        }
        Ok(())
    }

    async fn on_ping_query(
        &mut self,
        req: KrpcQuery,
        tran_id: String,
        mut addr: SocketAddr,
    ) -> Result<()> {
        let id = self.config.read().unwrap().id.clone();
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

    async fn on_find_node_query(
        &mut self,
        mut req: KrpcQuery,
        tran_id: String,
        mut addr: SocketAddr,
    ) -> Result<()> {
        let target = req.target.take().ok_or(Error::ProtocolErr)?;
        let id;
        let k;
        {
            let config_guard = self.config.read().unwrap();
            id = config_guard.id.clone();
            k = config_guard.k;
        }
        let mut rsp = KrpcResponse {
            id,
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
        rsp.nodes = Some(CompactNodes::from(
            self.routing_table
                .closest(&target, k, |node| {
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
                })
                .into_iter(),
        ));
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

    async fn on_get_peers_query(
        &mut self,
        mut req: KrpcQuery,
        tran_id: String,
        mut addr: SocketAddr,
    ) -> Result<()> {
        let id;
        let k;
        {
            let config_guard = self.config.read().unwrap();
            id = config_guard.id.clone();
            k = config_guard.k;
        }
        let info_hash = req.info_hash.take().ok_or(Error::ProtocolErr)?;
        let mut rsp = KrpcResponse {
            id,
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

        rsp.values = Some(CompactAddresses::from(
            self.peer_store
                .get(&info_hash, k)
                .unwrap_or(Vec::new())
                .into_iter()
                .filter_map(|update_node| {
                    if !want_ipv4 && !want_ipv6 {
                        if addr.is_ipv4() {
                            if update_node.peer_address.0.is_ipv4() {
                                Some(update_node.peer_address)
                            } else {
                                None
                            }
                        } else {
                            if update_node.peer_address.0.is_ipv6() {
                                Some(update_node.peer_address)
                            } else {
                                None
                            }
                        }
                    } else if want_ipv4 && want_ipv6 {
                        Some(update_node.peer_address)
                    } else if want_ipv4 {
                        if update_node.peer_address.0.is_ipv4() {
                            Some(update_node.peer_address)
                        } else {
                            None
                        }
                    } else {
                        if update_node.peer_address.0.is_ipv6() {
                            Some(update_node.peer_address)
                        } else {
                            None
                        }
                    }
                }),
        ));
        rsp.token = Some(self.token_manager.create_token(None, &PeerAddress(addr)));
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

    async fn on_announce_peer(
        &mut self,
        mut req: KrpcQuery,
        tran_id: String,
        mut addr: SocketAddr,
    ) -> Result<()> {
        let id = self.config.read().unwrap().id.clone();
        let info_hash = req.info_hash.take().ok_or(Error::ProtocolErr)?;
        let token = req.token.take().ok_or(Error::ProtocolErr)?;
        if !self.token_manager.valid_token(token, &PeerAddress(addr)) {
            return Err(Error::ProtocolErr);
        }
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
        let _ = self.peer_store.insert(
            info_hash,
            Node {
                id: req.id.clone(),
                peer_address: PeerAddress(addr),
            },
        );
        Ok(())
    }

    async fn handle(&mut self, refresh_timer: &mut Timer, buf: &mut [u8]) -> Result<bool> {
        let depth = self.config.read().unwrap().depth;
        match race(self.receiver_req(refresh_timer), self.receiver_rsp(buf)).await {
            Ok(DhtMessage::Req(req, cb)) => match req {
                DhtReq::ShutDown => {
                    let _ = cb.send(Ok(DhtRsp::ShutDown));
                    return Ok(true);
                }
                DhtReq::Ping(addr) => {
                    self.send_ping(addr, Transaction::new(cb, 0, None, QueryType::Ping))
                        .await?;
                }
                DhtReq::FindNode(addr, target) => {
                    self.send_find_node(
                        addr,
                        target.clone(),
                        Transaction::new(cb, depth, Some(target), QueryType::FindNode),
                    )
                    .await?;
                }
                DhtReq::GetPeers(info_hash) => {
                    let tran =
                        Transaction::new(cb, depth, Some(info_hash.clone()), QueryType::GetPeers);
                    for n in self.get_closest_nodes(&info_hash) {
                        self.send_get_peers(n, info_hash.clone(), tran.clone())
                            .await?;
                    }
                }
                DhtReq::AnnouncePeer(info_hash) => {
                    let tran = Transaction::new(cb, depth, None, QueryType::AnnouncePeer);
                    for n in self.get_closest_nodes(&info_hash) {
                        match self
                            .send_announce_peer(n, info_hash.clone(), tran.clone())
                            .await
                        {
                            Err(e) => error!("send_announce_peer failed, err:{}", e),
                            Ok(_) => {}
                        };
                    }
                }
            },
            Ok(DhtMessage::Message(message, addr)) => match message.y {
                MessageType::Query => self.on_query(message, addr).await?,
                MessageType::Error => self.on_error(message).await?,
                MessageType::Response => self.on_response(message, addr).await?,
            },
            Ok(DhtMessage::Refresh) => self.refresh().await?,
            Err(Error::ChannelRecvErr(_)) => return Ok(true),
            Err(e) => return Err(e),
        }
        Ok(false)
    }

    async fn refresh(&mut self) -> Result<()> {
        let mut removed_nodes = Vec::new();
        removed_nodes.extend(self.routing_table.refresh());
        for node in removed_nodes.iter() {
            let (cb, _) = bounded(1);
            self.send_ping(
                node.peer_address.clone(),
                Transaction::new(cb, 0, None, QueryType::Ping),
            )
            .await?;
        }
        let now = Instant::now();
        let max_transaction_time_out = self.config.read().unwrap().max_transaction_time_out;
        for (_, tran) in self
            .trans
            .drain_filter(|_, tran| now - tran.last_updated > max_transaction_time_out)
        {
            let _ = tran.callback(Err(Error::TransactionTimeout)).await;
            error!("tran {:?}, timeout", tran);
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut buf = [0; MAX_KRPC_MESSAGE_SIZE];
        let refresh_interval = self.config.read().unwrap().refresh_interval;
        let mut refresh_timer = Timer::interval(refresh_interval);
        loop {
            match self.handle(&mut refresh_timer, &mut buf).await {
                Ok(true) => break,
                Ok(false) => {}
                Err(e) => error!("handle err : {}", e),
            }
        }
        debug!("dht server shutdown!");
        Ok(())
    }
}

pub struct DhtClient {
    sender: Sender<DhtMessage>,
}

impl DhtClient {
    pub async fn ping<A: AsyncToSocketAddrs>(&self, addr: A) -> Result<HashPiece> {
        let peer_addrs: Vec<SocketAddr> = addr.to_socket_addrs().await?.collect();
        if peer_addrs.len() != 1 {
            error!("more than one address");
            return Err(Error::ProtocolErr);
        }
        let addr = PeerAddress(peer_addrs[0].clone());
        let (sender, receiver) = unbounded();
        match self
            .sender
            .send(DhtMessage::Req(DhtReq::Ping(addr.clone()), sender))
            .await
        {
            Ok(_) => {}
            Err(e) => error!("ping failed, addr {:?}, message {:?}", addr, e.into_inner()),
        }
        match receiver.recv().await? {
            Ok(DhtRsp::Pong(id)) => Ok(id),
            Ok(rsp) => {
                error!("expect receive pong, but receive {:?}", rsp);
                Err(Error::ProtocolErr)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn find_node<A: AsyncToSocketAddrs>(
        &self,
        addr: A,
        id: HashPiece,
    ) -> Result<Option<Node>> {
        let peer_addrs: Vec<SocketAddr> = addr.to_socket_addrs().await?.collect();
        if peer_addrs.len() != 1 {
            error!("more than one address");
            return Err(Error::ProtocolErr);
        }
        let addr = PeerAddress(peer_addrs[0].clone());
        let (sender, receiver) = unbounded();
        match self
            .sender
            .send(DhtMessage::Req(DhtReq::FindNode(addr.clone(), id), sender))
            .await
        {
            Ok(_) => {}
            Err(e) => error!(
                "find_node failed, addr {:?} , message {:?}",
                addr,
                e.into_inner()
            ),
        }
        match receiver.recv().await? {
            Ok(DhtRsp::FindNode(node)) => Ok(node),
            Ok(rsp) => {
                error!("expect receive find_node, but receive {:?}", rsp);
                Err(Error::ProtocolErr)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn get_peers(&self, info_hash: HashPiece) -> Result<Vec<PeerAddress>> {
        let (sender, receiver) = unbounded();
        match self
            .sender
            .send(DhtMessage::Req(DhtReq::GetPeers(info_hash.clone()), sender))
            .await
        {
            Ok(_) => {}
            Err(e) => error!(
                "get_peers failed, info_hash {:?}, message {:?}",
                info_hash,
                e.into_inner()
            ),
        }
        match receiver.recv().await? {
            Ok(DhtRsp::GetPeers(addr)) => Ok(addr),
            Ok(rsp) => {
                error!("expect receive find_node, but receive {:?}", rsp);
                Err(Error::ProtocolErr)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn announce(&self, info_hash: HashPiece) -> Result<()> {
        let (sender, receiver) = unbounded();
        match self
            .sender
            .send(DhtMessage::Req(
                DhtReq::AnnouncePeer(info_hash.clone()),
                sender,
            ))
            .await
        {
            Ok(_) => {}
            Err(e) => error!(
                "announce failed, info_hash {:?}, message {:?}",
                info_hash,
                e.into_inner()
            ),
        }
        match receiver.recv().await? {
            Ok(DhtRsp::Announced) => Ok(()),
            Ok(rsp) => {
                error!("expect receive announce, but receive {:?}", rsp);
                Err(Error::ProtocolErr)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn shutdown(&self) -> Result<()> {
        let (sender, receiver) = unbounded();
        match self
            .sender
            .send(DhtMessage::Req(DhtReq::ShutDown, sender))
            .await
        {
            Ok(_) => {}
            Err(e) => error!("shutdown failed, message {:?}", e.into_inner()),
        }
        match receiver.recv().await? {
            Ok(DhtRsp::ShutDown) => Ok(()),
            Ok(rsp) => {
                error!("expect receive shutdown, but receive {:?}", rsp);
                Err(Error::ProtocolErr)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smol::block_on;
    use std::time::Duration;

    #[test]
    fn test_dht_bootstrap() {
        let config = DhtConfig::default();
        block_on(async {
            let (mut server, client) = DhtServer::new(
                "0.0.0.0:6881",
                Arc::new(RwLock::new(config)),
                MemPeerStore::default(),
            )
            .await
            .unwrap();
            assert!(server.bootstrap("router.bittorrent.com:6881").await.is_ok());

            assert!(or(server.run(), async move {
                Timer::after(Duration::from_secs(5)).await;
                drop(client);
                Ok(())
            })
            .await
            .is_ok());
        });
    }

    #[test]
    fn test_dht_ping() {
        let config = DhtConfig::default();
        let fut0 = async {
            let (mut server, client) = DhtServer::new(
                "0.0.0.0:6881",
                Arc::new(RwLock::new(config.clone())),
                MemPeerStore::default(),
            )
            .await
            .unwrap();

            assert!(or(server.run(), async move {
                Timer::after(Duration::from_secs(1)).await;
                drop(client);
                Ok(())
            })
            .await
            .is_ok());
        };
        let fut1 = async {
            let (mut server, client) = DhtServer::new(
                "0.0.0.0:6882",
                Arc::new(RwLock::new(config.clone())),
                MemPeerStore::default(),
            )
            .await
            .unwrap();

            assert!(or(server.run(), async move {
                assert!(client.ping("127.0.0.1:6881").await.is_ok());
                Ok(())
            })
            .await
            .is_ok());
        };
        block_on(or(fut0, fut1));
    }

    #[test]
    fn test_dht_find_node() {
        let config0 = DhtConfig::default();
        let fut0 = async {
            let (mut server, client) = DhtServer::new(
                "0.0.0.0:6881",
                Arc::new(RwLock::new(config0.clone())),
                MemPeerStore::default(),
            )
            .await
            .unwrap();

            assert!(or(server.run(), async move {
                assert!(client.ping("127.0.0.1:6882").await.is_ok());
                Timer::after(Duration::from_secs(2)).await;
                drop(client);
                Ok(())
            })
            .await
            .is_ok());
        };
        let config1 = DhtConfig::default();
        let fut1 = async {
            let (mut server, client) = DhtServer::new(
                "0.0.0.0:6882",
                Arc::new(RwLock::new(config1.clone())),
                MemPeerStore::default(),
            )
            .await
            .unwrap();

            assert!(or(server.run(), async move {
                Timer::after(Duration::from_secs(1)).await;
                assert!(client.find_node("127.0.0.1:6881", config1.id).await.is_ok());
                Ok(())
            })
            .await
            .is_ok());
        };
        block_on(or(fut0, fut1));
    }

    #[test]
    fn test_dht_get_peers() {
        let config0 = DhtConfig::default();
        let info_hash0 = HashPiece::rand_new();
        let info_hash1 = info_hash0.clone();
        let fut0 = async {
            let (mut server, client) = DhtServer::new(
                "0.0.0.0:6881",
                Arc::new(RwLock::new(config0.clone())),
                MemPeerStore::default(),
            )
            .await
            .unwrap();

            assert!(server
                .peer_store
                .insert(
                    info_hash0.clone(),
                    Node {
                        id: info_hash0.clone(),
                        peer_address: PeerAddress("127.0.0.1:1".parse().unwrap()),
                    },
                )
                .is_ok());

            assert!(or(server.run(), async move {
                Timer::after(Duration::from_secs(2)).await;
                drop(client);
                Ok(())
            })
            .await
            .is_ok());
        };
        let config1 = DhtConfig::default();
        let fut1 = async {
            let (mut server, client) = DhtServer::new(
                "0.0.0.0:6882",
                Arc::new(RwLock::new(config1.clone())),
                MemPeerStore::default(),
            )
            .await
            .unwrap();

            assert!(or(server.run(), async move {
                assert!(client.ping("127.0.0.1:6881").await.is_ok());
                let res = client.get_peers(info_hash1.clone()).await;
                assert!(res.is_ok());
                for addr in res.unwrap() {
                    assert_eq!(addr, PeerAddress("127.0.0.1:1".parse().unwrap()));
                }
                Ok(())
            })
            .await
            .is_ok());
        };
        block_on(or(fut0, fut1));
    }

    #[test]
    fn test_dht_on_announce() {
        let config0 = DhtConfig::default();
        let info_hash0 = HashPiece::rand_new();
        let info_hash1 = info_hash0.clone();
        let fut0 = async {
            let (mut server, client) = DhtServer::new(
                "0.0.0.0:6881",
                Arc::new(RwLock::new(config0.clone())),
                MemPeerStore::default(),
            )
            .await
            .unwrap();

            assert!(or(server.run(), async move {
                Timer::after(Duration::from_secs(2)).await;
                drop(client);
                Ok(())
            })
            .await
            .is_ok());
        };
        let config1 = DhtConfig::default();
        let fut1 = async {
            let (mut server, client) = DhtServer::new(
                "0.0.0.0:6882",
                Arc::new(RwLock::new(config1.clone())),
                MemPeerStore::default(),
            )
            .await
            .unwrap();

            assert!(or(server.run(), async move {
                assert!(client.ping("127.0.0.1:6881").await.is_ok());
                let mut res = client.get_peers(info_hash1.clone()).await;
                assert!(res.is_ok());
                assert!(res.unwrap().is_empty());
                assert!(client.announce(info_hash1.clone()).await.is_ok());
                let mut res = client.get_peers(info_hash1.clone()).await;
                assert!(res.is_ok());
                for addr in res.unwrap() {
                    assert_eq!(addr, PeerAddress("127.0.0.1:6882".parse().unwrap()));
                }
                Ok(())
            })
            .await
            .is_ok());
        };
        block_on(or(fut0, fut1));
    }
}
