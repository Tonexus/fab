// representation of broadcaster service

mod node;
mod message;
mod error;

use std::{sync::Arc, collections::HashMap, net::SocketAddr};
use rand::Rng;
use futures::future::{RemoteHandle, FutureExt};
use tokio::{prelude::*, sync::{mpsc, Mutex, RwLock}, net::{TcpListener, TcpStream}};
use indexmap::IndexMap;

use node::Node;
use message::Message;
use error::{Result, FabError};

type SafeNodes    = Arc<RwLock<IndexMap<SocketAddr, Node>>>;
type SafeReceived = Arc<Mutex<HashMap<String, ()>>>;

#[derive(Debug)]
struct Listener {
    // hashmap of messages received by this listener
    safe_recvd: SafeReceived,
    // when dropped, listener dies
    handle:     RemoteHandle<()>,
}

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Debug)]
pub struct Broadcaster {
    // vector of nodes to send/listen to
    safe_nodes: SafeNodes,
    // hashmap of messages received by the overall broadcaster
    safe_recvd: SafeReceived,
    msg_recv:   mpsc::UnboundedReceiver<String>,
    msg_send:   mpsc::UnboundedSender<String>,
    // maps port number to a listener
    listeners:  HashMap<u16, Listener>,
}

impl Broadcaster {
    pub fn new() -> Broadcaster {
        let (s, r) = mpsc::unbounded_channel();
        Broadcaster {
            safe_nodes: Arc::new(RwLock::new(IndexMap::new())),
            safe_recvd: Arc::new(Mutex::new(HashMap::new())),
            msg_recv:   r,
            msg_send:   s,
            listeners:  HashMap::new(),
        }
    }

    // opens a listener (not required to send)
    pub async fn listen(&mut self, port: u16) -> Result<()> {
        // make sure not already have a listener
        self.listeners.get(&port).map_or(Ok(()), |_| Err(FabError::AlreadyListeningError))?;

        // bind to listening port
        // TODO change to 0.0.0.0 when want to expose publicly
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;

        // create hashmap of received messages for new listener
        let safe_recvd_l = Arc::new(Mutex::new(HashMap::new()));

        // create copies for other thread
        let safe_nodes = self.safe_nodes.clone();
        let safe_recvd_l2 = safe_recvd_l.clone();
        let safe_recvd_b = self.safe_recvd.clone();
        let msg_send = self.msg_send.clone();

        let (task, handle) = async move { loop {
            // accept new connections and handle them
            if let Ok((mut sock, _)) = listener.accept().await {
                let safe_nodes_2 = safe_nodes.clone();
                let safe_recvd_l3 = safe_recvd_l2.clone();
                let safe_recvd_b2 = safe_recvd_b.clone();
                let msg_send_2 = msg_send.clone();
                tokio::spawn(async move {
                    match handle_socket(
                        &mut sock,
                        safe_nodes_2,
                        safe_recvd_l3,
                        safe_recvd_b2,
                        msg_send_2,
                    ).await {
                        Ok(_)  => (),
                        // TODO better process/log errors
                        Err(e) => println!("Got error: {:?}", e),
                    }
                });
            }
        }}.remote_handle();

        tokio::spawn(task);

        self.listeners.insert(port, Listener {
            safe_recvd: safe_recvd_l,
            handle:     handle,
        });

        Ok(())
    }

    // close listener
    pub fn close(&mut self, port: u16) -> Result<()> {
        if let Some(_) = self.listeners.remove(&port) {
            return Ok(());
        } else {
            return Err(FabError::NotListeningError);
        }
    }

    // registers another node to broadcast to
    pub async fn add_node(
        &mut self,
        address: SocketAddr,
        public_key: Option<()>,
    ) {
        self.safe_nodes.write().await.insert(address, Node::new(address, public_key));
    }

    // registers another node to broadcast to
    pub async fn remove_node(
        &mut self,
        address: SocketAddr,
    ) {
        self.safe_nodes.write().await.remove(&address);
    }

    pub async fn broadcast(&mut self, content: &str) -> Result<()> {
        // indicate that message to send has been received just in case it gets echoed back
        self.safe_recvd.lock().await.insert(content.to_string(), ());
        for (_, l) in self.listeners.iter() {
            l.safe_recvd.lock().await.insert(content.to_string(), ());
        }
        // TODO also indicate on listeners

        gbroadcast(content, self.safe_nodes.read().await.clone()).await;
        Ok(())
    }

    pub async fn receive(&mut self) -> Result<String> {
        if self.listeners.is_empty() {
            Err(FabError::NotListeningError)
        } else {
            // technically error should never happen, as broadcaster has a sender
            self.msg_recv.recv().await.ok_or(FabError::ChannelRecvError)
        }
    }
}

