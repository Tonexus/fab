// representation of broadcaster service

mod node;
mod message;
mod error;

use std::{
    sync::Arc,
    collections::HashMap,
    net::SocketAddr,
};
use rand::Rng;
use futures::future::{RemoteHandle, FutureExt};
use tokio::{prelude::*, sync::{Mutex, RwLock}, net::{TcpListener, TcpStream}};
use indexmap::IndexMap;

use node::Node;
use message::Message;
use error::{Result, FabError};

type SafeNodes    = Arc<RwLock<IndexMap<SocketAddr, Node>>>;
type SafeReceived = Arc<Mutex<HashMap<String, ()>>>;

#[derive(Debug)]
struct Listener {
    // hashmap of messages received by this listener
    safe_received: SafeReceived,
    // when dropped, listener dies
    handle:        RemoteHandle<()>,
}

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Debug)]
pub struct Broadcaster {
    // vector of nodes to send/listen to
    safe_nodes:    SafeNodes,
    // hashmap of messages received by the overall broadcaster
    safe_received: SafeReceived,
    // maps port number to a listener
    listeners:     HashMap<u16, Listener>,
}

impl Broadcaster {
    pub fn new() -> Broadcaster {
        Broadcaster {
            safe_nodes:    Arc::new(RwLock::new(IndexMap::new())),
            safe_received: Arc::new(Mutex::new(HashMap::new())),
            listeners:     HashMap::new(),
        }
    }

    // opens the listener (not required to send)
    pub async fn listen(&mut self, port: u16) -> Result<()> {
        // make sure not already have a listener
        self.listeners.get(&port).map_or(Ok(()), |_| Err(FabError::AlreadyListeningError))?;

        // bind to listening port
        // TODO change to 0.0.0.0 when want to expose publicly
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;

        // create hashmap of received messages for new listener
        let safe_received_l = Arc::new(Mutex::new(HashMap::new()));

        // create copies for other thread
        let safe_nodes = self.safe_nodes.clone();
        let safe_received_l2 = safe_received_l.clone();
        let safe_received_b = self.safe_received.clone();

        let (task, handle) = async move { loop {
            // accept new connections and handle them
            if let Ok((mut sock, addr)) = listener.accept().await {
                let safe_nodes_2 = safe_nodes.clone();
                let safe_received_l3 = safe_received_l2.clone();
                let safe_received_b2 = safe_received_b.clone();
                tokio::spawn(async move {
                    match handle_socket(
                        &mut sock,
                        addr,
                        safe_nodes_2,
                        safe_received_l3,
                        safe_received_b2,
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
            safe_received: safe_received_l,
            handle:        handle,
        });

        Ok(())
    }

    // close listener
    pub fn close(&mut self, port: u16) -> Result<()> {
        if let Some(_) = self.listeners.remove(&port) {
            // send shutdown, ok if error since means listener already dead
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
        self.safe_received.lock().await.insert(content.to_string(), ());
        for (_, l) in self.listeners.iter() {
            l.safe_received.lock().await.insert(content.to_string(), ());
        }
        // TODO also indicate on listeners

        gbroadcast(content, self.safe_nodes.read().await.clone()).await;
        Ok(())
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
    source_addr: SocketAddr,
    safe_nodes: SafeNodes,
    safe_received_l: SafeReceived,
    safe_received_b: SafeReceived,
) -> Result<()> {
    let message = Message::from_socket(sock).await?; // TODO eventually do history signature processing
    let content = message.get_content();
    // insert content and get whether listener has seen content
    let seen_l = safe_received_l.lock().await.insert(content.to_string(), ()).is_some();
    // reply back whether content has been seen or not
    sock.write_u8(if seen_l { 1 } else { 0 }).await?;
    // if not seen, propagate up and start broadcast
    if !seen_l {
        println!("Received \"{}\" as new message", &content);
        // insert content and get whether broadcaster has seen content TODO async with broadcast
        let seen_b = safe_received_b.lock().await.insert(content.to_string(), ()).is_some();
        if !seen_b {
            // TODO insert to channel
        }
        // start own broadcast
        // TODO move node remove into loop?
        let mut nodes = safe_nodes.read().await.clone();
        nodes.remove(&source_addr);
        tokio::spawn(async move {
            gbroadcast(&content, nodes).await;
        });
    }
    Ok(())
}

impl Drop for Broadcaster {
    fn drop(&mut self) {
        // close all listeners
        let ports: Vec<u16> = self.listeners.keys().cloned().collect();
        for port in ports {
            let _ = self.close(port);
        }
    }
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
        assert!(b2.close(8082).is_ok());
        assert!(b2.broadcast("test 2 message 2").await.is_ok());
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
    }
}
