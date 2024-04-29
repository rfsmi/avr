#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]
#![feature(asm_experimental_arch)]

mod bme280;
mod i2c;
mod lcd1602;
mod uart;

use attiny_hal as hal;
use avr_device::attiny85::Peripherals;
use avr_device::interrupt::{free, CriticalSection, Mutex};
use core::cell::Cell;
use core::iter::from_fn;
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

fn i32_fp_to_u8s(mut n: i32) -> impl Iterator<Item = u8> {
    #[derive(PartialEq)]
    enum Next {
        Minus,
        Digit(bool),
        Dot,
    }
    use Next::*;
    let mut next = Digit(false);
    if n < 0 {
        n *= -1;
        next = Minus;
    };
    let mut buffer = [0; 16];
    let mut i = buffer.len() - 1;
    while n > 0 {
        buffer[i] = (n % 10) as u8;
        n /= 10;
        i -= 1;
    }
    let mut digits = buffer.into_iter();

    from_fn(move || loop {
        match next {
            Minus => {
                next = Digit(false);
                return Some(b'-');
            }
            Dot => {
                next = Digit(true);
                return Some(b'.');
            }
            Digit(force) => {
                let d = digits.next()?;
                if digits.len() == 2 {
                    next = Dot;
                    return Some(b'0' + d);
                }
                if force || d != 0 {
                    next = Digit(true);
                    return Some(b'0' + d);
                }
            }
        }
    })
}

fn main_loop() -> Result<(), &'static str> {
    let mut lcd = lcd1602::setup()?;
    let bme280 = bme280::setup()?;

    loop {
        lcd.set_cursor(0, 0);
        lcd.write_str("Temp: ");
        uart::write_str("\r\nTemperature: ");
        let temp = bme280.get_temperature()?;
        for byte in i32_fp_to_u8s(temp) {
            uart::write(byte);
            lcd.write(byte);
        }
        lcd.sync()?;
        Delay::new().delay_ms(500);
    }
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

    if let Err(msg) = main_loop() {
        uart::write_line(msg);
    }

    loop {}
}