// generic broadcast of string to nodes
async fn gbroadcast(content: &str, mut nodes: IndexMap<SocketAddr, Node>) {
    loop {
        if nodes.len() == 0 {
            return;
        }
        // pick random node
        let i = rand::thread_rng().gen_range(0, nodes.len());
        // TODO chain ifs
        if let Some((_, node)) = nodes.get_index(i) {
            // if no error and other end has seen message, exit
            if let Ok(seen) = node.send(content).await {
                if seen {
                    return;
                }
            }
        }
        // TODO retry if connection is just bad?
        // TODO batch?
        // otherwise, remove from list of nodes to try and continue
        nodes.swap_remove_index(i);
    }
}

async fn handle_socket(
    sock: &mut TcpStream,
    safe_nodes: SafeNodes,
    safe_recvd_l: SafeReceived,
    safe_recvd_b: SafeReceived,
    msg_send: mpsc::UnboundedSender<String>,
) -> Result<()> {
    let message = Message::from_socket(sock).await?; // TODO eventually do history signature processing
    let content = message.get_content();
    // insert content and get whether listener has seen content
    let seen_l = safe_recvd_l.lock().await.insert(content.to_string(), ()).is_some();
    // reply back whether content has been seen or not
    sock.write_u8(if seen_l { 1 } else { 0 }).await?;
    // if not seen, propagate up and start broadcast
    if !seen_l {
        // insert content and get whether broadcaster has seen content TODO async with broadcast
        let seen_b = safe_recvd_b.lock().await.insert(content.to_string(), ()).is_some();
        if !seen_b {
            msg_send.send(content.clone())?;
        }
        // start own broadcast
        // TODO figure out source node from history and remove
        let nodes = safe_nodes.read().await.clone();
        tokio::spawn(async move {
            gbroadcast(&content, nodes).await;
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Broadcaster;

    #[tokio::test]
    async fn broadcast_send_one_party() {
        let mut b: Broadcaster = Broadcaster::new();
        assert!(b.broadcast("test 1 message 1").await.is_ok());
        assert!(b.listen(8080).await.is_ok());
        b.add_node("127.0.0.1:8080".parse().unwrap(), None).await;
        assert!(b.broadcast("test 1 message 2").await.is_ok());
    }

    #[tokio::test]
    async fn broadcast_send_two_party() {
        let mut b1: Broadcaster = Broadcaster::new();
        let mut b2: Broadcaster = Broadcaster::new();
        assert!(b1.listen(8081).await.is_ok());
        assert!(b2.listen(8082).await.is_ok());
        b1.add_node("127.0.0.1:8082".parse().unwrap(), None).await;
        b2.add_node("127.0.0.1:8081".parse().unwrap(), None).await;
        assert!(b1.broadcast("test 2 message 1").await.is_ok());
        let recv = b2.receive().await;
        assert!(recv.is_ok());
        assert_eq!(recv.unwrap(), "test 2 message 1");
        assert!(b2.close(8082).is_ok());
        assert!(b2.broadcast("test 2 message 2").await.is_ok());
        let recv = b1.receive().await;
        assert!(recv.is_ok());
        assert_eq!(recv.unwrap(), "test 2 message 2");
        assert!(b1.broadcast("test 2 message 3").await.is_ok());
    }

    #[tokio::test]
    async fn broadcast_send_three_party() {
        let mut b1: Broadcaster = Broadcaster::new();
        let mut b2: Broadcaster = Broadcaster::new();
        let mut b3: Broadcaster = Broadcaster::new();
        assert!(b1.listen(8083).await.is_ok());
        assert!(b2.listen(8084).await.is_ok());
        assert!(b3.listen(8085).await.is_ok());
        b1.add_node("127.0.0.1:8084".parse().unwrap(), None).await;
        b1.add_node("127.0.0.1:8085".parse().unwrap(), None).await;
        b2.add_node("127.0.0.1:8083".parse().unwrap(), None).await;
        b2.add_node("127.0.0.1:8085".parse().unwrap(), None).await;
        b3.add_node("127.0.0.1:8083".parse().unwrap(), None).await;
        b3.add_node("127.0.0.1:8084".parse().unwrap(), None).await;
        assert!(b1.broadcast("test 3 message 1").await.is_ok());
        let recv = b2.receive().await;
        assert!(recv.is_ok());
        assert_eq!(recv.unwrap(), "test 3 message 1");
        let recv = b3.receive().await;
        assert!(recv.is_ok());
        assert_eq!(recv.unwrap(), "test 3 message 1");
    }
}
