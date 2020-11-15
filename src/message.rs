// basic representation of a message
extern crate serde;

use std::net::Ipv4Addr;
use serde::{Serialize, Deserialize};

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    version: u8,
    content: String,
    history: Vec<String>,
    history_signature: Vec<String>,
}
