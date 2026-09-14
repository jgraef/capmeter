#![no_std]
#![no_main]

pub mod mux;
mod print;
pub mod serial;

use core::task::{
    ready,
    Poll,
};

use arduino_hal::{
    pins,
    port::{
        mode::{
            Analog,
            Floating,
            Input,
            Output,
        },
        Pin,
    },
    Peripherals,
};
use embedded_io::Write;
use panic_halt as _;
use protocol::{
    ClientMessage,
    DeviceMessage,
    Version,
    DEBUG_PORT,
    PROTOCOL_PORT,
};

use crate::{
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
    let mut debug_led = pins.d13.into_output();
    debug_led.set_high();

    // send hello via debug print
    println!("Capacitor Meter v0.0.1");

    // send hello message
    mux::with(|mux| {
        mux.send_message(
            PROTOCOL_PORT,
            &DeviceMessage::Hello {
                version: FIRMWARE_VERSION,
            },
        );
    });

    let mut adc = arduino_hal::Adc::new(peripherals.ADC, Default::default());
    let mut meter = Meter {
        pin_read: pins.a0.into_analog_input(&mut adc),
        pin_charge: pins.d12.into_output(),
        pin_discharge: pins.d11.into_floating_input(),
    };

    loop {
        match poll_client_message() {
            Poll::Pending => {}
            Poll::Ready(None) => break,
            Poll::Ready(Some(message)) => {
                println!("Received protocol message: {:?}", message);
            }
        }

        //debug_led.toggle();
        //arduino_hal::delay_ms(50);
    }

    // serial closed? let's just wait until reset here
    loop {}
}

fn poll_client_message() -> Poll<Option<ClientMessage>> {
    mux::with(|mux| {
        match mux.poll_receive() {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(chunk)) => {
                match chunk.port {
                    PROTOCOL_PORT => {
                        match chunk.deserialize::<ClientMessage>() {
                            Ok(message) => Poll::Ready(Some(message)),
                            Err(_error) => {
                                eprintln!(mux, "Error during deserialization");
                                Poll::Pending
                            }
                        }
                    }
                    _ => {
                        // ignore unexpected port.
                        let port = chunk.port;
                        eprintln!(mux, "Message on unexpected port: {}", port);
                        Poll::Pending
                    }
                }
            }
        }
    })
}

struct Meter<READ, CHARGE, DISCHARGE> {
    pin_read: Pin<Analog, READ>,
    pin_charge: Pin<Output, CHARGE>,
    pin_discharge: Pin<Input<Floating>, DISCHARGE>,
}
