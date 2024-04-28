use core::cell::Cell;

use avr_device::interrupt;
use avr_device::interrupt::{free, Mutex};

use crate::peripherals;

#[derive(PartialEq, Eq, Clone, Copy)]
enum UsiState {
    Available,
    NextByte(u8),
    Waiting,
}

static USI_STATE: Mutex<Cell<UsiState>> = Mutex::new(Cell::new(UsiState::Available));

pub fn write_line(msg: &str) {
    write_str(msg);
    write_str("\r\n");
}

pub fn write_str(msg: &str) {
    for &byte in msg.as_bytes() {
        write(byte)
    }
}

pub fn write(byte: u8) {
    #[inline(always)]
    fn wait_until_available() {
        while free(|cs| USI_STATE.borrow(cs).get() != UsiState::Available) {}
    }
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
        dp.PORTB.portb.modify(|_, w| w.pb1().set_bit());
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
