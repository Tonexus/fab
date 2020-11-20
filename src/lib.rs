// representation of broadcaster service

mod node;
mod message;
mod error;

use std::{
    sync::{Arc, RwLock},
    collections::HashMap,
    net::SocketAddrV4,
};
use futures::future::{RemoteHandle, Remote, FutureExt};
use tokio::{prelude::*, sync::oneshot, net::{TcpListener, TcpStream}};
// tokio channels for data into/within tokio
// crossbeam channels for data out of tokio
use rand::Rng;

use node::Node;
use message::Message;
use error::{Result, FabError};

type SafeNodes    = Arc<RwLock<Vec<Node>>>;
type SafeReceived = Arc<RwLock<HashMap<String, ()>>>;

#[derive(Debug)]
struct Listener {
    safe_received: SafeReceived,
    handle:        RemoteHandle<()>,
}

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Debug)]
pub struct Broadcaster {
    safe_nodes: SafeNodes,
    // maps port number to a listener
    listeners:  HashMap<u16, Listener>,
}

impl Broadcaster {
    pub fn new() -> Broadcaster {
        Broadcaster {
            safe_nodes: Arc::new(RwLock::new(Vec::<Node>::new())),
            listeners:  HashMap::new(),
        }
    }

    // opens the listener (not required to send)
    pub async fn listen(&mut self, port: u16) -> Result<()> {
        // make sure not already have a listener
        self.listeners.get(&port).map_or(Ok(()), |_| Err(FabError::AlreadyListeningError))?;

        let safe_nodes = self.safe_nodes.clone();
        let safe_received = Arc::new(RwLock::new(HashMap::new()));
        let safe_received2 = safe_received.clone();
        // TODO change to 0.0.0.0 when want to expose publically
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;

        let (task, handle) = async move { loop {
            // accept new connections and handle them
            if let Ok((mut sock, _)) = listener.accept().await {
                let safe_nodes2 = safe_nodes.clone();
                let safe_received2 = safe_received.clone();
                tokio::spawn(async move {
                    // TODO process/log errors
                    let _ = handle_socket(&mut sock, safe_nodes2, safe_received2).await;
                });
            }
        }}.remote_handle();

        tokio::spawn(task);

        self.listeners.insert(port, Listener {
            safe_received: safe_received2,
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
    pub fn add_node(
        &mut self,
        address: SocketAddrV4,
        public_key: Option<()>
    ) -> Result<()> {
        self.safe_nodes.write()?.push(Node::new(address, public_key));
        Ok(())
    }

    pub async fn broadcast(&self, content: &str) -> Result<()> {
        // if has a listener, indicate that message to send has been received,
        // just in case it gets echoed back
        /*if let Some(ref mut l) = self.listener {
            l.safe_received.write()?.insert(content.to_string(), ());
        }*/

        gbroadcast(content, self.safe_nodes.read()?.clone()).await;
        Ok(())
    }
}

// generic broadcast of string to nodes
async fn gbroadcast(content: &str, mut nodes: Vec<Node>) {
    loop {
        if nodes.len() == 0 {
            return;
        }
        // pick random node
        let i = rand::thread_rng().gen_range(0, nodes.len());
        // if no error and other end has seen message, exit
        if let Ok(seen) = nodes[i].send(content).await {
            if seen {
                return;
            }
        }
        // TODO retry if connection is just bad?
        // otherwise, remove from list of nodes to try and continue
        nodes.swap_remove(i);
    }
}

async fn handle_socket(
    sock: &mut TcpStream,
    safe_nodes: SafeNodes,
    safe_received: SafeReceived,
) -> Result<()> {
    let message = Message::from_socket(sock).await?; // TODO eventually do history signature processing
    let content = message.get_content();
    println!("Received \"{}\" as message", &content);
    let seen = safe_received.read()?.get(&content).is_some();
    // send whether message already seen
    sock.write_u8(if seen { 1 } else { 0 }).await?;
    if !seen {
        // insert into received
        safe_received.write()?.insert(content.to_string(), ());

        // start own broadcast
        let nodes = safe_nodes.read()?.clone();
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
    use std::{net::{Ipv4Addr, SocketAddrV4}};
    use super::Broadcaster;

    const LOCALHOST: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);

    #[tokio::test]
    async fn broadcast_send_one_party() {
        let mut b: Broadcaster = Broadcaster::new();
        assert!(b.broadcast("test 1 message 1").await.is_ok());
        assert!(b.listen(8080).await.is_ok());
        assert!(b.add_node(SocketAddrV4::new(LOCALHOST, 8080), None).is_ok());
        assert!(b.broadcast("test 1 message 2").await.is_ok());
    }

    #[tokio::test]
    async fn broadcast_send_two_party() {
        let mut b1: Broadcaster = Broadcaster::new();
        let mut b2: Broadcaster = Broadcaster::new();
        assert!(b1.listen(8080).await.is_ok());
        assert!(b2.listen(8081).await.is_ok());
        assert!(b1.add_node(SocketAddrV4::new(LOCALHOST, 8081), None).is_ok());
        assert!(b2.add_node(SocketAddrV4::new(LOCALHOST, 8080), None).is_ok());
        assert!(b1.broadcast("test 2 message 1").await.is_ok());
        assert!(b2.close(8081).is_ok());
        assert!(b2.broadcast("test 2 message 2").await.is_ok());
        assert!(b1.broadcast("test 2 message 3").await.is_ok());
    }

    #[tokio::test]
    async fn broadcast_send_three_party() {
        let mut b1: Broadcaster = Broadcaster::new();
        let mut b2: Broadcaster = Broadcaster::new();
        let mut b3: Broadcaster = Broadcaster::new();
        assert!(b1.listen(8080).await.is_ok());
        assert!(b2.listen(8081).await.is_ok());
        assert!(b3.listen(8082).await.is_ok());
        assert!(b1.add_node(SocketAddrV4::new(LOCALHOST, 8081), None).is_ok());
        assert!(b1.add_node(SocketAddrV4::new(LOCALHOST, 8082), None).is_ok());
        assert!(b2.add_node(SocketAddrV4::new(LOCALHOST, 8080), None).is_ok());
        assert!(b2.add_node(SocketAddrV4::new(LOCALHOST, 8082), None).is_ok());
        assert!(b3.add_node(SocketAddrV4::new(LOCALHOST, 8080), None).is_ok());
        assert!(b3.add_node(SocketAddrV4::new(LOCALHOST, 8081), None).is_ok());
        assert!(b1.broadcast("test 3 message 1").await.is_ok());
    }
}
