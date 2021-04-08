use super::{DhtConfig, DhtMessage, DhtReq, DhtRsp, RoutingTable, TokenManager, Transaction};
use crate::bencode::{from_bytes, to_bytes};
use crate::error::{Error, Result};
use crate::krpc::{KrpcMessage, KrpcQuery, MessageType, QueryType, MAX_KRPC_MESSAGE_SIZE};
use crate::metainfo::HashPiece;
use crate::metainfo::PeerAddress;
use log::{debug, error};
use smol::{
    channel::{unbounded, Receiver, Sender},
    future::or,
    net::{AsyncToSocketAddrs, UdpSocket},
};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
};

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

    async fn receiver_req(&self) -> Result<DhtMessage> {
        Ok(self.receiver.recv().await?)
    }

    async fn receiver_rsp(&self, buf: &mut [u8]) -> Result<DhtMessage> {
        let size = self.socket.recv(buf).await?;
        let krpc_message = from_bytes(&buf[0..size])?;
        Ok(DhtMessage::Rsp(krpc_message))
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

    async fn send_ping(
        &mut self,
        seq: usize,
        addr: PeerAddress,
        cb: Sender<Result<DhtRsp>>,
    ) -> Result<()> {
        let message = KrpcMessage {
            t: seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::Ping),
            ..Default::default()
        };
        let transaction = Transaction::new(cb, 0);
        self.callback_map.insert(self.seq, transaction);
        self.seq += 1;
        self.send_krpc_message(message, addr).await
    }

    async fn send_find_node(
        &mut self,
        seq: usize,
        addr: PeerAddress,
        id: HashPiece,
        cb: Sender<Result<DhtRsp>>,
    ) -> Result<()> {
        let mut want = Vec::new();
        if self.support_ipv4 {
            want.push("n4".to_string());
        }
        if self.support_ipv6 {
            want.push("n6".to_string());
        }
        let query = KrpcQuery {
            target: Some(id),
            want,
            ..Default::default()
        };
        let message = KrpcMessage {
            t: seq.to_string(),
            y: MessageType::Query,
            q: Some(QueryType::FindNode),
            a: Some(query),
            ..Default::default()
        };
        let transaction = Transaction::new(cb, self.depth);
        self.callback_map.insert(self.seq, transaction);
        self.seq += 1;
        self.send_krpc_message(message, addr).await
    }

    async fn send_get_peers(
        &mut self,
        seq: usize,
        info_hash: HashPiece,
        cb: Sender<Result<DhtRsp>>,
    ) -> Result<()> {
        let mut nodes = Vec::with_capacity(self.k);
        if self.support_ipv4 {
            nodes.extend(self.routing_table4.closest(&info_hash, self.k));
        }
        if self.support_ipv6 {
            nodes.extend(self.routing_table6.closest(&info_hash, self.k));
        }
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
        for node in nodes.drain(..) {
            let message = KrpcMessage {
                t: seq.to_string(),
                y: MessageType::Query,
                q: Some(QueryType::GetPeers),
                a: Some(query.clone()),
                ..Default::default()
            };
            let transaction = Transaction::new(cb.clone(), self.depth);
            self.callback_map.insert(self.seq, transaction);
            self.seq += 1;
            self.send_krpc_message(message, node.node.peer_address)
                .await?;
        }
        Ok(())
    }

    async fn send_announce_peer(
        &mut self,
        seq: usize,
        info_hash: HashPiece,
        cb: Sender<Result<DhtRsp>>,
    ) -> Result<()> {
        let mut nodes = Vec::with_capacity(self.k);
        if self.support_ipv4 {
            nodes.extend(self.routing_table4.closest(&info_hash, self.k));
        }
        if self.support_ipv6 {
            nodes.extend(self.routing_table6.closest(&info_hash, self.k));
        }
        let mut want = Vec::new();
        if self.support_ipv4 {
            want.push("n4".to_string());
        }
        if self.support_ipv6 {
            want.push("n6".to_string());
        }
        let query = KrpcQuery {
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
        for node in nodes.drain(..) {
            let message = KrpcMessage {
                t: seq.to_string(),
                y: MessageType::Query,
                q: Some(QueryType::AnnouncePeer),
                a: Some(query.clone()),
                ..Default::default()
            };
            let transaction = Transaction::new(cb.clone(), self.depth);
            self.callback_map.insert(self.seq, transaction);
            self.seq += 1;
            self.send_krpc_message(message, node.node.peer_address)
                .await?;
        }
        Ok(())
    }

    async fn handle(&mut self, buf: &mut [u8]) -> Result<bool> {
        match or(self.receiver_req(), self.receiver_rsp(buf)).await {
            Ok(DhtMessage::Req(req, cb)) => match req {
                DhtReq::ShutDown => {
                    let _ = cb.send(Ok(DhtRsp::Done));
                    return Ok(true);
                }
                DhtReq::Ping(addr) => {
                    self.send_ping(self.seq, addr, cb).await?;
                }
                DhtReq::FindNode(addr, id) => {
                    self.send_find_node(self.seq, addr, id, cb).await?;
                }
                DhtReq::GetPeers(id) => {
                    self.send_get_peers(self.seq, id, cb).await?;
                }
                DhtReq::AnnouncePeer(id) => {
                    self.send_announce_peer(self.seq, id, cb).await?;
                }
            },
            Ok(DhtMessage::Rsp(rsp)) => {}
            Err(Error::ChannelClosed(_)) => return Ok(true),
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
