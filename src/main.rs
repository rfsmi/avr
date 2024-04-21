#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]
#![feature(asm_experimental_arch)]

use avr_device::interrupt;
use avr_device::interrupt::{free, CriticalSection, Mutex};
use core::arch::asm;
use core::borrow::{Borrow, BorrowMut};
use core::cell::{Cell, RefCell, UnsafeCell};
use core::iter::*;
use core::mem::MaybeUninit;
use core::ops::DerefMut;
use embedded_hal::delay::DelayNs;
use itertools::*;

use attiny_hal as hal;
use hal::delay::Delay;

use panic_halt as _;

static USI_STATE: Mutex<Cell<UsiState>> = Mutex::new(Cell::new(UsiState::Available));

static PERIPHERALS: Mutex<Cell<MaybeUninit<avr_device::attiny85::Peripherals>>> =
    Mutex::new(Cell::new(MaybeUninit::uninit()));

#[inline(always)]
fn peripherals<'cs>(cs: CriticalSection<'cs>) -> &'cs avr_device::attiny85::Peripherals {
    unsafe { (*PERIPHERALS.borrow(cs).as_ptr()).assume_init_ref() }
}

#[avr_device::entry]
fn main() -> ! {
    // Initialize the static PERIPHERALS
    free(|cs| {
        let peripherals = avr_device::attiny85::Peripherals::take().unwrap();
        unsafe { (*PERIPHERALS.borrow(cs).as_ptr()).write(peripherals) }
    });

    // Configure timer/counter 0 to count up and fire the TIMER0_COMPA at a
    // regular interval to act as a clock for our timers The compare interrupt
    // is set to fire roughly every 1ms: 1 / (1Mhz / 8) * 125 = 1ms

    free(|cs| {
        let dp = peripherals(cs);

        dp.PORTB.ddrb.modify(|_, w| {
            w.pb0().set_bit();
            w.pb1().set_bit();
            w.pb2().set_bit();
            w.pb3().set_bit();
            w.pb4().set_bit();
            w.pb5().set_bit()
        });
        dp.PORTB.portb.modify(|_, w| {
            w.pb0().clear_bit();
            w.pb1().set_bit();
            w.pb2().clear_bit();
            w.pb3().clear_bit();
            w.pb4().clear_bit();
            w.pb5().clear_bit()
        });
    });

    // Flip pin every cycle
    // dp.TC1.tccr1.write(|w| w.ctc1().set_bit().cs1().direct());
    // dp.TC1.ocr1c.write(|w| w.bits(0)); // Every cycle
    // dp.TC1.gtccr.write(|w| w.com1b().match_toggle());

    // Lastly enable global interrupts
    unsafe { avr_device::interrupt::enable() };
    // DEBUG.write(b"hello world!");

    // Send byte via USI
    free(|cs| {
        let dp = peripherals(cs);

        // Put timer into sync mode
        dp.TC0.gtccr.write(|w| {
            w.tsm().set_bit();
            w.psr0().set_bit()
        });
        // ocr0a = clock / (baud * prescale)
        // So we end up with 104.17 with 1Mhz clock (no prescale) and 9600 baud
        dp.TC0.tccr0a.write(|w| w.wgm0().ctc());
        // dp.TC0.tccr0b.write(|w| w.cs0().direct());
        // dp.TC0.ocr0a.write(|w| w.bits(104));
        dp.TC0.tccr0b.write(|w| w.cs0().prescale_8());
        dp.TC0.ocr0a.write(|w| w.bits(125));
        // dp.TC0.timsk.write(|w| w.ocie0a().set_bit());

        // Disable sync mode
        dp.TC0.gtccr.write(|w| {
            w.tsm().clear_bit();
            w.psr0().clear_bit()
        });

        // Configure USI
        dp.USI.usicr.write(|w| {
            w.usiwm().three_wire();
            w.usioie().set_bit();
            w.usics().tc0()
        });

        if let UsiState::Transmitting {
            next_bits: (send_bits, send_bits_length),
            buffer,
            data,
        } = next_transmit_state((0, 0), b"hello world!")
        {
            USI_STATE.borrow(cs).set(next_transmit_state(buffer, data));
            dp.USI.usidr.write(|w| w.bits(send_bits));
            dp.USI.usisr.write(|w| {
                w.usioif().set_bit();
                w.usicnt().bits(16 - send_bits_length as u8)
            });
        }
    });

    // for byte in b"hello world!" {
    // }

    loop {
        // free(|cs| {
        //     if let Some(signal) = DEBUG.get_signal(cs) {
        //         dp.PORTB.portb.write(|w| w.pb3().bit(signal));
        //     }
        // });
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum UsiState<'data> {
    Available,
    Transmitting {
        next_bits: (u8, usize),
        buffer: (u32, usize),
        data: &'data [u8],
    },
}

#[inline(never)]
fn next_transmit_state((mut buffer, mut buffer_size): (u32, usize), mut data: &[u8]) -> UsiState {
    if buffer_size < 8 && !data.is_empty() {
        // Push the start bit
        buffer_size += 1;
        // Push the data byte
        buffer |= (data[0] as u32) << buffer_size;
        buffer_size += 8;
        data = &data[1..];
        // Push the stop bit
        buffer |= 1 << buffer_size;
        buffer_size += 1;
    }
    let next_bits = (buffer as u8).reverse_bits();
    let next_bits_length = buffer_size.min(8);
    buffer_size -= next_bits_length;
    buffer >>= next_bits_length;
    UsiState::Transmitting {
        next_bits: (next_bits, next_bits_length),
        buffer: (buffer, buffer_size),
        data,
    }
}

#[interrupt(attiny85)]
fn USI_OVF() {
    free(|cs| {
        let dp = peripherals(cs);
        let output = USI_STATE.borrow(cs);
        match output.get() {
            UsiState::Available => {}
            UsiState::Transmitting {
                buffer: (_, 0),
                data: &[],
                ..
            } => {
                dp.PORTB.portb.write(|w| w.pb1().set_bit());
                dp.USI.usicr.write(|w| {
                    w.usiwm().disabled();
                    w.usioie().clear_bit();
                    w.usics().no_clock()
                });
                dp.USI.usisr.write(|w| w.usioif().set_bit());
                output.set(UsiState::Available)
            }
            UsiState::Transmitting {
                next_bits: (send_bits, send_bits_length),
                buffer,
                data,
            } => {
                dp.USI.usidr.write(|w| w.bits(send_bits));
                dp.USI.usisr.write(|w| {
                    w.usioif().set_bit();
                    w.usicnt().bits(16 - send_bits_length as u8)
                });
                output.set(next_transmit_state(buffer, data))
            }
        }
    });
}

// #[interrupt(attiny85)]
// fn TIMER0_COMPA() {
//     DEBUG.tick();
// }
