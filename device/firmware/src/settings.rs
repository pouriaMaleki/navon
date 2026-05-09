use std::sync::{Arc, Mutex};

use runtime_core::api::SpeedUnit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceSettings {
    pub speed_unit: SpeedUnit,
    /// True once the device has bonded with a companion via the
    /// QR-OOB pairing flow. While false, the device boots into pairing
    /// mode (renders QR, accepts only the `pairing_confirm`
    /// characteristic, ignores route writes).
    pub device_paired: bool,
    /// Truncated SHA-256 digest of the bonded peer's BD_ADDR + IRK,
    /// used to detect bond mismatch (e.g., the phone forgot us but our
    /// NVS still says paired). `None` when unpaired.
    pub peer_identity: Option<[u8; 16]>,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            speed_unit: SpeedUnit::Kph,
            device_paired: false,
            peer_identity: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    Backend(String),
    CorruptData(String),
}

pub trait SettingsStore {
    fn load_settings(&mut self) -> Result<Option<DeviceSettings>, SettingsError>;
    fn save_settings(&mut self, settings: &DeviceSettings) -> Result<(), SettingsError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NullSettingsStore;

impl SettingsStore for NullSettingsStore {
    fn load_settings(&mut self) -> Result<Option<DeviceSettings>, SettingsError> {
        Ok(None)
    }

    fn save_settings(&mut self, _settings: &DeviceSettings) -> Result<(), SettingsError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemorySettingsStore {
    value: Arc<Mutex<Option<DeviceSettings>>>,
}

impl MemorySettingsStore {
    pub fn new(initial: Option<DeviceSettings>) -> Self {
        Self {
            value: Arc::new(Mutex::new(initial)),
        }
    }

    pub fn shared_value(&self) -> Option<DeviceSettings> {
        *self.value.lock().expect("settings store mutex poisoned")
    }
}

impl SettingsStore for MemorySettingsStore {
    fn load_settings(&mut self) -> Result<Option<DeviceSettings>, SettingsError> {
        Ok(self.shared_value())
    }

    fn save_settings(&mut self, settings: &DeviceSettings) -> Result<(), SettingsError> {
        *self.value.lock().expect("settings store mutex poisoned") = Some(*settings);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_settings_round_trip_includes_pairing_state() {
        let mut store = MemorySettingsStore::default();
        let settings = DeviceSettings {
            speed_unit: SpeedUnit::Kph,
            device_paired: true,
            peer_identity: Some([0x42; 16]),
        };
        store.save_settings(&settings).expect("save");
        let loaded = store
            .load_settings()
            .expect("load")
            .expect("a value was saved");
        assert_eq!(loaded, settings, "speed unit + pairing fields must round-trip");
    }
}

#[cfg(target_os = "espidf")]
mod espidf {
    use super::{DeviceSettings, SettingsError, SettingsStore};
    use runtime_core::api::SpeedUnit;

    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};

    const SETTINGS_NAMESPACE: &str = "esp32_minimap";
    const SPEED_UNIT_KEY: &str = "speed_unit";
    const SPEED_UNIT_KPH: u8 = 0;
    const SPEED_UNIT_MPH: u8 = 1;
    /// `1` once a bond is stored. Absence ⇒ unpaired.
    const PAIRED_FLAG_KEY: &str = "paired_flag";
    /// 16-byte SHA-256-truncated digest of the bonded peer's BD_ADDR + IRK.
    /// Present only while `paired_flag == 1`.
    const PEER_IDENTITY_KEY: &str = "peer_identity";

    pub struct EspIdfSettingsStore {
        nvs: EspNvs<NvsDefault>,
    }

    impl EspIdfSettingsStore {
        pub fn new_default() -> Result<Self, SettingsError> {
            let partition = EspDefaultNvsPartition::take()
                .map_err(|error| SettingsError::Backend(format!("{error:?}")))?;
            let nvs = EspNvs::new(partition, SETTINGS_NAMESPACE, true)
                .map_err(|error| SettingsError::Backend(format!("{error:?}")))?;
            Ok(Self { nvs })
        }
    }

    impl SettingsStore for EspIdfSettingsStore {
        fn load_settings(&mut self) -> Result<Option<DeviceSettings>, SettingsError> {
            let raw = self
                .nvs
                .get_u8(SPEED_UNIT_KEY)
                .map_err(|error| SettingsError::Backend(format!("{error:?}")))?;
            let Some(raw) = raw else {
                return Ok(None);
            };
            let speed_unit = match raw {
                SPEED_UNIT_KPH => SpeedUnit::Kph,
                SPEED_UNIT_MPH => SpeedUnit::Mph,
                other => {
                    return Err(SettingsError::CorruptData(format!(
                        "unsupported persisted speed unit value: {other}"
                    )));
                }
            };
            // Optional pairing fields. A device that's never been paired
            // simply has neither key in NVS.
            let device_paired = self
                .nvs
                .get_u8(PAIRED_FLAG_KEY)
                .map_err(|error| SettingsError::Backend(format!("{error:?}")))?
                .map(|v| v != 0)
                .unwrap_or(false);
            let mut peer_identity_buf = [0u8; 16];
            let blob_len = self
                .nvs
                .get_blob(PEER_IDENTITY_KEY, &mut peer_identity_buf)
                .map_err(|error| SettingsError::Backend(format!("{error:?}")))?
                .map(|slice| slice.len());
            let peer_identity = match blob_len {
                Some(16) => Some(peer_identity_buf),
                _ => None,
            };
            Ok(Some(DeviceSettings {
                speed_unit,
                device_paired,
                peer_identity,
            }))
        }

        fn save_settings(&mut self, settings: &DeviceSettings) -> Result<(), SettingsError> {
            let raw = match settings.speed_unit {
                SpeedUnit::Kph => SPEED_UNIT_KPH,
                SpeedUnit::Mph => SPEED_UNIT_MPH,
            };
            self.nvs
                .set_u8(SPEED_UNIT_KEY, raw)
                .map_err(|error| SettingsError::Backend(format!("{error:?}")))?;
            self.nvs
                .set_u8(PAIRED_FLAG_KEY, if settings.device_paired { 1 } else { 0 })
                .map_err(|error| SettingsError::Backend(format!("{error:?}")))?;
            if let Some(identity) = &settings.peer_identity {
                self.nvs
                    .set_blob(PEER_IDENTITY_KEY, identity)
                    .map_err(|error| SettingsError::Backend(format!("{error:?}")))?;
            } else {
                // Drop a stale identity when the user calls Forget so a
                // future bond doesn't accidentally inherit the old peer.
                let _ = self.nvs.remove(PEER_IDENTITY_KEY);
            }
            Ok(())
        }
    }

    pub type DefaultSettingsStore = EspIdfSettingsStore;

    pub fn default_settings_store() -> Result<DefaultSettingsStore, SettingsError> {
        EspIdfSettingsStore::new_default()
    }
}

#[cfg(not(target_os = "espidf"))]
mod espidf {
    use super::{NullSettingsStore, SettingsError};

    pub type DefaultSettingsStore = NullSettingsStore;

    pub fn default_settings_store() -> Result<DefaultSettingsStore, SettingsError> {
        Ok(NullSettingsStore)
    }
}

pub use espidf::{DefaultSettingsStore, default_settings_store};
