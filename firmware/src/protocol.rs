use core::task::Poll;

// re-export protocol crate
pub use ::protocol::*;

use crate::{
    eprintln,
    mux,
};

pub fn receive() -> Poll<Option<ClientMessage>> {
    mux::with(|mux| {
        match mux.poll_receive() {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(chunk)) => {
                match chunk.port {
                    PROTOCOL_PORT => {
                        match chunk.deserialize::<ClientMessage>() {
                            Ok(message) => Poll::Ready(Some(message)),
                            Err(_error) => {
                                eprintln!(mux, "Error during deserialization");
                                Poll::Pending
                            }
                        }
                    }
                    _ => {
                        // ignore unexpected port.
                        let port = chunk.port;
                        eprintln!(mux, "Message on unexpected port: {}", port);
                        Poll::Pending
                    }
                }
            }
        }
    })
}

#[inline(always)]
pub fn send(message: &DeviceMessage) {
    mux::with(|mux| {
        mux.send_message(PROTOCOL_PORT, message);
    });
}
