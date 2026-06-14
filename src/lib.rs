#![doc = include_str!("../README.md")]
#![deny(unsafe_code, missing_docs)]
#![no_std]

use bitflags::bitflags;
use core::fmt::Display;
use crc::{Crc, CRC_8_NRSC_5};

#[cfg(not(feature = "async"))]
use embedded_hal as hal;
#[cfg(feature = "async")]
use embedded_hal_async as hal;

use hal::i2c::{Operation, SevenBitAddress};

pub use weather_utils::Temperature;
use weather_utils::{Celsius, RelativeHumidity, TemperatureAndRelativeHumidity};

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
    I2cE: hal::i2c::Error,
{
    /// I²C bus error
    I2c(I2cE),
    /// The computed CRC and the one sent by the device mismatch
    BadCrc,
}

impl<I2cE> From<I2cE> for Error<I2cE>
where
    I2cE: hal::i2c::Error,
{
    fn from(value: I2cE) -> Self {
        Error::I2c(value)
    }
}

/// The repeatability influences the measurement duration and the energy consumption of the sensor
/// It also gives a more or less accurate measurement
///
/// Here are the repeatability values for humidity and temperature:
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

#[derive(Clone, Copy, Debug, Default)]
struct SensorMeasurement {
    humidity: f32,
    temperature: f32,
}

impl From<SensorMeasurement> for TemperatureAndRelativeHumidity<Celsius> {
    fn from(value: SensorMeasurement) -> Self {
        TemperatureAndRelativeHumidity {
            temperature: Celsius(value.temperature),
            relative_humidity: RelativeHumidity::new(value.humidity).unwrap(),
        }
    }
}

/// SHT3x device driver
#[derive(Debug)]
pub struct Sht3x<I2C, D> {
    address: SevenBitAddress,
    delay: D,
    i2c: I2C,
    /// The repeatability to use for measurements (defaults to medium).
    pub repeatability: Repeatability,
}

impl<I2C, D> Sht3x<I2C, D>
where
    I2C: hal::i2c::I2c,
    D: hal::delay::DelayNs,
{
    /// Clear the status of the sensor.
    ///
    /// All the flags of the status register will be cleared (set to zero).
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn clear_status(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, CLEAR_STATUS_COMMAND).await?;
        Ok(())
    }

    /// Deactivate the internal heater.
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn disable_heater(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, DISABLE_HEATER_COMMAND).await?;
        Ok(())
    }

    /// Activate the internal heater.
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn enable_heater(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, ENABLE_HEATER_COMMAND).await?;
        Ok(())
    }

    /// Get the current status of the sensor
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn get_status(&mut self) -> Result<Status, Error<I2C::Error>> {
        let mut data = [0u8; 3];
        let mut operations = [
            Operation::Write(GET_STATUS_COMMAND),
            Operation::Read(&mut data),
        ];
        self.i2c.transaction(self.address, &mut operations).await?;
        let status: &[u8; 2] = &data[0..2].try_into().unwrap();
        let status_crc = data[2];
        Self::check_crc(status, status_crc)?;
        Ok(Status::from_bits_retain(Self::get_u16_value(status)))
    }

    /// Perform a single-shot measurement
    ///
    /// This driver uses clock stretching so the result of the measurement is returned
    /// as soon as the data is available after the measurement command has been sent to the sensor.
    /// Therefore this call will take at least 4 ms and at most 15.5 ms depending on the chosen
    /// repeatability and the supply voltage of the sensor.
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn single_measurement(
        &mut self,
    ) -> Result<TemperatureAndRelativeHumidity<Celsius>, Error<I2C::Error>> {
        let command = match self.repeatability {
            Repeatability::High => MEASUREMENT_HIGH_REPEATIBILITY_COMMAND,
            Repeatability::Medium => MEASUREMENT_MEDIUM_REPEATIBILITY_COMMAND,
            Repeatability::Low => MEASUREMENT_LOW_REPEATIBILITY_COMMAND,
        };
        let mut data = [0u8; 6];
        let mut operations = [Operation::Write(command), Operation::Read(&mut data)];
        self.i2c.transaction(self.address, &mut operations).await?;
        let temperature: &[u8; 2] = &data[0..2].try_into().unwrap();
        let temperature_crc = data[2];
        let humidity: &[u8; 2] = &data[3..5].try_into().unwrap();
        let humidity_crc = data[5];
        Self::check_crc(temperature, temperature_crc)?;
        Self::check_crc(humidity, humidity_crc)?;
        let temperature = Self::get_u16_value(temperature);
        let humidity = Self::get_u16_value(humidity);

        let measurement = SensorMeasurement {
            temperature: ((temperature as f32 * 175.0) / 65535.0) - 45.0,
            humidity: (humidity as f32 * 100.0) / 65535.0,
        };
        Ok(measurement.into())
    }

    /// Create a new instance of the SHT3x device.
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn new(
        i2c: I2C,
        address: SevenBitAddress,
        delay: D,
    ) -> Result<Self, Error<I2C::Error>> {
        let mut dev = Self {
            address,
            delay,
            i2c,
            repeatability: Repeatability::Medium,
        };

        dev.reset().await?;

        Ok(dev)
    }

    /// Perform a soft reset to force the system into a well-defined state without removing
    /// the power supply.
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn reset(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, RESET_COMMAND).await?;
        self.delay.delay_us(1500).await; // Wait for the sensor to enter idle state
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

