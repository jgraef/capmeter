use core::{
    convert::Infallible,
    ops::{
        Deref,
        DerefMut,
    },
    task::Poll,
};

use arduino_hal::prelude::_unwrap_infallible_UnwrapInfallible;
use embedded_io::{
    ErrorType,
    Read,
    ReadExactError,
    ReadReady,
    Write,
};
use heapless::{
    Vec,
    VecView,
};
use protocol::{
    checksum::CRC,
    Port,
    DEFAULT_MTU,
    HEADER_LENGTH,
};
use serde::{
    de::DeserializeOwned,
    Serialize,
};
use ufmt::uWrite;

use crate::{
    eprintln,
    global::Global,
    serial::Serial,
};

const BUFFER_SIZE: usize = DEFAULT_MTU + HEADER_LENGTH;

static GLOBAL_MUX: Global<Mux> = Global::new();

pub fn install(mux: Mux) {
    GLOBAL_MUX.install(mux);
}

pub fn with<F, R>(f: F) -> R
where
    F: FnOnce(&mut Mux) -> R,
{
    GLOBAL_MUX.with(f)
}

pub struct Mux {
    serial: Serial,
    buffer: Vec<u8, BUFFER_SIZE>,
}

impl Mux {
    #[inline(always)]
    pub fn new(serial: Serial) -> Self {
        Self {
            serial,
            buffer: Vec::new(),
        }
    }

    pub fn poll_receive(&mut self) -> Poll<Option<Chunk<'_>>> {
        // check if there is any data to be read
        if !self.serial.read_ready().unwrap_infallible() {
            return Poll::Pending;
        }

        let mut digest = CRC.digest();

        // read header
        self.buffer.resize_default(HEADER_LENGTH).unwrap();
        if let Err(ReadExactError::UnexpectedEof) = self.serial.read_exact(&mut self.buffer) {
            // EOF. Serial connection closed? Exit.
            return Poll::Ready(None);
        }

        let port = u16::from_be_bytes(self.buffer[0..2].try_into().unwrap());
        let chunk_length = u16::from_be_bytes(self.buffer[2..4].try_into().unwrap());
        let checksum = u16::from_be_bytes(self.buffer[4..6].try_into().unwrap());

        // clear out remote checksum for local checksum calculation
        self.buffer[4..6].fill(0);
        // add header to local checksum
        digest.update(&self.buffer);

        eprintln!(
            self,
            "port={}, chunk_length={}, checksum={:02x}", port, chunk_length, checksum
        );

        // resize buffer for data
        let chunk_length: usize = chunk_length.into();
        if let Err(_error) = self.buffer.resize_default(chunk_length) {
            eprintln!(self, "Received chunk that is too large: {}", chunk_length);

            // Chunk is too large for us. Skip it
            self.serial.skip(chunk_length);

            return Poll::Pending;
        }

        // receive data
        if let Err(ReadExactError::UnexpectedEof) = self.serial.read_exact(&mut self.buffer) {
            // EOF. There would be no way to recover the stream, so exit.
            return Poll::Ready(None);
        }

        digest.update(&self.buffer);

        // verify checksum
        let local_checksum = digest.finalize();

        if local_checksum != checksum {
            // invalid checksum. ignore chunk.
            eprintln!(
                self,
                "Chunk with invalid checksum: local={:02x}, remote={:02x}",
                local_checksum,
                checksum
            );

            return Poll::Pending;
        }

        Poll::Ready(Some(Chunk {
            port,
            data: &self.buffer,
        }))
    }

    pub fn send_chunk(&mut self, port: Port) -> SendChunk<'_> {
        self.buffer.resize_default(HEADER_LENGTH).unwrap();

        // set port
        self.buffer[0..2].copy_from_slice(&port.to_be_bytes());

        SendChunk {
            serial: &mut self.serial,
            buffer: self.buffer.as_mut_view(),
        }
    }

    pub fn send_message<T>(&mut self, port: Port, message: &T)
    where
        T: Serialize,
    {
        let mut send = self.send_chunk(port);
        postcard::to_extend(message, &mut send).expect("serialization error");
        send.finish();
    }

    #[inline(always)]
    pub fn writer(&mut self, port: Port) -> Writer<'_> {
        Writer {
            send: self.send_chunk(port),
        }
    }
}

