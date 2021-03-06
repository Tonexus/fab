// basic representation of system nodes

use std::net::SocketAddr;
use tokio::{prelude::*, net::TcpStream};

use crate::message::Message;
use crate::error::Result;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Node {
    address: SocketAddr,
    // TODO add real key option here
    public_key: Option<()>,
}

impl Node {
    pub const fn new(address: SocketAddr, public_key: Option<()>) -> Node {
        Node {address, public_key}
    }

    // sends message. if no error, returns whether content was seen before by
    // other node
    pub async fn send(&self, content: &str) -> Result<bool> {
        //let message = Message::new(content);
        let mut sock = TcpStream::connect(self.address).await?;
        Message::new(content).into_socket(&mut sock).await?;
        Ok(sock.read_u8().await.map(|u| if u == 0 { false } else { true })?)
    }
}
