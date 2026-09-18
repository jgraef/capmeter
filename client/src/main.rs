pub mod calibration;
pub mod client;
pub mod util;

use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
    time::Duration,
};

use anyhow::Error;
use clap::{Parser, Subcommand};
use dialoguer::Confirm;
use futures_util::{TryStreamExt, pin_mut};

use crate::{
    calibration::Calibration,
    client::{Client, Prescaler},
    util::{StreamExt as _, format_si, parse_si},
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Command::Read { count, interval } => {
            let mut client = Client::open().await?;

            let measurements = client.stream_measurements(interval).maybe_limit(count);
            pin_mut!(measurements);

            while let Some(measurement) = measurements.try_next().await? {
                tracing::debug!(?measurement);

                //println!("Capacity: {} μF", outcome.capacity);
                println!("#{}: Period: {} μs", measurement.serial, measurement.period);
            }
        }
        Command::Discharge {
            on_off,
            toggle_interactive,
        } => {
            let mut client = Client::open().await?;
            client.set_discharge(on_off).await?;

            if toggle_interactive {
                if prompt_interactive_discharge(on_off).await? {
                    client.set_discharge(!on_off).await?;
                }
            }
        }
        Command::SetPrescaler { divisor } => {
            let mut client = Client::open().await?;
            client
                .set_prescaler(Prescaler::from_divisor(divisor)?)
                .await?;
        }
        Command::Calibrate {
            calibration: file,
            count,
            interval,
            capacitors,
        } => {
            let capacitors = capacitors
                .split(',')
                .map(|s| parse_si(s.trim(), "F"))
                .collect::<Result<Vec<f64>, Error>>()?;

            let mut client = Client::open().await?;
            let calibration = client
                .calibrate(
                    &capacitors,
                    count,
                    interval,
                    Prescaler::Div1,
                    prompt_capacitor_swap,
                )
                .await?;

            let writer = BufWriter::new(File::create(&file)?);
            serde_json::to_writer_pretty(writer, &calibration)?;
        }
        Command::Measure {
            calibration,
            count,
            interval,
        } => {
            let reader = BufReader::new(File::open(&calibration)?);
            let calibration: Calibration = serde_json::from_reader(reader)?;

            let mut client = Client::open().await?;
            let period = client.read_average(count, interval).await?;
            let capacitance = calibration.model.period_to_capacitance(period);

            println!("Capacitance: {}", format_si(capacitance, "F"));
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
        #[clap(short = 'n', long)]
        count: Option<usize>,

        #[clap(short, long, value_parser = parse_time)]
        interval: Option<Duration>,
    },
    Discharge {
        on_off: std::primitive::bool,

        #[clap(short, long)]
        toggle_interactive: bool,
    },
    SetPrescaler {
        divisor: u8,
    },
    Calibrate {
        #[clap(short, long, default_value = "calibration.json")]
        calibration: PathBuf,

        #[clap(short = 'n', long, default_value = "100")]
        count: usize,

        #[clap(short, long, value_parser = parse_time, default_value = "10ms")]
        interval: Option<Duration>,

        #[clap(short, long)]
        capacitors: String,
    },
    Measure {
        #[clap(short, long, default_value = "calibration.json")]
        calibration: PathBuf,

        #[clap(short = 'n', long, default_value = "10")]
        count: usize,

        #[clap(short, long, value_parser = parse_time, default_value = "10ms")]
        interval: Option<Duration>,
    },
}

async fn prompt_capacitor_swap(capacitor: f64) -> Result<bool, Error> {
    tokio::task::spawn_blocking(move || {
        Confirm::new()
            .with_prompt(format!(
                "Place {} capacitor into test fixture and confirm.",
                format_si(capacitor, "F")
            ))
            .interact()
            .map_err(Into::into)
    })
    .await
    .unwrap()
}

async fn prompt_interactive_discharge(on_off: bool) -> Result<bool, Error> {
    tokio::task::spawn_blocking(move || {
        Confirm::new()
            .with_prompt(format!(
                "Confirm to turn discharge back {}",
                if on_off { "off " } else { "on " },
            ))
            .interact()
            .map_err(Into::into)
    })
    .await
    .unwrap()
}

fn parse_time(s: &str) -> Result<Duration, Error> {
    parse_si(s, "s").map(Duration::from_secs_f64)
}
