// basic representation of system nodes

use std::net::Ipv4Addr;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Node {
    address: Ipv4Addr,
    port: u16
}

