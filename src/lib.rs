#![doc = include_str!("../README.md")]
#![deny(unsafe_code, missing_docs)]
#![no_std]

use bitflags::bitflags;
use core::fmt::Display;
use crc::{Crc, CRC_8_NRSC_5};
use embedded_hal::i2c::{Operation, SevenBitAddress};
#[allow(unused_imports)]
use micromath::F32Ext;

/// The I2C address when the ADDR pin is connected to logic low
pub const I2C_ADDRESS_LOGIC_LOW: SevenBitAddress = 0x44;
/// The I2C address when the ADDR pin is connected to logic high
pub const I2C_ADDRESS_LOGIC_HIGH: SevenBitAddress = 0x45;
/// The default I2C address (ADDR pin connected to low)
pub const DEFAULT_I2C_ADDRESS: SevenBitAddress = I2C_ADDRESS_LOGIC_LOW;

const CLEAR_STATUS_COMMAND: &[u8] = &[0x30, 0x41];
const DISABLE_HEATER_COMMAND: &[u8] = &[0x30, 0x66];
const ENABLE_HEATER_COMMAND: &[u8] = &[0x30, 0x6d];
const GET_STATUS_COMMAND: &[u8] = &[0xf3, 0x2d];
const MEASUREMENT_HIGH_REPEATIBILITY_COMMAND: &[u8] = &[0x2c, 0x06];
const MEASUREMENT_MEDIUM_REPEATIBILITY_COMMAND: &[u8] = &[0x2c, 0x0d];
const MEASUREMENT_LOW_REPEATIBILITY_COMMAND: &[u8] = &[0x2c, 0x10];
const RESET_COMMAND: &[u8] = &[0x30, 0xa2];

/// All possible errors generated when using the Sht3x struct
#[derive(Debug)]
pub enum Error<I2cE>
where
    I2cE: embedded_hal::i2c::Error,
{
    /// I²C bus error
    I2c(I2cE),
    /// The computed CRC and the one sent by the device mismatch
    BadCrc,
}

impl<I2cE> From<I2cE> for Error<I2cE>
where
    I2cE: embedded_hal::i2c::Error,
{
    fn from(value: I2cE) -> Self {
        Error::I2c(value)
    }
}

/// The repeatability influences the measument duration and the energy consumption of the sensor
/// It also gives a more or less accurate measurement
///
/// Here are the repeatibility values for humidity and temperature:
///  - Low repeatability: 0.21 %RH - 0.15 °C
///  - Medium repeatability: 0.15 %RH - 0.08 °C
///  - High repeatability: 0.08 %RH - 0.04 °C
///
/// The measurement durations are the following:
///  - Low repeatability: 4 ms (with supply voltage of 2.4-5.5 V) or 4.5 ms (with supply voltage of 2.15-2.4 V)
///  - Medium repeatability: 6 ms (with supply voltage of 2.4-5.5 V) or 6.5 ms (with supply voltage of 2.15-2.4 V)
///  - High repeatability: 15 ms (with supply voltage of 2.4-5.5 V) or 15.5 ms (with supply voltage of 2.15-2.4 V)
#[derive(Debug)]
pub enum Repeatability {
    /// High repeatability: 0.08 %RH - 0.04 °C
    High,
    /// Medium repeatability: 0.15 %RH - 0.08 °C
    Medium,
    /// Low repeatability: 0.21 %RH - 0.15 °C
    Low,
}

