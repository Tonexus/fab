// sender that deduplicates messages based on a cache

use std::{collections::HashMap, time::{Duration, Instant}};
use tokio::sync::mpsc;

use crate::error::{Result, FabError};

#[derive(Debug)]
pub struct DedupSender {
    cache:  HashMap<String, Instant>,
    expire: Duration,
    sender: mpsc::UnboundedSender<String>,
}

impl DedupSender {
    pub fn new(sender: mpsc::UnboundedSender<String>) -> DedupSender {
        DedupSender {
            cache:  HashMap::new(),
            expire: Duration::new(10, 0), // TODO param for num of seconds
            sender: sender,
        }
    }

    pub fn register(&mut self, msg: String) {
        self.cache.insert(msg, Instant::now());
    }

    // sends a message if the message is new. returns whether the message was sent
    pub fn send(&mut self, msg: String) -> Result<bool> {
        let now = Instant::now();
        if let Some(time) = self.cache.insert(msg.clone(), now) {
            if now.duration_since(time) < self.expire {
                return Ok(false);
            }
        }
        self.sender.send(msg)?;
        return Ok(true);
    }
}
