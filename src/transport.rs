use tokio::io::{AsyncRead, AsyncWrite};

pub trait AsyncTransport: AsyncRead + AsyncWrite {
    fn boxed(self) -> BoxedTransport
    where
        Self: Send + Unpin + Sized + 'static,
    {
        Box::new(self)
    }
}

impl<T> AsyncTransport for T where T: AsyncRead + AsyncWrite {}

pub type BoxedTransport = Box<dyn AsyncTransport + Send + Unpin>;
