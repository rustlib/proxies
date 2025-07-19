mod http;
mod socks5;

use std::io::Error;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

pub use http::HttpHandle;
pub use socks5::Socks5Handle;

use std::fmt::Debug;
use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio_stream::{Stream, StreamExt};

use crate::{ProxyError, connector::Connector, util::BufIoExt};

pub struct ProxyServer<C, I = TcpIncoming> {
    incoming: I,
    client_handle: Arc<ClientHandle<C>>,
}

impl<C> ProxyServer<C, TcpIncoming> {
    pub async fn bind<A>(connector: C, addr: A) -> Result<Self, ProxyError>
    where
        A: ToSocketAddrs + Clone + Debug,
    {
        let listener = match TcpListener::bind(addr.clone()).await {
            Ok(l) => l,
            Err(e) => {
                bail!("bind {:?} fail: {}", addr, e);
            }
        };
        Ok(ProxyServer {
            incoming: TcpIncoming { listener },
            client_handle: Arc::new(ClientHandle::new(connector)),
        })
    }

    pub fn from_listener(connector: C, listener: TcpListener) -> Self {
        ProxyServer {
            incoming: TcpIncoming { listener },
            client_handle: Arc::new(ClientHandle::new(connector)),
        }
    }
}

impl<C, I> ProxyServer<C, I> {
    pub fn from_incoming(connector: C, incoming: I) -> Self {
        Self {
            incoming,
            client_handle: Arc::new(ClientHandle::new(connector)),
        }
    }
}

impl<C, I, T> ProxyServer<C, I>
where
    C: Connector + Send + Sync + 'static,
    <C as Connector>::Transport: Unpin + Send,
    I: Stream<Item = Result<(T, SocketAddr), Error>> + Unpin,
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub fn new(connector: C, incoming: I) -> Self {
        Self {
            incoming,
            client_handle: Arc::new(ClientHandle::new(connector)),
        }
    }

    pub async fn run(mut self) -> Result<(), ProxyError> {
        while let Some(result) = self.incoming.next().await {
            match result {
                Ok((sock, addr)) => {
                    let client_handle = self.client_handle.clone();
                    tokio::spawn(async move {
                        if let Err(e) = client_handle.handle(sock, addr).await {
                            warn!("handle {} fail: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    bail!("accept incoming fail: {}", e);
                }
            }
        }
        Ok(())
    }
}

struct ClientHandle<C> {
    connector: C,

    socks5_handle: Socks5Handle,
    http_handle: HttpHandle,
}

impl<C> ClientHandle<C> {
    fn new(connector: C) -> Self {
        Self {
            connector,
            socks5_handle: Socks5Handle::new(),
            http_handle: HttpHandle::new(),
        }
    }
}

impl<C> ClientHandle<C>
where
    C: Connector,
    <C as Connector>::Transport: Unpin,
{
    async fn handle<T>(&self, sock: T, addr: SocketAddr) -> Result<(), ProxyError>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let mut stream = BufReader::new(sock);
        match stream.try_peek_byte().await {
            Ok(Some(0x05)) => self.socks5_handle.handle(&self.connector, stream).await?,
            Ok(Some(_)) => self.http_handle.handle(&self.connector, stream).await?,
            Ok(None) => {
                debug!("local socket({}) EOF with no data", addr);
            }
            Err(e) => {
                bail!("read local socket({}) fail: {}", addr, e);
            }
        }
        Ok(())
    }
}

pub struct TcpIncoming {
    listener: TcpListener,
}

impl Stream for TcpIncoming {
    type Item = Result<(TcpStream, SocketAddr), Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (sock, addr) = ready!(Pin::new(&mut self.listener).poll_accept(cx))?;
        sock.set_nodelay(true)?;
        Poll::Ready(Some(Ok((sock, addr))))
    }
}
