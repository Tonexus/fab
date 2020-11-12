// representation of broadcaster service

extern crate futures;
extern crate tokio;
extern crate crossbeam;

use crate::node::Node;
use crate::message::Message;
use std::{io::{Result, Error, ErrorKind}, mem, thread};
use tokio::{sync::oneshot, net::{TcpListener, TcpStream}};
use crossbeam::channel;
// tokio channels for data into/within tokio
// crossbeam channels for data out of tokio

fn handle_socket(sock: TcpStream) {
    ()
}

#[tokio::main]
async fn listen(
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
                    Ok((sock, addr)) => {
                        println!("conn to {:?}", addr);
                        handle_socket(sock);
                    },
                    Err(e)           => println!("couldn't accept: {:?}", e),
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

    pub fn open(&mut self, port: u16) -> Result<()> {
        match self.listener {
            Some(_) => Err(Error::new(ErrorKind::Other, "already initted")),
            None    => Ok(()),
        }?;

        let (send_shutdown, recv_shutdown) = oneshot::channel();
        let (send_ready, recv_ready) = channel::bounded(1);

        let handle = thread::spawn(move || {
            listen(port, send_ready, recv_shutdown);
        });

        // ignore receiver error, as should not happen
        match recv_ready.recv() {
            Ok(_)  => Ok(()),
            Err(_) => Err(Error::new(ErrorKind::Other, "failed to receive channel")),
        }?;

        self.listener = Some(Listener {
            handle: handle,
            shutdown: send_shutdown,
        });

        Ok(())
    }

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

    pub fn broadcast(&self, msg: &str) -> Result<()> {
        Ok(())
    }
}

impl Drop for Broadcaster {
    fn drop(&mut self) {
        self.close();
    }
}

