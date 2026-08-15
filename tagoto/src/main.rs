use esp_idf_svc::hal::adc::attenuation::DB_12;
use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_svc::hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::units::KiloHertz;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};

mod aht;
mod gas;

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;

    log::info!("Initializing NVS partition");
    let nvs_default = EspDefaultNvsPartition::take()?;
    let mut nvs_config = EspNvs::new(nvs_default, "config", true)?;

    log::info!("Initializing ADC device");
    let adc = AdcDriver::new(peripherals.adc1)?;
    let adc_config = AdcChannelConfig {
        attenuation: DB_12,
        calibration: esp_idf_svc::hal::adc::oneshot::config::Calibration::Curve,
        ..Default::default()
    };

    let mut mq135_pin = AdcChannelDriver::new(&adc, peripherals.pins.gpio0, &adc_config)?;

    log::info!("Initializing MQ-135 sensor");
    let r0 = gas::get_or_calibrate_r0(&adc, &mut mq135_pin, &mut nvs_config)?;

    log::info!("Initializing I2C device!");
    let i2c_config = I2cConfig::default().baudrate(KiloHertz(50).into());
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio6,
        peripherals.pins.gpio7,
        &i2c_config,
    )?;

    log::info!("Checking to see if AHT10 sensor is present");
    i2c.write(0x38, &[], 1000)?;
    log::info!("AHT10 sensor is responsive");

    let mut aht10 = aht::Aht10Controller::new(i2c, FreeRtos);

    aht10.reset();
    aht10.init()?;

    loop {
        FreeRtos::delay_ms(500);
        let aht10_reading = aht10.read()?;
        let mq135_reading = gas::perform_reading(&adc, &mut mq135_pin, r0)?;

        log::info!(
            "Raw Gas: {:?} | T: {:.2} C | H: {:.2} %RH",
            mq135_reading,
            aht10_reading.temperature,
            aht10_reading.humidity
        );
    }
}
