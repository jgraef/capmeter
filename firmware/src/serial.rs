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

    pub fn peek(&mut self) -> Option<u8> {
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

    pub fn skip(&mut self, mut count: usize) {
        if count == 0 {
            return;
        }

        if self.peeked.take().is_some() {
            count -= 1;
        }

        while count > 0 {
            match self.usart.read() {
                Ok(_) => {
                    count -= 1;
                }
                Err(nb::Error::WouldBlock) => {
                    // keep polling
                }
            }
        }
    }
}

impl Read for Serial {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if buf.len() == 0 {
            return Ok(0);
        }

        let mut position = 0;
        let mut already_read_something = false;

        // take any byte we peeked first
        if let Some(peeked) = self.peeked.take() {
            buf[position] = peeked;
            position += 1;
        }

        loop {
            match self.usart.read() {
                Ok(byte) => {
                    buf[position] = byte;
                    position += 1;
                    already_read_something = true;
                }
                Err(nb::Error::WouldBlock) => {
                    // if we already read something, return now. otherwise keep
                    // polling
                    if already_read_something {
                        break;
                    }
                }
            }
        }

        Ok(position)
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
