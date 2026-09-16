pub mod chunk_channel;
mod codec;
mod reactor;
pub mod reader_writer;
pub mod serde;

use std::{
    collections::HashSet,
    path::Path,
};

use protocol::{
    DEFAULT_MTU,
    Port,
};
use tokio::{
    sync::mpsc,
    task::JoinHandle,
};
use tokio_serial::SerialStream;

use crate::mux::reactor::{
    Command,
    Reactor,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Serial(#[from] tokio_serial::Error),

    #[error(transparent)]
    Postcard(#[from] postcard::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Invalid chunk checksum for channel #{port}")]
    InvalidChunkChecksum {
        port: Port,
        local_checksum: u16,
        remote_checksum: u16,
    },

    #[error("Tried to send a chunk that is too large: {chunk_length}")]
    ChunkTooLarge { chunk_length: usize },

    #[error("Port already in use: {port}")]
    PortAlreadyInUse { port: Port },

    #[error("Channel closed: {port}")]
    ChannelClosed { port: Port },
}

impl Error {
    fn into_io_error(self) -> std::io::Error {
        let error_kind = match &self {
            Error::Serial(error) => {
                match error.kind {
                    tokio_serial::ErrorKind::NoDevice => std::io::ErrorKind::ResourceBusy,
                    tokio_serial::ErrorKind::InvalidInput => std::io::ErrorKind::InvalidInput,
                    tokio_serial::ErrorKind::Unknown => std::io::ErrorKind::Other,
                    tokio_serial::ErrorKind::Io(error_kind) => error_kind,
                }
            }
            Error::Postcard(_error) => std::io::ErrorKind::InvalidData,
            Error::Io(error) => error.kind(),
            Error::InvalidChunkChecksum {
                port: _,
                local_checksum: _,
                remote_checksum: _,
            } => std::io::ErrorKind::InvalidData,
            Error::ChunkTooLarge { chunk_length: _ } => std::io::ErrorKind::Unsupported,
            Error::PortAlreadyInUse { port: _ } => std::io::ErrorKind::AddrInUse,
            Error::ChannelClosed { port: _ } => std::io::ErrorKind::BrokenPipe,
        };

        std::io::Error::new(error_kind, self)
    }
}

const COMMAND_CHANNEL_CAPACITY: usize = 64;
const STREAM_CHANNEL_CAPACITY: usize = 64;

#[derive(Debug)]
pub struct Mux {
    command_sender: mpsc::Sender<Command>,

    #[allow(dead_code)]
    join_handle: JoinHandle<()>,

    open_streams: HashSet<Port>,

    mtu: usize,
}

impl Mux {
    pub fn new(stream: SerialStream) -> Self {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
        let reactor = Reactor::new(stream, command_receiver);
        let join_handle = tokio::spawn(reactor.run());

        Self {
            command_sender,
            join_handle,
            open_streams: HashSet::new(),
            mtu: DEFAULT_MTU,
        }
    }

    pub fn open(path: impl AsRef<Path>, baud_rate: u32) -> Result<Self, Error> {
        let builder = tokio_serial::new(path.as_ref().to_string_lossy(), baud_rate);
        // DTR will reset the arduino, which we don't want to happen when we
        // connect. unfortunately Linux will still send DTR on connect.
        // The only solution for Linux seems to be to have a pullup
        // resistor connected to GND or a 10 µF capacitor from RESET to
        // GND.
        //
        // On the other hand: For now it's fine if we ignore this behaviorr.
        //
        //.dtr_on_open(false);
        Ok(Self::new(SerialStream::open(&builder)?))
    }

    pub fn mtu(&self) -> usize {
        self.mtu
    }

    pub fn set_mtu(&self, mtu: usize) {
        assert!(mtu > 0, "MTU must not be 0");
        assert!(mtu <= usize::from(u16::MAX), "MTU must be at most 65536");
    }

    pub async fn chunk_channel(
        &mut self,
        port: Port,
    ) -> Result<(chunk_channel::Sender, chunk_channel::Receiver), Error> {
        if !self.open_streams.insert(port) {
            return Err(Error::PortAlreadyInUse { port });
        }

        let (channel_sender, chunk_receiver) = mpsc::channel(STREAM_CHANNEL_CAPACITY);

        // the reactor should only terminate when all command senders are
        // dropped.
        self.command_sender
            .send(Command::RegisterChannelReceiver {
                port,
                channel_sender,
            })
            .await
            .expect("reactor dead");

        let sender = chunk_channel::Sender::new(self.command_sender.clone(), port, self.mtu);
        let receiver = chunk_channel::Receiver::new(chunk_receiver, port);

        Ok((sender, receiver))
    }

    pub async fn pipe(
        &mut self,
        port: Port,
    ) -> Result<(reader_writer::Writer, reader_writer::Reader), Error> {
        let (sender, receiver) = self.chunk_channel(port).await?;
        Ok((sender.into_writer(), receiver.into_reader()))
    }

    pub async fn channel<T, R>(
        &mut self,
        port: Port,
    ) -> Result<(serde::Sender<T>, serde::Receiver<R>), Error> {
        let (sender, receiver) = self.chunk_channel(port).await?;
        Ok((sender.into_serialized(), receiver.into_deserialized()))
    }
}
