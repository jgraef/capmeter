use std::time::Duration;

use nusb::{
    DeviceSelector,
    transfer::{ControlIn, ControlOut, ControlType, Recipient},
};
use protocol::{MeasureError, MeasureOutcome, Request};

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

    #[error("Measurement error")]
    MeasureError(MeasureError),
}

impl From<MeasureError> for Error {
    fn from(value: MeasureError) -> Self {
        Self::MeasureError(value)
    }
}

#[derive(Debug)]
pub struct Client {
    usb_interface: nusb::Interface,
    usb_timeout: Duration,
    poll_interval: Duration,
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
            usb_timeout: Duration::from_millis(100),
            poll_interval: Duration::from_millis(250),
        })
    }

    pub async fn measure(
        &mut self,
        charge_resistor: u32,
        timeout: Duration,
    ) -> Result<MeasureOutcome, Error> {
        let request = protocol::MeasureRequest {
            timeout: timeout.as_millis().try_into().unwrap_or(u32::MAX),
            charge_resistor,
        };
        let data = postcard::to_allocvec(&request)?;

        // start measurement
        self.usb_interface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Interface,
                    request: protocol::MeasureRequest::REQUEST,
                    value: 0,
                    index: 0,
                    data: &data,
                },
                self.usb_timeout,
            )
            .await?;

        // poll for result
        let mut interval = tokio::time::interval(self.poll_interval);
        loop {
            interval.tick().await;

            let data = self
                .usb_interface
                .control_in(
                    ControlIn {
                        control_type: ControlType::Vendor,
                        recipient: Recipient::Interface,
                        request: protocol::MeasureRequest::REQUEST,
                        value: 0,
                        index: 0,
                        length: 1024, // todo: make an option or constant
                    },
                    self.usb_timeout,
                )
                .await?;

            let response: Option<Result<MeasureOutcome, MeasureError>> =
                postcard::from_bytes(&data)?;

            if let Some(result) = response {
                return result.map_err(Into::into);
            }
        }
    }
}
