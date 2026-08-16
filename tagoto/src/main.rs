use esp_idf_svc::hal::adc::attenuation::DB_12;
use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_svc::hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyInputPin, AnyOutputPin};
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::uart::{UartConfig, UartDriver};
use esp_idf_svc::hal::units::{Hertz, KiloHertz};
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};

use pms7003_rs::{Pms7003Controller, Pms7003DataFrame};
use std::sync::LazyLock;
use zerocopy::Ref;

mod aht;
mod bluetooth;
mod gas;

static DEFAULT_PMS_BYTES: LazyLock<[u8; core::mem::size_of::<Pms7003DataFrame>()]> =
    LazyLock::new(|| {
        let frame = Pms7003DataFrame::default();
        let mut buf = [0u8; core::mem::size_of::<Pms7003DataFrame>()];
        buf.copy_from_slice(zerocopy::IntoBytes::as_bytes(&frame));
        buf
    });

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
    let r0 = gas::get_or_calibrate_r0(&adc, &mut mq135_pin, &mut nvs_config, &mut FreeRtos)?;

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

    log::info!("Initializing PMS7003");
    let uart_config = UartConfig::new()
        .baudrate(Hertz(9600))
        .stop_bits(esp_idf_svc::hal::uart::config::StopBits::STOP1)
        .parity_none();
    let uart = UartDriver::new(
        peripherals.uart0,
        peripherals.pins.gpio21,
        peripherals.pins.gpio20,
        Option::<AnyInputPin>::None,
        Option::<AnyOutputPin>::None,
        &uart_config,
    )?;

    log::info!("Initializing PMS7003 sensor");
    let mut pms7003 = Pms7003Controller::new(uart, FreeRtos);

    let _ = pms7003
        .sleep()
        .map_err(|err| log::error!("Failed to put to sleep: {:?}", err));

    FreeRtos::delay_ms(500);
    let _ = pms7003
        .wake()
        .map_err(|err| log::error!("Failed to issue wakeup command: {:?}", err));

    FreeRtos::delay_ms(500);
    let _ = pms7003
        .passive()
        .map_err(|err| log::error!("Failed to issue set passive: {:?}", err));

    log::info!("Initializing BLE stack");
    let sensor_chara = bluetooth::init_ble()?;

    loop {
        FreeRtos::delay_ms(1000);
        let pms7003_reading: Ref<&[u8], Pms7003DataFrame> = match pms7003.read_passive() {
            Ok(f) => f,
            Err(e) => {
                log::error!("Failed to perform PM reading: {:?}", e);
                Ref::from_bytes(&DEFAULT_PMS_BYTES[..]).unwrap()
            }
        };
        let aht10_reading = aht10.read()?;
        let mq135_reading = gas::perform_reading(&adc, &mut mq135_pin, r0, &mut FreeRtos)?;

        let render_string = format!(
            "T: {:.2} C | H: {:.2} %RH | H2: {:.2} ppm | NH3: {:.2} ppm | Toluene: {:.2} ppm | PM2.5: {} ug/m3 | PM10: {} ug/m3\n",
            aht10_reading.temperature,
            aht10_reading.humidity,
            mq135_reading.h2,
            mq135_reading.nh3,
            mq135_reading.toluene,
            pms7003_reading.pm2_5_atm,
            pms7003_reading.pm10_std
        );

        log::info!(
            "T: {:.2} C | H: {:.2} %RH | H2: {:.2} ppm | NH3: {:.2} ppm | Toluene: {:.2} ppm | PM2.5: {} ug/m3 | PM10: {} ug/m3",
            aht10_reading.temperature,
            aht10_reading.humidity,
            mq135_reading.h2,
            mq135_reading.nh3,
            mq135_reading.toluene,
            pms7003_reading.pm2_5_atm,
            pms7003_reading.pm10_std
        );

        let mut sc_lock = sensor_chara.lock();
        sc_lock.set_value(render_string.as_bytes());
        sc_lock.notify();
    }
}
