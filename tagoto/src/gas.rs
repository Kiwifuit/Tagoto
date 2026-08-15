use esp_idf_svc::hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
use esp_idf_svc::nvs::{EspNvs, NvsDefault};

const R1: f32 = 100_000.0;
const R2: f32 = 180_000.0;
const RL: f32 = 1_000.0;

const VCC: f32 = 5.0;

const MQ135_H2: GasCurve = GasCurve { m: -1.83, b: 0.69 };
const MQ135_NH3: GasCurve = GasCurve { m: -2.11, b: 0.75 };
const MQ135_TOLUENE: GasCurve = GasCurve { m: -2.30, b: 0.78 };

/// Represents a sensitivity curve for a particular
/// gas
struct GasCurve {
    /// Slope of the curve
    m: f32,
    /// y-intercept of the curve
    b: f32,
}

#[derive(Debug)]
pub struct GasReading {
    pub h2: f32,
    pub nh3: f32,
    pub toluene: f32,
}

fn store_r0(nvs: &mut EspNvs<NvsDefault>, r0: f32) -> anyhow::Result<()> {
    nvs.set_u32("mq135_r0", r0.to_bits())?;
    log::info!("Saved R0 value to NVS");

    Ok(())
}

fn load_r0(nvs: &EspNvs<NvsDefault>) -> Option<f32> {
    log::info!("Attempting to load R0 value from NVS");
    nvs.get_u32("mq135_r0").ok().flatten().map(f32::from_bits)
}

fn read_averaged<'a>(
    adc: &AdcDriver<'a, esp_idf_svc::hal::adc::ADCU1>,
    pin: &mut AdcChannelDriver<
        'a,
        esp_idf_svc::hal::adc::ADCCH0<esp_idf_svc::hal::adc::ADCU1>,
        &AdcDriver<'a, esp_idf_svc::hal::adc::ADCU1>,
    >,
    samples: u8,
) -> u16 {
    let sum: u32 = (0..samples).map(|_| adc.read(pin).unwrap() as u32).sum();
    (sum / samples as u32) as u16
}

fn undivide(measured_mv: u16) -> f32 {
    let measured_v = measured_mv as f32 / 1000.0;
    measured_v * (R1 + R2) / R2
}

fn calc_rs(vout: f32) -> f32 {
    RL * (VCC - vout) / vout
}

fn ratio_to_ppm(rs_r0_ratio: f32, curve: &GasCurve) -> f32 {
    10f32.powf(curve.m * rs_r0_ratio.log10() + curve.b)
}

pub fn get_or_calibrate_r0<'a>(
    adc: &AdcDriver<'a, esp_idf_svc::hal::adc::ADCU1>,
    pin: &mut AdcChannelDriver<
        'a,
        esp_idf_svc::hal::adc::ADCCH0<esp_idf_svc::hal::adc::ADCU1>,
        &AdcDriver<'a, esp_idf_svc::hal::adc::ADCU1>,
    >,
    nvs: &mut EspNvs<NvsDefault>,
) -> anyhow::Result<f32> {
    if let Some(r0) = load_r0(nvs) {
        return Ok(r0); // already calibrated on a previous boot — just reuse it
    }

    // No stored value yet — this must be a fresh calibration in clean air.
    log::info!("Calibrating sensor to determine R0");
    let mv = read_averaged(adc, pin, 10);
    let vout = undivide(mv);
    let r0 = calc_rs(vout);

    store_r0(nvs, r0)?;
    Ok(r0)
}

pub fn perform_reading<'a>(
    adc: &AdcDriver<'a, esp_idf_svc::hal::adc::ADCU1>,
    pin: &mut AdcChannelDriver<
        'a,
        esp_idf_svc::hal::adc::ADCCH0<esp_idf_svc::hal::adc::ADCU1>,
        &AdcDriver<'a, esp_idf_svc::hal::adc::ADCU1>,
    >,
    r0: f32,
) -> anyhow::Result<GasReading> {
    let mv = read_averaged(adc, pin, 10);
    let vout = undivide(mv);
    let rs = calc_rs(vout);
    let ratio = rs / r0;

    let nh3_ppm = ratio_to_ppm(ratio, &MQ135_NH3);
    let h2_ppm = ratio_to_ppm(ratio, &MQ135_H2);
    let toluene_ppm = ratio_to_ppm(ratio, &MQ135_TOLUENE);

    Ok(GasReading {
        h2: h2_ppm,
        nh3: nh3_ppm,
        toluene: toluene_ppm,
    })
}
