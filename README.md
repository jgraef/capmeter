# Simple DIY capacitance measurement

This is a little experiment we performed to measure capacitances. We set out to replicate the [measurements of capacitances of common diodes by Hans Summers][1], but haven't achieved this yet.

Initially we tried this we an Arduino Uno, but the device was too constrained. We also later found out that simply measuring the charge time would not be accurate enough.

Then we built an oscillator circuit with a inverting Schmitt trigger (HEF40106BP). The DUT would be the capacitor that couples the schmitt triggers input to ground. We used an STM32F401CC on a Black Pill board to control the setup and measure the resonant frequency. The circuit also has a transistor that discharges the capacitor when needed (orange LED is on when discharging).

The firmware uses input capture to measure the period of the signal on PA0. The discharge transistor and input capture prescaler can be controlled via USB. Also measurements can be read back over USB. Note, that this uses a test PID from [pid.codes](https://pid.codes/1209/0001/), specifically `1209:0001` and thus should only be used for test purposes and not be distributed.

This is the measurement circuit on a breadboard. It's not very accurate due to the parasicit impedances of the setup - although the client program uses calibration to account for some of it. The DUT is circled in pink.

![Measurement circuit on a breadboard.](docs/breadboard.jpg "Measurement circuit on a breadboard.")

[1]: https://www.hanssummers.com/varicap/varicaporig.html
