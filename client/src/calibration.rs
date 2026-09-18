use std::time::Duration;

use anyhow::Error;
use linfa::{Dataset, traits::Fit};
use linfa_linear::LinearRegression;
use ndarray::{Array1, Array2};
use protocol::Prescaler;
use serde::{Deserialize, Serialize};

use crate::{client::Client, util::format_si};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Calibration {
    /// The measurements taken for calibration
    pub measurements: Vec<Measurement>,

    /// The linear fit model for period/capacitance relationship
    pub model: Model,

    /// Whether this calibration was partial. I.e. it was aborted due to user input or error at some point.
    pub partial: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Measurement {
    pub capacitor: f64,
    pub period: f64,
    pub prescaler: Prescaler,
}

/// Model for linear period/capacitance relationship: `T = a * R + b`
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Model {
    pub a: f64,
    pub b: f64,
}

impl Model {
    /// Convert measured period to capacitance.
    ///
    /// The argument is expected in seconds.
    pub fn period_to_capacitance(&self, period: f64) -> f64 {
        (period - self.b) / self.a
    }
}

impl Client {
    pub async fn calibrate(
        &mut self,
        capacitors: &[f64],
        measurement_count: usize,
        measurement_interval: Option<Duration>,
        prescaler: Prescaler,
        mut prompt_capacitor_swap: impl AsyncFnMut(f64) -> Result<bool, Error>,
    ) -> Result<Calibration, Error> {
        self.set_prescaler(prescaler).await?;

        let delay = Duration::from_secs(1);

        let mut measurements = Vec::with_capacity(capacitors.len());
        let mut partial = false;

        for capacitor in capacitors.iter().copied() {
            tracing::debug!(capacitor = %format_si(capacitor, "F"), "Measuring capacitor");

            if let Err(error) = self.set_discharge(true).await {
                tracing::error!(?error, "usb error when enabling discharge");
                partial = true;
                break;
            }
            tokio::time::sleep(delay).await;

            if !prompt_capacitor_swap(capacitor).await? {
                println!("User didn't confirm. Aborting");
                partial = true;
                break;
            }

            if let Err(error) = self.set_discharge(false).await {
                tracing::error!(?error, "usb error when disbling discharge");
                partial = true;
                break;
            }
            tokio::time::sleep(delay).await;

            match self
                .read_average(measurement_count, measurement_interval)
                .await
            {
                Ok(period) => {
                    tracing::info!(?capacitor, ?period, "calibration measurement");

                    measurements.push(Measurement {
                        capacitor,
                        period,
                        prescaler,
                    });
                }
                Err(error) => {
                    tracing::error!(?error, "usb error during measurement");
                    partial = true;
                    break;
                }
            }
        }

        // fit model
        let model = fit_model(&measurements);

        Ok(Calibration {
            measurements,
            model,
            partial,
        })
    }
}

fn fit_model(measurements: &[Measurement]) -> Model {
    let n = measurements.len();
    let records = Array2::from_shape_fn((n, 1), |(i, _j)| measurements[i].capacitor);
    let targets = Array1::from_shape_fn(n, |i| measurements[i].period);
    let dataset = Dataset::new(records, targets);

    let model = LinearRegression::default().fit(&dataset).unwrap();

    let a = model.params()[0];
    let b = model.intercept();

    Model { a, b }
}
