use core::convert::Infallible;

use arduino_hal::{
    hal::{
        port::{
            PD0,
            PD1,
        },
        usart::BaudrateArduinoExt,
    },
    pac::USART0,
    port::{
        mode::Io,
        Pin,
    },
    prelude::_embedded_hal_serial_Read,
};
use embedded_io::{
    ErrorType,
    Read,
    ReadReady,
    Write,
};

/// Wrapper that implements embedded-io traits for interop with postcard.
pub struct Serial {
    usart: arduino_hal::hal::usart::Usart0<arduino_hal::DefaultClock>,
    peeked: Option<u8>,
}

impl Serial {
    pub fn new(
        usart: USART0,
        rx: Pin<impl Io, PD0>,
        tx: Pin<impl Io, PD1>,
        baud_rate: impl BaudrateArduinoExt,
    ) -> Self {
        let rx = rx.into_floating_input();
        let tx = tx.into_output();
        let usart = arduino_hal::Usart::new(usart, rx, tx, baud_rate.into_baudrate());

        Self {
            usart,
            peeked: None,
        }
    }

    fn peek(&mut self) -> Option<u8> {
        if self.peeked.is_none() {
            match self.usart.read() {
                Ok(byte) => {
                    self.peeked = Some(byte);
                }
                Err(nb::Error::WouldBlock) => {}
            }
        }

        self.peeked
    }
}

impl Read for Serial {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if buf.len() == 0 {
            return Ok(0);
        }

        let mut num_read = 0;

        // take any byte we peeked first
        if let Some(peeked) = self.peeked.take() {
            buf[0] = peeked;
            num_read = 1;
        }

        loop {
            match self.usart.read() {
                Ok(byte) => {
                    buf[num_read] = byte;
                    num_read += 1;
                }
                Err(nb::Error::WouldBlock) => {
                    // if we already read something, return now. otherwise keep
                    // polling
                    if num_read > 0 {
                        break;
                    }
                }
            }
        }

        Ok(num_read)
    }
}

impl ReadReady for Serial {
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        Ok(self.peek().is_some())
    }
}

impl Write for Serial {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        for byte in buf.iter() {
            self.usart.write_byte(*byte);
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // nop
        Ok(())
    }
}

impl ErrorType for Serial {
    type Error = Infallible;
}
