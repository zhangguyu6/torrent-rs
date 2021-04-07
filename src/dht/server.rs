use super::{DhtConfig, DhtMessage, DhtReq, DhtRsp, RoutingTable, TokenManager, Transaction};
use crate::error::{Error, Result};
use crate::krpc::MAX_KRPC_MESSAGE_SIZE;
use crate::metainfo::HashPiece;
use crate::{bencode::from_bytes, metainfo::PeerAddress};
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
    support_ipv4: bool,
    support_ipv6: bool,
    socket: UdpSocket,
    routing_table4: RoutingTable,
    routing_table6: RoutingTable,
    token_manager: TokenManager,
    receiver: Receiver<DhtMessage>,
    seq: usize,
    callback_map: HashMap<usize, Transaction>,
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
        let (sender, receiver) = unbounded();
        let seq = 0;
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
                seq,
                callback_map,
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

    async fn send_ping(&mut self, seq: usize, addr: PeerAddress) -> Result<()> {
        unimplemented!()
    }

    async fn send_find_node(&mut self, seq: usize, addr: PeerAddress, id: HashPiece) -> Result<()> {
        unimplemented!()
    }

    async fn send_get_peers(&mut self, seq: usize, id: HashPiece) -> Result<()> {
        unimplemented!()
    }

    async fn send_announce_peer(&mut self, seq: usize, id: HashPiece) -> Result<()> {
        unimplemented!()
    }

    async fn handle(&mut self, buf: &mut [u8]) -> Result<bool> {
        match or(self.receiver_req(), self.receiver_rsp(buf)).await {
            Ok(DhtMessage::Req(req, mut callback)) => {
                self.seq += 1;
                match req {
                    DhtReq::ShutDown => {
                        let _ = callback.send(Ok(DhtRsp::Done));
                        return Ok(true);
                    }
                    DhtReq::Ping(addr) => {
                        self.callback_map
                            .insert(self.seq, Transaction::new(callback));
                        self.send_ping(self.seq, addr).await?;
                    }
                    DhtReq::FindNode(addr, id) => {
                        self.callback_map
                            .insert(self.seq, Transaction::new(callback));
                        self.send_find_node(self.seq, addr, id).await?;
                    }
                    DhtReq::GetPeers(id) => {
                        self.callback_map
                            .insert(self.seq, Transaction::new(callback));
                        self.send_get_peers(self.seq, id).await?;
                    }
                    DhtReq::AnnouncePeer(id) => {
                        self.callback_map
                            .insert(self.seq, Transaction::new(callback));
                        self.send_announce_peer(self.seq, id).await?;
                    }
                }
            }
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
