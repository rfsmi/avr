use attiny_hal as hal;
use avr_device::interrupt::{free, CriticalSection};
use embedded_hal::delay::DelayNs;
use hal::Peripherals;

use crate::{peripherals, Delay};

// Read/Write loop invariants:
//  - SCL is output/low
//  - SDA is output/high
//  - No delay required

pub fn read(address: u8, buffer: &mut [u8]) -> Result<(), &'static str> {
    free(|cs| {
        let i2c = I2C::new(cs);
        i2c.start();
        i2c.write_byte(address << 1 | 0x01)
            .map_err(|_| "No response to address")?;
        let mut data = buffer.iter_mut().peekable();
        while let Some(byte) = data.next() {
            *byte = i2c.read_byte(!data.peek().is_some());
        }
        Ok(())
    })
}

pub fn write(address: u8, buffer: &[u8]) -> Result<(), &'static str> {
    free(|cs| {
        let i2c = I2C::new(cs);
        i2c.start();
        i2c.write_byte(address << 1 | 0x00)
            .map_err(|_| "No response to address")?;
        for &byte in buffer {
            i2c.write_byte(byte)?;
        }
        Ok(())
    })
}

#[inline(always)]
fn delay_low() {
    Delay::new().delay_us(5)
}

#[inline(always)]
fn delay_high() {
    Delay::new().delay_us(4)
}

struct I2C<'cs> {
    dp: &'cs Peripherals,
}

impl<'cs> I2C<'cs> {
    #[inline(always)]
    fn new(cs: CriticalSection<'cs>) -> Self {
        Self {
            dp: peripherals(cs),
        }
    }

    #[inline(always)]
    fn wait_for_scl_high(&self) {
        while self.dp.PORTB.pinb.read().pb2().bit_is_clear() {}
    }

    #[inline(always)]
    fn transfer(&self, n_bits: u8) {
        self.dp.USI.usisr.modify(|_, w| {
            w.usioif().set_bit();
            w.usicnt().bits(16 - 2 * n_bits)
        });
        while self.dp.USI.usisr.read().usioif().bit_is_clear() {
            // Strobe clock (positive edge)
            self.dp.USI.usicr.modify(|_, w| w.usitc().set_bit());
            self.wait_for_scl_high();
            delay_high();
            // Strobe clock (negative edge)
            self.dp.USI.usicr.modify(|_, w| w.usitc().set_bit());
            delay_low();
        }
    }

    fn write_byte(&self, byte: u8) -> Result<(), &'static str> {
        self.dp.USI.usidr.write(|w| w.bits(byte));
        self.transfer(8);
        // Set SDA to input (pull-up) and read ACK bit
        self.dp.PORTB.ddrb.modify(|_, w| w.pb0().clear_bit());
        self.transfer(1);
        // Maintain loop invariant
        self.dp.PORTB.ddrb.modify(|_, w| w.pb0().set_bit());
        // Check ACK
        if self.dp.USI.usidr.read().bits() & 0x01 == 1 {
            Err("No ACK")
        } else {
            Ok(())
        }
    }

    fn read_byte(&self, last: bool) -> u8 {
        // Set SDA to input (pull-up) and read byte
        self.dp.PORTB.ddrb.modify(|_, w| w.pb0().clear_bit());
        self.transfer(8);
        let byte = self.dp.USI.usidr.read().bits();
        // Set SDA to output for ACK (or NACK)
        self.dp.PORTB.ddrb.modify(|_, w| w.pb0().set_bit());
        // ACK is 0; NACK is 1 and will end transmission
        let ack = if last { 0xFF } else { 0x00 };
        self.dp.USI.usidr.write(|w| w.bits(ack));
        self.transfer(1);
        byte
    }

    fn start(&self) {
        // Configure USI
        self.dp.USI.usicr.write(|w| {
            w.usiwm().two_wire_slave();
            w.usics().ext_pos();
            w.usiclk().set_bit()
        });

        // Set lines to output and release
        self.dp.PORTB.ddrb.modify(|_, w| {
            w.pb0().set_bit();
            w.pb2().set_bit()
        });
        self.dp.PORTB.portb.modify(|_, w| {
            w.pb0().set_bit();
            w.pb2().set_bit()
        });

        // Start condition: drive SDA low, wait, then drive SCL low
        self.wait_for_scl_high();
        delay_low(); // Start condition setup time
        self.dp.PORTB.portb.modify(|_, w| w.pb0().clear_bit());
        delay_high(); // Start condition hold time
        self.dp.PORTB.portb.modify(|_, w| w.pb2().clear_bit());
        delay_low();
        self.dp.PORTB.portb.modify(|_, w| w.pb0().set_bit());
    }
}

impl Drop for I2C<'_> {
    fn drop(&mut self) {
        // Stop condition
        self.dp.PORTB.portb.modify(|_, w| w.pb0().clear_bit());
        delay_low();
        self.dp.PORTB.portb.modify(|_, w| w.pb2().set_bit());
        self.dp.PORTB.ddrb.modify(|_, w| w.pb2().clear_bit());
        self.wait_for_scl_high();
        self.dp.PORTB.portb.modify(|_, w| w.pb0().set_bit());
        self.dp.PORTB.ddrb.modify(|_, w| w.pb0().clear_bit());
        self.dp.USI.usicr.reset();
    }
}
