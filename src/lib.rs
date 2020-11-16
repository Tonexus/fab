// representation of broadcaster service

mod node;
mod message;

use std::{
    io::{Result, Error, ErrorKind},
    mem,
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

type SafeNodes    = Arc<RwLock<Vec<Node>>>;
type SafeReceived = Arc<RwLock<HashMap<String, ()>>>;

#[derive(Debug)]
struct Listener {
    received: SafeReceived,
    handle:   thread::JoinHandle<()>,
    shutdown: oneshot::Sender<()>,
}

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Debug)]
pub struct Broadcaster {
    nodes:    SafeNodes,
    listener: Option<Listener>,
}

impl Broadcaster {
    pub fn new() -> Broadcaster {
        Broadcaster {
            nodes:    Arc::new(RwLock::new(Vec::<Node>::new())),
            listener: None,
        }
    }

    // opens the listener (not required to send)
    pub fn listen(&mut self, port: u16) -> Result<()> {
        match self.listener {
            Some(_) => Err(Error::new(ErrorKind::Other, "already initted")),
            None    => Ok(()),
        }?;

        let nodes = self.nodes.clone();
        let received = Arc::new(RwLock::new(HashMap::new()));
        let received2 = received.clone();
        let (send_shutdown, recv_shutdown) = oneshot::channel();
        let (send_ready, recv_ready) = channel::bounded(1);

        let handle = thread::spawn(move || {
            alisten(port, nodes, received, send_ready, recv_shutdown);
        });

        // ignore receiver error, as should not happen
        match recv_ready.recv() {
            Ok(_)  => Ok(()),
            Err(_) => Err(Error::new(ErrorKind::Other, "failed to receive ready")),
        }?; // TODO real error conversion

        self.listener = Some(Listener {
            received: received2,
            handle:   handle,
            shutdown: send_shutdown,
        });

        Ok(())
    }

    // close listener
    pub fn close(&mut self) {
        let listener = mem::replace(&mut self.listener, None);
        match listener {
            Some(l) => {
                // send shutdown, ok if error since means listener already dead
                let _ = l.shutdown.send(());
                // join child, ok if error since means listener already dead
                let _ = l.handle.join();
            },
            None    => ()
        }
    }

    // registers another node to broadcast to
    pub fn add_node(
        &mut self,
        address: SocketAddrV4,
        public_key: Option<()>
    ) -> Result<()> {
        match self.nodes.write() {
            Ok(n)  => Ok(n),
            Err(_) => Err(Error::new(ErrorKind::Other, "failed to unlock write")),
        }?.push(Node::new(address, public_key)); // TODO real err conversion
        Ok(())
    }

    // TODO immutable version w/o write to listener
    pub fn broadcast(&mut self, content: &str) -> Result<()> {
        let nodes = match self.nodes.read() {
            Ok(n)  => Ok(n),
            Err(_) => Err(Error::new(ErrorKind::Other, "failed to read lock nodes")),
        }?; // TODO real err conversion

        // if has a listener, indicate that message to send has been received,
        // just in case it gets echoed back
        match &mut self.listener {
            Some(l) => {
                let mut received = match l.received.write() {
                    Ok(r)  => Ok(r),
                    Err(_) => Err(Error::new(ErrorKind::Other, "failed to write lock received")),
                }?; // TODO real err conversion
                received.insert(content.to_string(), ());
            },
            None    => ()
        }
        gbroadcast(content, &nodes)
    }
}

// generic broadcast of string to nodes
fn gbroadcast(content: &str, nodes: &Vec<Node>) -> Result<()> {
    if nodes.len() == 0 {
        return Ok(());
    }
    let mut rng = rand::thread_rng();
    loop {
        // pick random node
        let i = rng.gen_range(0, nodes.len());
        println!("sending");
        if nodes[i].send(content)? {
            println!("sent");
            return Ok(());
        }
    }
}

async fn handle_socket(
    sock: &mut TcpStream,
    nodes: SafeNodes,
    received: SafeReceived,
) -> Result<()> {
    let received2 = match received.read() {
        Ok(r)  => Ok(r),
        Err(_) => Err(Error::new(ErrorKind::Other, "failed to read lock received")),
    }?;
    let message = Message::from_socket(sock).await?;
    let content = message.get_content();
    println!("Received \"{}\" as message", &content);
    // send whether message already seen
    sock.write_u8(match received2.get(&content) {
        Some(_) => 1,
        None    => 0,
    }).await
}

#[tokio::main]
async fn alisten(
    port: u16,
    nodes: SafeNodes,
    received: SafeReceived,
    ready: channel::Sender<Result<()>>,
    mut shutdown: oneshot::Receiver<()>,
) {
    // TODO change to 0.0.0.0 when want to expose publically
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(stream) => stream,
        // if error, signal that error occurred, ignoring further errors
        Err(e)     => {
            let _ = ready.send(Err(e));
            return;
        },
    };

    // signal that ready to accept
    match ready.send(Ok(())) {
        Ok(_)  => (),
        // if error on send ready, just exit
        Err(_) => return,
    }

    loop {
        tokio::select! {
            res = listener.accept() => {
                match res {
                    Ok((mut sock, addr)) => {
                        println!("conn to {:?}", addr);
                        match handle_socket(&mut sock, nodes.clone(), received.clone()).await {
                            Ok(_)  => (),
                            Err(e) => println!("coudn't handle socket: {:?}", e),
                        }; // TODO spawn thread to do
                    },
                    Err(e) => println!("couldn't accept: {:?}", e),
                }
            }
            // shutdown if sender drops or sends shutdown msg
            _ = &mut shutdown => {
                println!("got shutdown signal, exiting");
                return;
            }
        }
    }
}

impl Drop for Broadcaster {
    fn drop(&mut self) {
        self.close();
    }
}


#[cfg(test)]
mod tests {
    use std::{net::{Ipv4Addr, SocketAddrV4}};
    use super::Broadcaster;

    #[test]
    fn broadcast_send2() {
        let mut b: Broadcaster = Broadcaster::new();
        println!("1");
        assert!(b.broadcast("Hello").is_ok());
        println!("2");
        assert!(b.add_node(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080), None).is_ok());
        println!("3");
        assert!(b.listen(8080).is_ok());
        println!("4");
        assert!(b.broadcast("Hello2").is_ok());
        println!("5");
    }
}
