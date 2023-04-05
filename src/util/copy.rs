use pin_project_lite::pin_project;
use std::io::Error;
use std::pin::Pin;
use std::string::ToString;
use std::task::{Context, Poll};
use std::{future::Future, io::ErrorKind};
use tokio::io::{split, AsyncBufRead, AsyncRead, AsyncWrite, BufReader, ReadHalf, WriteHalf};

pin_project! {
    struct Copy<S, D> {
        reader_label: String,
        writer_label: String,

        #[pin]
        reader: S,
        #[pin]
        writer: D,
        writer_done: bool,
        flush_on_pending: bool,
        amt: usize,
    }
}

impl<S, D> Copy<S, D> {
    fn new(
        reader_label: String,
        reader: S,
        writer_label: String,
        writer: D,
        flush_on_pending: bool,
    ) -> Copy<S, D> {
        Copy {
            reader_label,
            writer_label,
            reader,
            writer,
            writer_done: false,
            flush_on_pending,
            amt: 0,
        }
    }
}

macro_rules! bail_other_err {
    ($fmt:expr $(, $arg:expr)*) => {
        return Poll::Ready(Err(Error::new(ErrorKind::Other, format!($fmt $(,$arg)*))))
    };
}

impl<S, D> Future for Copy<S, D>
where
    S: AsyncBufRead,
    D: AsyncWrite,
{
    type Output = Result<usize, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        let mut write_flush = *this.flush_on_pending;

        loop {
            let buffer = match this.reader.as_mut().poll_fill_buf(cx) {
                Poll::Pending => {
                    if *this.writer_done {
                        return Poll::Ready(Ok(*this.amt));
                    }
                    if write_flush {
                        if let Err(e) = ready!(this.writer.as_mut().poll_flush(cx)) {
                            bail_other_err!("flush {} fail: {}", this.writer_label, e);
                        }
                        *this.flush_on_pending = false;
                    }
                    return Poll::Pending;
                }
                Poll::Ready(Ok(buf)) => buf,
                Poll::Ready(Err(e)) => {
                    bail_other_err!("read {} fail: {}", this.reader_label, e);
                }
            };
            if buffer.is_empty() {
                if let Err(e) = ready!(this.writer.as_mut().poll_flush(cx)) {
                    bail_other_err!("flush {} fail: {}", this.writer_label, e);
                }
                return Poll::Ready(Ok(*this.amt));
            }
            if *this.writer_done {
                bail_other_err!(
                    "{} unexpected closed, remain {} bytes to write",
                    this.writer_label,
                    buffer.len()
                );
            }
            match ready!(this.writer.as_mut().poll_write(cx, buffer)) {
                Ok(n) => {
                    if n == 0 {
                        bail_other_err!("write {} zero bytes", this.writer_label);
                    }
                    write_flush = true;
                    this.reader.as_mut().consume(n);
                    *this.amt += n;
                }
                Err(e) => {
                    bail_other_err!("write {} fail: {}", this.writer_label, e);
                }
            }
        }
    }
}

pub struct DuplexCopy<L, R> {
    left: Copy<BufReader<ReadHalf<L>>, WriteHalf<R>>,
    left_done: bool,
    left_amt: usize,

    right: Copy<BufReader<ReadHalf<R>>, WriteHalf<L>>,
    right_done: bool,
    right_amt: usize,
}

impl<L, R> DuplexCopy<L, R>
where
    L: AsyncRead + AsyncWrite,
    R: AsyncRead + AsyncWrite,
{
    pub fn new(left: L, right: R) -> DuplexCopy<L, R> {
        Self::with_pending("left", left, false, "right", right, false)
    }

    pub fn with_label<A: ToString, B: ToString>(
        left_label: A,
        left: L,
        right_label: B,
        right: R,
    ) -> DuplexCopy<L, R> {
        Self::with_pending(left_label, left, false, right_label, right, false)
    }

    pub fn with_pending<A: ToString, B: ToString>(
        left_label: A,
        left: L,
        left_flush_pending: bool,
        right_label: B,
        right: R,
        right_flush_pending: bool,
    ) -> DuplexCopy<L, R> {
        let (left_label, right_label) = (left_label.to_string(), right_label.to_string());
        let (left_r, left_w) = split(left);
        let (right_r, right_w) = split(right);

        DuplexCopy {
            left: Copy::new(
                left_label.clone(),
                BufReader::new(left_r),
                right_label.clone(),
                right_w,
                right_flush_pending,
            ),
            left_done: false,
            left_amt: 0,

            right: Copy::new(
                right_label,
                BufReader::new(right_r),
                left_label,
                left_w,
                left_flush_pending,
            ),
            right_done: false,
            right_amt: 0,
        }
    }
}

impl<L, R> Future for DuplexCopy<L, R>
where
    L: AsyncRead + AsyncWrite + Unpin,
    R: AsyncRead + AsyncWrite + Unpin,
{
    type Output = Result<(usize, usize), Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            if !self.left_done {
                match Pin::new(&mut self.left).poll(cx) {
                    Poll::Ready(Ok(amt)) => {
                        self.left_amt = amt;
                        self.left_done = true;
                        self.right.writer_done = true;
                    }
                    Poll::Ready(Err(e)) => {
                        return Poll::Ready(Err(e));
                    }
                    _ => {}
                }
            }
            if !self.right_done {
                match Pin::new(&mut self.right).poll(cx) {
                    Poll::Ready(Ok(amt)) => {
                        self.right_amt = amt;
                        self.right_done = true;
                        self.left.writer_done = true;
                        continue;
                    }
                    Poll::Ready(Err(e)) => {
                        return Poll::Ready(Err(e));
                    }
                    _ => {}
                }
            }
            break;
        }

        if self.left_done && self.right_done {
            Poll::Ready(Ok((self.left_amt, self.right_amt)))
        } else {
            Poll::Pending
        }
    }
}
