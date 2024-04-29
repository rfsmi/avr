use crate::{i2c, Delay};

use embedded_hal::delay::DelayNs;

const ADDR: u8 = 0x27;
const COLS: usize = 16;
const ROWS: usize = 2;

#[derive(Default)]
pub struct LCD1602 {
    display: [[u8; COLS]; ROWS],
    buffer: [[u8; COLS]; ROWS],
    buffer_cursor: (usize, usize),
}

enum Value {
    Command(u8),
    Data(u8),
}
use itertools::Itertools;
use Value::*;

fn send(value: Value) -> Result<(), &'static str> {
    let (value, mask) = match value {
        Command(value) => (value, 0b1000), // RS=0, RW=0, EN=0
        Data(value) => (value, 0b1001),    // RS=1, RW=0, EN=0
    };
    for nibble in [value & 0xF0, value << 4] {
        i2c::write(ADDR, &[nibble | mask | 0b0100])?; // EN=1
        Delay::new().delay_ms(2);
        i2c::write(ADDR, &[nibble | mask])?;
        Delay::new().delay_ms(2);
    }
    Ok(())
}

pub fn setup() -> Result<LCD1602, &'static str> {
    send(Command(0x33)).map_err(|_| "Failed to initialise to 8-line mode")?;
    Delay::new().delay_ms(5);
    send(Command(0x32)).map_err(|_| "Failed to initialise to 4-line mode")?;
    Delay::new().delay_ms(5);
    send(Command(0x28)).map_err(|_| "Failed to set 2-line and and 5*7 dots mode")?;
    Delay::new().delay_ms(5);
    send(Command(0x0C)).map_err(|_| "Failed to enable display")?;
    Delay::new().delay_ms(5);
    send(Command(0x01)).map_err(|_| "Failed to clear screen")?;
    let lcd = LCD1602 {
        display: [[b' '; COLS]; ROWS],
        buffer: [[b' '; COLS]; ROWS],
        buffer_cursor: (0, 0),
    };
    Ok(lcd)
}

impl LCD1602 {
    pub fn sync(&mut self) -> Result<(), &'static str> {
        let (mut cursor_y, mut cursor_x) = (ROWS, COLS);
        for (y, x) in (0..ROWS).cartesian_product(0..COLS) {
            if self.buffer[y][x] == self.display[y][x] {
                continue;
            }
            if (cursor_x, cursor_y) != (x, y) {
                let cmd = 0x80 + 0x40 * (y as u8 & 0x01) + (x as u8 & 0x0F);
                send(Command(cmd)).map_err(|_| "Failed to move LCD cursor")?;
                (cursor_x, cursor_y) = (x, y);
            }
            send(Data(self.buffer[y][x])).map_err(|_| "Failed to send byte to LCD")?;
            self.display[y][x] = self.buffer[y][x];
            cursor_x += 1;
        }
        Ok(())
    }

    pub fn set_cursor(&mut self, x: usize, y: usize) {
        self.buffer_cursor = (x, y);
    }

    pub fn write(&mut self, byte: u8) {
        let (x, y) = &mut self.buffer_cursor;
        if (0..COLS).contains(x) && (0..ROWS).contains(y) {
            self.buffer[*y][*x] = byte;
            *x += 1;
        }
    }

    pub fn write_str(&mut self, msg: &str) {
        for &byte in msg.as_bytes() {
            self.write(byte);
        }
    }
}
