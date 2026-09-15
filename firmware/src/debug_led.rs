use arduino_hal::{
    hal::port::PB5,
    port::{
        mode::{
            Io,
            Output,
        },
        Pin,
    },
};

use crate::global::Global;

static DEBUG_LED: Global<DebugLed> = Global::new();

pub fn install(debug_led: DebugLed) {
    DEBUG_LED.install(debug_led);
}

pub fn with<F, R>(f: F) -> R
where
    F: FnOnce(&mut DebugLed) -> R,
{
    DEBUG_LED.with(f)
}

pub struct DebugLed {
    pin: Pin<Output, PB5>,
}

impl DebugLed {
    pub fn new(pin: Pin<impl Io, PB5>) -> Self {
        let mut pin = pin.into_output();
        pin.set_low();

        Self { pin }
    }

    pub fn flash(&mut self, duration_ms: u32) {
        self.pin.set_high();
        arduino_hal::delay_ms(duration_ms);
        self.pin.set_low();
    }

    pub fn blink(&mut self, duration_on: u32, duration_off: u32) {
        self.flash(duration_on);
        arduino_hal::delay_ms(duration_off);
    }
}
