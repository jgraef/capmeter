use std::time::Duration;

use async_stream::try_stream;
use futures_util::{Stream, StreamExt, TryStreamExt};
use nusb::{
    DeviceSelector,
    transfer::{ControlIn, ControlOut, ControlType, Recipient},
};
pub use protocol::Prescaler;
use protocol::{Command, Measurement};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Usb(#[from] nusb::Error),

    #[error(transparent)]
    UsbTransfer(#[from] nusb::transfer::TransferError),

    #[error("Device not found")]
    DeviceNotFound,

    #[error(transparent)]
    Postcard(#[from] postcard::Error),

    #[error("Device hasn't taken a measurement yet")]
    NoMeasurement,
}

#[derive(Debug)]
pub struct Client {
    usb_interface: nusb::Interface,
    usb_timeout: Duration,
    retry_interval: Duration,
    num_retries: usize,
}

impl Client {
    pub async fn open() -> Result<Self, Error> {
        // request device
        let device_info = nusb::request_device(&[
            DeviceSelector::all().with_vid_pid(protocol::VENDOR_ID, protocol::PRODUCT_ID)
        ])
        .await?
        .ok_or(Error::DeviceNotFound)?;

        tracing::debug!(?device_info);

        // open device
        tracing::debug!("open device");
        let device = device_info.open().await?;

        tracing::debug!("claim interface");
        let interface = device.claim_interface(0).await?;

        Ok(Self {
            usb_interface: interface,
            usb_timeout: Duration::from_millis(500),
            retry_interval: Duration::from_millis(250),
            num_retries: 10,
        })
    }

    pub async fn read_measure(&mut self) -> Result<Measurement, Error> {
        // note: the usb device can respond with None if it hasn't taken a measurement yet. in this case we should just try again.
        let mut interval = tokio::time::interval(self.retry_interval);

        for _ in 0..self.num_retries {
            // note: first call doesn't wait
            interval.tick().await;

            let data = self
                .usb_interface
                .control_in(
                    ControlIn {
                        control_type: ControlType::Vendor,
                        recipient: Recipient::Interface,
                        request: 0,
                        value: 0,
                        index: 0,
                        length: 256,
                    },
                    self.usb_timeout,
                )
                .await?;

            if let Some(measurement) = postcard::from_bytes::<Option<Measurement>>(&data)? {
                return Ok(measurement);
            }
        }

        Err(Error::NoMeasurement)
    }

    pub fn stream_measurements(
        &mut self,
        interval: Option<Duration>,
    ) -> impl Stream<Item = Result<Measurement, Error>> {
        try_stream! {
            let mut previous_serial = None;
            let mut interval = interval.map(tokio::time::interval);

            loop {
                if let Some(interval) = &mut interval {
                    interval.tick().await;
                }

                let measurement = self.read_measure().await?;

                let serial = measurement.serial;
                if previous_serial.is_none_or(|previous_serial| previous_serial != serial) {
                    yield measurement;
                }

                previous_serial = Some(serial);
            }
        }
    }

    /// Returns average period in s.
    pub async fn read_average(
        &mut self,
        count: usize,
        interval: Option<Duration>,
    ) -> Result<f64, Error> {
        let (sum, n) = self
            .stream_measurements(interval)
            .take(count)
            .map_ok(|measurement| measurement.period)
            .try_fold((0, 0), async |(sum, n), period| Ok((sum + period, n + 1)))
            .await?;

        if n == count {
            Ok(sum as f64 / n as f64 * 1e-6)
        } else {
            tracing::error!("We tried reading {count} measurements, but only got {n}");
            Err(Error::NoMeasurement)
        }
    }

    async fn send_command(&mut self, command: &Command) -> Result<(), Error> {
        let data = postcard::to_allocvec(command)?;

        self.usb_interface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Interface,
                    request: 0,
                    value: 0,
                    index: 0,
                    data: &data,
                },
                self.usb_timeout,
            )
            .await?;

        Ok(())
    }

    pub async fn set_discharge(&mut self, enable: bool) -> Result<(), Error> {
        self.send_command(&Command::SetDischarge(enable)).await
    }

    pub async fn set_prescaler(&mut self, prescaler: Prescaler) -> Result<(), Error> {
        self.send_command(&Command::SetPrescaler(prescaler)).await
    }
}
