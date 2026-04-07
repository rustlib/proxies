use std::io::Error;
use std::ops::Deref;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::net::TcpStream;

use crate::address::Address;
use crate::transport::{AsyncTransport, BoxedTransport};

/// transport connector
#[async_trait]
pub trait Connector {
    type Transport: AsyncTransport;

    async fn connect_tcp(&self, addr: &Address) -> Result<Self::Transport, Error>;

    fn map_transport<M>(self, m: M) -> MapConnector<Self, M>
    where
        Self: Sized,
    {
        MapConnector::new(self, m)
    }

    fn make_arc(self) -> ArcConnector
    where
        Self: Sized + Send + Sync + 'static,
        Self::Transport: Send + Unpin + Sized + 'static,
    {
        Arc::new(self.map_transport(|t: Self::Transport| t.boxed()))
    }
}

pub type ArcConnector = Arc<dyn Connector<Transport = BoxedTransport> + Send + Sync>;

/// direct connector
pub struct DirectConnector;

#[async_trait]
impl Connector for DirectConnector {
    type Transport = TcpStream;

    async fn connect_tcp(&self, addr: &Address) -> Result<Self::Transport, Error> {
        addr.connect_tcp().await
    }
}

#[async_trait]
impl<T> Connector for T
where
    T: Deref + Sync,
    <T as Deref>::Target: Connector + Sync,
    <T::Target as Connector>::Transport: Send,
{
    type Transport = <T::Target as Connector>::Transport;

    async fn connect_tcp(&self, addr: &Address) -> Result<Self::Transport, Error> {
        self.deref().connect_tcp(addr).await
    }
}

pub struct MapConnector<C, M> {
    connector: C,
    map: M,
}

impl<C, M> MapConnector<C, M> {
    fn new(connector: C, map: M) -> Self {
        Self { connector, map }
    }
}

#[async_trait]
impl<C, M, T> Connector for MapConnector<C, M>
where
    C: Connector + Sync,
    M: Sync + Fn(C::Transport) -> T,
    T: AsyncTransport,
{
    type Transport = T;

    async fn connect_tcp(&self, addr: &Address) -> Result<Self::Transport, Error> {
        let transport = self.connector.connect_tcp(addr).await?;
        Ok((self.map)(transport))
    }
}
