use super::{
    DhtConfig, DhtMessage, DhtReq, DhtRsp, MemPeerStore, PeerStore, RoutingTable, TokenManager,
    Transaction, UpdatedNode,
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
    future::or,
    net::{AsyncToSocketAddrs, UdpSocket},
};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

pub struct DhtServer<S = MemPeerStore> {
    addr: SocketAddr,
    id: HashPiece,
    support_ipv4: bool,
    support_ipv6: bool,
    socket: UdpSocket,
    routing_table4: RoutingTable,
    routing_table6: RoutingTable,
    token_manager: TokenManager,
    receiver: Receiver<DhtMessage>,
    seq: usize,
    depth: usize,
    callback_map: HashMap<usize, Transaction>,
    k: usize,
    implied_port: bool,
    peer_store: S,
}

impl<S: PeerStore> DhtServer<S> {
    async fn new<A: AsyncToSocketAddrs>(
        addr: A,
        config: &DhtConfig,
        store: S,
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
        let routing_table4 = RoutingTable::new();
        let routing_table6 = RoutingTable::new();
        let token_manager = TokenManager::new(config);
        let id = config.id.clone();
        let (sender, receiver) = unbounded();
        let seq = 0;
        let depth = config.depth;
        let k = config.k;
        let implied_port = config.implied_port;
        let callback_map = HashMap::new();
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
                callback_map,
                id,
                seq,
                depth,
                k,
                implied_port,
                peer_store: store,
            },
            DhtClient { sender },
        ))
    }

    pub async fn bootstrap(&mut self) -> Result<()> {
        unimplemented!()
    }

    fn insert_node(&mut self, node: UpdatedNode) -> bool {
        let mut existed = false;
        if node.node.peer_address.0.is_ipv4() {
            if self.support_ipv4 {
                if let Some(_) = self.routing_table4.insert(node.clone()) {
                    existed = true;
                }
            }
        }
        if node.node.peer_address.0.is_ipv6() {
            if self.support_ipv6 {
                if let Some(_) = self.routing_table6.insert(node.clone()) {
                    existed = true;
                }
            }
        }
        existed
    }

    fn get_closest_nodes(&self, hash_piece: &HashPiece) -> Vec<UpdatedNode> {
        let mut nodes = Vec::with_capacity(self.k);
        if self.support_ipv4 {
            nodes.extend(self.routing_table4.closest(hash_piece, self.k));
        }
        if self.support_ipv6 {
            nodes.extend(self.routing_table6.closest(hash_piece, self.k));
        }
        nodes
    }

    async fn receiver_req(&self) -> Result<DhtMessage> {
        Ok(self.receiver.recv().await?)
    }

    async fn receiver_rsp(&self, buf: &mut [u8]) -> Result<DhtMessage> {
        let (size, addr) = self.socket.recv_from(buf).await?;
        let krpc_message = from_bytes(&buf[0..size])?;
        Ok(DhtMessage::Message(krpc_message, addr))
    }

    async fn send_krpc_message(
        &mut self,
        mut message: KrpcMessage,
        addr: SocketAddr,
    ) -> Result<()> {
        match message.a.as_mut() {
            Some(query) => query.id = self.id.clone(),
            None => {
                message.a = Some(KrpcQuery {
                    id: self.id.clone(),
                    ..Default::default()
                })
            }
        }
        let mut buf = to_bytes(&message)?;
        self.socket.send_to(&mut buf, addr).await?;
        Ok(())
    }

    async fn send_ping(&mut self, addr: PeerAddress, tran: Transaction) -> Result<()> {
        self.seq += 1;
        let query = KrpcQuery {
            id: self.id.clone(),
            ..Default::default()
        };
        let message = KrpcMessage {
            t: self.seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::Ping),
            a: Some(query),
            ..Default::default()
        };
        self.send_krpc_message(message, addr.0).await?;
        self.callback_map.insert(self.seq, tran);
        Ok(())
    }

    async fn send_find_node(
        &mut self,
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
            id: self.id.clone(),
            target: Some(target.clone()),
            want,
            ..Default::default()
        };
        self.seq += 1;
        let message = KrpcMessage {
            t: self.seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::FindNode),
            a: Some(query),
            ..Default::default()
        };
        self.send_krpc_message(message, addr.0).await?;
        self.callback_map.insert(self.seq, tran);
        Ok(())
    }

    async fn send_get_peers(
        &mut self,
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
        let query = KrpcQuery {
            id: self.id.clone(),
            info_hash: Some(info_hash),
            want,
            ..Default::default()
        };
        self.seq += 1;
        let message = KrpcMessage {
            t: self.seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::GetPeers),
            a: Some(query),
            ..Default::default()
        };
        self.send_krpc_message(message, node.peer_address.0).await?;
        self.callback_map.insert(self.seq, tran.clone());
        Ok(())
    }

    async fn send_announce_peer(
        &mut self,
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
            id: self.id.clone(),
            info_hash: Some(info_hash),
            implied_port: if !self.implied_port {
                None
            } else {
                Some(self.implied_port)
            },
            port: Some(self.addr.port()),
            want,
            ..Default::default()
        };
        if let Some(token) = self.token_manager.get_token(&node.id) {
            self.seq += 1;
            query.token = Some(token.clone());
            let message = KrpcMessage {
                t: self.seq.to_string(),
                y: MessageType::Query,
                q: Some(QueryType::AnnouncePeer),
                a: Some(query),
                ..Default::default()
            };
            self.callback_map.insert(self.seq, tran.clone());
            self.send_krpc_message(message, node.peer_address.0).await?;
        }
        Ok(())
    }

    async fn on_error(&mut self, mut message: KrpcMessage) -> Result<()> {
        let e = message.e.take().ok_or(Error::ProtocolErr)?;
        let err = Error::KrpcErr(e);
        let tran_id: usize = message.t.parse()?;
        let tran = self
            .callback_map
            .remove(&tran_id)
            .ok_or(Error::TransactionNotFound(tran_id))?;
        // ignore if client dropped
        let _ = tran.callback.send(Err(err)).await;
        Ok(())
    }

    async fn on_response(&mut self, mut message: KrpcMessage, addr: SocketAddr) -> Result<()> {
        let tran_id: usize = message.t.parse()?;
        let tran = self
            .callback_map
            .remove(&tran_id)
            .ok_or(Error::TransactionNotFound(tran_id))?;
        let mut rsp = message.r.take().ok_or(Error::ProtocolErr)?;
        if message.ro != Some(true) {
            self.insert_node(UpdatedNode::new_id_addr(rsp.id.clone(), addr));
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
        let _ = tran.callback.send(Ok(DhtRsp::Pong));
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
                && !self.insert_node(UpdatedNode::new(n.clone()))
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
                && !self.insert_node(UpdatedNode::new(n.clone()))
                && find_node.is_none()
                && tran.depth > 0
            {
                self.send_find_node(n.peer_address, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        if find_node.is_some() {
            let _ = tran.callback.send(Ok(DhtRsp::FindNode(find_node)));
        }
        Ok(())
    }

    async fn on_get_peers_rsp(
        &mut self,
        mut rsp: KrpcResponse,
        mut tran: Transaction,
    ) -> Result<()> {
        let _ = tran.callback.send(Ok(DhtRsp::Pong));
        if let Some(mut addrs) = rsp.values.take() {
            for addr in addrs.0.drain(..) {
                let _ = tran.callback.send(Ok(DhtRsp::GetPeers(addr)));
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
            if self.support_ipv4 && !self.insert_node(UpdatedNode::new(n.clone())) && tran.depth > 0
            {
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
            if self.support_ipv6 && !self.insert_node(UpdatedNode::new(n.clone())) && tran.depth > 0
            {
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
        let _ = tran.callback.send(Ok(DhtRsp::Announced));
        Ok(())
    }

    async fn on_query(&mut self, message: KrpcMessage, addr: SocketAddr) -> Result<()> {
        let query_t = message.q.ok_or(Error::ProtocolErr)?;
        let query = message.a.ok_or(Error::ProtocolErr)?;
        match query_t {
            QueryType::Ping => self.on_ping_query(query, message.t, addr).await?,
            QueryType::FindNode => self.on_find_node_query(query, message.t, addr).await?,
            QueryType::GetPeers => self.on_get_peers_query(query, message.t, addr).await?,
            QueryType::AnnouncePeer => {}
        }
        unimplemented!()
    }

    async fn on_ping_query(
        &mut self,
        req: KrpcQuery,
        tran_id: String,
        mut addr: SocketAddr,
    ) -> Result<()> {
        let rsp = KrpcResponse {
            id: self.id.clone(),
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
        let mut rsp = KrpcResponse {
            id: self.id.clone(),
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
                    self.routing_table4
                        .closest(&target, self.k)
                        .into_iter()
                        .map(|update_node| update_node.node),
                ));
            } else {
                rsp.nodes6 = Some(CompactNodes::from(
                    self.routing_table6
                        .closest(&target, self.k)
                        .into_iter()
                        .map(|update_node| update_node.node),
                ));
            }
        }
        if want_ipv4 {
            rsp.nodes = Some(CompactNodes::from(
                self.routing_table4
                    .closest(&target, self.k)
                    .into_iter()
                    .map(|update_node| update_node.node),
            ));
        }
        if want_ipv6 {
            rsp.nodes6 = Some(CompactNodes::from(
                self.routing_table6
                    .closest(&target, self.k)
                    .into_iter()
                    .map(|update_node| update_node.node),
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
        let info_hash = req.info_hash.take().ok_or(Error::ProtocolErr)?;
        let mut rsp = KrpcResponse {
            id: self.id.clone(),
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
                        .get(&info_hash, self.k)
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
                        .get(&info_hash, self.k)
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
                        .get(&info_hash, self.k)
                        .unwrap_or(Vec::new())
                        .into_iter()
                        .map(|update_node| update_node.peer_address),
                ));
            } else if want_ipv4 {
                rsp.values = Some(CompactAddresses::from(
                    self.peer_store
                        .get(&info_hash, self.k)
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
                        .get(&info_hash, self.k)
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
        let info_hash = req.info_hash.take().ok_or(Error::ProtocolErr)?;
        let token = req.token.take().ok_or(Error::ProtocolErr)?;
        if !self.token_manager.valid_token(token, &PeerAddress(addr)) {
            return Err(Error::ProtocolErr);
        }
        let rsp = KrpcResponse {
            id: self.id.clone(),
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

    async fn handle(&mut self, buf: &mut [u8]) -> Result<bool> {
        match or(self.receiver_req(), self.receiver_rsp(buf)).await {
            Ok(DhtMessage::Req(req, cb)) => match req {
                DhtReq::ShutDown => {
                    let _ = cb.send(Ok(DhtRsp::ShutDown));
                    return Ok(true);
                }
                DhtReq::Ping(addr) => {
                    self.send_ping(addr, Transaction::new(cb, 0, QueryType::Ping, None))
                        .await?;
                }
                DhtReq::FindNode(addr, target) => {
                    self.send_find_node(
                        addr,
                        target.clone(),
                        Transaction::new(cb, self.depth, QueryType::FindNode, Some(target)),
                    )
                    .await?;
                }
                DhtReq::GetPeers(info_hash) => {
                    let tran = Transaction::new(
                        cb,
                        self.depth,
                        QueryType::GetPeers,
                        Some(info_hash.clone()),
                    );
                    for n in self.get_closest_nodes(&info_hash) {
                        self.send_get_peers(n.node, info_hash.clone(), tran.clone())
                            .await?;
                    }
                }
                DhtReq::AnnouncePeer(info_hash) => {
                    let tran =
                        Transaction::new(cb.clone(), self.depth, QueryType::AnnouncePeer, None);
                    for n in self.get_closest_nodes(&info_hash) {
                        match self
                            .send_announce_peer(n.node, info_hash.clone(), tran.clone())
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
            Err(Error::ChannelRecvErr(_)) => return Ok(true),
            Err(e) => return Err(e),
        }
        Ok(false)
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut buf = [0; MAX_KRPC_MESSAGE_SIZE];
        loop {
            match self.handle(&mut buf).await {
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
