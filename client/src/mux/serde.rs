use std::{
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytes::{
    BufMut,
    BytesMut,
};
use futures_util::{
    Sink,
    Stream,
};
use pin_project_lite::pin_project;
use protocol::Port;
use serde::{
    Serialize,
    de::DeserializeOwned,
};

use crate::mux::{
    Error,
    chunk_channel,
};

pin_project! {
    #[derive(Debug)]
    pub struct Sender<T> {
        #[pin]
        sender: chunk_channel::Sender,
        _marker: PhantomData<T>,
    }
}

impl<T> Sender<T> {
    pub(super) fn new(sender: chunk_channel::Sender) -> Self {
        Self {
            sender,
            _marker: PhantomData,
        }
    }

    pub fn port(&self) -> Port {
        self.sender.port()
    }

    pub fn mtu(&self) -> usize {
        self.sender.mtu()
    }

    pub fn into_inner(self) -> chunk_channel::Sender {
        self.sender
    }
}

impl<T> Sink<&T> for Sender<T>
where
    T: Serialize,
{
    // todo: make specific error type for when channel is closed
    type Error = Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: &T) -> Result<(), Self::Error> {
        let mut data = BytesMut::new();
        postcard::to_io(item, (&mut data).writer())?;
        self.project().sender.start_send(data.freeze())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_close(cx)
    }
}

pin_project! {
    #[derive(Debug)]
    pub struct Receiver<T> {
        #[pin]
        receiver: chunk_channel::Receiver,
        _marker: PhantomData<T>,
    }
}

impl<T> Receiver<T> {
    pub(super) fn new(receiver: chunk_channel::Receiver) -> Self {
        Self {
            receiver,
            _marker: PhantomData,
        }
    }

    pub fn port(&self) -> Port {
        self.receiver.port()
    }

    pub fn into_inner(self) -> chunk_channel::Receiver {
        self.receiver
    }
}

impl<T> Stream for Receiver<T>
where
    T: DeserializeOwned,
{
    type Item = Result<T, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        this.receiver.poll_next(cx).map(|option| {
            option.map(|result| {
                result.and_then(|data| postcard::from_bytes(&data).map_err(Into::into))
            })
        })
    }
}
