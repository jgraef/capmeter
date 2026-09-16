#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]

pub mod debug_led;
pub mod global;
pub mod mux;
mod panic;
mod print;
pub mod protocol;
pub mod serial;
pub mod time;

use core::task::Poll;

use arduino_hal::{
    adc::AdcChannel,
    hal::Atmega,
    pac::ADC,
    pins,
    port::{
        mode::{
            Analog,
            Floating,
            Input,
            Output,
        },
        Pin,
        PinOps,
    },
    Adc,
    Peripherals,
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
    time::{
        Duration,
        Instant,
    },
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

    // Setup time
    time::install(peripherals.TC0);

    // Enable interrupts globally
    unsafe { avr_device::interrupt::enable() };

    // open serial and initialize multiplexer
    let serial = Serial::new(peripherals.USART0, pins.d0, pins.d1, SERIAL_BAUD_RATE);
    let mux = Mux::new(serial);
    mux::install(mux);

    // debug LED
    debug_led::install(DebugLed::new(pins.d13));

    // setup measurement
    let mut adc = arduino_hal::Adc::new(peripherals.ADC, Default::default());
    let read_pin = pins.a0.into_analog_input(&mut adc);
    let mut meter = Meter {
        adc,
        read_pin,
        charge_pin: pins.d12.into_output(),
        discharge_pin: Some(pins.d11.into_floating_input()),
    };
    meter.charge_pin.set_low();

    // send bootup hello message
    protocol::send(&DeviceMessage::Hello(DeviceHello {
        boot: true,
        version: FIRMWARE_VERSION,
    }));

    // send hello via debug print
    println!(
        "Capacitor Meter v{}.{}.{}",
        FIRMWARE_VERSION.major, FIRMWARE_VERSION.minor, FIRMWARE_VERSION.patch
    );

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
                    ClientMessage::Measure {
                        timeout,
                        charge_resistor,
                    } => {
                        let capacity =
                            meter.measure(Duration::from_millis(timeout), charge_resistor);
                        protocol::send(&DeviceMessage::Measurement { capacity });
                    }
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

struct Meter<READ, CHARGE, DISCHARGE> {
    adc: Adc,
    read_pin: Pin<Analog, READ>,
    charge_pin: Pin<Output, CHARGE>,
    discharge_pin: Option<Pin<Input<Floating>, DISCHARGE>>,
}

impl<READ, CHARGE, DISCHARGE> Meter<READ, CHARGE, DISCHARGE>
where
    READ: PinOps,
    CHARGE: PinOps,
    DISCHARGE: PinOps,
    Pin<Analog, READ>: AdcChannel<Atmega, ADC>,
{
    /// Measures the capacitor
    ///
    /// Returns capacity in μF
    pub fn measure(&mut self, timeout: Duration, charge_resistor: u32) -> Result<u32, ()> {
        let charge_result = self.charge(timeout).map(|charge_time| {
            // calculate capacity in μF
            let capacity = charge_time.as_millis() * 1000 / charge_resistor;
            println!("Derived capacity: {} μF", capacity);
            capacity
        });

        // always discharge. ignore result
        let _ = self.discharge(timeout);

        charge_result
    }

    /// Charges capacitor
    ///
    /// Returns time it took to charge the capacitor.
    fn charge(&mut self, timeout: Duration) -> Result<Duration, ()> {
        let time_start = Instant::now();

        println!("Charging capacitor");
        self.charge_pin.set_high();

        // wait for voltage to reach 63.2 %
        let result = loop {
            // voltage in 5V / 1024
            let voltage = self.read_pin.analog_read(&mut self.adc);

            let now = Instant::now();
            let charge_time = now - time_start;

            // check if voltage is at or above 63.2 %
            if voltage >= 647 {
                println!(
                    "Took {} ms to charge capacitor to 63.2 %",
                    charge_time.as_millis()
                );

                break Ok(charge_time);
            }

            if charge_time > timeout {
                println!("Timeout while charging after {} s", timeout.as_secs());
                return Err(());
            }
        };

        self.charge_pin.set_low();

        result
    }

    /// Discharges capacitor
    ///
    /// Returns time it took to discharge the capacitor.
    fn discharge(&mut self, timeout: Duration) -> Result<Duration, ()> {
        println!("Discharging capacitor");
        self.charge_pin.set_low();
        let discharge_pin = self.discharge_pin.take().unwrap();
        let mut discharge_pin = discharge_pin.into_output();
        discharge_pin.set_low();

        let time_start = Instant::now();

        let result = loop {
            let voltage = self.read_pin.analog_read(&mut self.adc);

            let now = Instant::now();
            let discharge_time = now - time_start;

            if voltage == 0 {
                break Ok(discharge_time);
            }

            if discharge_time > timeout {
                println!("Timeout while discharging after {} s", timeout.as_secs());
                break Err(());
            }
        };

        // restore pin
        let discharge_pin = discharge_pin.into_floating_input();
        self.discharge_pin = Some(discharge_pin);

        result
    }
}
