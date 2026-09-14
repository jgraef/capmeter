use std::collections::HashMap;

use futures_util::{
    SinkExt,
    TryStreamExt,
};
use protocol::Port;
use tokio::sync::mpsc;
use tokio_serial::SerialStream;
use tokio_util::codec::Framed;

use crate::mux::codec::{
    Chunk,
    ChunkedCodec,
};

#[derive(Debug)]
pub(super) struct Reactor {
    stream: Framed<SerialStream, ChunkedCodec>,
    command_receiver: mpsc::Receiver<Command>,
    channel_receivers: HashMap<Port, mpsc::Sender<Chunk>>,
}

impl Reactor {
    pub(super) fn new(stream: SerialStream, command_receiver: mpsc::Receiver<Command>) -> Self {
        Self {
            stream: Framed::new(stream, Default::default()),
            command_receiver,
            channel_receivers: HashMap::new(),
        }
    }

    #[tracing::instrument("reactor", skip_all)]
    pub(super) async fn run(mut self) {
        loop {
            tokio::select! {
                command_opt = self.command_receiver.recv() => {
                    let Some(command) = command_opt else { break; };
                    self.handle_command(command).await;
                }
                result = self.stream.try_next() => {
                    match result {
                        Ok(None) => break,
                        Ok(Some(chunk)) => {
                            self.handle_chunk(chunk).await;
                        },
                        Err(error) => {
                            // todo: propagate error in some way
                            tracing::error!(?error, "error when receiving chunk");
                        }
                    }

                }
            }
        }
    }

    async fn handle_command(&mut self, command: Command) {
        tracing::debug!(?command, "handling command");

        match command {
            Command::RegisterChannelReceiver {
                port,
                channel_sender,
            } => {
                self.channel_receivers.insert(port, channel_sender);
            }
            Command::SendChunk { chunk } => {
                if let Err(error) = self.stream.send(chunk).await {
                    // todo: propagate error in some way
                    tracing::error!(?error, "error when sending chunk");
                }
            }
        }
    }

    async fn handle_chunk(&mut self, chunk: Chunk) {
        tracing::debug!(?chunk, "handling chunk");

        let stream_id = chunk.port;

        if let Some(stream_receiver) = self.channel_receivers.get(&stream_id) {
            if stream_receiver.send(chunk).await.is_err() {
                self.channel_receivers.remove(&stream_id);
            }
        }
        else {
            tracing::debug!(stream_id, "No stream receiver registered");
        }
    }
}

#[derive(Debug)]
pub(super) enum Command {
    RegisterChannelReceiver {
        port: Port,
        channel_sender: mpsc::Sender<Chunk>,
    },
    SendChunk {
        chunk: Chunk,
    },
}
