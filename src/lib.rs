// representation of broadcaster service

mod node;
mod message;

use std::{io::{Result, Error, ErrorKind}, mem, thread, net::SocketAddrV4};
use tokio::{prelude::*, sync::oneshot, net::{TcpListener, TcpStream}};
use crossbeam::channel;
// tokio channels for data into/within tokio
// crossbeam channels for data out of tokio
use rand::Rng;

use node::Node;
use message::Message;

async fn handle_socket(sock: &mut TcpStream) -> Result<()> {
    let mut buf = Vec::<u8>::new();
    sock.read_to_end(&mut buf).await?;
    let message: Message = match bincode::deserialize(&buf) {
        Ok(b)  => Ok(b),
        Err(_) => Err(Error::new(ErrorKind::Other, "deserialization error")), 
    }?; // TODO real error conversion
    println!("{}", &message.get_content());
    Ok(())
}

#[tokio::main]
async fn alisten(
    port: u16,
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
                        let _ = handle_socket(&mut sock).await; // TODO spawn thread to do
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

#[derive(Debug)]
struct Listener {
    handle: thread::JoinHandle<()>,
    shutdown: oneshot::Sender<()>,
}

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Debug)]
pub struct Broadcaster {
    nodes: Vec<Node>,
    listener: Option<Listener>,
}

impl Broadcaster {
    pub const fn new() -> Broadcaster {
        Broadcaster {
            nodes: Vec::<Node>::new(),
            listener: None,
        }
    }

    // opens the listener (not required to send)
    pub fn listen(&mut self, port: u16) -> Result<()> {
        match self.listener {
            Some(_) => Err(Error::new(ErrorKind::Other, "already initted")),
            None    => Ok(()),
        }?;

        let (send_shutdown, recv_shutdown) = oneshot::channel();
        let (send_ready, recv_ready) = channel::bounded(1);

        let handle = thread::spawn(move || {
            alisten(port, send_ready, recv_shutdown);
        });

        // ignore receiver error, as should not happen
        match recv_ready.recv() {
            Ok(_)  => Ok(()),
            Err(_) => Err(Error::new(ErrorKind::Other, "failed to receive ready")),
        }?; // TODO real error conversion

        self.listener = Some(Listener {
            handle: handle,
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
            None => ()
        }
    }

    pub fn add_node(&mut self, address: SocketAddrV4, public_key: Option<()>) {
        self.nodes.push(Node::new(address, public_key));
    }

    pub fn broadcast(&self, content: &str) -> Result<()> {
        if self.nodes.len() == 0 {
            return Ok(());
        }
        let mut rng = rand::thread_rng();
        loop {
            // pick random node
            let i = rng.gen_range(0, self.nodes.len());
            if self.nodes[i].send(content)? {
                println!("seen");
                return Ok(());
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
    use std::{thread, time, net::{Ipv4Addr, SocketAddrV4}};
    use super::Broadcaster;
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn broadcast_init() {
        let mut b: Broadcaster = Broadcaster::new();
        assert!(b.listen(8080).is_ok());
        //thread::sleep(time::Duration::from_millis(1000));
    }

    #[test]
    fn broadcast_send() {
        let b: Broadcaster = Broadcaster::new();
        assert!(b.broadcast("Hello").is_ok());
    }

    #[test]
    fn broadcast_send2() {
        let mut b: Broadcaster = Broadcaster::new();
        b.add_node(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080), None);
        assert!(b.listen(8080).is_ok());
        assert!(b.broadcast("Hello").is_ok());
    }
}
