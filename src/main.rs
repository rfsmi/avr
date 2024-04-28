#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]
#![feature(asm_experimental_arch)]

mod bme280;
mod i2c;
mod serial;

use attiny_hal as hal;
use avr_device::attiny85::Peripherals;
use avr_device::interrupt::{free, CriticalSection, Mutex};
use core::cell::Cell;
use core::mem::MaybeUninit;
use embedded_hal::delay::DelayNs;

use panic_halt as _;

type Delay = hal::delay::Delay<hal::clock::MHz1>;

static PERIPHERALS: Mutex<Cell<MaybeUninit<Peripherals>>> =
    Mutex::new(Cell::new(MaybeUninit::uninit()));

#[inline(always)]
fn peripherals<'cs>(cs: CriticalSection<'cs>) -> &'cs Peripherals {
    unsafe { (*PERIPHERALS.borrow(cs).as_ptr()).assume_init_ref() }
}

#[avr_device::entry]
fn main() -> ! {
    free(|cs| {
        let dp = Peripherals::take().unwrap();

        // Enable internal pull-ups on PB0 and PB2 for I2C
        dp.PORTB.ddrb.write(|w| {
            w.pb0().clear_bit();
            w.pb1().set_bit();
            w.pb2().clear_bit();
            w.pb3().set_bit();
            w.pb4().set_bit();
            w.pb5().set_bit()
        });
        dp.PORTB.portb.write(|w| {
            w.pb0().set_bit();
            w.pb1().set_bit(); // Serial (UART) driven high
            w.pb2().set_bit();
            w.pb3().clear_bit();
            w.pb4().clear_bit();
            w.pb5().clear_bit()
        });

        // Initialize the peripherals global
        unsafe { (*PERIPHERALS.borrow(cs).as_ptr()).write(dp) }
    });

    unsafe { avr_device::interrupt::enable() };

    let bme280 = match bme280::BME280::setup() {
        Ok(device) => device,
        Err(msg) => {
            serial::write_line(msg);
            panic!();
        }
    };

    loop {
        match bme280.get_temperature() {
            Ok(mut temp) => {
                serial::write_str("Temperature: ");
                if temp < 0 {
                    serial::write(b'-');
                }
                let mut digits = [0; 10];
                let mut i = digits.len() - 1;
                while temp > 0 {
                    digits[i] = (temp % 10) as u8;
                    temp /= 10;
                    i -= 1;
                }
                let mut has_nonzero = false;
                for (i, d) in digits.into_iter().enumerate() {
                    let ones_digit = i == digits.len() - 3;
                    if d != 0 || ones_digit {
                        has_nonzero = true;
                    }
                    if has_nonzero {
                        serial::write(d + b'0')
                    }
                    if ones_digit {
                        serial::write(b'.');
                    }
                }
                serial::write_str("\r\n");
            }
            Err(msg) => {
                serial::write_line(msg);
                break;
            }
        }
        Delay::new().delay_ms(1000);
    }

    loop {}
}
