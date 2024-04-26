#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]
#![feature(asm_experimental_arch)]

use avr_device::attiny85::Peripherals;
use avr_device::interrupt;
use avr_device::interrupt::{free, CriticalSection, Mutex};
use core::cell::Cell;
use core::mem::MaybeUninit;
use embedded_hal::delay::DelayNs;

use attiny_hal as hal;

use panic_halt as _;

#[derive(PartialEq, Eq, Clone, Copy)]
enum UsiState {
    Available,
    NextByte(u8),
    Waiting,
}

static USI_STATE: Mutex<Cell<UsiState>> = Mutex::new(Cell::new(UsiState::Available));

static PERIPHERALS: Mutex<Cell<MaybeUninit<Peripherals>>> =
    Mutex::new(Cell::new(MaybeUninit::uninit()));

#[inline(always)]
fn peripherals<'cs>(cs: CriticalSection<'cs>) -> &'cs Peripherals {
    unsafe { (*PERIPHERALS.borrow(cs).as_ptr()).assume_init_ref() }
}

#[avr_device::entry]
fn main() -> ! {
    // Initialize the peripherals global
    free(|cs| {
        let peripherals = Peripherals::take().unwrap();
        unsafe { (*PERIPHERALS.borrow(cs).as_ptr()).write(peripherals) }
    });

    unsafe { avr_device::interrupt::enable() };

    // Initialize all pins to output
    free(|cs| {
        let dp = peripherals(cs);

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
    });

    // Wait a bit
    let mut delay = hal::delay::Delay::<hal::clock::MHz1>::new();
    delay.delay_ms(500);

    // Send a message
    serial_out(b"hello world!");
    free(|cs| {
        let mut buffer = [0x77, 0xFF, 0xFF];
        i2c(cs, &mut buffer);
    });

    loop {}
}

fn i2c(cs: CriticalSection, buffer: &mut [u8]) -> bool {
    type Delay = hal::delay::Delay<hal::clock::MHz1>;
    let delay_low = || Delay::new().delay_us(5);
    let delay_high = || Delay::new().delay_us(4);

    let dp = peripherals(cs);

    let wait_for_scl_high = || {
        while dp.PORTB.pinb.read().pb2().bit_is_clear() {}
    };

    let transfer = |n_bits: u8| {
        dp.USI.usisr.modify(|_, w| {
            w.usioif().set_bit();
            w.usicnt().bits(16 - 2 * n_bits)
        });
        loop {
            // Strobe clock (positive edge)
            dp.USI.usicr.modify(|_, w| w.usitc().set_bit());
            wait_for_scl_high();
            delay_high();
            // Strobe clock (negative edge)
            dp.USI.usicr.modify(|_, w| w.usitc().set_bit());
            delay_low();
            if dp.USI.usisr.read().usioif().bit_is_set() {
                break;
            }
        }
    };

    // Configure USI
    dp.USI.usicr.write(|w| {
        w.usiwm().two_wire_slave();
        w.usics().ext_pos();
        w.usiclk().set_bit()
    });

    // Set lines to output and release
    dp.PORTB.ddrb.modify(|_, w| {
        w.pb0().set_bit();
        w.pb2().set_bit()
    });
    dp.PORTB.portb.modify(|_, w| {
        w.pb0().set_bit();
        w.pb2().set_bit()
    });
    wait_for_scl_high();
    delay_low();

    // Start condition: drive SDA low, wait, then drive SCL low
    dp.PORTB.portb.modify(|_, w| w.pb0().clear_bit());
    delay_high();
    dp.PORTB.portb.modify(|_, w| w.pb2().clear_bit());
    delay_low();
    dp.PORTB.portb.modify(|_, w| w.pb0().set_bit());

    // LSB of first byte indicates Read (1) or Write (0)
    let write = buffer[0] & 0x01 == 0;
    let mut nack = false;
    let mut bytes = buffer.iter_mut().peekable();
    while let Some(byte) = bytes.next() {
        // Invariant: SCL is output/low; SDA is output/high; no delay required
        if write {
            dp.USI.usidr.write(|w| w.bits(*byte));
            transfer(8);
            // Set SDA to input (pull-up) and read ACK bit
            dp.PORTB.ddrb.modify(|_, w| w.pb0().clear_bit());
            transfer(1);
            // Maintain loop invariant
            dp.PORTB.ddrb.modify(|_, w| w.pb0().set_bit());
            // Check ACK
            if dp.USI.usidr.read().bits() & 0x01 == 1 {
                nack = true;
                break;
            }
        } else {
            // Set SDA to input (pull-up) and read byte
            dp.PORTB.ddrb.modify(|_, w| w.pb0().clear_bit());
            transfer(8);
            *byte = dp.USI.usidr.read().bits();
            // Set SDA to output for ACK (or NACK)
            dp.PORTB.ddrb.modify(|_, w| w.pb0().set_bit());
            if bytes.peek().is_some() {
                dp.USI.usidr.write(|w| w.bits(0x00));
            } else {
                dp.USI.usidr.write(|w| w.bits(0xFF));
            }
            transfer(1);
        }
    }

    // Stop condition
    dp.PORTB.portb.modify(|_, w| w.pb0().clear_bit());
    delay_low();
    dp.PORTB.portb.modify(|_, w| w.pb2().set_bit());
    dp.PORTB.ddrb.modify(|_, w| w.pb2().clear_bit());
    wait_for_scl_high();
    dp.PORTB.portb.modify(|_, w| w.pb0().set_bit());
    dp.PORTB.ddrb.modify(|_, w| w.pb0().clear_bit());
    dp.USI.usicr.reset();
    !nack
}

