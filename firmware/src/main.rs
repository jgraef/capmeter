#![no_std]
#![no_main]

pub mod channel;
pub mod measure;
pub mod usb;

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::{
    Config,
    gpio::{
        Level,
        Output,
        Speed,
    },
    rcc::mux::Clk48sel,
    time::Hertz,
};
use embassy_time::Timer;
use panic_probe as _;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    defmt::info!("Capmeter v{}", core::env!("CARGO_PKG_VERSION"));

    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;

        // The Black Pill board has an external 25 MHz crystal oscillator
        config.rcc.hse = Some(Hse {
            freq: Hertz(25_000_000),
            mode: HseMode::Oscillator,
        });

        // This Black Pill board has a STM32F401CC.
        // The datasheet lists max bus speeds under 3.11

        // feed PLL from HSE
        config.rcc.pll_src = PllSource::Hse;

        // For STM32401CC the AHB and APB2 can run at 84 MHz, so we want PLL_P
        // to be that. Since we use USB, we must have PLL_Q be 48 MHz.
        config.rcc.pll = Some(Pll {
            // VCO = 25 MHz / 25 * 336 = 336 MHz
            prediv: PllPreDiv::Div25,
            mul: PllMul::Mul336,
            // PLL_P (SYSCLK) = 336 MHz / 4 = 84 MHz
            divp: Some(PllPDiv::Div4),
            // PLL_Q (USB) = 336 MHz / 7 = 48 MHz
            divq: Some(PllQDiv::Div7),
            // can't be used
            divr: None,
        });

        // AHB buses. max 84 Mhz
        // PLL_P / 1
        config.rcc.ahb_pre = AHBPrescaler::Div1;

        // low-speed APB domain. max 42 Mhz
        // PLL_P / 2
        config.rcc.apb1_pre = APBPrescaler::Div2;

        // high-speed APB domain. max 84 Mhz
        // PLL_P / 1
        config.rcc.apb2_pre = APBPrescaler::Div1;

        // use PLL P as system clock
        config.rcc.sys = Sysclk::Pll1P;

        // use PLL Q as 48 MHz clock
        config.rcc.mux.clk48sel = Clk48sel::Pll1Q;
    }

    let peripherals = embassy_stm32::init(config);

    let (sender, receiver) = channel::new();

    // usb
    spawner.spawn(
        usb::run(
            usb::Peripherals {
                USB_OTG_FS: peripherals.USB_OTG_FS,
                PA12: peripherals.PA12,
                PA11: peripherals.PA11,
            },
            sender,
        )
        .unwrap(),
    );

    // measurement
    spawner.spawn(
        measure::run(
            measure::Peripherals {
                ADC1: peripherals.ADC1,
                PA0: peripherals.PA0,
                PA1: peripherals.PA1,
            },
            receiver,
        )
        .unwrap(),
    );

    // blinky
    let mut led = Output::new(peripherals.PC13, Level::High, Speed::Low);

    loop {
        led.set_low();
        Timer::after_millis(5000).await;

        led.set_high();
        Timer::after_millis(1000).await;
    }
}