bitflags! {
    /// The status of the sensor.
    ///
    /// It gives information on the operational status of the heater, the alert
    /// mode and on the execution status of the last command and the last write
    /// sequence.
    #[derive(Debug)]
    pub struct Status: u16 {
        /// Write data checksum status
        ///
        /// - '0': checksum of last write transfer was correct
        /// - '1': checksum of last write transfer was incorrect
        const WRITE_DATA_CHECKSUM = 1 << 0;
        /// Command status
        ///
        /// - '0': last command executed successfully
        /// - '1': last command not processed. It was either invalid or failed
        /// the integrated command checksum
        const COMMAND = 1 << 1;
        /// System reset detected
        ///
        /// - '0': no reset detected since last [Sht3x<I2C, D>::clear_status()]
        /// call
        /// - '1': reset detected (hard reset, supply fail or soft reset
        /// ([Sht3x<I2c, D>::reset()])
        const RESET = 1 << 4;
        /// Temperature tracking alert
        ///
        /// - '0': no alert
        /// - '1': alert
        const T_TRACKING_ALERT = 1 << 10;
        /// Relative humidity tracking alert
        ///
        /// - '0': no alert
        /// - '1': alert
        const RH_TRACKING_ALERT = 1 << 11;
        /// Heater status
        ///
        /// - '0': Heater OFF
        /// - '1': Heater ON
        const HEATER = 1 << 13;
        /// Alert pending status
        ///
        /// - '0': no pending alerts
        /// - '1': at least one pending alert
        const ALERT_PENDING = 1 << 15;
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        bitflags::parser::to_writer(self, f)
    }
}

/// The temperature unit to use in the measurements.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TemperatureUnit {
    #[default]
    /// Temperature in °C.
    Celcius,
    /// Temperature in °F.
    Farenheit,
}

/// The result of a measurement.
///
/// Such a measurement can be obtained using [`Sht3x::single_measurement()`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Measurement {
    /// The measured relative humidity (in %).
    pub humidity: f32,
    /// The measured temperature (either in °C or °F according to the configuration of the device)
    pub temperature: f32,
    /// The temperature unit used for the measurement
    pub unit: TemperatureUnit,
}

/// SHT3x device driver
#[derive(Debug)]
pub struct Sht3x<I2C, D> {
    address: SevenBitAddress,
    delay: D,
    i2c: I2C,
    /// The repeatability to use for measurements (defaults to medium).
    pub repeatability: Repeatability,
    /// The temperature unit to use for measurements (defaults to celcius).
    pub unit: TemperatureUnit,
}

