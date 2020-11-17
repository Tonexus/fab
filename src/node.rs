// basic representation of system nodes

use std::net::SocketAddrV4;
use tokio::{prelude::*, net::TcpStream};

use crate::message::Message;
use crate::error::Result;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Node {
    address: SocketAddrV4,
    // TODO add real key option here
    public_key: Option<()>,
}

impl Node {
    pub const fn new(address: SocketAddrV4, public_key: Option<()>) -> Node {
        Node {address, public_key}
    }

    // sends message. if no error, returns whether content was seen before by
    // other node
    #[tokio::main]
    pub async fn send(&self, content: &str) -> Result<bool> {
        //let message = Message::new(content);
        let mut sock = TcpStream::connect(self.address).await?;
        Message::new(content).into_socket(&mut sock).await?;
        Ok(sock.read_u8().await.map(|u| if u == 0 { false } else { true })?)
    }
}
