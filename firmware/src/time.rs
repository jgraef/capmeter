/*!
 * A basic implementation of the `millis()` function from Arduino:
 *
 *     https://www.arduino.cc/reference/en/language/functions/time/millis/
 *
 * Uses timer TC0 and one of its interrupts to update a global millisecond
 * counter.  A walkthough of this code is available here:
 *
 *     https://blog.rahix.de/005-avr-hal-millis/
 */

use core::{
    cell,
    ops::{
        Add,
        AddAssign,
        Sub,
        SubAssign,
    },
};

use ufmt::derive::uDebug;

// Possible Values:
//
// ╔═══════════╦══════════════╦═══════════════════╗
// ║ PRESCALER ║ TIMER_COUNTS ║ Overflow Interval ║
// ╠═══════════╬══════════════╬═══════════════════╣
// ║        64 ║          250 ║              1 ms ║
// ║       256 ║          125 ║              2 ms ║
// ║       256 ║          250 ║              4 ms ║
// ║      1024 ║          125 ║              8 ms ║
// ║      1024 ║          250 ║             16 ms ║
// ╚═══════════╩══════════════╩═══════════════════╝
const PRESCALER: u32 = 1024;
const TIMER_COUNTS: u32 = 125;

const MILLIS_INCREMENT: u32 = PRESCALER * TIMER_COUNTS / 16000;

static MILLIS_COUNTER: avr_device::interrupt::Mutex<cell::Cell<u32>> =
    avr_device::interrupt::Mutex::new(cell::Cell::new(0));

pub fn install(tc0: arduino_hal::pac::TC0) {
    // Configure the timer for the above interval (in CTC mode)
    // and enable its interrupt.
    tc0.tccr0a().write(|w| w.wgm0().ctc());
    tc0.ocr0a().write(|w| w.set(TIMER_COUNTS as u8));
    tc0.tccr0b().write(|w| {
        match PRESCALER {
            8 => w.cs0().prescale_8(),
            64 => w.cs0().prescale_64(),
            256 => w.cs0().prescale_256(),
            1024 => w.cs0().prescale_1024(),
            _ => panic!(),
        }
    });
    tc0.timsk0().write(|w| w.ocie0a().set_bit());

    // Reset the global millisecond counter
    avr_device::interrupt::free(|cs| {
        MILLIS_COUNTER.borrow(cs).set(0);
    });
}

#[avr_device::interrupt(atmega328p)]
fn TIMER0_COMPA() {
    avr_device::interrupt::free(|cs| {
        let counter_cell = MILLIS_COUNTER.borrow(cs);
        let counter = counter_cell.get();
        counter_cell.set(counter + MILLIS_INCREMENT);
    })
}

fn millis() -> u32 {
    avr_device::interrupt::free(|cs| MILLIS_COUNTER.borrow(cs).get())
}

#[derive(Clone, Copy, uDebug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    millis: u32,
}

impl Instant {
    pub fn now() -> Self {
        Self { millis: millis() }
    }

    pub fn elapsed(&self) -> Duration {
        Instant::now() - *self
    }

    pub fn install() -> Self {
        Self { millis: 0 }
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, rhs: Duration) -> Self::Output {
        Self {
            millis: self.millis + rhs.millis,
        }
    }
}

impl AddAssign<Duration> for Instant {
    fn add_assign(&mut self, rhs: Duration) {
        self.millis += rhs.millis
    }
}

impl Sub for Instant {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        Duration {
            millis: self.millis - rhs.millis,
        }
    }
}

impl Sub<Duration> for Instant {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {
        Self {
            millis: self.millis - rhs.millis,
        }
    }
}

impl SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, rhs: Duration) {
        self.millis -= rhs.millis
    }
}

#[derive(Clone, Copy, uDebug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Duration {
    millis: u32,
}

impl Duration {
    pub fn from_secs(seconds: u32) -> Self {
        Self {
            millis: seconds * 1000,
        }
    }

    pub fn from_millis(millis: u32) -> Self {
        Self { millis }
    }

    pub fn as_millis(&self) -> u32 {
        self.millis
    }

    pub fn as_secs(&self) -> u32 {
        self.millis / 1000
    }
}
