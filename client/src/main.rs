pub mod mux;

use std::{
    path::PathBuf,
    time::Duration,
};

use anyhow::{
    Error,
    anyhow,
    bail,
};
use clap::{
    Parser,
    Subcommand,
};
use futures_util::{
    SinkExt,
    StreamExt,
    TryStreamExt,
};
use protocol::{
    ClientHello,
    ClientMessage,
    DEBUG_PORT,
    DeviceHello,
    DeviceMessage,
    PROTOCOL_PORT,
    Version,
};
use tokio::io::AsyncBufReadExt;
use tracing::Instrument;

use crate::mux::{
    Mux,
    serde::{
        Receiver,
        Sender,
    },
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let mut client = Mux::open(&args.device, args.baud_rate)?;

    // forward debug prints from device to stderr
    let (_, mut debug_in) = client.pipe(DEBUG_PORT).await?;
    let span = tracing::info_span!("device");
    tokio::spawn(
        async move {
            let mut line = String::new();
            loop {
                debug_in.read_line(&mut line).await.unwrap();

                let trimmed_line = line.trim();
                if trimmed_line.is_empty() {
                    break;
                }

                tracing::debug!("{trimmed_line}");

                line.clear();
            }
        }
        .instrument(span),
    );

    let (mut sender, mut receiver) = client
        .channel::<ClientMessage, DeviceMessage>(PROTOCOL_PORT)
        .await?;

    let device_hello = handshake(&mut sender, &mut receiver)
        .await
        .ok_or_else(|| anyhow!("Device didn't greet us"))?;
    tracing::debug!(version = ?device_hello.version, "Device version");

    match args.command {
        Command::Measure {} => {
            sender.send(&ClientMessage::Measure {}).await?;

            while let Some(message) = receiver.try_next().await? {
                match message {
                    DeviceMessage::Hello(_device_hello) => {
                        // ignore, since we're only interested in these at
                        // startup
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handshake(
    sender: &mut Sender<ClientMessage>,
    receiver: &mut Receiver<DeviceMessage>,
) -> Option<DeviceHello> {
    // the device will send a bootup hello at startup and a hello whenever it
    // receives a hello from us.
    //
    // we'll send hello's in a loop with some delay, until we hear back. we
    // don't actually care which hello we receive back.
    //
    // note: if we don't wait for the device to talk to use first, we would be
    // sending commands to it while it isn't ready to receive them yet. this
    // took us quite some time to figure out why the device wasn't receiving
    // anything from us.

    let client_hello = ClientHello {
        version: Version {
            major: std::env!("CARGO_PKG_VERSION_MAJOR")
                .parse()
                .unwrap_or_default(),
            minor: std::env!("CARGO_PKG_VERSION_MINOR")
                .parse()
                .unwrap_or_default(),
            patch: std::env!("CARGO_PKG_VERSION_PATCH")
                .parse()
                .unwrap_or_default(),
        },
    };

    let mut error_count = 0;

    while error_count < 5 {
        // send client hello
        sender.send(&ClientMessage::Hello(client_hello)).await;

        match receiver.next().await {
            None => {
                // end of stream, but nothing received :'(
                return None;
            }
            Some(Err(error)) => {
                tracing::error!(?error, ?error_count, "Error while waiting for hello");
                error_count += 1;
                // just continue waiting
            }
            Some(Ok(DeviceMessage::Hello(device_hello))) => {
                return Some(device_hello);
            }
            // todo: remove this lint exception. we didn't have any other message at the time
            #[allow(unreachable_patterns)]
            Some(Ok(_message)) => {
                //tracing::warn!(?message, "Device greeted us with unexpected
                // message");

                // ignore this and continue waiting
                return None;
            }
        }

        tokio::time::sleep(Duration::from_millis(200));
    }

    None
}

#[derive(Debug, Parser)]
struct Args {
    #[clap(long, env = "CAPMETER_DEVICE")]
    device: PathBuf,

    #[clap(long, default_value = "57600", env = "CAPMETER_BAUD_RATE")]
    baud_rate: u32,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Measure {
        // todo
    },
}
