#![no_std]
#![no_main]

use panic_halt as _;

#[avr_device::entry]
fn main() -> ! {
    loop {}
}
