use super::{
    DhtConfig, DhtMessage, DhtReq, DhtRsp, RoutingTable, TokenManager, Transaction, UpdatedNode,
};
use crate::bencode::{from_bytes, to_bytes};
use crate::error::{Error, Result};
use crate::krpc::{
    KrpcMessage, KrpcQuery, KrpcResponse, MessageType, QueryType, MAX_KRPC_MESSAGE_SIZE,
};
use crate::metainfo::{HashPiece, Node, PeerAddress};
use log::{debug, error};
use smol::{
    channel::{unbounded, Receiver, Sender},
    future::or,
    net::{AsyncToSocketAddrs, UdpSocket},
};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

pub struct DhtServer {
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
}

impl DhtServer {
    async fn new<A: AsyncToSocketAddrs>(addr: A, config: &DhtConfig) -> Result<(Self, DhtClient)> {
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
        Ok(DhtMessage::Rsp(krpc_message, addr))
    }

    async fn send_krpc_message(
        &mut self,
        mut message: KrpcMessage,
        addr: PeerAddress,
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
        self.socket.send_to(&mut buf, addr.0).await?;
        Ok(())
    }

    async fn send_ping(&mut self, addr: PeerAddress, tran: Transaction) -> Result<()> {
        self.seq += 1;
        let message = KrpcMessage {
            t: self.seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::Ping),
            ..Default::default()
        };
        self.callback_map.insert(self.seq, tran);
        self.send_krpc_message(message, addr).await
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
        self.callback_map.insert(self.seq, tran);
        self.send_krpc_message(message, addr).await
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
            info_hash: Some(info_hash),
            want,
            ..Default::default()
        };
        self.seq += 1;
        let message = KrpcMessage {
            t: self.seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::GetPeers),
            a: Some(query.clone()),
            ..Default::default()
        };
        self.callback_map.insert(self.seq, tran.clone());
        self.send_krpc_message(message, node.peer_address).await?;
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
                a: Some(query.clone()),
                ..Default::default()
            };
            self.callback_map.insert(self.seq, tran.clone());
            self.send_krpc_message(message, node.peer_address).await?;
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
        let rsp = message.r.take().ok_or(Error::ProtocolErr)?;
        if message.ro != Some(true) {
            self.insert_node(UpdatedNode::new_id_addr(rsp.id.clone(), addr));
        }
        match tran.query_type {
            QueryType::Ping => {
                self.on_ping_rsp(rsp, tran).await?;
            }
            QueryType::FindNode => {
                self.on_find_node_rsp(rsp, tran).await?;
            }
            QueryType::GetPeers => {
                if let Some(token) = rsp.token.clone() {
                    self.token_manager.insert_token(rsp.id.clone(), token);
                }
                self.on_get_peers_rsp(rsp, tran).await?;
            }
            QueryType::AnnouncePeer => {}
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
                self.send_get_peers(n, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        for n in node6s.drain(..) {
            if tran.contain_id(&n.id) {
                continue;
            }
            tran.insert_id(n.id.clone());
            if self.support_ipv6 && !self.insert_node(UpdatedNode::new(n.clone())) && tran.depth > 0
            {
                self.send_get_peers(n, tran.target.clone().unwrap(), tran.clone())
                    .await?;
            }
        }
        Ok(())
    }

    async fn on_announce_rsp(&mut self, _: KrpcResponse, tran: Transaction) -> Result<()> {
        let _ = tran.callback.send(Ok(DhtRsp::Announced));
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
                        self.send_announce_peer(n.node, info_hash.clone(), tran.clone())
                            .await?;
                    }
                }
            },
            Ok(DhtMessage::Rsp(rsp, addr)) => match rsp.y {
                MessageType::Query => unreachable!(),
                MessageType::Error => self.on_error(rsp).await?,
                MessageType::Response => self.on_response(rsp, addr).await?,
            },
            Err(Error::ChannelRecvErr(_)) => return Ok(true),
            Err(e) => return Err(e),
        }
        Ok(false)
    }

    async fn run(&mut self) -> Result<()> {
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
