use cortex_m::singleton;
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel,
    watch,
};

const CAPACITY: usize = 1;

pub enum Request {
    Measure(protocol::MeasureRequest),
}

#[derive(Clone)]
pub enum Response {
    Measure(Result<protocol::MeasureOutcome, protocol::MeasureError>),
}

pub fn new() -> (Sender, Receiver) {
    let request_channel =
        singleton!(: channel::Channel<NoopRawMutex, Request, CAPACITY> = channel::Channel::new())
            .unwrap();

    let response_channel =
        singleton!(: watch::Watch<NoopRawMutex, Response, CAPACITY> = watch::Watch::new()).unwrap();

    (
        Sender {
            request_sender: request_channel.sender(),
            response_receiver: response_channel.receiver().unwrap(),
        },
        Receiver {
            request_receiver: request_channel.receiver(),
            response_sender: response_channel.sender(),
        },
    )
}

pub struct Sender {
    request_sender: channel::Sender<'static, NoopRawMutex, Request, CAPACITY>,
    response_receiver: watch::Receiver<'static, NoopRawMutex, Response, CAPACITY>,
}

impl Sender {
    pub fn send_request(&self, command: Request) -> Result<(), channel::TrySendError<Request>> {
        self.request_sender.try_send(command)
    }

    pub fn receive_response(&mut self) -> Option<Response> {
        self.response_receiver.try_changed()
    }
}

pub struct Receiver {
    request_receiver: channel::Receiver<'static, NoopRawMutex, Request, CAPACITY>,
    response_sender: watch::Sender<'static, NoopRawMutex, Response, CAPACITY>,
}

impl Receiver {
    pub async fn receive_request(&self) -> Request {
        self.request_receiver.receive().await
    }

    pub fn send_response(&self, response: Response) {
        self.response_sender.send(response);
    }
}
