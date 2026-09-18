use embassy_futures::select::Either;
use embassy_stm32::{
    Peri,
    bind_interrupts,
    gpio::{
        Level,
        Output,
        Pull,
        Speed,
    },
    peripherals::{
        PA0,
        PA1,
        TIM2,
    },
    time::Hertz,
    timer::{
        CaptureCompareInterruptHandler,
        Channel,
        input_capture::{
            CaptureInput,
            InputCapture,
        },
        low_level::CountingMode,
    },
};
use embassy_time::{
    Duration,
    Instant,
};
use protocol::{
    Measurement,
    Prescaler,
};

use crate::channel;

#[allow(non_snake_case)]
pub struct Peripherals {
    pub TIM2: Peri<'static, TIM2>,
    pub PA0: Peri<'static, PA0>,
    pub PA1: Peri<'static, PA1>,
}

bind_interrupts!(
    struct Irqs {
        TIM2 => CaptureCompareInterruptHandler<TIM2>;
    }
);

#[embassy_executor::task]
pub async fn run(peripherals: Peripherals, channel: channel::MeasurementSide) {
    defmt::info!("Initializing measurement");

    let mut discharge_switch = Output::new(peripherals.PA1, Level::Low, Speed::Low);

    // run timer at 1 MHz
    //
    // maximum would be 84 MHz with our clock config. TIM2 is on APB2 (48 MHz),
    // but is doubled if APB2's prescaler is > 1 - which it is.
    let timer_frequency = 1_000_000;

    // use TIM2 CH1 with PA0
    let channel1 = defmt::unwrap!(CaptureInput::from_pin(peripherals.PA0, Pull::None));
    let mut input_capture = InputCapture::new(
        peripherals.TIM2,
        Some(channel1),
        None,
        None,
        None,
        Irqs,
        Hertz(timer_frequency),
        CountingMode::EdgeAlignedUp,
    );

    let mut previous_tick = None;
    let mut previous_log = None;
    let mut serial = 0;
    let mut prescaler = Prescaler::default();
    let log_interval = Duration::from_secs(2);

    loop {
        // select between 2 futures:
        //
        // 1. wait for rising edge of signal and returns "timestamp"
        // 2. wait for control command from USB
        match embassy_futures::select::select(
            input_capture.wait_for_rising_edge(Channel::Ch1),
            channel.receive_command(),
        )
        .await
        {
            Either::First(current_tick) => {
                if let Some(previous_tick) = previous_tick {
                    // from "timestamps" from the previous and current rising
                    // edge we can calculate long it took
                    // for one signal period.
                    //
                    // the input capture is configured at 1 MHz, so each tick
                    // corresponds to 1 μs.
                    let period = current_tick.wrapping_sub(previous_tick);

                    // log frequency every now and then
                    let now = Instant::now();
                    if previous_log.is_none_or(|previous_log| now - previous_log > log_interval) {
                        defmt::info!("Period: {} μs (serial {})", period, serial);
                        previous_log = Some(now);
                    }

                    channel.send_measurement(Measurement {
                        serial,
                        prescaler,
                        discharge: discharge_switch.is_set_high(),
                        period,
                    });
                    serial = serial.wrapping_add(1);
                }

                previous_tick = Some(current_tick);
            }
            Either::Second(command) => {
                match command {
                    protocol::Command::SetDischarge(enable) => {
                        discharge_switch.set_level(enable.into());
                    }
                    protocol::Command::SetPrescaler(value) => {
                        if value != prescaler {
                            input_capture.set_input_capture_prescaler(Channel::Ch1, value.ic1psc());
                            prescaler = value;
                        }
                    }
                }
            }
        }
    }
}
