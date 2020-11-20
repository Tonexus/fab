// error conversion

use std::{io, sync, error, fmt};
use tokio::sync::mpsc;

pub type Result<T> = std::result::Result<T, FabError>;

#[derive(Debug)]
pub enum FabError {
    AlreadyListeningError,
    NotListeningError,
    IoError(io::Error),
    MutexPoisonedError,
    ChannelSendError,
    ChannelRecvError,
    BincodeError(bincode::Error),
}

use FabError::*;

impl fmt::Display for FabError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AlreadyListeningError => write!(f, "Already listening on that port"),
            NotListeningError     => write!(f, "Not listening on that port"),
            IoError(e)            => write!(f, "IO error: {:?}", e),
            MutexPoisonedError    => write!(f, "Mutex poisoned"),
            ChannelSendError      => write!(f, "Channel send error"),
            ChannelRecvError      => write!(f, "Channel receive error"),
            BincodeError(e)       => write!(f, "Error with bincode: {:?}", e),
        }
    }
}

impl error::Error for FabError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            IoError(ref e)          => Some(e),
            BincodeError(ref e)     => Some(e),
            _                       => None,
        }
    }
}

impl From<io::Error> for FabError {
    fn from(e: io::Error) -> Self {
        IoError(e)
    }
}

impl<T> From<sync::PoisonError<T>> for FabError {
    fn from(_: sync::PoisonError<T>) -> Self {
        MutexPoisonedError
    }
}

impl<T> From<mpsc::error::SendError<T>> for FabError {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        ChannelSendError
    }
}

impl From<bincode::Error> for FabError {
    fn from(e: bincode::Error) -> Self {
        BincodeError(e)
    }
}

