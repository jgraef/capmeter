pub mod client;

use std::time::Duration;

use anyhow::Error;
use clap::{Parser, Subcommand};

use crate::client::Client;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Command::Measure {
            timeout,
            charge_resistor,
        } => {
            let mut client = Client::open().await?;

            let outcome = client
                .measure(charge_resistor, Duration::from_millis(timeout.into()))
                .await
                .inspect_err(|error| tracing::error!(?error, "Device returned error"))?;
            tracing::debug!(?outcome);

            println!("Capacity: {} μF", outcome.capacity);
        }
    }

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Measure {
        /// Timeout for measurement in ms.
        #[clap(short, long, default_value = "10000")]
        timeout: u32,

        /// Resistance of charge resistor in Ω
        #[clap(short = 'R', long, default_value = "1000")]
        charge_resistor: u32,
    },
}
