#![no_std]

use serde::{Deserialize, Serialize};

// test VID/PID from https://pid.codes/1209/0001/
pub const VENDOR_ID: u16 = 0x1209;
pub const PRODUCT_ID: u16 = 0x0002;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Command {
    SetDischarge(bool),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Measurement {
    /// Serial number to distinguish measurements
    pub serial: u32,

    /// Period of two rising edges in μs
    pub period: u32,
}
