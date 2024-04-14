#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]

use avr_device::interrupt;
use avr_device::interrupt::{free, Mutex};
use core::cell::RefCell;

use panic_halt as _;

static SHOW_LED: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

#[avr_device::entry]
fn main() -> ! {
    let dp = avr_device::attiny85::Peripherals::take().unwrap();
    dp.PORTB.ddrb.modify(|_, w| w.pb3().set_bit());

    // Configure timer/counter 0 to count up and fire the TIMER0_COMPA at a
    // regular interval to act as a clock for our timers The compare interrupt
    // is set to fire roughly every 1ms: 1 / (1Mhz / 8) * 125 = 1ms
    dp.TC0.tccr0a.write(|w| w.wgm0().ctc());
    dp.TC0.tccr0b.write(|w| w.cs0().prescale_1024());
    dp.TC0.ocr0a.write(|w| w.bits(124));
    dp.TC0.timsk.write(|w| w.ocie0a().bit(true));

    // Lastly enable global interrupts
    unsafe { avr_device::interrupt::enable() };

    loop {
        free(|cs| {
            let led_on = *SHOW_LED.borrow(cs).borrow();
            dp.PORTB.portb.modify(|_, w| w.pb3().bit(led_on));
        });
        avr_device::asm::sleep();
    }
}

#[interrupt(attiny85)]
fn TIMER0_COMPA() {
    free(|cs| {
        // Just toggle the led
        SHOW_LED.borrow(cs).replace_with(|&mut old| !old);
    })
}
