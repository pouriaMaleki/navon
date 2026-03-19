use std::sync::{Arc, Mutex};

use runtime_core::api::SpeedUnit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceSettings {
    pub speed_unit: SpeedUnit,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            speed_unit: SpeedUnit::Kph,
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

#[cfg(target_os = "espidf")]
mod espidf {
    use super::{DeviceSettings, SettingsError, SettingsStore};
    use runtime_core::api::SpeedUnit;

    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};

    const SETTINGS_NAMESPACE: &str = "esp32_minimap";
    const SPEED_UNIT_KEY: &str = "speed_unit";
    const SPEED_UNIT_KPH: u8 = 0;
    const SPEED_UNIT_MPH: u8 = 1;

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
            Ok(Some(DeviceSettings { speed_unit }))
        }

        fn save_settings(&mut self, settings: &DeviceSettings) -> Result<(), SettingsError> {
            let raw = match settings.speed_unit {
                SpeedUnit::Kph => SPEED_UNIT_KPH,
                SpeedUnit::Mph => SPEED_UNIT_MPH,
            };
            self.nvs
                .set_u8(SPEED_UNIT_KEY, raw)
                .map_err(|error| SettingsError::Backend(format!("{error:?}")))?;
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
