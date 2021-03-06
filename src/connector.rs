use async_trait::async_trait;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};

use crate::{address::Address, error::ProxyError};

/// transport connector
#[async_trait]
pub trait Connector {
    type Transport: AsyncRead + AsyncWrite;

    async fn connect_tcp(&self, addr: &Address) -> Result<Self::Transport, ProxyError>;
}

/// direct connector
pub struct DirectConnector;

#[async_trait]
impl Connector for DirectConnector {
    type Transport = TcpStream;

    async fn connect_tcp(&self, addr: &Address) -> Result<Self::Transport, ProxyError> {
        Ok(addr.connect_tcp().await?)
    }
}
