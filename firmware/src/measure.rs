use embassy_stm32::{
    Peri,
    adc::{
        Adc,
        AdcChannel,
        BorrowedAdcChannel,
        InterruptHandler,
        Resolution,
        SampleTime,
    },
    bind_interrupts,
    gpio::{
        Level,
        Output,
        Speed,
    },
    mode::Async,
    peripherals::{
        ADC1,
        PA0,
        PA1,
    },
};
use embassy_time::{
    Duration,
    Instant,
};
use protocol::{
    MeasureError,
    MeasureOutcome,
};

use crate::channel;

#[allow(non_snake_case)]
pub struct Peripherals {
    pub ADC1: Peri<'static, ADC1>,
    pub PA0: Peri<'static, PA0>,
    pub PA1: Peri<'static, PA1>,
}

bind_interrupts!(
    struct Irqs {
        ADC => InterruptHandler<ADC1>;
    }
);

#[embassy_executor::task]
pub async fn run(peripherals: Peripherals, receiver: channel::Receiver) {
    let mut meter = Meter::from_peripherals(peripherals);

    loop {
        let request = receiver.receive_request().await;
        match request {
            channel::Request::Measure(measure_request) => {
                defmt::info!("Starting measurement: {:?}", measure_request);
                let mut measurement = Measurement::new(
                    &mut meter,
                    measure_request.charge_resistor,
                    Duration::from_millis(measure_request.timeout.into()),
                );

                let result = measurement.measure().await;
                defmt::debug!("measurement result: {:?}", result);

                receiver.send_response(channel::Response::Measure(result));
            }
        }
    }
}

struct Meter {
    adc: Adc<'static, ADC1, Async>,
    charge_pin: Output<'static>,
    read_pin: BorrowedAdcChannel<'static, ADC1>,
}

impl Meter {
    pub fn from_peripherals(peripherals: Peripherals) -> Self {
        let mut adc = Adc::new(peripherals.ADC1, Irqs, Default::default());
        // this is the best resolution and it is the default. still we'll make
        // sure we know what resolution we're using.
        adc.set_resolution(Resolution::Bits12);

        defmt::debug!("ADC clock rate: {}", adc.clock());

        let charge_pin = Output::new(peripherals.PA1, Level::Low, Speed::Low);
        let read_pin = peripherals.PA0.degrade_adc();

        Self {
            adc,
            charge_pin,
            read_pin,
        }
    }
}

struct Measurement<'a> {
    meter: &'a mut Meter,
    charge_resistor: u32,
    timeout: Duration,
    sample_time: SampleTime,
}

impl<'a> Measurement<'a> {
    pub fn new(meter: &'a mut Meter, charge_resistor: u32, timeout: Duration) -> Self {
        // ADC rate is 21 MHz, so this is 43.75 ms
        let sample_time = SampleTime::Cycles480;

        Self {
            meter,
            charge_resistor,
            timeout,
            sample_time,
        }
    }

    async fn measure(&mut self) -> Result<MeasureOutcome, MeasureError> {
        let charge_result = self.charge().await.map(|charge_time| {
            let charge_time = u32::try_from(charge_time.as_millis()).unwrap();

            // calculate capacity in μF
            let capacity = charge_time * 1000 / self.charge_resistor;

            defmt::debug!("Derived capacity: {} μF", capacity);

            MeasureOutcome {
                charge_time,
                capacity,
            }
        });

        // always discharge. ignore result
        if let Err(error) = self.discharge().await {
            defmt::error!("Failed to discharge capacitor: {}", error);
        }

        charge_result
    }

    async fn charge(&mut self) -> Result<Duration, MeasureError> {
        defmt::debug!("Charging capacitor");
        self.charge_impl(Level::High, |voltage| voltage >= 2589)
            .await
    }

    async fn discharge(&mut self) -> Result<Duration, MeasureError> {
        defmt::debug!("Discharging capacitor");
        self.charge_impl(Level::Low, |voltage| voltage == 0).await
    }

    async fn charge_impl(
        &mut self,
        charge_pin_level: Level,
        mut condition: impl FnMut(u16) -> bool,
    ) -> Result<Duration, MeasureError> {
        let time_start = Instant::now();

        self.meter.charge_pin.set_level(charge_pin_level);

        loop {
            // voltage in 5V / 2^12
            let voltage = self
                .meter
                .adc
                .read(self.meter.read_pin.reborrow_adc(), self.sample_time)
                .await;

            let now = Instant::now();
            let charge_time = now - time_start;

            /*defmt::trace!(
                "charging: level={:?}, charge_time={} ms, voltage={}",
                charge_pin_level,
                charge_time.as_millis(),
                voltage
            );*/

            if condition(voltage) {
                return Ok(charge_time);
            }

            if charge_time > self.timeout {
                return Err(MeasureError::Timeout {
                    timeout: charge_time.as_millis().try_into().unwrap(),
                });
            }
        }
    }
}
