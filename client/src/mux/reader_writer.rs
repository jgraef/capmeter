use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
        ready,
    },
};

use bytes::{
    BufMut,
    Bytes,
    BytesMut,
};
use futures_util::{
    SinkExt,
    StreamExt,
};
use protocol::Port;
use tokio::io::{
    AsyncBufRead,
    AsyncRead,
    AsyncWrite,
    ReadBuf,
};

use crate::mux::{
    Error,
    chunk_channel,
};

#[derive(Debug)]
pub struct Writer {
    sender: chunk_channel::Sender,
    write_buffer: BytesMut,
}

impl Writer {
    pub(super) fn new(sender: chunk_channel::Sender) -> Self {
        Self {
            sender,
            write_buffer: Default::default(),
        }
    }

    pub fn port(&self) -> Port {
        self.sender.port()
    }

    pub fn mtu(&self) -> usize {
        self.sender.mtu()
    }

    pub fn into_chunk_sender(self) -> chunk_channel::Sender {
        // todo: document the bahviour in regards to dropping buffers
        self.sender
    }
}

impl AsyncWrite for Writer {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        if self.write_buffer.len() == self.mtu() {
            // buffer is full, so flush it first.
            ready!(Pin::new(&mut *self).poll_flush(cx)?);

            assert!(self.write_buffer.is_empty());
        }

        let n = buf.len().min(self.mtu() - self.write_buffer.len());

        self.write_buffer.put_slice(&buf[..n]);
        Poll::Ready(Ok(n))
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        if !self.write_buffer.is_empty() {
            if ready!(self.sender.poll_ready_unpin(cx)).is_err() {
                panic!("reactor dead");
            }

            let data = std::mem::take(&mut self.write_buffer).freeze();
            self.sender
                .start_send_unpin(data)
                .map_err(|error| error.into_io_error())?;
        }

        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        // todo: if we do eof signaling, we need to do this here.
        self.poll_flush(cx)
    }
}

#[derive(Debug)]
pub struct Reader {
    receiver: chunk_channel::Receiver,
    read_buffer: Bytes,
}

impl Reader {
    pub(super) fn new(receiver: chunk_channel::Receiver) -> Self {
        Self {
            receiver,
            read_buffer: Default::default(),
        }
    }

    pub fn port(&self) -> Port {
        self.receiver.port()
    }

    pub fn into_chunk_receiver(self) -> chunk_channel::Receiver {
        // todo: document the bahviour in regards to dropping buffers
        self.receiver
    }

    fn poll_read_impl(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        while self.read_buffer.is_empty() {
            match self.receiver.poll_next_unpin(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    // return with empty buffer to signal that it's EOF
                    break;
                }
                Poll::Ready(Some(Err(error))) => {
                    return Poll::Ready(Err(error));
                }
                Poll::Ready(Some(Ok(data))) => {
                    // if data is empty the loop will keep iterating to try to
                    // actually get some data into the buffer
                    self.read_buffer = data;
                }
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for Reader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let read_buffer = ready!(Pin::new(&mut *self).poll_fill_buf(cx)?);

        let n = read_buffer.len().min(buf.remaining_mut());
        buf.put_slice(&read_buffer[..n]);

        Pin::new(&mut *self).consume(n);

        Poll::Ready(Ok(()))
    }
}

impl AsyncBufRead for Reader {
    fn poll_fill_buf(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<&[u8], std::io::Error>> {
        ready!(
            Pin::new(&mut *self)
                .poll_read_impl(cx)
                .map_err(|e| e.into_io_error())?
        );

        // note: poll_read_impl only ever returns Poll::Ready with a empty
        // read_buffer if the stream is EOF. this is also expected from the
        // trait.

        Poll::Ready(Ok(&Pin::into_inner(self).read_buffer))
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        let _consumed = self.read_buffer.split_to(amt);
    }
}
