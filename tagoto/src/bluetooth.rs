use std::sync::Arc;

use esp32_nimble::utilities::mutex::Mutex;
use esp32_nimble::{
    // enums::{AuthReq, SecurityIOCap},
    uuid128,
    BLEAdvertisementData,
    BLECharacteristic,
    BLEDevice,
    NimbleProperties,
};

const BLE_PIN: u32 = 800815;

pub fn init_ble() -> anyhow::Result<Arc<Mutex<BLECharacteristic>>> {
    let ble_device = BLEDevice::take();

    // ble_device
    //     .security()
    //     .set_auth(AuthReq::Bond | AuthReq::Mitm)
    //     .set_passkey(BLE_PIN)
    //     .set_io_cap(SecurityIOCap::DisplayOnly)
    //     .resolve_rpa();

    let server = ble_device.get_server();

    let service = server.create_service(uuid128!("6E400001-B5A3-F393-E0A9-E50E24DCCA9E"));

    let sensor_reading = service.lock().create_characteristic(
        uuid128!("6E400003-B5A3-F393-E0A9-E50E24DCCA9E"),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );

    let _rx = service.lock().create_characteristic(
        uuid128!("6E400002-B5A3-F393-E0A9-E50E24DCCA9E"),
        NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
    );

    log::info!("Starting BLE advertisement");
    let mut ble_ad_data = BLEAdvertisementData::new();
    let ble_ad_data = ble_ad_data
        .name("Tagoto Node 1")
        .appearance(0x0540)
        .add_service_uuid(uuid128!("6E400001-B5A3-F393-E0A9-E50E24DCCA9E"));
    let ble_ad = ble_device.get_advertising();

    ble_ad.lock().set_data(ble_ad_data)?;
    ble_ad.lock().start()?;

    log::info!("My pin is: {}", BLE_PIN);

    Ok(sensor_reading)
}
