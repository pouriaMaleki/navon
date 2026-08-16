//! MAX17048 LiPoly/LiIon fuel gauge driver (Adafruit 5580 breakout).
//!
//! The gauge sits **inline** between the 1S cell and the load — see
//! `docs/wiring-lc29h-max17048.md` — so it sees the raw 3.7–4.2 V cell
//! voltage and can report a true state-of-charge via Maxim's ModelGauge
//! algorithm. It talks I²C at the fixed 7-bit address 0x36; on the
//! Waveshare 3.4C it shares the touch/panel bus (SDA=GPIO7, SCL=GPIO8)
//! with the GT911 and CH422G, which is fine — no address collision and
//! the board already has external pull-ups.
//!
//! ## Registers used
//!
//! * `VCELL` (0x02) — cell voltage, 16-bit, 78.125 µV per LSB.
//! * `SOC` (0x04) — state of charge, 16-bit; the high byte is the
//!   integer percent, the low byte the 1/256 % fraction.
//!
//! The conversion helpers (`decode_*`) and [`FuelGaugeReading`] are
//! host-testable; the I²C plumbing is `espidf`-only and reuses the
//! legacy master driver that `i2c_scan`/`touch_gt911` already install.

#[cfg(target_os = "espidf")]
use esp_idf_svc::sys::{
    self, esp_err_t, i2c_cmd_link_create, i2c_cmd_link_delete, i2c_master_cmd_begin,
    i2c_master_read, i2c_master_start, i2c_master_stop, i2c_master_write_byte,
    i2c_port_t_I2C_NUM_0,
};

#[cfg(target_os = "espidf")]
use crate::board_config::FuelGaugeConfig;
#[cfg(target_os = "espidf")]
use crate::esp_idf::EspIdfError;
#[cfg(target_os = "espidf")]
use crate::i2c_scan;

/// MAX17048 register addresses.
#[cfg(target_os = "espidf")]
const REG_VCELL: u8 = 0x02;
#[cfg(target_os = "espidf")]
const REG_SOC: u8 = 0x04;
#[cfg(target_os = "espidf")]
const REG_MODE: u8 = 0x06;
/// MODE bit that requests a quick-start SOC estimate; the IC clears
/// the bit itself when the estimate is ready.
#[cfg(target_os = "espidf")]
const MODE_QUICK_START: u16 = 0x4000;
/// Minimum plausible cell voltage (mV) for quick-start. Below this
/// the battery is absent or too deep-discharged for the estimate to
/// mean anything.
#[cfg(target_os = "espidf")]
const QUICK_START_MIN_CELL_MV: u16 = 3000;

/// VCELL LSB size in nanovolts — 78.125 µV per LSB is 78 125 nV.
const VCELL_NV_PER_LSB: u32 = 78_125;

/// One reading of the gauge: integer battery percent plus the raw cell
/// voltage in millivolts. `present: false` marks a bus failure so the
/// overlay can show the readout as stale/grey instead of fabricating
/// numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FuelGaugeReading {
    pub percent: u8,
    pub voltage_mv: u16,
    pub present: bool,
}

/// Decode a 16-bit `VCELL` register value into millivolts.
/// 78.125 µV per LSB (78 125 nV); the result is truncated to the
/// integer mV. The `u32` intermediate handles the full scale: the
/// maximum raw value (~65 535) is ~5.12 V, well within `u32`.
pub fn decode_vcell_mv(raw: u16) -> u16 {
    ((u32::from(raw) * VCELL_NV_PER_LSB) / 1_000_000) as u16
}

/// Decode a 16-bit `SOC` register value into the integer percent
/// (high byte; low byte holds 1/256 % fractions which we drop).
pub fn decode_soc_percent(raw: u16) -> u8 {
    (raw >> 8) as u8
}

#[cfg(target_os = "espidf")]
pub struct Max17048FuelGauge {
    addr: u8,
    sda: i32,
    scl: i32,
    /// `true` once the bus install succeeded and the gauge ACK'd its
    /// address during construction. While `false`, `read()` never
    /// touches the bus — the shared touch I²C bus gets zero extra
    /// traffic from an absent gauge.
    present: bool,
    /// Gates the per-poll failure log so a dead gauge produces one
    /// warn on the healthy→failing edge (and one info on recovery)
    /// instead of a line every 2 s.
    read_failing: bool,
}

