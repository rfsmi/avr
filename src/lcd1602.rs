use crate::i2c;

const ADDR: u8 = 0x27;
const COLS: usize = 16;
const ROWS: usize = 2;

#[derive(Default)]
pub enum Align {
    #[default]
    Left,
    Right,
}

#[derive(Default)]
pub struct LCD1602 {
    display: [[u8; COLS]; ROWS],
    buffer: [[u8; COLS]; ROWS],
    buffer_cursor: (usize, usize),
    align: Align,
}

enum Value {
    Command(u8),
    Data(u8),
}
use itertools::Itertools;
use Value::*;

fn send(value: Value) -> Result<(), &'static str> {
    const BACKLIGHT: u8 = 1 << 3;
    const RS: u8 = 1 << 0;
    const EN: u8 = 1 << 2;
    let (value, mask) = match value {
        Command(value) => (value, BACKLIGHT),
        Data(value) => (value, BACKLIGHT | RS),
    };
    for nibble in [value & 0xF0, value << 4] {
        i2c::write(ADDR, &[nibble | mask | EN])?;
        i2c::write(ADDR, &[nibble | mask])?;
    }
    Ok(())
}

pub fn setup() -> Result<LCD1602, &'static str> {
    send(Command(0x33)).map_err(|_| "Failed to initialise to 8-line mode")?;
    send(Command(0x32)).map_err(|_| "Failed to initialise to 4-line mode")?;
    send(Command(0x28)).map_err(|_| "Failed to set 2-line and and 5*7 dots mode")?;
    send(Command(0x0C)).map_err(|_| "Failed to enable display")?;
    send(Command(0x01)).map_err(|_| "Failed to clear screen")?;
    let lcd = LCD1602 {
        display: [[b' '; COLS]; ROWS],
        buffer: [[b' '; COLS]; ROWS],
        ..Default::default()
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
                let cmd = 0x80 + 0x40 * (y as u8) + (x as u8);
                send(Command(cmd)).map_err(|_| "Failed to move LCD cursor")?;
                (cursor_x, cursor_y) = (x, y);
            }
            send(Data(self.buffer[y][x])).map_err(|_| "Failed to send byte to LCD")?;
            self.display[y][x] = self.buffer[y][x];
            cursor_x += 1;
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        *self = Self {
            display: self.display,
            buffer: [[b' '; COLS]; ROWS],
            ..Default::default()
        }
    }

    pub fn set_cursor(&mut self, x: usize, y: usize) {
        self.buffer_cursor = (x, y);
    }

    pub fn set_align(&mut self, align: Align) {
        self.align = align;
    }

    pub fn write(&mut self, byte: u8) {
        let (x, y) = &mut self.buffer_cursor;
        if (0..COLS).contains(x) && (0..ROWS).contains(y) {
            self.buffer[*y][*x] = byte;
            match self.align {
                Align::Left => *x += 1,
                Align::Right => *x -= 1,
            };
        }
    }

    pub fn write_str(&mut self, msg: &str) {
        let bytes = msg.as_bytes();
        for i in 0..bytes.len() {
            let i = match self.align {
                Align::Left => i,
                Align::Right => bytes.len() - i - 1,
            };
            self.write(bytes[i]);
        }
    }
}
