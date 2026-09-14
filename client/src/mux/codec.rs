use bytes::{
    Buf,
    BufMut,
    Bytes,
    BytesMut,
};
use crc::Digest;
use protocol::{
    HEADER_LENGTH,
    checksum::{
        CRC,
        Checksum,
    },
};
use tokio_util::codec::{
    Decoder,
    Encoder,
};

use crate::mux::Error;

#[derive(Debug)]
pub struct Chunk {
    pub port: u16,
    pub data: Bytes,
}

#[derive(Debug)]
pub struct ChunkedDecoder {
    state: ChunkedDecoderState,
}

#[derive(Clone, derive_more::Debug)]
enum ChunkedDecoderState {
    ReadHeader,
    ReadBody {
        port: u16,
        chunk_length: usize,
        checksum: u16,
        #[debug(skip)]
        digest: Digest<'static, u16>,
    },
}

impl Default for ChunkedDecoder {
    fn default() -> Self {
        Self {
            state: ChunkedDecoderState::ReadHeader,
        }
    }
}

impl Decoder for ChunkedDecoder {
    type Error = Error;
    type Item = Chunk;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        //tracing::debug!(src = ?&src[..], "decoding chunk");

        loop {
            match self.state.clone() {
                ChunkedDecoderState::ReadHeader => {
                    if src.len() < HEADER_LENGTH {
                        return Ok(None);
                    }

                    let mut src = CrcBuf::new(&mut *src);

                    let port = src.get_u16();
                    let chunk_length = src.get_u16();

                    // don't include this in the checksum
                    // todo: we might be able to just include it. then the
                    // resulting checksum should be 0 if it's correct, right?
                    let checksum = src.inner.get_u16();
                    src.digest.update(&[0, 0]);

                    //tracing::debug!(?port, chunk_length, checksum, "decoded
                    // header");

                    self.state = ChunkedDecoderState::ReadBody {
                        port,
                        chunk_length: chunk_length.into(),
                        checksum,
                        digest: src.digest,
                    };
                }
                ChunkedDecoderState::ReadBody {
                    port,
                    chunk_length,
                    checksum,
                    mut digest,
                } => {
                    if src.len() < chunk_length {
                        return Ok(None);
                    }

                    let data = src.split_to(chunk_length).freeze();
                    digest.update(&data);
                    let local_checksum = digest.clone().finalize();

                    // no matter if we fail checksum verification or not, we
                    // will reset the state.
                    self.state = ChunkedDecoderState::ReadHeader;

                    // verify checksum

                    if local_checksum != checksum {
                        return Err(Error::InvalidChunkChecksum {
                            port,
                            local_checksum,
                            remote_checksum: checksum,
                        });
                    }

                    return Ok(Some(Chunk { port, data }));
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct ChunkedEncoder;

impl Encoder<Chunk> for ChunkedEncoder {
    type Error = Error;

    fn encode(&mut self, chunk: Chunk, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let chunk_length: u16 = chunk.data.len().try_into().expect("chunk too large");

        let mut scratch = [0; HEADER_LENGTH];
        let mut dst_crc = CrcBuf::new(&mut scratch[..]);

        dst_crc.put_u16(chunk.port);
        dst_crc.put_u16(chunk_length);
        dst_crc.put_u16(0); // placeholder for crc
        dst_crc.put(chunk.data);

        let checksum = dst_crc.digest.finalize();

        dst.put_slice(&scratch);
        (&mut dst[4..6]).put_u16(checksum);

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ChunkedCodec {
    pub decoder: ChunkedDecoder,
    pub encoder: ChunkedEncoder,
}

impl Decoder for ChunkedCodec {
    type Item = Chunk;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.decoder.decode(src)
    }
}

impl Encoder<Chunk> for ChunkedCodec {
    type Error = Error;

    fn encode(&mut self, item: Chunk, dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.encoder.encode(item, dst)
    }
}

pub struct CrcBuf<B> {
    pub inner: B,
    pub digest: Digest<'static, Checksum>,
}

impl<B> CrcBuf<B> {
    pub fn new(inner: B) -> Self {
        Self {
            inner,
            digest: CRC.digest(),
        }
    }
}

impl<B> Buf for CrcBuf<B>
where
    B: Buf,
{
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }

    fn chunk(&self) -> &[u8] {
        self.inner.chunk()
    }

    fn advance(&mut self, cnt: usize) {
        let chunk = self.inner.chunk();
        self.digest.update(&chunk[..cnt]);
        self.inner.advance(cnt);
    }
}

unsafe impl<B> BufMut for CrcBuf<B>
where
    B: BufMut,
{
    fn remaining_mut(&self) -> usize {
        self.inner.remaining_mut()
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        let chunk = self.inner.chunk_mut();

        let data = unsafe {
            // From the contract of BufMut:
            //
            // The caller must ensure that the next cnt bytes of chunk are
            // initialized.
            chunk[..cnt].as_uninit_slice_mut().assume_init_ref()
        };

        self.digest.update(data);
    }

    fn chunk_mut(&mut self) -> &mut bytes::buf::UninitSlice {
        self.inner.chunk_mut()
    }
}
