// basic representation of system nodes

use std::{io::{Result, Error, ErrorKind}, net::SocketAddrV4};
use tokio::{prelude::*, net::TcpStream};

use crate::message::Message;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Node {
    address: SocketAddrV4,
    // TODO add real key option here
    public_key: Option<()>,
}

#[tokio::main]
async fn asend(address: &SocketAddrV4, message: &Message) -> Result<bool> {
    let mut sock = TcpStream::connect(address).await?;
    let buf = match bincode::serialize(&message) {
        Ok(b)  => Ok(b),
        Err(_) => Err(Error::new(ErrorKind::Other, "serialization error")), 
    }?; // TODO real error conversion
    sock.write_all(&buf).await?;
    Ok(false)
}

impl Node {
    pub const fn new(address: SocketAddrV4, public_key: Option<()>) -> Node {
        Node {address, public_key}
    }

    // sends message. if no error, returns whether content was seen before by
    // other node
    pub fn send(&self, content: &str) -> Result<bool> {
        let message = Message::new(content);
        let _ret = asend(&self.address, &message)?; // TODO actually return
        Ok(true)
    }
}
