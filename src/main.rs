#![no_std]
#![no_main]

#![feature(abi_avr_interrupt)]

use avr_device::interrupt;
use avr_device::interrupt::{free, Mutex};
use panic_halt as _;
use core::cell::RefCell;

type InterruptFlag = Mutex<RefCell<bool>>;
static TIMER_INTERRUPT: InterruptFlag = Mutex::new(RefCell::new(false));

#[avr_device::entry]
fn main() -> ! {
    let dp = avr_device::attiny85::Peripherals::take().unwrap();
    dp.PORTB.ddrb.modify(|_, w| w.pb3().set_bit());
    dp.PORTB.portb.modify(|_, w| w.pb3().bit(true));

    // Enable global interrupts. Last thig you do.
    unsafe { avr_device::interrupt::enable() };
    
    loop {
        // dp.PORTB.pinb.modify(|_, w| w.pb3().bit(true));
    }
}

#[interrupt(attiny85)]
fn TIMER0_COMPA() {
    free(|cs| {
        TIMER_INTERRUPT.borrow(cs).replace(true);
    })
}
