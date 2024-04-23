#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]
#![feature(asm_experimental_arch)]

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

static PERIPHERALS: Mutex<Cell<MaybeUninit<avr_device::attiny85::Peripherals>>> =
    Mutex::new(Cell::new(MaybeUninit::uninit()));

#[inline(always)]
fn peripherals<'cs>(cs: CriticalSection<'cs>) -> &'cs avr_device::attiny85::Peripherals {
    unsafe { (*PERIPHERALS.borrow(cs).as_ptr()).assume_init_ref() }
}

#[inline(always)]
fn send_byte(byte: u8) {
    loop {
        if free(|cs| {
            let dp = peripherals(cs);
            let output = USI_STATE.borrow(cs);
            if output.get() != UsiState::Available {
                return false;
            }
            let reversed = byte.reverse_bits();
            let b1 = reversed >> 1;
            let b2 = reversed << 6 | 0b00111111;
            output.set(UsiState::NextByte(b2));

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
            true
        }) {
            break;
        }
    }
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

#[avr_device::entry]
fn main() -> ! {
    // Initialize the peripherals global
    free(|cs| {
        let peripherals = avr_device::attiny85::Peripherals::take().unwrap();
        unsafe { (*PERIPHERALS.borrow(cs).as_ptr()).write(peripherals) }
    });

    unsafe { avr_device::interrupt::enable() };

    // Initialize all pins to output
    free(|cs| {
        let dp = peripherals(cs);

        dp.PORTB.ddrb.write(|w| {
            w.pb0().set_bit();
            w.pb1().set_bit();
            w.pb2().set_bit();
            w.pb3().set_bit();
            w.pb4().set_bit();
            w.pb5().set_bit()
        });
        dp.PORTB.portb.write(|w| {
            w.pb0().clear_bit();
            w.pb1().set_bit();
            w.pb2().clear_bit();
            w.pb3().clear_bit();
            w.pb4().clear_bit();
            w.pb5().clear_bit()
        });
    });

    // Wait a bit
    let mut delay = hal::delay::Delay::<hal::clock::MHz1>::new();
    delay.delay_ms(500);

    // Send a message
    for &byte in b"hello world!" {
        send_byte(byte);
    }

    loop {}
}
