// basic representation of a message

use std::net::Ipv4Addr;

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    content: String,
    history: Vec<String>,
    history_signature: Vec<String>,
}
