use core::panic::PanicInfo;

use crate::debug_led;

#[inline(never)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug_led::with(|debug_led| {
        loop {
            debug_led.blink(200, 200);
            debug_led.blink(200, 200);
            debug_led.blink(200, 200);

            debug_led.blink(400, 200);
            debug_led.blink(400, 200);
            debug_led.blink(400, 200);

            debug_led.blink(200, 200);
            debug_led.blink(200, 200);
            debug_led.blink(200, 200);

            arduino_hal::delay_ms(500);
        }
    })
}
