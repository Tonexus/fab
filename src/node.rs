// basic representation of system nodes

use std::{io::Result, net::SocketAddrV4};
use tokio::net::TcpStream;

const VERSION: u8 = 1;

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
    pub fn send(&self, content: &str) -> Result<bool> {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
            let sock = TcpStream::connect(self.address).await;
        });
        Ok(true)
    }
}
