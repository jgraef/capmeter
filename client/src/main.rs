pub mod mux;

use std::path::PathBuf;

use anyhow::Error;
use clap::{
    Parser,
    Subcommand,
};
use futures_util::{
    SinkExt,
    TryStreamExt,
};
use protocol::{
    ClientMessage,
    DEBUG_PORT,
    DeviceMessage,
    PROTOCOL_PORT,
};
use tokio::io::AsyncBufReadExt;
use tracing::Instrument;

use crate::mux::Mux;

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
                if line.is_empty() {
                    break;
                }

                tracing::debug!("{line}");

                line.clear();
            }
        }
        .instrument(span),
    );

    let (mut sender, mut receiver) = client
        .channel::<ClientMessage, DeviceMessage>(PROTOCOL_PORT)
        .await?;

    match args.command {
        Command::Measure {} => {
            sender.send(&ClientMessage::Measure {}).await?;

            while let Some(message) = receiver.try_next().await? {
                match message {
                    DeviceMessage::Hello { version } => {
                        tracing::debug!(?version, "Device Hello");
                    }
                }
            }
        }
    }

    Ok(())
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