pub struct Chunk<'a> {
    pub port: Port,
    pub data: &'a [u8],
}

impl<'a> Chunk<'a> {
    #[inline(always)]
    pub fn deserialize<T>(&self) -> Result<T, postcard::Error>
    where
        T: DeserializeOwned,
    {
        postcard::from_bytes(&self.data)
    }
}

pub struct SendChunk<'a> {
    serial: &'a mut Serial,
    buffer: &'a mut VecView<u8>,
}

impl<'a> SendChunk<'a> {
    #[inline(always)]
    pub fn put_slice(&mut self, slice: &[u8]) {
        self.buffer.extend_from_slice(slice).unwrap();
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.buffer.len() - HEADER_LENGTH
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.buffer.resize_default(HEADER_LENGTH).unwrap();
    }

    #[inline(always)]
    pub fn truncate(&mut self, mut new_length: usize) {
        new_length += HEADER_LENGTH;
        if new_length < self.buffer.len() {
            self.buffer.resize_default(new_length).unwrap();
        }
    }

    #[inline(always)]
    pub fn remaining(&self) -> usize {
        self.buffer.capacity() - self.len()
    }

    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.buffer.is_full()
    }

    pub fn flush(&mut self) {
        // set chunk length
        let chunk_length = self.len();
        if chunk_length > DEFAULT_MTU {
            // todo: log error
            panic!("mtu overflow");
        }
        let chunk_length: u16 = chunk_length.try_into().unwrap();
        self.buffer[2..4].copy_from_slice(&chunk_length.to_be_bytes());
        self.buffer[4..6].fill(0);

        // calculate checksum
        let mut digest = CRC.digest();
        digest.update(&self.buffer);
        let checksum = digest.finalize();
        self.buffer[4..6].copy_from_slice(&checksum.to_be_bytes());

        // send chunk
        self.serial.write_all(&self.buffer);

        self.clear();
    }

    #[inline(always)]
    pub fn finish(mut self) {
        self.flush();
    }
}

impl<'a> AsRef<[u8]> for SendChunk<'a> {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.buffer
    }
}

impl<'a> AsMut<[u8]> for SendChunk<'a> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }
}

impl<'a> Deref for SendChunk<'a> {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl<'a> DerefMut for SendChunk<'a> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl<'a> Extend<u8> for SendChunk<'a> {
    #[inline(always)]
    fn extend<T: IntoIterator<Item = u8>>(&mut self, iter: T) {
        self.buffer.extend(iter);
    }
}

impl<'a> Extend<u8> for &mut SendChunk<'a> {
    #[inline(always)]
    fn extend<T: IntoIterator<Item = u8>>(&mut self, iter: T) {
        self.buffer.extend(iter);
    }
}

pub struct Writer<'a> {
    send: SendChunk<'a>,
}

impl<'a> ErrorType for Writer<'a> {
    type Error = Infallible;
}

impl<'a> embedded_io::Write for Writer<'a> {
    fn write(&mut self, data: &[u8]) -> Result<usize, Self::Error> {
        if self.send.is_full() {
            self.send.flush();
        }

        let remaining = self.send.remaining();
        let n = data.len().min(remaining);
        self.send.put_slice(&data[..n]);

        Ok(n)
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<(), Self::Error> {
        self.send.flush();
        Ok(())
    }
}

impl<'a> uWrite for Writer<'a> {
    type Error = Infallible;

    #[inline(always)]
    fn write_str(&mut self, s: &str) -> Result<(), Self::Error> {
        self.write_all(s.as_bytes())
    }
}

impl<'a> Drop for Writer<'a> {
    #[inline(always)]
    fn drop(&mut self) {
        self.send.flush();
    }
}
