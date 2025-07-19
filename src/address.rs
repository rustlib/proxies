use std::fmt;
use std::io::Error;
use std::net::SocketAddr;
use std::str::FromStr;

use serde::{Deserialize, Serialize, de::Visitor};
use tokio::net::TcpStream;

/// proxy address
#[derive(Debug, Clone)]
pub enum Address {
    /// host, port
    Domain(String, u16),
    /// `SocketAddr`
    Sock(SocketAddr),
}

impl Address {
    pub async fn connect_tcp(&self) -> Result<TcpStream, Error> {
        match self {
            Self::Domain(host, port) => TcpStream::connect((host.as_str(), *port)).await,
            Self::Sock(addr) => TcpStream::connect(addr).await,
        }
    }

    pub fn port(&self) -> u16 {
        match self {
            Self::Domain(_, port) => *port,
            Self::Sock(addr) => addr.port(),
        }
    }
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
            Self::Domain(host, port) => write!(f, "{host}:{port}"),
            Self::Sock(addr) => write!(f, "{addr}"),
        }
    }
}

#[derive(Debug)]
pub struct InvalidAddress;

impl std::fmt::Display for InvalidAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid address")
    }
}

impl std::error::Error for InvalidAddress {}

impl FromStr for Address {
    type Err = InvalidAddress;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(addr) = s.parse() {
            Ok(Address::Sock(addr))
        } else {
            let mut parts = s.rsplitn(2, ':');
            let port = parts.next().unwrap();
            if let Some(host) = parts.next() {
                let port = port.parse().map_err(|_| InvalidAddress)?;
                Ok(Address::Domain(host.to_string(), port))
            } else {
                Err(InvalidAddress)
            }
        }
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(AddressVisitor)
    }
}

struct AddressVisitor;

impl<'de> Visitor<'de> for AddressVisitor {
    type Value = Address;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "<host/ip>:<port>")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse()
            .map_err(|_| serde::de::Error::custom(format!("invalid address: {v}")))
    }
}

impl Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Domain(host, port) => serializer.serialize_str(&format!("{host}:{port}")),
            Self::Sock(addr) => serializer.serialize_str(&format!("{addr}")),
        }
    }
}
