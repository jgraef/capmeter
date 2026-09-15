#![no_std]

pub mod checksum;

use serde::{
    Deserialize,
    Serialize,
};

pub type Port = u16;

pub const PROTOCOL_PORT: Port = 1;
pub const DEBUG_PORT: Port = 2;
pub const DEFAULT_MTU: usize = 0x100;
pub const HEADER_LENGTH: usize = 6;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ufmt", derive(ufmt::derive::uDebug))]
pub enum DeviceMessage {
    Hello(DeviceHello),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ufmt", derive(ufmt::derive::uDebug))]
pub struct DeviceHello {
    pub boot: bool,
    pub version: Version,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ufmt", derive(ufmt::derive::uDebug))]
pub enum ClientMessage {
    Hello(ClientHello),
    Measure {
        // todo
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ufmt", derive(ufmt::derive::uDebug))]
pub struct ClientHello {
    pub version: Version,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ufmt", derive(ufmt::derive::uDebug))]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}
