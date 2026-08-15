// TODO: Proper embedded_hal library goes here

use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::{ErrorType, I2c};

const AHT10_ADDR: u8 = 0x38;

// pub enum Error {
//     I2c(ErrorType::Error),
// }

#[derive(Debug)]
pub struct Aht10Reading {
    pub temperature: f32,
    pub humidity: f32,
}

pub struct Aht10Controller<I2C, Delay> {
    i2c: I2C,
    delay: Delay,
    _command: [u8; 3],
    response: [u8; 7],
}

impl<I2C: I2c, Delay: DelayNs> Aht10Controller<I2C, Delay> {
    pub fn new(i2c: I2C, delay: Delay) -> Self {
        Self {
            i2c,
            delay,
            _command: [0; _],
            response: [0; _],
        }
    }

    pub fn reset(&mut self) {
        log::info!("Sending soft reset (single byte)");
        match self.i2c.write(AHT10_ADDR, &[0xBA]) {
            Ok(()) => log::info!("Soft reset ACKed"),
            Err(e) => log::error!("Soft reset failed: {:?}", e),
        };
        self.delay.delay_ms(30);
    }

    pub fn init(&mut self) -> Result<(), <I2C as ErrorType>::Error> {
        log::info!("Sending init/calibrate command");
        self.i2c.write(AHT10_ADDR, &[0xBE, 0x08, 0x00])?;
        self.delay.delay_ms(20);
        Ok(())
    }

    pub fn read(&mut self) -> Result<Aht10Reading, <I2C as ErrorType>::Error> {
        log::info!("Triggering measurement");
        self.i2c.write(AHT10_ADDR, &[0xAC, 0x33, 0x00])?;

        log::info!("Reading data");
        loop {
            match self.i2c.read(AHT10_ADDR, &mut self.response) {
                Ok(()) => {
                    let busy_bit = self.response[0] >> 7;
                    if busy_bit == 1 {
                        log::info!("State: {:b}. Sensor is still busy", self.response[0]);
                        self.delay.delay_ms(500);
                        continue;
                    }

                    log::info!("Data: {:?}", self.response);
                    break;
                }
                Err(e) => log::error!("Error: {:?}", e),
            };
        }

        let raw_humidity = ((self.response[1] as u32) << 12)
            | ((self.response[2] as u32) << 4)
            | ((self.response[3] as u32) >> 4);
        let humidity = (raw_humidity as f32 / (1 << 20) as f32) * 100.0;

        let raw_temperature = (((self.response[3] & 0x0F) as u32) << 16)
            | ((self.response[4] as u32) << 8)
            | (self.response[5] as u32);
        // https://github.com/enjoyneering/AHT10/blob/master/src/AHT10.cpp
        let temperature = (raw_temperature as f32) * 0.000191 - 50.0;

        Ok(Aht10Reading {
            temperature,
            humidity,
        })
    }
}
