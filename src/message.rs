// basic representation of a message

use serde::{Serialize, Deserialize};
use tokio::{prelude::*, net::TcpStream};

use crate::error::Result;

const VERSION: u8    = 1;
const MAX_LEN: usize = 4096;

//#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    content: String,
    history: Vec<String>,
    history_signature: Vec<String>,
}

impl Message {
    pub fn new(content: &str) -> Message {
        Message {
            content: content.to_string(),
            history: Vec::<String>::new(),
            history_signature: Vec::<String>::new(),
        }
    }

    pub fn get_content(&self) -> String {
        self.content.clone()
    }

    pub async fn from_socket(sock: &mut TcpStream) -> Result<Message> {
        let mut buf: [u8; MAX_LEN] = [0; MAX_LEN];
        let mut buf_out = Vec::new();
        loop {
            // recv version
            let _v = sock.read_u8().await?;
            // recv length
            let l = sock.read_u64().await? as usize;
            // recv whether more are incoming (0 == false, 1 == true)
            let b = sock.read_u8().await?;
            // recv chunk
            sock.read_exact(&mut buf[0..l]).await?;
            buf_out.extend_from_slice(&buf[0..l]);
            if b == 0 {
                break;
            }
        }
        Ok(bincode::deserialize(&buf_out)?)
    }

    pub async fn into_socket(&self, sock: &mut TcpStream) -> Result<()> {
        let buf = bincode::serialize(&self)?;
        let l = buf.len();
        for i in (0..l).step_by(MAX_LEN) {
            // send version
            sock.write_u8(VERSION).await?;
            // split into chunks of size at most MAX_LEN
            if i + MAX_LEN >= l {
                // send length
                sock.write_u64((l - i) as u64).await?;
                // send if more bytes incoming
                sock.write_u8(0).await?;
                // send chunk
                sock.write_all(&buf[i..l]).await?;
            } else {
                // see above
                sock.write_u64(MAX_LEN as u64).await?;
                sock.write_u8(1).await?;
                sock.write_all(&buf[i..i+MAX_LEN]).await?;
            }
        }
        Ok(())
    }
}