#[cfg(test)]
mod tests {
    use core::fmt::Write;

    use embedded_hal::i2c::ErrorKind;
    use embedded_hal_mock::eh1::delay::StdSleep as Delay;
    use embedded_hal_mock::eh1::i2c::{Mock as I2cMock, Transaction as I2cTransaction};
    use heapless::String;

    use super::*;

    fn create_device() -> Sht3x<I2cMock, Delay> {
        let expectations = [I2cTransaction::write(
            DEFAULT_I2C_ADDRESS,
            RESET_COMMAND.to_vec(),
        )];
        let i2c = I2cMock::new(&expectations);
        let mut device = Sht3x::new(i2c, DEFAULT_I2C_ADDRESS, Delay {}).unwrap();
        device.i2c.done();
        device
    }

    #[test]
    fn clear_status() {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, CLEAR_STATUS_COMMAND.to_vec()),
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_STATUS_COMMAND.to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x00, 0x81].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        assert!(matches!(device.clear_status(), Ok(())));
        let status = device.get_status();
        let status = match status {
            Ok(s) => s,
            Err(e) => panic!("Expected Ok(status), got Err({e:?})"),
        };
        assert!(!status.contains(Status::WRITE_DATA_CHECKSUM));
        assert!(!status.contains(Status::COMMAND));
        assert!(!status.contains(Status::RESET));
        assert!(!status.contains(Status::T_TRACKING_ALERT));
        assert!(!status.contains(Status::RH_TRACKING_ALERT));
        assert!(!status.contains(Status::HEATER));
        assert!(!status.contains(Status::ALERT_PENDING));
        let mut buffer: String<64> = String::new();
        write!(&mut buffer, "{status}").unwrap();
        assert_eq!(buffer, "");
        device.i2c.done();
    }

    #[test]
    fn get_status() {
        let expectations = [
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_STATUS_COMMAND.to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x00, 0x81].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        let status = device.get_status();
        let status = match status {
            Ok(s) => s,
            Err(e) => panic!("Expected Ok(status), got Err({e:?})"),
        };
        assert!(!status.contains(Status::WRITE_DATA_CHECKSUM));
        assert!(!status.contains(Status::COMMAND));
        assert!(!status.contains(Status::RESET));
        assert!(!status.contains(Status::T_TRACKING_ALERT));
        assert!(!status.contains(Status::RH_TRACKING_ALERT));
        assert!(!status.contains(Status::HEATER));
        assert!(!status.contains(Status::ALERT_PENDING));
        let mut buffer: String<64> = String::new();
        write!(&mut buffer, "{status}").unwrap();
        assert_eq!(buffer, "");
        device.i2c.done();
    }

    #[test]
    fn get_status_bad_crc() {
        let expectations = [
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_STATUS_COMMAND.to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x00, 0x73].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        let err = device.get_status().expect_err("Bad CRC");
        assert!(matches!(err, Error::BadCrc));
        device.i2c.done();
    }

    #[test]
    fn heater() {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, ENABLE_HEATER_COMMAND.to_vec()),
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_STATUS_COMMAND.to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x20, 0x03, 0x0e].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, DISABLE_HEATER_COMMAND.to_vec()),
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_STATUS_COMMAND.to_vec()),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x03, 0xd2].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        assert!(matches!(device.enable_heater(), Ok(())));
        let status = device.get_status();
        let status = match status {
            Ok(s) => s,
            Err(e) => panic!("Expected Ok(status), got Err({e:?})"),
        };
        assert!(status.contains(Status::WRITE_DATA_CHECKSUM));
        assert!(status.contains(Status::COMMAND));
        assert!(!status.contains(Status::RESET));
        assert!(!status.contains(Status::T_TRACKING_ALERT));
        assert!(!status.contains(Status::RH_TRACKING_ALERT));
        assert!(status.contains(Status::HEATER));
        assert!(!status.contains(Status::ALERT_PENDING));
        let mut buffer: String<64> = String::new();
        write!(&mut buffer, "{status}").unwrap();
        assert_eq!(buffer, "WRITE_DATA_CHECKSUM | COMMAND | HEATER");
        assert!(matches!(device.disable_heater(), Ok(())));
        let status = device.get_status();
        let status = match status {
            Ok(s) => s,
            Err(e) => panic!("Expected Ok(status), got Err({e:?})"),
        };
        assert!(status.contains(Status::WRITE_DATA_CHECKSUM));
        assert!(status.contains(Status::COMMAND));
        assert!(!status.contains(Status::RESET));
        assert!(!status.contains(Status::T_TRACKING_ALERT));
        assert!(!status.contains(Status::RH_TRACKING_ALERT));
        assert!(!status.contains(Status::HEATER));
        assert!(!status.contains(Status::ALERT_PENDING));
        let mut buffer: String<64> = String::new();
        write!(&mut buffer, "{status}").unwrap();
        assert_eq!(buffer, "WRITE_DATA_CHECKSUM | COMMAND");
        device.i2c.done();
    }

    #[test]
    fn reset() {
        let expectations = [I2cTransaction::write(
            DEFAULT_I2C_ADDRESS,
            RESET_COMMAND.to_vec(),
        )];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        assert!(matches!(device.reset(), Ok(())));
        device.i2c.done();
    }

    #[test]
    fn reset_with_arbitration_loss_error() {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, RESET_COMMAND.to_vec())
                .with_error(ErrorKind::ArbitrationLoss),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        assert!(matches!(device.reset(), Err(Error::I2c(_))));
        device.i2c.done();
    }

    #[test]
    fn single_measurement_high_repeatability() {
        let expectations = [
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                MEASUREMENT_HIGH_REPEATIBILITY_COMMAND.to_vec(),
            ),
            I2cTransaction::read(
                DEFAULT_I2C_ADDRESS,
                [0x5f, 0x58, 0x38, 0x7b, 0xb2, 0x7d].to_vec(),
            ),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.repeatability = Repeatability::High;
        device.i2c.update_expectations(&expectations);
        let measurement = device.single_measurement();
        let measurement = match measurement {
            Ok(m) => m,
            Err(e) => panic!("Expected Ok(measurement), got Err({e:?})"),
        };
        assert_eq!(
            measurement,
            TemperatureAndRelativeHumidity {
                temperature: Celsius(20.18),
                relative_humidity: RelativeHumidity::new(48.32).unwrap()
            }
        );
        device.i2c.done();
    }

    #[test]
    fn single_measurement_low_repeatability() {
        let expectations = [
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                MEASUREMENT_LOW_REPEATIBILITY_COMMAND.to_vec(),
            ),
            I2cTransaction::read(
                DEFAULT_I2C_ADDRESS,
                [0x5f, 0x58, 0x38, 0x7b, 0xb2, 0x7d].to_vec(),
            ),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.repeatability = Repeatability::Low;
        device.i2c.update_expectations(&expectations);
        let measurement = device.single_measurement();
        let measurement = match measurement {
            Ok(m) => m,
            Err(e) => panic!("Expected Ok(measurement), got Err({e:?})"),
        };
        assert_eq!(
            measurement,
            TemperatureAndRelativeHumidity {
                temperature: Celsius(20.18),
                relative_humidity: RelativeHumidity::new(48.32).unwrap()
            }
        );
        device.i2c.done();
    }

    #[test]
    fn single_measurement_medium_repeatability() {
        let expectations = [
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                MEASUREMENT_MEDIUM_REPEATIBILITY_COMMAND.to_vec(),
            ),
            I2cTransaction::read(
                DEFAULT_I2C_ADDRESS,
                [0x71, 0x17, 0x9a, 0xcb, 0x91, 0x39].to_vec(),
            ),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        let measurement = device.single_measurement();
        let measurement = match measurement {
            Ok(m) => m,
            Err(e) => panic!("Expected Ok(measurement), got Err({e:?})"),
        };
        assert_eq!(
            measurement,
            TemperatureAndRelativeHumidity {
                temperature: Celsius(32.31),
                relative_humidity: RelativeHumidity::new(79.52).unwrap()
            }
        );
        device.i2c.done();
    }
}
