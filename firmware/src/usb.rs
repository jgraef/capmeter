use embassy_stm32::{
    Peri,
    bind_interrupts,
    peripherals::{
        PA11,
        PA12,
        USB_OTG_FS,
    },
    usb::{
        Driver,
        InterruptHandler,
    },
};
use embassy_usb::{
    Builder,
    Handler,
    control::{
        InResponse,
        OutResponse,
        Recipient,
        Request,
        RequestType,
    },
    msos::{
        self,
        windows_version,
    },
    types::InterfaceNumber,
};
use protocol::Command;

use crate::channel;

pub mod info {
    pub use protocol::{
        PRODUCT_ID,
        VENDOR_ID,
    };

    pub const MANUFACTURER_NAME: &'static str = "switch";
    pub const PRODUCT_NAME: &'static str = "capmeter";

    pub const DEVICE_INTERFACE_GUIDS: &[&str] = &["{E755224F-B69E-4BAE-ABED-6F104BE4D72E}"];
}

// Note: We have to pick a number below the max endpoint count supported by the
// USB_OTG_FS peripheral. For STM32F401 this is 4.
//
// see https://github.com/embassy-rs/embassy/blob/822a76ea91f2c0885c6eb0c65fe91181e31133e1/embassy-stm32/src/usb/otg.rs#L539-L850
pub const BULK_ENDPOINT_INDEX: u8 = 0x01;
pub const BULK_MAX_PACKET_SIZE: usize = 0x200;

#[allow(non_snake_case)]
pub struct Peripherals {
    pub USB_OTG_FS: Peri<'static, USB_OTG_FS>,
    pub PA12: Peri<'static, PA12>,
    pub PA11: Peri<'static, PA11>,
}

bind_interrupts!(struct Irqs {
    OTG_FS => InterruptHandler<USB_OTG_FS>;
});

#[embassy_executor::task]
pub async fn run(peripherals: Peripherals, channel: channel::UsbSide) {
    let mut ep_out_buffer = [0u8; 256];
    let mut config = embassy_stm32::usb::Config::default();
    config.vbus_detection = false;

    let driver = Driver::new_fs(
        peripherals.USB_OTG_FS,
        peripherals.PA12,
        peripherals.PA11,
        Irqs,
        &mut ep_out_buffer,
        config,
    );

    // device info
    let mut config = embassy_usb::Config::new(info::VENDOR_ID, info::PRODUCT_ID);
    config.manufacturer = Some(info::MANUFACTURER_NAME);
    config.product = Some(info::PRODUCT_NAME);
    config.serial_number = Some("00000001"); // todo

    // buffers
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut msos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    // needs to be created before the builder
    let mut handler = ControlHandler {
        interface: InterfaceNumber(0),
        channel,
    };

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut msos_descriptor,
        &mut control_buf,
    );

    // Add the Microsoft OS Descriptor (MSOS/MOD) descriptor.
    builder.msos_descriptor(windows_version::WIN8_1, 0);
    builder.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
    builder.msos_feature(msos::RegistryPropertyFeatureDescriptor::new(
        "DeviceInterfaceGUIDs",
        msos::PropertyData::RegMultiSz(info::DEVICE_INTERFACE_GUIDS),
    ));

    // vendor-specific function
    let mut function = builder.function(0xff, 0, 0);
    let mut interface = function.interface();
    let _alternate = interface.alt_setting(0xff, 0, 0, None);
    handler.interface = interface.interface_number();
    drop(function);

    builder.handler(&mut handler);

    let mut usb = builder.build();

    usb.run().await;
}

struct ControlHandler {
    interface: InterfaceNumber,
    channel: channel::UsbSide,
}

impl ControlHandler {
    fn filter_request(&self, request: &Request) -> bool {
        let accept = request.request_type == RequestType::Vendor
            && request.recipient == Recipient::Interface
            && request.index == self.interface.0 as u16
            && request.request == 0;

        if accept {
            defmt::trace!("accepted control request: {:?}", request);
        }
        else {
            defmt::debug!("rejected control request: {:?}", request);
        }

        accept
    }
}

impl Handler for ControlHandler {
    fn enabled(&mut self, enabled: bool) {
        if enabled {
            defmt::debug!("USB device enabled");
        }
        else {
            defmt::debug!("USB device disabled");
        }
    }

    fn reset(&mut self) {
        defmt::debug!("USB device reset");
    }

    fn addressed(&mut self, address: u8) {
        defmt::debug!("USB device address set: {=u8:#02x}", address);
    }

    fn configured(&mut self, configured: bool) {
        if configured {
            defmt::debug!("USB device configuration enabled");
        }
        else {
            defmt::debug!("USB device configuration disabled");
        }
    }

    fn suspended(&mut self, suspended: bool) {
        if suspended {
            defmt::debug!("USB bus entered suspended state");
        }
        else {
            defmt::debug!("USB bus left suspended state");
        }
    }

    fn set_alternate_setting(&mut self, interface: InterfaceNumber, alternate_setting: u8) {
        defmt::debug!(
            "USB device set alternate setting: interface={=u8:#02x}, alternate_setting={=u8:#02x}",
            interface.0,
            alternate_setting,
        );
    }

    fn control_in<'a>(
        &'a mut self,
        request: Request,
        buffer: &'a mut [u8],
    ) -> Option<InResponse<'a>> {
        if self.filter_request(&request) {
            match request.request {
                0 => {
                    let measurement = self.channel.read_measurement();

                    match postcard::to_slice(&measurement, buffer) {
                        Ok(data) => Some(InResponse::Accepted(data)),
                        Err(error) => {
                            defmt::error!(
                                "Failed to serialize response: {:?}: {}",
                                measurement,
                                error
                            );
                            Some(InResponse::Rejected)
                        }
                    }
                }
                _ => Some(InResponse::Rejected),
            }
        }
        else {
            None
        }
    }

    fn control_out(&mut self, request: Request, data: &[u8]) -> Option<OutResponse> {
        if self.filter_request(&request) {
            match request.request {
                0 => {
                    match postcard::from_bytes::<Command>(data) {
                        Ok(command) => {
                            defmt::debug!("Control command: {:?}", command);

                            match self.channel.send_command(command) {
                                Ok(()) => Some(OutResponse::Accepted),
                                Err(_error) => {
                                    defmt::error!("channel full");
                                    Some(OutResponse::Rejected)
                                }
                            }
                        }
                        Err(error) => {
                            defmt::error!(
                                "Could not deserialize request: request={=u8:#02x}: {}",
                                request.request,
                                error,
                            );

                            Some(OutResponse::Rejected)
                        }
                    }
                }
                _ => Some(OutResponse::Rejected),
            }
        }
        else {
            None
        }
    }
}
