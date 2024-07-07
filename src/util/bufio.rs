use bytes::{BufMut, Bytes, BytesMut};
use std::future::Future;
use std::io::{Error, ErrorKind};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncBufRead;

pub trait BufIoExt: AsyncBufRead + Sized {
    fn read_until_bytes<'a>(&'a mut self, pattern: &'a [u8]) -> ReadUntilBytes<'a, Self> {
        ReadUntilBytes {
            bufio: self,
            pattern,
            buffer: BytesMut::new(),
            pattern_matched: 0,
        }
    }

    fn try_read_byte(&mut self) -> TryReadByte<'_, Self> {
        TryReadByte { bufio: self }
    }

    fn try_peek_byte(&mut self) -> TryPeekByte<'_, Self> {
        TryPeekByte { bufio: self }
    }
}

impl<T: AsyncBufRead> BufIoExt for T {}

pub struct ReadUntilBytes<'a, T> {
    bufio: &'a mut T,
    pattern: &'a [u8],
    buffer: BytesMut,
    pattern_matched: usize,
}

impl<'a, T> Future for ReadUntilBytes<'a, T>
where
    T: AsyncBufRead + Unpin,
{
    type Output = Result<Bytes, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        let data: &[u8] = ready!(Pin::new(&mut *this.bufio).poll_fill_buf(cx))?;
        if data.is_empty() {
            return Poll::Ready(Err(Error::new(ErrorKind::UnexpectedEof, "unexpected EOF")));
        }

        if this.pattern_matched > 0 {
            let part_pattern = &this.pattern[this.pattern_matched..];
            if data.starts_with(part_pattern) {
                this.buffer.put_slice(part_pattern);
                Pin::new(&mut *this.bufio).consume(part_pattern.len());
                return Poll::Ready(Ok(this.buffer.split().freeze()));
            }
            this.pattern_matched = 0;
        }

        match end_of(data, this.pattern) {
            Ok(size) => {
                this.buffer.put_slice(&data[..size]);
                Pin::new(&mut *this.bufio).consume(size);
                Poll::Ready(Ok(this.buffer.split().freeze()))
            }
            Err(got) => {
                this.pattern_matched = got;
                this.buffer.put_slice(data);
                let data_len = data.len();
                Pin::new(&mut *this.bufio).consume(data_len);
                Poll::Pending
            }
        }
    }
}

fn end_of(data: &[u8], pat: &[u8]) -> Result<usize, usize> {
    // TODO: kmp
    if pat.len() <= data.len() {
        'outter: for i in 0..data.len() {
            for (j, b) in pat.iter().enumerate() {
                let idx = i + j;
                if idx >= data.len() {
                    return Err(j);
                }
                if data[idx] != *b {
                    continue 'outter;
                }
            }
            return Ok(i + pat.len());
        }
    }
    Err(0)
}

pub struct TryReadByte<'a, T> {
    bufio: &'a mut T,
}

impl<'a, T> Future for TryReadByte<'a, T>
where
    T: AsyncBufRead + Unpin,
{
    type Output = Result<Option<u8>, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut bufio = Pin::new(&mut self.bufio);
        let byte = {
            let buf = ready!(bufio.as_mut().poll_fill_buf(cx))?;
            if buf.is_empty() {
                return Poll::Ready(Ok(None));
            }
            buf[0]
        };
        bufio.consume(1);
        Poll::Ready(Ok(Some(byte)))
    }
}

pub struct TryPeekByte<'a, T> {
    bufio: &'a mut T,
}

impl<'a, T> Future for TryPeekByte<'a, T>
where
    T: AsyncBufRead + Unpin,
{
    type Output = Result<Option<u8>, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let buf = ready!(Pin::new(&mut self.bufio).poll_fill_buf(cx))?;
        if buf.is_empty() {
            Poll::Ready(Ok(None))
        } else {
            Poll::Ready(Ok(Some(buf[0])))
        }
    }
}

#[cfg(test)]
mod test {
    use super::end_of;
    #[test]
    fn test_end_of() {
        assert_eq!(end_of(b"123456", b"123"), Ok(3));
        assert_eq!(end_of(b"123456", b"234"), Ok(4));
        assert_eq!(end_of(b"123456", b"456"), Ok(6));

        assert_eq!(end_of(b"123456", b"23a"), Err(0));
        assert_eq!(end_of(b"123456", b"abc"), Err(0));
        assert_eq!(end_of(b"123456", b"567"), Err(2));
        assert_eq!(end_of(b"123456", b"67"), Err(1));
    }
}