impl<I2C, D> Sht3x<I2C, D>
where
    I2C: embedded_hal::i2c::I2c,
    D: embedded_hal::delay::DelayNs,
{
    /// Clear the status of the sensor.
    ///
    /// All the flags of the status register will be cleared (set to zero).
    pub fn clear_status(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, CLEAR_STATUS_COMMAND)?;
        Ok(())
    }

    /// Deactivate the internal heater.
    pub fn disable_heater(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, DISABLE_HEATER_COMMAND)?;
        Ok(())
    }

    /// Activate the internal heater.
    pub fn enable_heater(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, ENABLE_HEATER_COMMAND)?;
        Ok(())
    }

    /// Get the current status of the sensor
    pub fn get_status(&mut self) -> Result<Status, Error<I2C::Error>> {
        let mut status = [0u8; 2];
        let mut status_crc = [0u8; 1];
        let mut operations = [
            Operation::Write(GET_STATUS_COMMAND),
            Operation::Read(&mut status),
            Operation::Read(&mut status_crc),
        ];
        self.i2c.transaction(self.address, &mut operations)?;
        Self::check_crc(&status, status_crc[0])?;
        Ok(Status::from_bits_retain(Self::get_u16_value(&status)))
    }

    /// Perform a single-shot measurement
    ///
    /// This driver uses clock stretching so the result of the measurement is returned
    /// as soon as the data is available after the measurement command has been sent to the sensor.
    /// Therefore this call will block for a least 4 ms and at most 15.5 ms depending on the chosen
    /// repeatability and the supply voltage of the sensor.
    pub fn single_measurement(&mut self) -> Result<Measurement, Error<I2C::Error>> {
        let command = match self.repeatability {
            Repeatability::High => MEASUREMENT_HIGH_REPEATIBILITY_COMMAND,
            Repeatability::Medium => MEASUREMENT_MEDIUM_REPEATIBILITY_COMMAND,
            Repeatability::Low => MEASUREMENT_LOW_REPEATIBILITY_COMMAND,
        };
        let mut temperature = [0u8; 2];
        let mut humidity = [0u8; 2];
        let mut temperature_crc = [0u8; 1];
        let mut humidity_crc = [0u8; 1];
        let mut operations = [
            Operation::Write(command),
            Operation::Read(&mut temperature),
            Operation::Read(&mut temperature_crc),
            Operation::Read(&mut humidity),
            Operation::Read(&mut humidity_crc),
        ];
        self.i2c.transaction(self.address, &mut operations)?;
        Self::check_crc(&temperature, temperature_crc[0])?;
        Self::check_crc(&humidity, humidity_crc[0])?;
        let temperature = Self::get_u16_value(&temperature);
        let humidity = Self::get_u16_value(&humidity);

        Ok(Measurement {
            temperature: match self.unit {
                TemperatureUnit::Celcius => ((temperature as f32 * 175.0) / 65535.0) - 45.0,
                TemperatureUnit::Farenheit => ((temperature as f32 * 315.0) / 65535.0) - 49.0,
            },
            humidity: (humidity as f32 * 100.0) / 65535.0,
            unit: self.unit,
        })
    }

    /// Create a new instance of the SHT3x device.
    pub fn new(i2c: I2C, address: SevenBitAddress, delay: D) -> Self {
        Self {
            address,
            delay,
            i2c,
            repeatability: Repeatability::Medium,
            unit: TemperatureUnit::Celcius,
        }
    }

    /// Perform a soft reset to force the system into a well-defined state without removing
    /// the power supply.
    pub fn reset(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, RESET_COMMAND)?;
        self.delay.delay_us(1500); // Wait for the sensor to enter idle state
        Ok(())
    }

    fn calc_crc(data: &[u8; 2]) -> u8 {
        let crc = Crc::<u8>::new(&CRC_8_NRSC_5);
        let mut digest = crc.digest();
        digest.update(data);
        digest.finalize()
    }

    fn check_crc(data: &[u8; 2], expected_crc: u8) -> Result<(), Error<I2C::Error>> {
        if Self::calc_crc(data) != expected_crc {
            Err(Error::BadCrc)
        } else {
            Ok(())
        }
    }

    #[inline]
    fn get_u16_value(data: &[u8; 2]) -> u16 {
        (data[0] as u16) << 8 | (data[1] as u16)
    }
}

/// Converts a relative humidity value in % to an absolute humidity value in g/m³,
/// temperature being in °C.
pub fn calculate_absolute_humidity(measurement: Measurement) -> f32 {
    let temperature = match measurement.unit {
        TemperatureUnit::Celcius => measurement.temperature,
        TemperatureUnit::Farenheit => convert_farenheit_to_celcius(measurement.temperature),
    };
    (6.112 * ((17.67 * temperature) / (temperature + 243.5)).exp() * measurement.humidity * 2.1674)
        / (273.15 + temperature)
}

/// Converts a temperature in °C to °F.
pub fn convert_celcius_to_farenheit(temperature: f32) -> f32 {
    temperature * 1.8 + 32.0
}

/// Converts a temperature in °F to °C.
pub fn convert_farenheit_to_celcius(temperature: f32) -> f32 {
    (temperature - 32.0) * 0.55555
}

#[cfg(test)]
mod tests {
    use crate::*;
    use embedded_hal_mock::eh1::delay::StdSleep as Delay;
    use embedded_hal_mock::eh1::i2c::{Mock as I2cMock, Transaction as I2cTransaction};

