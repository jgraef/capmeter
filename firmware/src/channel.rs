use cortex_m::singleton;
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel,
    watch,
};
use protocol::{
    Command,
    Measurement,
};

const CAPACITY: usize = 1;

pub fn new() -> (UsbSide, MeasurementSide) {
    let control_channel = defmt::unwrap!(
        singleton!(: channel::Channel<NoopRawMutex, Command, CAPACITY> = channel::Channel::new())
    );

    let measurement_channel = defmt::unwrap!(
        singleton!(: watch::Watch<NoopRawMutex, Measurement, CAPACITY> = watch::Watch::new())
    );

    (
        UsbSide {
            control_sender: control_channel.sender(),
            measurement_receiver: defmt::unwrap!(measurement_channel.receiver()),
        },
        MeasurementSide {
            control_receiver: control_channel.receiver(),
            measurement_sender: measurement_channel.sender(),
        },
    )
}

pub struct UsbSide {
    control_sender: channel::Sender<'static, NoopRawMutex, Command, CAPACITY>,
    measurement_receiver: watch::Receiver<'static, NoopRawMutex, Measurement, CAPACITY>,
}

impl UsbSide {
    pub fn send_command(&self, command: Command) -> Result<(), channel::TrySendError<Command>> {
        self.control_sender.try_send(command)
    }

    pub fn read_measurement(&mut self) -> Option<Measurement> {
        self.measurement_receiver.try_get()
    }
}

pub struct MeasurementSide {
    control_receiver: channel::Receiver<'static, NoopRawMutex, Command, CAPACITY>,
    measurement_sender: watch::Sender<'static, NoopRawMutex, Measurement, CAPACITY>,
}

impl MeasurementSide {
    pub async fn receive_command(&self) -> Command {
        self.control_receiver.receive().await
    }

    pub fn send_measurement(&self, measurement: Measurement) {
        self.measurement_sender.send(measurement);
    }
}
