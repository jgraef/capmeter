#![no_std]

use serde::{Deserialize, Serialize};

// test VID/PID from https://pid.codes/1209/0001/
pub const VENDOR_ID: u16 = 0x1209;
pub const PRODUCT_ID: u16 = 0x0002;

/// Assign USB request numbers to structs
pub trait Request {
    const REQUEST: u8;
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MeasureRequest {
    /// Timeout in ms
    pub timeout: u32,
    /// Resistance of charge resistor
    pub charge_resistor: u32,
}

impl Request for MeasureRequest {
    const REQUEST: u8 = 0x01;
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MeasureOutcome {
    pub charge_time: u32,
    pub capacity: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MeasureError {
    Timeout { timeout: u32 },
}
