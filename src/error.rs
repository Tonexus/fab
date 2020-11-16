// error conversion

use std::{io, error, fmt};
use crossbeam::channel::{SendError, RecvError};

pub type Result<T> = std::result::Result<T, FabError>;

#[derive(Debug)]
pub enum FabError {
    AlreadyInit,
    IoError(io::Error),
}

impl fmt::Display for FabError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FabError::AlreadyInit => write!(f, "Already initialized"),
            FabError::IoError(e)  => write!(f, "IO error: {:?}", e),
        }
    }
}

impl error::Error for FabError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            FabError::IoError(ref e) => Some(e),
            _                        => None,
        }
    }
}

impl From<io::Error> for FabError {
    fn from(error: io::Error) -> Self {
        FabError::IoError(error)
    }
}

