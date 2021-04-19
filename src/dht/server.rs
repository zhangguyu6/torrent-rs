use super::{
    DhtConfig, DhtMessage, DhtReq, DhtRsp, MemPeerStore, PeerStore, RoutingTable, TokenManager,
    Transaction, TransactionManager,
};
use crate::bencode::{from_bytes, to_bytes};
use crate::error::{Error, Result};
use crate::krpc::{
    KrpcMessage, KrpcQuery, KrpcResponse, MessageType, QueryType, MAX_KRPC_MESSAGE_SIZE,
};
use crate::metainfo::{CompactAddresses, CompactNodes, HashPiece, Node, PeerAddress};
use log::{debug, error};
use smol::{
    channel::{unbounded, Receiver, Sender},
    future::{or, race},
    net::{AsyncToSocketAddrs, UdpSocket},
    stream::{Stream, StreamExt},
    Timer,
};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, RwLock};

/// Dht Sever Instance
pub struct DhtServer<S = MemPeerStore> {
    addr: SocketAddr,
    support_ipv4: bool,
    support_ipv6: bool,
    socket: UdpSocket,
    routing_table4: RoutingTable,
    routing_table6: RoutingTable,
    token_manager: TokenManager,
    receiver: Receiver<DhtMessage>,
    tran_seq: usize,
    transaction_manager: TransactionManager,
    peer_store: S,
    config: Arc<RwLock<DhtConfig>>,
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
        let routing_table4 = RoutingTable::new(&config_guard);
        let routing_table6 = RoutingTable::new(&config_guard);
        let token_manager = TokenManager::new(
            config_guard.secret.clone(),
            config_guard.token_interval,
            config_guard.max_token_interval_count,
        );
        let (sender, receiver) = unbounded();
        let seq = 0;
        let transaction_manager = TransactionManager::default();
        drop(config_guard);
        Ok((
            Self {
                addr,
                socket: UdpSocket::bind(addr).await.unwrap(),
                support_ipv4,
                support_ipv6,
                routing_table4,
                routing_table6,
                token_manager,
                receiver,
                transaction_manager,
                tran_seq: seq,
                peer_store,
                config,
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
                    Transaction::new(
                        Some(sender.clone()),
                        depth,
                        Some(id.clone()),
                        QueryType::FindNode,
                    ),
                )
                .await
            {
                Ok(_) => {}
                Err(e) => error!("send_find_node failed, addr {}, err {}", addr, e),
            }
        }
        Ok(())
    }

    fn insert_node(&mut self, node: Node) -> bool {
        let mut existed = false;
        if node.peer_address.0.is_ipv4() {
            if self.support_ipv4 {
                if let Some(_) = self.routing_table4.insert(node.clone()) {
                    existed = true;
                }
            }
        }
        if node.peer_address.0.is_ipv6() {
            if self.support_ipv6 {
                if let Some(_) = self.routing_table6.insert(node.clone()) {
                    existed = true;
                }
            }
        }
        existed
    }

