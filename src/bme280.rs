use core::iter::zip;

use crate::i2c;
use itertools::Itertools;

const ADDR: u8 = 0x77;

#[derive(Default)]
struct Settings {
    osr_p: u8,        // Pressure oversampling
    osr_t: u8,        // Temperature oversampling
    osr_h: u8,        // Humidity oversampling
    filter: u8,       // Filter coefficient
    standby_time: u8, // Standby time
}

#[derive(Default)]
pub struct BME280 {
    t: [i16; 3],
    settings: Settings,
}

pub fn setup() -> Result<BME280, &'static str> {
    // Set the mode to Normal, no upsampling
    i2c::write(
        ADDR,
        &[
            0xF2, 0b00000001, // ctrl_hum
            0xF4, 0b00100111, // ctrl_meas
            0x88,       // Next read from calibration register
        ],
    )
    .map_err(|_| "Failed to set device mode")?;
    let mut device = BME280 {
        ..Default::default()
    };
    // Read the temperature calibration data.
    let mut data = [0; 6];
    i2c::read(ADDR, &mut data).map_err(|_| "Failed to get calibration data")?;
    for (dest, (&lo, &hi)) in zip(&mut device.t, data.iter().tuples()) {
        *dest = (hi as i16) << 8 | lo as i16;
    }
    Ok(device)
}

impl BME280 {
    // Returns temperature in DegC, resolution is 0.01 DegC. Output value of
    // “5123” equals 51.23 DegC
    pub fn get_temperature(&self) -> Result<i32, &'static str> {
        i2c::write(ADDR, &[0xFA])?;
        let mut bytes = [0; 3];
        i2c::read(ADDR, &mut bytes)?;
        let adc = (bytes[0] as i32) << 12 | (bytes[1] as i32) << 4 | (bytes[2] as i32) >> 4;
        // Compensate
        let t1 = (self.t[0] as u16) as i32;
        let t2 = self.t[1] as i32;
        let t3 = self.t[2] as i32;
        let a = (((adc >> 3) - (t1 << 1)) * t2) >> 11;
        let b = ((((adc >> 4) - t1) * ((adc >> 4) - t1) >> 12) * t3) >> 14;
        Ok(((a + b) * 5 + 128) >> 8)
    }
}
