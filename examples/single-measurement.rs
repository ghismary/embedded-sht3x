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
