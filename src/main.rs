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

fn format_i32(mut n: i32, decimal_place: Option<usize>) -> impl Iterator<Item = u8> {
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
    let mut buffer = [0; 12];
    for byte in buffer.iter_mut().rev() {
        *byte = (n % 10) as u8;
        n /= 10;
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
                if Some(digits.len()) == decimal_place {
                    next = Dot;
                    return Some(b'0' + d);
                }
                if force || digits.len() == 0 || d != 0 {
                    next = Digit(true);
                    return Some(b'0' + d);
                }
            }
        }
    })
}

fn read_pot() -> u16 {
    free(|cs| {
        let pb = peripherals(cs);
        // Start measurement
        pb.ADC.adcsra.modify(|_, w| w.adsc().set_bit());
        while pb.ADC.adcsra.read().adsc().bit_is_set() {
            // Wait for conversion to complete
        }
        pb.ADC.adc.read().bits()
    })
}

fn main_loop() -> Result<(), &'static str> {
    let mut lcd = lcd1602::setup()?;
    let bme280 = bme280::setup()?;

    loop {
        let sensor_temp = bme280.get_temperature()?;
        const MIN_TEMP: i32 = 1600; // 16 degrees
        const MAX_TEMP: i32 = 2400; // 24 degrees
        const MAX_POT: i32 = 1024;
        let pot = read_pot() as i32;
        let set_temp = (MIN_TEMP * (MAX_POT - pot) + MAX_TEMP * pot) / MAX_POT;
        lcd.reset();
        lcd.set_cursor(0, 0);
        lcd.write_str("Temp: ");
        for byte in format_i32(sensor_temp, Some(2)) {
            lcd.write(byte);
        }
        lcd.set_cursor(0, 1);
        lcd.write_str(" Set: ");
        for byte in format_i32(set_temp, Some(2)) {
            lcd.write(byte);
        }
        lcd.set_align(lcd1602::Align::Right);
        lcd.set_cursor(15, 0);
        lcd.write_str(if sensor_temp < set_temp { "ON" } else { "OFF" });
        lcd.sync()?;
        Delay::new().delay_ms(250);
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
            w.pb4().clear_bit();
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

        // Setup pb4 for pot input
        dp.ADC.admux.write(|w| w.mux().adc2());
        dp.ADC.adcsra.write(|w| {
            w.aden().set_bit(); // Enable ADC
            w.adps().prescaler_8() // Set prescaler to 8 (1Mhz -> 125Khz)
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
