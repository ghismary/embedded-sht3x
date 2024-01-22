This is a platform agnostic Rust driver the SHT3x (SHT30, SHT31 and SHT35) digital
humidity and temperature sensors using the [`embedded-hal`] traits.

[`embedded-hal`]: https://github.com/rust-embedded/embedded-hal

## The device

The sensors of the SHT3x family are humidity and temperature sensors,
that are fully calibrated and working with wide range of supply voltages,
from 2.15 V to 5.5 V, and using an I²C interface.

They procure fast start-up and measurement times, and great accuracy.
The typical accuracies are the following:

| SHT30            | SHT31            | SHT35              |
| ---------------- | ---------------- | ------------------ |
| ±2 %RH / ±0.2 °C | ±2 %RH / ±0.2 °C | ±1.5 %RH / ±0.1 °C |

The SHT3x is equipped with an internal heater which can increase the
temperature in the range of a few degrees centigrade, and that is useful
for pausibility checks.

### Documentation:

- [Datasheet](https://sensirion.com/media/documents/213E6A3B/63A5A569/Datasheet_SHT3x_DIS.pdf)
- [Introduction to humidity](https://www.sensirion.com/media/documents/8AB2AD38/61642ADD/Sensirion_AppNotes_Humidity_Sensors_Introduction_to_Relative_Humidit.pdf)
- [Humidity at a glance](https://www.sensirion.com/media/documents/A419127A/6163F5FE/Sensirion_AppNotes_Humidity_Sensors_at_a_Glance.pdf)
- [Arduino driver](https://github.com/Sensirion/arduino-sht)

## Features

- [x] Get the status of the sensor.
- [x] Clear the status of the sensor.
- [x] Enable the internal heater.
- [x] Disable the internal heater.
- [x] Perform a single-shot measurement of temperature and relative humidity.
- [x] Do a sofware set.
- [ ] Perform periodic measurement of temperature and relative humidity.
- [ ] Include a no floating-point variant for systems without fpu.

## Usage

To use this driver, import what you need from this crate and an `embedded-hal`
implentation, then instatiate the device.

```rust,no_run
use embedded_sht3x::{Repeatability::High, Sht3x, DEFAULT_I2C_ADDRESS};
use linux_embedded_hal as hal;

fn main() -> Result<(), embedded_sht3x::Error<linux_embedded_hal::I2CError>> {
    // Create the I2C device from the chosen embedded-hal implementation,
    // in this case linux-embedded-hal
    let i2c = match hal::I2cdev::new("/dev/i2c-1") {
        Err(err) => {
            eprintln!("Could not create I2C device: {}", err);
            std::process::exit(1);
        }
        Ok(i2c) => i2c,
    };

    // Create the sensor and configure its repeatability
    let mut sensor = Sht3x::new(i2c, DEFAULT_I2C_ADDRESS, hal::Delay {});
    sensor.repeatability = High;

    // Perform a temperature and humidity measurement
    let measurement = sensor.single_measurement()?;
    println!(
        "Temperature: {:.2} °C, Relative humidity: {:.2} %",
        measurement.temperature, measurement.humidity
    );
    Ok(())
}
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
