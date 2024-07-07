use std::io;

use thiserror::Error;

use crate::address::Address;

#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("{0}")]
    Io(io::Error),
    #[error("{0}")]
    InvalidData(String),
    #[error("protocol parse fail: {0}")]
    ProtocolFail(String),
    #[error("conntect remote({0}) fail: {1}")]
    ConnectRemoteFail(Address, String),
    #[error("{0}")]
    Other(String),
}

impl From<io::Error> for ProxyError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

macro_rules! invalid_data {
    ($fmt:expr $(, $arg:expr)*) => {
        $crate::error::ProxyError::InvalidData(format!($fmt $(, $arg)*))
    };
}

macro_rules! protocol_fail {
    ($fmt:expr $(, $arg:expr)*) => {
        $crate::error::ProxyError::ProtocolFail(format!($fmt $(, $arg)*))
    };
}

macro_rules! io_fail {
    ($e:expr, $fmt:expr $(, $arg:expr)*) => {
        $crate::error::ProxyError::Io(::std::io::Error::new($e.kind(),
            format!(concat!($fmt, " fail: {}") $(, $arg)*, $e)))
    };
}

macro_rules! connect_remote_fail {
    ($addr:expr, $fmt:expr $(, $arg:expr)*) => {
        $crate::error::ProxyError::ConnectRemoteFail($addr, format!($fmt $(, $arg)*))
    };
}

macro_rules! format_err {
    ($fmt:expr $(, $arg:expr)*) => {
        $crate::error::ProxyError::Other(format!($fmt $(, $arg)*))
    };
}

macro_rules! bail {
    ($fmt:expr $(, $arg:expr)*) => {
        return Err(format_err!($fmt $(, $arg)*))
    };
}