#[cfg(target_os = "espidf")]
impl Max17048FuelGauge {
    /// Bring up the shared legacy I²C master (idempotent — the touch
    /// driver installs the same bus) and verify the gauge responds.
    /// Returns `Ok` even when the gauge is absent so the runtime still
    /// boots; the absence is surfaced via `read()`'s `present` flag
    /// and a one-time log.
    pub fn new(config: &FuelGaugeConfig) -> Result<Self, EspIdfError> {
        let gauge = Self {
            addr: config.address as u8,
            sda: i32::from(config.i2c_sda_gpio),
            scl: i32::from(config.i2c_scl_gpio),
            present: false,
            read_failing: false,
        };
        i2c_scan::install_legacy_master_if_needed(gauge.sda, gauge.scl)?;
        if read_reg16(gauge.addr, REG_VCELL).is_ok() {
            // On a freshly connected cell the IC reports SOC = 0%
            // until it runs battery-insertion detection or learns the
            // cell over a charge/discharge cycle. Quick-start forces
            // an immediate voltage-based estimate — the same dance
            // Adafruit's library performs for new batteries. Skip it
            // when the cell reads too low to be a plausible insertion
            // (the IC would ignore it anyway).
            if let Ok(vcell_raw) = read_reg16(gauge.addr, REG_VCELL) {
                if decode_vcell_mv(vcell_raw) >= QUICK_START_MIN_CELL_MV
                    && write_reg16(gauge.addr, REG_MODE, MODE_QUICK_START).is_ok()
                {
                    // The quick-start bit clears itself once the
                    // estimate is done (typ. ~175 ms); poll briefly.
                    for _ in 0..100 {
                        match read_reg16(gauge.addr, REG_MODE) {
                            Ok(mode) if mode & MODE_QUICK_START == 0 => break,
                            Ok(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
                            Err(_) => break,
                        }
                    }
                    log::info!("fuel gauge: quick-start issued (fresh cell detected)");
                }
            }
            log::info!(
                "fuel gauge: MAX17048 present at 0x{:02x} (sda=GPIO{} scl=GPIO{})",
                gauge.addr,
                gauge.sda,
                gauge.scl,
            );
            Ok(Self {
                present: true,
                ..gauge
            })
        } else {
            log::warn!(
                "fuel gauge: no ACK at 0x{:02x} on sda=GPIO{} scl=GPIO{} — \
                 battery readout disabled (wiring? pull-ups? powered via QT header?)",
                gauge.addr,
                gauge.sda,
                gauge.scl,
            );
            Ok(gauge)
        }
    }

    /// Gauge-less fallback used when the shared I²C bus itself fails
    /// to install. Never touches the bus; `read()` returns a
    /// `present: false` reading and the overlay stays hidden.
    pub fn disabled(config: &FuelGaugeConfig) -> Self {
        Self {
            addr: config.address as u8,
            sda: i32::from(config.i2c_sda_gpio),
            scl: i32::from(config.i2c_scl_gpio),
            present: false,
            read_failing: false,
        }
    }

    /// Read VCELL + SOC and decode them. On a bus error returns
    /// `present: false` without failing the frame loop — a transient
    /// I²C glitch should not take the nav screen down. When the
    /// gauge never ACK'd at init, this is a pure no-op that leaves
    /// the shared bus untouched.
    pub fn read(&mut self) -> FuelGaugeReading {
        if !self.present {
            return FuelGaugeReading {
                percent: 0,
                voltage_mv: 0,
                present: false,
            };
        }
        match (
            read_reg16(self.addr, REG_VCELL),
            read_reg16(self.addr, REG_SOC),
        ) {
            (Ok(vcell), Ok(soc)) => {
                if self.read_failing {
                    log::info!("fuel gauge: bus reads recovered — battery readout back online");
                }
                self.read_failing = false;
                FuelGaugeReading {
                    percent: decode_soc_percent(soc),
                    voltage_mv: decode_vcell_mv(vcell),
                    present: true,
                }
            }
            (vcell, soc) => {
                if !self.read_failing {
                    self.read_failing = true;
                    log::warn!(
                        "fuel gauge: read failed (vcell={vcell:?} soc={soc:?}) — \
                         hiding battery readout until the bus recovers",
                    );
                }
                FuelGaugeReading {
                    percent: 0,
                    voltage_mv: 0,
                    present: false,
                }
            }
        }
    }
}

/// Platform adapter: the trait contract says `None` means "nothing
/// new to show"; the device driver always has a reading (its
/// `present` flag carries the wiring status), so it never returns
/// `None`.
#[cfg(target_os = "espidf")]
impl crate::platform::FuelGaugeSource for Max17048FuelGauge {
    fn read(&mut self) -> Option<FuelGaugeReading> {
        Some(Max17048FuelGauge::read(self))
    }
}

#[cfg(target_os = "espidf")]
impl std::fmt::Debug for Max17048FuelGauge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Max17048FuelGauge")
            .field("addr", &format_args!("0x{:02x}", self.addr))
            .field("sda", &self.sda)
            .field("scl", &self.scl)
            .field("present", &self.present)
            .field("read_failing", &self.read_failing)
            .finish()
    }
}

