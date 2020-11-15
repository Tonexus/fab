// basic representation of a message

use serde::{Serialize, Deserialize};

const VERSION: u8 = 1;

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    version: u8,
    content: String,
    history: Vec<String>,
    history_signature: Vec<String>,
}

impl Message {
    pub fn new(content: &str) -> Message {
        Message {
            version: VERSION,
            content: content.to_string(),
            history: Vec::<String>::new(),
            history_signature: Vec::<String>::new(),
        }
    }

    pub fn get_content(&self) -> String {
        return self.content.clone();
    }
}