    fn get_closest_nodes(&self, hash_piece: &HashPiece) -> Vec<Node> {
        let k = self.config.read().unwrap().k;
        let mut nodes = Vec::with_capacity(k);
        if self.support_ipv4 {
            nodes.extend(self.routing_table4.closest(hash_piece, k));
        }
        if self.support_ipv6 {
            nodes.extend(self.routing_table6.closest(hash_piece, k));
        }
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
        self.transaction_manager.insert(self.tran_seq, tran);
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
        self.tran_seq += 1;
        let message = KrpcMessage {
            t: self.tran_seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::FindNode),
            a: Some(query),
            ..Default::default()
        };
        self.send_krpc_message(message, addr.0).await?;
        self.transaction_manager.insert(self.tran_seq, tran);
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
        self.transaction_manager.insert(self.tran_seq, tran.clone());
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
            self.transaction_manager.insert(self.tran_seq, tran.clone());
            self.send_krpc_message(message, node.peer_address.0).await?;
        }
        Ok(())
    }

    async fn on_error(&mut self, mut message: KrpcMessage) -> Result<()> {
        let e = message.e.take().ok_or(Error::ProtocolErr)?;
        let err = Error::KrpcErr(e);
        let seq: usize = message.t.parse()?;
        let tran = self
            .transaction_manager
            .remove(&seq)
            .ok_or(Error::TransactionNotFound(seq))?;
        // ignore if client dropped
        let _ = tran.callback(Err(err)).await;
        Ok(())
    }

    async fn on_response(&mut self, mut message: KrpcMessage, addr: SocketAddr) -> Result<()> {
        let tran_id: usize = message.t.parse()?;
        let tran = self
            .transaction_manager
            .remove(&tran_id)
            .ok_or(Error::TransactionNotFound(tran_id))?;
        let mut rsp = message.r.take().ok_or(Error::ProtocolErr)?;
        if message.ro != Some(true) {
            self.insert_node(Node {
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

    async fn on_ping_rsp(&mut self, _: KrpcResponse, tran: Transaction) -> Result<()> {
        let _ = tran.callback(Ok(DhtRsp::Pong)).await;
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
            if self.support_ipv4
                && !self.insert_node(n.clone())
                && find_node.is_none()
                && tran.depth > 0
            {
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
            if self.support_ipv6
                && !self.insert_node(n.clone())
                && find_node.is_none()
                && tran.depth > 0
            {
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
        if let Some(mut addrs) = rsp.values.take() {
            for addr in addrs.0.drain(..) {
                let _ = tran.callback(Ok(DhtRsp::GetPeers(addr))).await;
            }
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
            if self.support_ipv4 && !self.insert_node(n.clone()) && tran.depth > 0 {
                match self
                    .send_get_peers(n, tran.target.clone().unwrap(), tran.clone())
                    .await
                {
                    Err(e) => error!("send_get_peers failed, err:{}", e),
                    Ok(_) => {}
                };
            }
        }
        for n in node6s.drain(..) {
            if tran.contain_id(&n.id) {
                continue;
            }
            tran.insert_id(n.id.clone());
            if self.support_ipv6 && !self.insert_node(n.clone()) && tran.depth > 0 {
                match self
                    .send_get_peers(n, tran.target.clone().unwrap(), tran.clone())
                    .await
                {
                    Err(e) => error!("send_get_peers failed, err:{}", e),
                    Ok(_) => {}
                };
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
        if !want_ipv4 && !want_ipv6 {
            if addr.is_ipv4() {
                rsp.nodes = Some(CompactNodes::from(
                    self.routing_table4.closest(&target, k).into_iter(),
                ));
            } else {
                rsp.nodes6 = Some(CompactNodes::from(
                    self.routing_table6.closest(&target, k).into_iter(),
                ));
            }
        }
        if want_ipv4 {
            rsp.nodes = Some(CompactNodes::from(
                self.routing_table4.closest(&target, k).into_iter(),
            ));
        }
        if want_ipv6 {
            rsp.nodes6 = Some(CompactNodes::from(
                self.routing_table6.closest(&target, k).into_iter(),
            ));
        }
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
        if !want_ipv4 && !want_ipv6 {
            if addr.is_ipv4() {
                rsp.values = Some(CompactAddresses::from(
                    self.peer_store
                        .get(&info_hash, k)
                        .unwrap_or(Vec::new())
                        .into_iter()
                        .filter_map(|update_node| {
                            if update_node.peer_address.0.is_ipv4() {
                                Some(update_node.peer_address)
                            } else {
                                None
                            }
                        }),
                ));
            } else {
                rsp.values = Some(CompactAddresses::from(
                    self.peer_store
                        .get(&info_hash, k)
                        .unwrap_or(Vec::new())
                        .into_iter()
                        .filter_map(|update_node| {
                            if update_node.peer_address.0.is_ipv6() {
                                Some(update_node.peer_address)
                            } else {
                                None
                            }
                        }),
                ));
            }
            if want_ipv4 && want_ipv6 {
                rsp.values = Some(CompactAddresses::from(
                    self.peer_store
                        .get(&info_hash, k)
                        .unwrap_or(Vec::new())
                        .into_iter()
                        .map(|update_node| update_node.peer_address),
                ));
            } else if want_ipv4 {
                rsp.values = Some(CompactAddresses::from(
                    self.peer_store
                        .get(&info_hash, k)
                        .unwrap_or(Vec::new())
                        .into_iter()
                        .filter_map(|update_node| {
                            if update_node.peer_address.0.is_ipv4() {
                                Some(update_node.peer_address)
                            } else {
                                None
                            }
                        }),
                ));
            } else {
                rsp.values = Some(CompactAddresses::from(
                    self.peer_store
                        .get(&info_hash, k)
                        .unwrap_or(Vec::new())
                        .into_iter()
                        .filter_map(|update_node| {
                            if update_node.peer_address.0.is_ipv6() {
                                Some(update_node.peer_address)
                            } else {
                                None
                            }
                        }),
                ));
            }
        }
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
                    self.send_ping(addr, Transaction::new(Some(cb), 0, None, QueryType::Ping))
                        .await?;
                }
                DhtReq::FindNode(addr, target) => {
                    self.send_find_node(
                        addr,
                        target.clone(),
                        Transaction::new(Some(cb), depth, Some(target), QueryType::FindNode),
                    )
                    .await?;
                }
                DhtReq::GetPeers(info_hash) => {
                    let tran = Transaction::new(
                        Some(cb),
                        depth,
                        Some(info_hash.clone()),
                        QueryType::GetPeers,
                    );
                    for n in self.get_closest_nodes(&info_hash) {
                        self.send_get_peers(n, info_hash.clone(), tran.clone())
                            .await?;
                    }
                }
                DhtReq::AnnouncePeer(info_hash) => {
                    let tran =
                        Transaction::new(Some(cb.clone()), depth, None, QueryType::AnnouncePeer);
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
        if self.support_ipv4 {
            removed_nodes.extend(self.routing_table4.refresh());
        }
        if self.support_ipv6 {
            removed_nodes.extend(self.routing_table6.refresh());
        }
        for node in removed_nodes.iter() {
            for tran in self.transaction_manager.remove_by_node(&node.id) {
                let _ = tran.callback(Err(Error::TransactionTimeout)).await;
                error!("tran {:?}, timeout", tran);
            }
        }
        for tran in self
            .transaction_manager
            .refresh(self.config.read().unwrap().max_transaction_time_out)
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
    pub async fn ping<A: AsyncToSocketAddrs>(&self, addr: A) -> Result<()> {
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
            Ok(DhtRsp::Pong) => Ok(()),
            Ok(rsp) => {
                error!("expect receive pong, but receive {:?}", rsp);
                Err(Error::ProtocolErr)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn find_node(&self, addr: PeerAddress, id: HashPiece) -> Result<Option<Node>> {
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

    pub async fn get_peers(&self, info_hash: HashPiece) -> impl Stream<Item = PeerAddress> {
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
        receiver.filter_map(|rsp| match rsp {
            Ok(DhtRsp::GetPeers(addr)) => Some(addr),
            _ => None,
        })
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
    use std::time::Duration;

    use super::*;
    use smol::block_on;

    #[test]
    fn test_server_bootstrap() {
        let config = DhtConfig {
            k: 8,
            id: HashPiece::rand_new(),
            secret: "hello".to_string(),
            token_interval: Duration::from_secs(30),
            max_token_interval_count: 2,
            refresh_interval: Duration::from_secs(30),
            depth: 2,
            implied_port: true,
            max_transaction_time_out: Duration::from_secs(5),
        };
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
    fn test_client_ping() {
        let config = DhtConfig {
            k: 8,
            id: HashPiece::rand_new(),
            secret: "hello".to_string(),
            token_interval: Duration::from_secs(30),
            max_token_interval_count: 2,
            refresh_interval: Duration::from_secs(30),
            depth: 2,
            implied_port: true,
            max_transaction_time_out: Duration::from_secs(5),
        };
        block_on(async {
            let (mut server, client) = DhtServer::new(
                "0.0.0.0:6881",
                Arc::new(RwLock::new(config)),
                MemPeerStore::default(),
            )
            .await
            .unwrap();

            assert!(or(server.run(), async move {
                assert!(client.ping("router.bittorrent.com:6881").await.is_ok());
                Ok(())
            })
            .await
            .is_ok());
        });
    }
}
