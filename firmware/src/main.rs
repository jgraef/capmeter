#![no_std]
#![no_main]

pub mod debug_led;
pub mod global;
pub mod mux;
mod panic;
mod print;
pub mod protocol;
pub mod serial;

use core::task::Poll;

use ::protocol::checksum::CRC;
use arduino_hal::{
    pins,
    Peripherals,
};
use crc::{
    Crc,
    CRC_16_USB,
};
use protocol::{
    ClientMessage,
    DeviceHello,
    DeviceMessage,
    Version,
};

use crate::{
    debug_led::DebugLed,
    mux::Mux,
    serial::Serial,
};

const SERIAL_BAUD_RATE: u32 = 57600;

// todo: read from manifest
const FIRMWARE_VERSION: Version = Version {
    major: 0,
    minor: 1,
    patch: 0,
};

#[arduino_hal::entry]
fn main() -> ! {
    let peripherals = Peripherals::take().unwrap();
    let pins = pins!(peripherals);

    // open serial and initialize multiplexer
    let serial = Serial::new(peripherals.USART0, pins.d0, pins.d1, SERIAL_BAUD_RATE);
    let mux = Mux::new(serial);
    mux::install(mux);

    // debug LED
    debug_led::install(DebugLed::new(pins.d13));

    // send bootup hello message
    protocol::send(&DeviceMessage::Hello(DeviceHello {
        boot: true,
        version: FIRMWARE_VERSION,
    }));

    // send hello via debug print
    println!("Capacitor Meter v0.0.1",);

    {
        println!("crc table size: {}", &core::mem::size_of_val(&CRC));
        let mut digest = CRC.digest();

        digest.update(b"\x00\x01\x00\x05\x00\x00\x00\x01\x00\x01\x00");

        let checksum = digest.finalize();
        println!("checksum = {} (dec), {:04x} (hex)", checksum, checksum);
    }

    /*let mut adc = arduino_hal::Adc::new(peripherals.ADC, Default::default());
    let mut meter = Meter {
        pin_read: pins.a0.into_analog_input(&mut adc),
        pin_charge: pins.d12.into_output(),
        pin_discharge: pins.d11.into_floating_input(),
    };*/

    loop {
        match protocol::receive() {
            Poll::Pending => {}
            Poll::Ready(None) => break,
            Poll::Ready(Some(message)) => {
                println!("Received protocol message: {:?}", message);

                match message {
                    ClientMessage::Hello(_client_hello) => {
                        // when a new client says hello, we say it back!
                        protocol::send(&DeviceMessage::Hello(DeviceHello {
                            boot: false,
                            version: FIRMWARE_VERSION,
                        }));
                    }
                    ClientMessage::Measure {} => todo!(),
                }
            }
        }
    }

    // serial closed? let's just wait until reset here
    debug_led::with(|debug_led| {
        loop {
            // blink so we know the program stopped
            debug_led.blink(200, 200);
        }
    })
}

/*struct Meter<READ, CHARGE, DISCHARGE> {
    pin_read: Pin<Analog, READ>,
    pin_charge: Pin<Output, CHARGE>,
    pin_discharge: Pin<Input<Floating>, DISCHARGE>,
}*/
