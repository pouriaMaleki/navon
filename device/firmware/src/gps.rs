use std::collections::VecDeque;

use runtime_core::api::GpsSample;

/// Which GPS source the device uses for this session. Resets to
/// `Internal` on every boot; when the companion pushes phone GPS
/// samples the platform layer switches to `Phone` automatically.
/// Session-only by design — no NVS persistence so a reboot always
/// returns to the built-in GPS module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpsSource {
    #[default]
    Internal,
    Phone,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GpsInput {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub speed_mps: f32,
    pub course_rad: Option<f32>,
    pub horizontal_accuracy_m: Option<f32>,
}

impl From<GpsInput> for GpsSample {
    fn from(input: GpsInput) -> Self {
        Self {
            lat_deg: input.lat_deg,
            lon_deg: input.lon_deg,
            speed_mps: input.speed_mps,
            course_rad: input.course_rad,
            horizontal_accuracy_m: input.horizontal_accuracy_m,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpsError {
    Provider(String),
}

/// Counters the device exposes on the "GETTING GPS" overlay so an
/// operator in the field can tell *why* GPS isn't acquiring without
/// needing serial-console access. The three values triangulate the
/// failure mode:
///
/// * `bytes_seen == 0` → no electrical signal on RX. Check wiring
///   (TX/RX swapped, wrong GPIO, no 3.3 V on VCC, dead module).
/// * `bytes_seen > 0 && sentences_seen == 0` → bytes arriving but the
///   line parser never finds a `\n`. Check baud rate (some clones
///   ship at 38400 instead of 9600).
/// * `sentences_seen > 0 && fixes_seen == 0` → module alive, RMC
///   parser working, but the receiver hasn't locked onto enough
///   satellites for a fix yet. Almanac warm-up + clearer sky view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpsDiagnostics {
    pub bytes_seen: u64,
    pub sentences_seen: u64,
    pub fixes_seen: u64,
    /// Milliseconds since the most recent valid RMC fix. `None` while
    /// no fix has been received yet (cold boot). The platform layer
    /// uses this to detect signal loss after acquisition: if the age
    /// exceeds a threshold we flip [`crate::app::App::set_gps_acquired`]
    /// back to `false` so the "GETTING GPS" overlay reappears even
    /// after we'd previously locked.
    pub last_fix_age_ms: Option<u32>,
}

pub trait GpsProvider {
    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError>;

    /// Has the underlying hardware (or test fixture) ever produced a
    /// real fix? Defaults to `true` because almost every provider in
    /// this crate is either deterministic (`FixedGpsProvider`,
    /// `SequenceGpsProvider`) or doesn't have a notion of "real" data
    /// (`NullGpsProvider`). Only [`SeedThenRealGpsProvider`] overrides
    /// this — it returns `false` until the inner provider has handed
    /// up a real fix, which the platform layer reads to decide whether
    /// to draw the "GETTING GPS" overlay.
    fn has_acquired_fix(&self) -> bool {
        true
    }

    /// Optional live counters surfaced on the "GETTING GPS" overlay
    /// for in-field debugging. Default `None` keeps the overlay clean
    /// for trivial providers (`NullGpsProvider`, `FixedGpsProvider`)
    /// that have nothing to report. The real-hardware provider
    /// (`EspIdfGpsProvider` wrapping `UartGpsSerial`) overrides this.
    fn diagnostics_summary(&self) -> Option<GpsDiagnostics> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NullGpsProvider;

impl GpsProvider for NullGpsProvider {
    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError> {
        Ok(None)
    }
}

/// Returns the same fix on every `poll`, forever. Used during device
/// bring-up to park the camera on the embedded map's region while no
/// real GPS hardware is wired, so the runtime actually has geometry to
/// render instead of looking at the Gulf of Guinea at (0, 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedGpsProvider {
    fix: GpsInput,
}

impl FixedGpsProvider {
    pub const fn new(fix: GpsInput) -> Self {
        Self { fix }
    }
}

impl GpsProvider for FixedGpsProvider {
    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError> {
        Ok(Some(self.fix))
    }
}

#[derive(Debug, Clone, Default)]
pub struct SequenceGpsProvider {
    samples: VecDeque<Option<GpsInput>>,
}

impl SequenceGpsProvider {
    pub fn new(samples: impl IntoIterator<Item = Option<GpsInput>>) -> Self {
        Self {
            samples: samples.into_iter().collect(),
        }
    }
}

impl GpsProvider for SequenceGpsProvider {
    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError> {
        Ok(self.samples.pop_front().flatten())
    }
}

/// Wraps a real `GpsProvider` (typically the UART-backed NEO-6M reader on
/// device) and substitutes a known-good seed fix on every poll until the
/// inner provider returns its first real fix. Lets the runtime render
/// geometry against a real region — instead of (0, 0) "Gulf of Guinea" —
/// during the NEO-6M cold-start window (no fix until ≥ 3 satellites are
/// locked, which can take 30 s … several minutes outdoors and forever
/// indoors). Once a real fix arrives we hand it through unchanged and
/// stop forging seed values.
#[derive(Debug, Clone)]
pub struct SeedThenRealGpsProvider<P> {
    seed: GpsInput,
    real: P,
    seen_real: bool,
}

impl<P> SeedThenRealGpsProvider<P> {
    pub fn new(seed: GpsInput, real: P) -> Self {
        Self {
            seed,
            real,
            seen_real: false,
        }
    }

    pub fn has_seen_real_fix(&self) -> bool {
        self.seen_real
    }
}

impl<P> GpsProvider for SeedThenRealGpsProvider<P>
where
    P: GpsProvider,
{
    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError> {
        match self.real.poll()? {
            Some(fix) => {
                self.seen_real = true;
                Ok(Some(fix))
            }
            None => {
                if self.seen_real {
                    Ok(None)
                } else {
                    Ok(Some(self.seed))
                }
            }
        }
    }

    fn has_acquired_fix(&self) -> bool {
        self.seen_real
    }

    fn diagnostics_summary(&self) -> Option<GpsDiagnostics> {
        self.real.diagnostics_summary()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(lat: f64, lon: f64) -> GpsInput {
        GpsInput {
            lat_deg: lat,
            lon_deg: lon,
            speed_mps: 0.0,
            course_rad: None,
            horizontal_accuracy_m: None,
        }
    }

    #[test]
    fn seed_then_real_returns_seed_until_first_real_fix_then_passes_through() {
        let seed = fix(60.0, 24.0);
        // None, None, Some(real), None — mimics NEO-6M cold start: no
        // RMC for the first few polls, then a fix, then a frame with no
        // sentence. Once the first real fix lands we must stop forging
        // seeds even if subsequent polls return None.
        let real = SequenceGpsProvider::new([
            None,
            None,
            Some(fix(48.85, 2.35)),
            None,
        ]);
        let mut provider = SeedThenRealGpsProvider::new(seed, real);

        assert_eq!(provider.poll().unwrap(), Some(seed));
        assert!(!provider.has_seen_real_fix());
        assert_eq!(provider.poll().unwrap(), Some(seed));
        assert_eq!(provider.poll().unwrap(), Some(fix(48.85, 2.35)));
        assert!(provider.has_seen_real_fix());
        assert_eq!(provider.poll().unwrap(), None);
    }
}
