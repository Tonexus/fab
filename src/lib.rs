// representation of broadcaster service

mod node;
mod message;
mod dedup;
mod error;

use std::{sync::Arc, collections::HashMap, net::SocketAddr};
use rand::Rng;
use futures::future::{RemoteHandle, FutureExt};
use tokio::{prelude::*, sync::{mpsc, Mutex, RwLock}, net::{TcpListener, TcpStream}};
use indexmap::IndexMap;

use node::Node;
use message::Message;
use dedup::DedupSender;
use error::{Result, FabError};

type SafeNodes  = Arc<RwLock<IndexMap<SocketAddr, Node>>>;
type SafeSender = Arc<Mutex<DedupSender>>;

#[derive(Debug)]
struct Listener {
    // deduplicating sender, sending to broadcaster
    send:   SafeSender,
    // when dropped, listener thread dies
    handle: RemoteHandle<()>,
}

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Debug)]
pub struct Broadcaster {
    // vector of nodes to send/listen to
    safe_nodes: SafeNodes,
    // deduplicating sender for outbound messages
    out_send:   SafeSender,
    // receiver for outbound messages
    out_recv:   mpsc::UnboundedReceiver<String>,
    // sender for messages from listeners
    lst_send:   mpsc::UnboundedSender<String>,
    // maps port number to a listener
    listeners:  HashMap<u16, Listener>,
    // when dropped, thread that deduplicates messages from different listeners dies
    handle:     RemoteHandle<()>,
}

impl Broadcaster {
    pub fn new() -> Broadcaster {
        let (out_send, out_recv) = mpsc::unbounded_channel();
        let (lst_send, mut lst_recv) = mpsc::unbounded_channel();
        let dedup_out_send = Arc::new(Mutex::new(DedupSender::new(out_send)));

        // copy for other thread
        let dedup_out_send_2 = dedup_out_send.clone();

        // loop propagating messages from different listeners up if not duplicates
        let (task, handle) = async move { loop {
            if let Some(msg) = lst_recv.recv().await {
                if dedup_out_send_2.lock().await.send(msg).is_err() {
                    return;
                }
            } else {
                return;
            }
        }}.remote_handle();

        tokio::spawn(task);

        Broadcaster {
            safe_nodes: Arc::new(RwLock::new(IndexMap::new())),
            out_send:   dedup_out_send,
            out_recv:   out_recv,
            lst_send:   lst_send,
            listeners:  HashMap::new(),
            handle:     handle,
        }
    }

    // opens a listener (not required to send)
    pub async fn listen(&mut self, port: u16) -> Result<()> {
        // make sure not already have a listener
        self.listeners.get(&port).map_or(Ok(()), |_| Err(FabError::AlreadyListeningError))?;

        // bind to listening port
        // TODO change to 0.0.0.0 when want to expose publicly
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;

        // create deduplicating sender for new listener
        let send = Arc::new(Mutex::new(DedupSender::new(self.lst_send.clone())));

        // create copies for other thread
        let nodes_2 = self.safe_nodes.clone();
        let send_2 = send.clone();

        // new thread to listen for connections
        let (task, handle) = async move { loop {
            // accept new connections and handle them
            if let Ok((mut sock, _)) = listener.accept().await {
                // create copies for new thread
                let nodes_3 = nodes_2.clone();
                let send_3 = send_2.clone();
                // new thread to handle socket
                tokio::spawn(async move {
                    match handle_socket(&mut sock, nodes_3, send_3).await {
                        Ok(_)  => (),
                        // TODO better process/log errors
                        Err(e) => println!("Got error: {:?}", e),
                    }
                });
            }
        }}.remote_handle();

        tokio::spawn(task);

        self.listeners.insert(port, Listener{send: send, handle: handle});
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

    // removes a node to broadcast to
    pub async fn remove_node(
        &mut self,
        address: SocketAddr,
    ) {
        self.safe_nodes.write().await.remove(&address);
    }

    pub async fn broadcast(&mut self, content: &str) -> Result<()> {
        // indicate that message to send has been received just in case it gets echoed back
        self.out_send.lock().await.register(content.to_string());
        // TODO actually not work once history implemented?
        for (_, l) in self.listeners.iter() {
            l.send.lock().await.register(content.to_string());
        }

        gbroadcast(content, self.safe_nodes.read().await.clone()).await;
        Ok(())
    }

    pub async fn receive(&mut self) -> Result<String> {
        if self.listeners.is_empty() {
            Err(FabError::NotListeningError)
        } else {
            // technically error should never happen, as broadcaster has a sender
            self.out_recv.recv().await.ok_or(FabError::ChannelRecvError)
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

async fn handle_socket(sock: &mut TcpStream, nodes: SafeNodes, send: SafeSender) -> Result<()> {
    let message = Message::from_socket(sock).await?; // TODO eventually do history signature processing
    let content = message.get_content();
    // try to send to broadcaster and get whether message is new for listener
    let new = send.lock().await.send(content.to_string())?;
    // reply back whether content has been seen (not new) or not
    sock.write_u8(if new { 0 } else { 1 }).await?;
    // if new, start broadcast
    if new {
        // start own broadcast
        // TODO figure out source node from history and remove
        // TODO don't spawn thread, but don't hold lock open
        let nodes_2 = nodes.read().await.clone();
        tokio::spawn(async move {
            gbroadcast(&content, nodes_2).await;
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

        // test send 1 message to no one
        let tgt = "test 1 message 1";
        assert!(b.broadcast(tgt).await.is_ok());
        assert!(b.listen(8080).await.is_ok());

        b.add_node("127.0.0.1:8080".parse().unwrap(), None).await;

        // test send 1 message to self
        let tgt = "test 1 message 2";
        assert!(b.broadcast(tgt).await.is_ok());
    }

    #[tokio::test]
    async fn broadcast_send_two_party() {
        let mut b1: Broadcaster = Broadcaster::new();
        let mut b2: Broadcaster = Broadcaster::new();
        assert!(b1.listen(8081).await.is_ok());
        assert!(b2.listen(8082).await.is_ok());
        b1.add_node("127.0.0.1:8082".parse().unwrap(), None).await;
        b2.add_node("127.0.0.1:8081".parse().unwrap(), None).await;

        // test send 1 message between two broadcasters
        let tgt = "test 2 message 1";
        assert!(b1.broadcast(tgt).await.is_ok());
        let recv = b2.receive().await;
        assert!(recv.is_ok());
        let msg = recv.unwrap();
        println!("Got message \"{}\", want message \"{}\"", msg.clone(), tgt);
        assert_eq!(msg, tgt);

        // test send 1 message between two broadcasters, even if one is closed
        assert!(b2.close(8082).is_ok());
        let tgt = "test 2 message 2";
        assert!(b2.broadcast(tgt).await.is_ok());
        let recv = b1.receive().await;
        assert!(recv.is_ok());
        let msg = recv.unwrap();
        println!("Got message \"{}\", want message \"{}\"", msg.clone(), tgt);
        assert_eq!(msg, tgt);
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

        // test send 1 message between 3 broadcasters
        let tgt = "test 3 message 1";
        assert!(b1.broadcast(tgt).await.is_ok());
        let recv = b2.receive().await;
        assert!(recv.is_ok());
        let msg = recv.unwrap();
        println!("Got message \"{}\", want message \"{}\"", msg.clone(), tgt);
        assert_eq!(msg, tgt);
        let recv = b3.receive().await;
        assert!(recv.is_ok());
        let msg = recv.unwrap();
        println!("Got message \"{}\", want message \"{}\"", msg.clone(), tgt);
        assert_eq!(msg, tgt);
    }
}
