// representation of broadcaster service

mod node;
mod message;
mod error;

use std::{
    sync::{Arc, RwLock},
    thread,
    collections::HashMap,
    net::SocketAddrV4,
};
use tokio::{prelude::*, sync::oneshot, net::{TcpListener, TcpStream}};
use crossbeam::channel;
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
    shutdown:      oneshot::Sender<()>,
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
    pub fn listen(&mut self, port: u16) -> Result<()> {
        // make sure not already have a listener
        self.listeners.get(&port).map_or(Ok(()), |_| Err(FabError::AlreadyListeningError))?;

        let nodes = self.safe_nodes.clone();
        let received = Arc::new(RwLock::new(HashMap::new()));
        let received2 = received.clone();
        let (send_shutdown, recv_shutdown) = oneshot::channel();
        let (send_ready, recv_ready) = channel::bounded(1);

        thread::spawn(move || {
            alisten(port, nodes, received, send_ready, recv_shutdown);
        });

        // wait for listener ready, checking for both channel error and returned error
        recv_ready.recv()??;

        self.listeners.insert(port, Listener {
            safe_received: received2,
            shutdown:      send_shutdown,
        });

        Ok(())
    }

    // close listener
    pub fn close(&mut self, port: u16) -> Result<()> {
        if let Some(l) = self.listeners.remove(&port) {
            // send shutdown, ok if error since means listener already dead
            let _ = l.shutdown.send(());
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

    // TODO immutable version w/o write to listener
    pub fn broadcast(&mut self, content: &str) -> Result<()> {
        // if has a listener, indicate that message to send has been received,
        // just in case it gets echoed back
        /*if let Some(ref mut l) = self.listener {
            l.safe_received.write()?.insert(content.to_string(), ());
        }*/

        gbroadcast(content, self.safe_nodes.read()?.clone());
        Ok(())
    }
}

// generic broadcast of string to nodes
fn gbroadcast(content: &str, mut nodes: Vec<Node>) {
    let mut rng = rand::thread_rng();
    loop {
        if nodes.len() == 0 {
            return;
        }
        // pick random node
        let i = rng.gen_range(0, nodes.len());
        // if no error and other end has seen message, exit
        if let Ok(seen) = nodes[i].send(content) {
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
    //println!("Received \"{}\" as message", &content);
    let seen = safe_received.read()?.get(&content).is_some();
    // send whether message already seen
    sock.write_u8(if seen { 1 } else { 0 }).await?;
    if !seen {
        // insert into received
        safe_received.write()?.insert(content.to_string(), ());

        // start own broadcast
        let nodes = safe_nodes.read()?.clone();
        thread::spawn(move || {
            gbroadcast(&content, nodes);
        });
    }
    Ok(())
}

#[tokio::main]
async fn alisten(
    port: u16,
    safe_nodes: SafeNodes,
    safe_received: SafeReceived,
    ready: channel::Sender<Result<()>>,
    mut shutdown: oneshot::Receiver<()>,
) {
    // TODO change to 0.0.0.0 when want to expose publically
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(stream) => stream,
        // if error, signal that error occurred, ignoring further errors
        Err(e)     => {
            let _ = ready.send(Err(e.into()));
            return;
        },
    };

    // signal that ready to accept
    if ready.send(Ok(())).is_err() {
        // if error on send ready, just exit
        return;
    }

    loop { tokio::select! {
        // accept new connection and handle it
        res = listener.accept() => {
            if let Ok((mut sock, _)) = res {
                let safe_nodes2 = safe_nodes.clone();
                let safe_received2 = safe_received.clone();
                tokio::spawn(async move {
                    let _ = handle_socket(&mut sock, safe_nodes2, safe_received2).await;
                });
            }
        },
        // shutdown if sender drops or sends shutdown msg
        _ = &mut shutdown => {
            println!("got shutdown signal, exiting");
            return;
        }
    }}
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

    #[test]
    fn broadcast_send_one_party() {
        let mut b: Broadcaster = Broadcaster::new();
        assert!(b.broadcast("Hello").is_ok());
        assert!(b.listen(8080).is_ok());
        assert!(b.add_node(SocketAddrV4::new(LOCALHOST, 8080), None).is_ok());
        assert!(b.broadcast("Hello2").is_ok());
    }

    #[test]
    fn broadcast_send_two_party() {
        let mut b1: Broadcaster = Broadcaster::new();
        let mut b2: Broadcaster = Broadcaster::new();
        assert!(b1.listen(8080).is_ok());
        assert!(b2.listen(8081).is_ok());
        assert!(b1.add_node(SocketAddrV4::new(LOCALHOST, 8081), None).is_ok());
        assert!(b2.add_node(SocketAddrV4::new(LOCALHOST, 8080), None).is_ok());
        assert!(b1.broadcast("Hello").is_ok());
        assert!(b2.close(8081).is_ok());
        assert!(b2.broadcast("Hello2").is_ok());
        assert!(b1.broadcast("Hello3").is_ok());
    }

    #[test]
    fn broadcast_send_three_party() {
        let mut b1: Broadcaster = Broadcaster::new();
        let mut b2: Broadcaster = Broadcaster::new();
        let mut b3: Broadcaster = Broadcaster::new();
        assert!(b1.listen(8080).is_ok());
        assert!(b2.listen(8081).is_ok());
        assert!(b3.listen(8082).is_ok());
        assert!(b1.add_node(SocketAddrV4::new(LOCALHOST, 8081), None).is_ok());
        assert!(b1.add_node(SocketAddrV4::new(LOCALHOST, 8082), None).is_ok());
        assert!(b2.add_node(SocketAddrV4::new(LOCALHOST, 8080), None).is_ok());
        assert!(b2.add_node(SocketAddrV4::new(LOCALHOST, 8082), None).is_ok());
        assert!(b3.add_node(SocketAddrV4::new(LOCALHOST, 8080), None).is_ok());
        assert!(b3.add_node(SocketAddrV4::new(LOCALHOST, 8081), None).is_ok());
        assert!(b1.broadcast("Hello").is_ok());
    }
}