/// Two-phase 16-bit register read on the legacy I²C master: write the
/// register pointer byte, STOP, then read two bytes (MSB first) with a
/// NACK on the last byte. Mirrors the raw `i2c_cmd_link_*` style used
/// by `i2c_scan` — the firmware still links the legacy driver family.
#[cfg(target_os = "espidf")]
fn read_reg16(addr: u8, reg: u8) -> Result<u16, EspIdfError> {
    unsafe {
        // Phase 1: set the register pointer.
        let cmd = i2c_cmd_link_create();
        check(i2c_master_start(cmd), "i2c_master_start")?;
        check(
            i2c_master_write_byte(cmd, (addr << 1) | 0, true),
            "i2c_master_write_byte(addr)",
        )?;
        check(
            i2c_master_write_byte(cmd, reg, true),
            "i2c_master_write_byte(reg)",
        )?;
        check(i2c_master_stop(cmd), "i2c_master_stop")?;
        let result = i2c_master_cmd_begin(i2c_port_t_I2C_NUM_0, cmd, pd_ms_to_ticks(50));
        i2c_cmd_link_delete(cmd);
        check(result, "i2c_master_cmd_begin(pointer)")?;

        // Phase 2: read two bytes, NACK the last.
        let mut buffer = [0_u8; 2];
        let cmd = i2c_cmd_link_create();
        check(i2c_master_start(cmd), "i2c_master_start")?;
        check(
            i2c_master_write_byte(cmd, (addr << 1) | 1, true),
            "i2c_master_write_byte(addr|read)",
        )?;
        check(
            i2c_master_read(cmd, &mut buffer[0], 1, sys::i2c_ack_type_t_I2C_MASTER_ACK),
            "i2c_master_read(msb)",
        )?;
        check(
            i2c_master_read(
                cmd,
                &mut buffer[1],
                1,
                sys::i2c_ack_type_t_I2C_MASTER_LAST_NACK,
            ),
            "i2c_master_read(lsb)",
        )?;
        check(i2c_master_stop(cmd), "i2c_master_stop")?;
        let result = i2c_master_cmd_begin(i2c_port_t_I2C_NUM_0, cmd, pd_ms_to_ticks(50));
        i2c_cmd_link_delete(cmd);
        check(result, "i2c_master_cmd_begin(read)")?;

        Ok(u16::from_be_bytes([buffer[0], buffer[1]]))
    }
}

#[cfg(target_os = "espidf")]
fn check(status: esp_err_t, op: &str) -> Result<(), EspIdfError> {
    if status == sys::ESP_OK as esp_err_t {
        Ok(())
    } else {
        Err(EspIdfError::Io(format!("{op} failed: {status}")))
    }
}

/// Two-phase 16-bit register write (pointer byte, MSB, LSB, STOP) on
/// the legacy I²C master. Only used for the MODE quick-start poke.
#[cfg(target_os = "espidf")]
fn write_reg16(addr: u8, reg: u8, value: u16) -> Result<(), EspIdfError> {
    let bytes = value.to_be_bytes();
    unsafe {
        let cmd = i2c_cmd_link_create();
        check(i2c_master_start(cmd), "i2c_master_start")?;
        check(
            i2c_master_write_byte(cmd, (addr << 1) | 0, true),
            "i2c_master_write_byte(addr)",
        )?;
        check(
            i2c_master_write_byte(cmd, reg, true),
            "i2c_master_write_byte(reg)",
        )?;
        check(
            i2c_master_write_byte(cmd, bytes[0], true),
            "i2c_master_write_byte(msb)",
        )?;
        check(
            i2c_master_write_byte(cmd, bytes[1], true),
            "i2c_master_write_byte(lsb)",
        )?;
        check(i2c_master_stop(cmd), "i2c_master_stop")?;
        let result = i2c_master_cmd_begin(i2c_port_t_I2C_NUM_0, cmd, pd_ms_to_ticks(50));
        i2c_cmd_link_delete(cmd);
        check(result, "i2c_master_cmd_begin(write)")
    }
}

/// FreeRTOS tick conversion — 10 ms per tick at the default 100 Hz
/// tick rate; round up so callers never get a zero timeout. Same
/// arithmetic as `i2c_scan::pd_ms_to_ticks`.
#[cfg(target_os = "espidf")]
fn pd_ms_to_ticks(ms: u32) -> u32 {
    (ms + 9) / 10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vcell_decodes_to_millivolts() {
        // 4.20 V nominal full cell: 4_200_000 µV / 78.125 µV = 53_760.
        assert_eq!(decode_vcell_mv(53_760), 4200);
        // 3.70 V: 3_700_000 / 78.125 = 47_360.
        assert_eq!(decode_vcell_mv(47_360), 3700);
        // Empty register → 0 mV, not an underflow panic.
        assert_eq!(decode_vcell_mv(0), 0);
    }

    #[test]
    fn soc_decodes_integer_percent_from_high_byte() {
        assert_eq!(decode_soc_percent(0x6400), 100);
        assert_eq!(decode_soc_percent(0x5200), 82);
        // Fractional low byte is dropped, not rounded into the integer.
        assert_eq!(decode_soc_percent(0x52FF), 82);
        assert_eq!(decode_soc_percent(0x0000), 0);
    }
}