    #[test]
    fn calculate_absolute_humidity() {
        assert!(
            (crate::calculate_absolute_humidity(Measurement {
                humidity: 45.59,
                temperature: 21.18,
                unit: TemperatureUnit::Celcius
            }) - 8.43)
                .abs()
                < 0.01
        );
        assert!(
            (crate::calculate_absolute_humidity(Measurement {
                humidity: 45.59,
                temperature: 70.12,
                unit: TemperatureUnit::Farenheit
            }) - 8.43)
                .abs()
                < 0.01
        );
        assert!(
            (crate::calculate_absolute_humidity(Measurement {
                humidity: 34.71,
                temperature: 2.93,
                unit: TemperatureUnit::Celcius
            }) - 2.06)
                .abs()
                < 0.01
        );
        assert!(
            (crate::calculate_absolute_humidity(Measurement {
                humidity: 74.91,
                temperature: 107.7,
                unit: TemperatureUnit::Farenheit
            }) - 42.49)
                .abs()
                < 0.01
        );
    }

    #[test]
    fn clear_status() {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, CLEAR_STATUS_COMMAND.to_vec()),
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_STATUS_COMMAND.to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x00].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x81].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut i2c = I2cMock::new(&expectations);
        let mut device = Sht3x::new(&mut i2c, DEFAULT_I2C_ADDRESS, Delay {});
        device.clear_status().unwrap();
        let status = device.get_status().unwrap();
        assert!(!status.contains(Status::WRITE_DATA_CHECKSUM));
        assert!(!status.contains(Status::COMMAND));
        assert!(!status.contains(Status::RESET));
        assert!(!status.contains(Status::T_TRACKING_ALERT));
        assert!(!status.contains(Status::RH_TRACKING_ALERT));
        assert!(!status.contains(Status::HEATER));
        assert!(!status.contains(Status::ALERT_PENDING));
        i2c.done();
    }

    #[test]
    fn convert_celcius_to_farenheit() {
        assert!((crate::convert_celcius_to_farenheit(0.0) - 32.0).abs() < 0.01);
        assert!((crate::convert_celcius_to_farenheit(15.73) - 60.31).abs() < 0.01);
        assert!((crate::convert_celcius_to_farenheit(-7.49) - 18.52).abs() < 0.01);
        assert!((crate::convert_celcius_to_farenheit(37.5) - 99.5).abs() < 0.01);
    }

    #[test]
    fn convert_farenheit_to_celcius() {
        assert!((crate::convert_farenheit_to_celcius(32.0) - 0.0).abs() < 0.01);
        assert!((crate::convert_farenheit_to_celcius(60.31) - 15.73).abs() < 0.01);
        assert!((crate::convert_farenheit_to_celcius(18.52) - -7.49).abs() < 0.01);
        assert!((crate::convert_farenheit_to_celcius(99.5) - 37.5).abs() < 0.01);
    }

    #[test]
    fn get_status() {
        let expectations = [
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_STATUS_COMMAND.to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x00].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x81].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut i2c = I2cMock::new(&expectations);
        let mut device = Sht3x::new(&mut i2c, DEFAULT_I2C_ADDRESS, Delay {});
        let status = device.get_status().unwrap();
        assert!(!status.contains(Status::WRITE_DATA_CHECKSUM));
        assert!(!status.contains(Status::COMMAND));
        assert!(!status.contains(Status::RESET));
        assert!(!status.contains(Status::T_TRACKING_ALERT));
        assert!(!status.contains(Status::RH_TRACKING_ALERT));
        assert!(!status.contains(Status::HEATER));
        assert!(!status.contains(Status::ALERT_PENDING));
        i2c.done();
    }

    #[test]
    fn heater() {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, ENABLE_HEATER_COMMAND.to_vec()),
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_STATUS_COMMAND.to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x20, 0x03].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x0e].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, DISABLE_HEATER_COMMAND.to_vec()),
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_STATUS_COMMAND.to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x03].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0xd2].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut i2c = I2cMock::new(&expectations);
        let mut device = Sht3x::new(&mut i2c, DEFAULT_I2C_ADDRESS, Delay {});
        device.enable_heater().unwrap();
        let status = device.get_status().unwrap();
        assert!(status.contains(Status::WRITE_DATA_CHECKSUM));
        assert!(status.contains(Status::COMMAND));
        assert!(!status.contains(Status::RESET));
        assert!(!status.contains(Status::T_TRACKING_ALERT));
        assert!(!status.contains(Status::RH_TRACKING_ALERT));
        assert!(status.contains(Status::HEATER));
        assert!(!status.contains(Status::ALERT_PENDING));
        device.disable_heater().unwrap();
        let status = device.get_status().unwrap();
        assert!(status.contains(Status::WRITE_DATA_CHECKSUM));
        assert!(status.contains(Status::COMMAND));
        assert!(!status.contains(Status::RESET));
        assert!(!status.contains(Status::T_TRACKING_ALERT));
        assert!(!status.contains(Status::RH_TRACKING_ALERT));
        assert!(!status.contains(Status::HEATER));
        assert!(!status.contains(Status::ALERT_PENDING));
        i2c.done();
    }

    #[test]
    fn reset() {
        let expectations = [I2cTransaction::write(
            DEFAULT_I2C_ADDRESS,
            RESET_COMMAND.to_vec(),
        )];
        let mut i2c = I2cMock::new(&expectations);
        let mut device = Sht3x::new(&mut i2c, DEFAULT_I2C_ADDRESS, Delay {});
        device.reset().unwrap();
        i2c.done();
    }

    #[test]
    fn single_measurement_farenheit() {
        let expectations = [
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                MEASUREMENT_MEDIUM_REPEATIBILITY_COMMAND.to_vec(),
            ),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x71, 0x17].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x9a].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0xcb, 0x91].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x39].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut i2c = I2cMock::new(&expectations);
        let mut device = Sht3x::new(&mut i2c, DEFAULT_I2C_ADDRESS, Delay {});
        device.unit = TemperatureUnit::Farenheit;
        let measurement = device.single_measurement().unwrap();
        assert!((measurement.temperature - 90.16).abs() < 0.01);
        assert!((measurement.humidity - 79.52).abs() < 0.01);
        i2c.done();
    }

    #[test]
    fn single_measurement_high_repeatability() {
        let expectations = [
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                MEASUREMENT_HIGH_REPEATIBILITY_COMMAND.to_vec(),
            ),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x5f, 0x58].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x38].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x7b, 0xb2].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x7d].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut i2c = I2cMock::new(&expectations);
        let mut device = Sht3x::new(&mut i2c, DEFAULT_I2C_ADDRESS, Delay {});
        device.repeatability = Repeatability::High;
        let measurement = device.single_measurement().unwrap();
        assert!((measurement.temperature - 20.18).abs() < 0.01);
        assert!((measurement.humidity - 48.32).abs() < 0.01);
        i2c.done();
    }

    #[test]
    fn single_measurement_low_repeatability() {
        let expectations = [
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                MEASUREMENT_LOW_REPEATIBILITY_COMMAND.to_vec(),
            ),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x5f, 0x58].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x38].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x7b, 0xb2].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x7d].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut i2c = I2cMock::new(&expectations);
        let mut device = Sht3x::new(&mut i2c, DEFAULT_I2C_ADDRESS, Delay {});
        device.repeatability = Repeatability::Low;
        let measurement = device.single_measurement().unwrap();
        assert!((measurement.temperature - 20.18).abs() < 0.01);
        assert!((measurement.humidity - 48.32).abs() < 0.01);
        i2c.done();
    }

    #[test]
    fn single_measurement_medium_repeatability() {
        let expectations = [
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                MEASUREMENT_MEDIUM_REPEATIBILITY_COMMAND.to_vec(),
            ),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x71, 0x17].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x9a].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0xcb, 0x91].to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x39].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut i2c = I2cMock::new(&expectations);
        let mut device = Sht3x::new(&mut i2c, DEFAULT_I2C_ADDRESS, Delay {});
        let measurement = device.single_measurement().unwrap();
        assert!((measurement.temperature - 32.31).abs() < 0.01);
        assert!((measurement.humidity - 79.52).abs() < 0.01);
        i2c.done();
    }
}
