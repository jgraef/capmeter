use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytes::Bytes;
use futures_util::{
    Sink,
    SinkExt,
    Stream,
};
use protocol::Port;
use tokio::sync::mpsc;
use tokio_util::sync::PollSender;

use crate::mux::{
    Error,
    codec::Chunk,
    reactor::Command,
    reader_writer::{
        Reader,
        Writer,
    },
    serde,
};

#[derive(derive_more::Debug)]
pub struct Sender {
    #[debug("PollSender({:?})", command_sender.get_ref())]
    command_sender: PollSender<Command>,

    port: Port,
    mtu: usize,
}

impl Sender {
    pub(super) fn new(command_sender: mpsc::Sender<Command>, port: Port, mtu: usize) -> Self {
        Self {
            command_sender: PollSender::new(command_sender),
            port,
            mtu,
        }
    }

    pub fn port(&self) -> Port {
        self.port
    }

    pub fn mtu(&self) -> usize {
        self.mtu
    }

    pub fn into_writer(self) -> Writer {
        Writer::new(self)
    }

    pub fn into_serialized<T>(self) -> serde::Sender<T> {
        serde::Sender::new(self)
    }
}

impl Sink<Bytes> for Sender {
    // todo: make specific error type for when channel is closed
    type Error = Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.command_sender
            .poll_ready_unpin(cx)
            .map_err(|_error| Error::ChannelClosed { port: self.port })
    }

    fn start_send(mut self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        let chunk = Chunk {
            port: self.port,
            data: item,
        };

        self.command_sender
            .start_send_unpin(Command::SendChunk { chunk })
            .map_err(|_error| Error::ChannelClosed { port: self.port })
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.command_sender
            .poll_flush_unpin(cx)
            .map_err(|_error| Error::ChannelClosed { port: self.port })
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.command_sender
            .poll_close_unpin(cx)
            .map_err(|_error| Error::ChannelClosed { port: self.port })
    }
}

#[derive(derive_more::Debug)]
pub struct Receiver {
    chunk_receiver: mpsc::Receiver<Chunk>,

    port: Port,
}

impl Receiver {
    pub(super) fn new(chunk_receiver: mpsc::Receiver<Chunk>, port: Port) -> Self {
        Self {
            chunk_receiver,
            port,
        }
    }

    pub fn port(&self) -> Port {
        self.port
    }

    pub fn into_reader(self) -> Reader {
        Reader::new(self)
    }

    pub fn into_deserialized<R>(self) -> serde::Receiver<R> {
        serde::Receiver::new(self)
    }
}

impl Stream for Receiver {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // todo: error propagation

        let this = &mut *self;

        this.chunk_receiver.poll_recv(cx).map(|chunk_op| {
            chunk_op.map(|chunk| {
                assert_eq!(chunk.port, this.port);

                Ok(chunk.data)
            })
        })
    }
}
