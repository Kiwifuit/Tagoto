use std::net::Ipv4Addr;
use std::time::Duration;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::ping::{Configuration as PingConfiguration, EspPing};
use esp_idf_svc::wifi::{ClientConfiguration, Configuration as WifiConfiguration, EspWifi};

pub struct WifiCredentials {
    pub ssid: &'static str,
    pub passwd: &'static str,
}

fn has_internet() -> bool {
    let mut ping = EspPing::default();

    log::info!("Checking for internet");

    let ip = Ipv4Addr::new(8, 8, 8, 8);

    let ping_config = PingConfiguration {
        count: 5,
        timeout: Duration::from_secs(1),
        ..Default::default()
    };

    match ping.ping(ip, &ping_config) {
        Ok(summary) => {
            log::info!("Ping Result: {}/{}", summary.received, summary.transmitted);
            summary.received > 0
        }
        Err(err) => {
            log::error!("Failed to ping: {}", err);
            false
        }
    }
}

fn connect_to_wifi(wifi: &mut EspWifi, network_pool: &[WifiCredentials]) -> anyhow::Result<()> {
    for cred in network_pool {
        log::info!("Trying network: {}", cred.ssid);

        let config = WifiConfiguration::Client(ClientConfiguration {
            ssid: cred.ssid.try_into()?,
            password: cred.passwd.try_into()?,
            ..Default::default()
        });
        wifi.set_configuration(&config)?;
        wifi.start()?;

        if wifi.connect().is_err() {
            log::warn!("Failed to connect to {}", cred.ssid);
            continue;
        }

        let mut waited_ms = 0;
        while !wifi.is_up()? && waited_ms < 10_000 {
            FreeRtos::delay_ms(500);
            waited_ms += 500;
        }

        if wifi.is_up()? && has_internet() {
            log::info!("Connected via {}", cred.ssid);
            return Ok(());
        }

        log::warn!("{} associated but no internet, trying next", cred.ssid);
    }

    anyhow::bail!("Exhausted all known networks")
}

fn wifi_watchdawg(wifi: &mut EspWifi)