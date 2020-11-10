// representation of broadcaster service

use crate::node::Node;
use crate::message::Message;
use std::io::Result;

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Broadcaster {
    nodes: Vec<Node>,
}

impl Broadcaster {
    pub const fn new() -> Broadcaster {
        Broadcaster {nodes: Vec::<Node>::new()}
    }

    pub fn broadcast(&self, content: &str) -> Result<()> {
        Ok(())
    }
}
