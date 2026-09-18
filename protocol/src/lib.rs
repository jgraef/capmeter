#![no_std]

use serde::{Deserialize, Serialize};

// test VID/PID from https://pid.codes/1209/0001/
pub const VENDOR_ID: u16 = 0x1209;
pub const PRODUCT_ID: u16 = 0x0002;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Command {
    SetDischarge(bool),
    SetPrescaler(Prescaler),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Measurement {
    /// Serial number to distinguish measurements
    pub serial: u32,

    /// What prescaler is set
    pub prescaler: Prescaler,

    /// Whether the capacitor is being discharged
    pub discharge: bool,

    /// Period of two rising edges in μs
    pub period: u32,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Prescaler {
    #[default]
    Div1 = 0b00,
    Div2 = 0b01,
    Div4 = 0b10,
    Div8 = 0b11,
}

impl Prescaler {
    pub fn divisor(&self) -> u8 {
        match self {
            Prescaler::Div1 => 1,
            Prescaler::Div2 => 2,
            Prescaler::Div4 => 4,
            Prescaler::Div8 => 8,
        }
    }

    pub fn from_divisor(value: u8) -> Result<Self, InvalidPrescaler> {
        match value {
            1 => Ok(Self::Div1),
            2 => Ok(Self::Div2),
            4 => Ok(Self::Div4),
            8 => Ok(Self::Div8),
            _ => Err(InvalidPrescaler { prescaler: value }),
        }
    }

    pub fn ic1psc(&self) -> u8 {
        *self as u8
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Only prescaler values 1, 2, 4, and 8 are allowed")]
pub struct InvalidPrescaler {
    pub prescaler: u8,
}