fn serial_out(data: &[u8]) {
    #[inline(always)]
    fn wait_until_available() {
        while free(|cs| USI_STATE.borrow(cs).get() != UsiState::Available) {}
    }
    for &byte in data {
        wait_until_available();
        free(|cs| {
            let dp = peripherals(cs);
            let reversed = byte.reverse_bits();
            let b1 = reversed >> 1;
            let b2 = reversed << 6 | 0b00111111;
            USI_STATE.borrow(cs).set(UsiState::NextByte(b2));

            // Reset clock
            dp.TC0.tccr0a.write(|w| w.wgm0().ctc());
            dp.TC0.tccr0b.write(|w| w.cs0().direct());
            dp.TC0.gtccr.write(|w| w.psr0().set_bit());
            dp.TC0.tcnt0.write(|w| w.bits(0));
            dp.TC0.ocr0a.write(|w| w.bits(104));

            // Write output
            dp.PORTB.ddrb.modify(|_, w| w.pb1().set_bit());
            dp.USI.usidr.write(|w| w.bits(b1));
            dp.USI.usisr.write(|w| {
                w.usioif().set_bit();
                w.usicnt().bits(9)
            });

            // Configure USI
            dp.USI.usicr.write(|w| {
                w.usiwm().three_wire();
                w.usioie().set_bit();
                w.usics().tc0()
            });
        });
    }
    wait_until_available();
}

#[interrupt(attiny85)]
fn USI_OVF() {
    free(|cs| {
        let dp = peripherals(cs);
        let output = USI_STATE.borrow(cs);
        match output.get() {
            UsiState::NextByte(byte) => {
                dp.USI.usidr.write(|w| w.bits(byte));
                dp.USI.usisr.write(|w| {
                    w.usioif().set_bit();
                    w.usicnt().bits(13)
                });
                output.set(UsiState::Waiting)
            }
            UsiState::Waiting => {
                dp.USI.usicr.reset();
                dp.PORTB.ddrb.modify(|_, w| w.pb1().set_bit());
                dp.PORTB.portb.modify(|_, w| w.pb1().set_bit());
                dp.USI.usisr.modify(|_, w| w.usioif().set_bit());
                output.set(UsiState::Available)
            }
            UsiState::Available => {}
        }
    });
}
