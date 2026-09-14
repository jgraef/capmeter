use crc::{
    Algorithm,
    CRC_16_USB,
    Crc,
};

pub type Checksum = u16;
pub const CRC_ALGORITHM: Algorithm<Checksum> = CRC_16_USB;
pub static CRC: Crc<Checksum> = Crc::<Checksum>::new(&CRC_ALGORITHM);
