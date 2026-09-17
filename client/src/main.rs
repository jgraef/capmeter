pub mod client;
pub mod util;

use std::{io::BufReader, path::PathBuf, time::Duration};

use anyhow::Error;
use clap::{Parser, Subcommand};
use dialoguer::Confirm;

use crate::{
    client::Client,
    util::{format_si, parse_si},
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Command::Read { count } => {
            let mut client = Client::open().await?;

            let mut previous_serial = None;
            let mut i = 0;
            while i < count {
                let measurement = client
                    .read_measure()
                    .await
                    .inspect_err(|error| tracing::error!(?error, "Device returned error"))?;

                if previous_serial
                    .is_none_or(|previous_serial| previous_serial != measurement.serial)
                {
                    tracing::debug!(?measurement);

                    //println!("Capacity: {} μF", outcome.capacity);
                    println!(
                        "Period (#{}): {} μs",
                        measurement.serial, measurement.period
                    );

                    i += 1;
                    previous_serial = Some(measurement.serial);
                }
            }
        }
        Command::Discharge { on_off } => {
            let mut client = Client::open().await?;
            client.set_discharge(on_off).await?;
        }
        Command::Calibrate {
            file,
            count,
            values,
        } => {
            let values = values
                .split(',')
                .map(|s| parse_si(s, "F"))
                .collect::<Result<Vec<f64>, Error>>()?;

            tracing::debug!(?values);

            let mut client = Client::open().await?;
            let delay = Duration::from_secs(1);

            for value in &values {
                client.set_discharge(true).await?;
                tokio::time::sleep(delay).await;

                if !Confirm::new()
                    .with_prompt(format!(
                        "Place {} capacitor into test fixture and confirm.",
                        format_si(*value, "F")
                    ))
                    .interact()?
                {
                    println!("User didn't confirm. Aborting");
                }

                client.set_discharge(false).await?;
                tokio::time::sleep(delay).await;
            }
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
    Read {
        #[clap(short = 'n', long, default_value = "1")]
        count: usize,
    },
    Discharge {
        on_off: std::primitive::bool,
    },
    Calibrate {
        #[clap(short, long, default_value = "calibration.json")]
        file: PathBuf,

        #[clap(short = 'n', long, default_value = "100")]
        count: usize,

        #[clap(short, long)]
        values: String,
    },
}
