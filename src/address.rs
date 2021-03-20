use std::{fmt, net::SocketAddr};

/// proxy address
#[derive(Debug, Clone)]
pub enum Address {
    /// host, port
    Domain(String, u16),
    /// `SocketAddr`
    Sock(SocketAddr),
}

impl From<(String, u16)> for Address {
    fn from(addr: (String, u16)) -> Self {
        Self::Domain(addr.0, addr.1)
    }
}

impl<S: ToString> From<(&S, u16)> for Address {
    fn from(addr: (&S, u16)) -> Self {
        Self::Domain(addr.0.to_string(), addr.1)
    }
}

impl From<SocketAddr> for Address {
    fn from(addr: SocketAddr) -> Self {
        Self::Sock(addr)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Domain(host, port) => write!(f, "{}:{}", host, port),
            Self::Sock(addr) => write!(f, "{}", addr),
        }
    }
}
