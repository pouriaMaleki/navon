use std::time::Duration;

use runtime_core::api::{
    CameraMode, GeometryCandidate, GpsSample, MapLayer, MapPolylineCandidate, MapPresentationBand,
    MapQueryResult, MapQuerySpec, RuntimeFrameOutput, ScreenPoint, TouchPhase, ViewportSize,
    WorldPoint,
};
use runtime_core::map::MapSource;

pub mod loader;

pub use loader::{load_gps_stream, load_ux_constants, RideSample, RideScenario, UxConstants};

pub const FIXTURE_VIEWPORT: ViewportSize = ViewportSize::new(800, 800);
const FLOAT_EPSILON: f32 = 0.000_1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixtureTouchContact {
    pub id: u64,
    pub phase: TouchPhase,
    pub position: ScreenPoint,
    pub pressure: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixtureTouchFrame {
    pub sequence: u64,
    pub contacts: Vec<FixtureTouchContact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixtureFrame {
    pub dt: Duration,
    pub viewport: ViewportSize,
    pub gps: Option<GpsSample>,
    pub touch: Option<FixtureTouchFrame>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixtureScenario {
    pub name: &'static str,
    pub frames: Vec<FixtureFrame>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FixtureMapSource;

impl MapSource for FixtureMapSource {
    fn query(&self, spec: &MapQuerySpec) -> MapQueryResult {
        let center = spec.center;
        let horizontal = GeometryCandidate::Polyline(MapPolylineCandidate {
            layer: MapLayer::ArterialRoad,
            points: vec![
                WorldPoint::new(spec.bounds.min.x_m, center.y_m),
                WorldPoint::new(spec.bounds.max.x_m, center.y_m),
            ],
        });
        let vertical = GeometryCandidate::Polyline(MapPolylineCandidate {
            layer: MapLayer::StreetRoad,
            points: vec![
                WorldPoint::new(center.x_m, spec.bounds.min.y_m),
                WorldPoint::new(center.x_m, spec.bounds.max.y_m),
            ],
        });
        let diagonal = GeometryCandidate::Polyline(MapPolylineCandidate {
            layer: MapLayer::BikeRouteMain,
            points: vec![
                WorldPoint::new(spec.bounds.min.x_m, spec.bounds.min.y_m),
                WorldPoint::new(spec.bounds.max.x_m, spec.bounds.max.y_m),
            ],
        });

        let mut geometry = vec![horizontal];
        if spec.lod_mask.contains(MapLayer::StreetRoad) {
            geometry.push(vertical);
        }
        if spec.lod_mask.contains(MapLayer::BikeRouteMain) {
            geometry.push(diagonal);
        }

        MapQueryResult { geometry }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParitySnapshot {
    pub camera_mode: CameraMode,
    pub zoom: f32,
    pub orientation_rad: f32,
    pub follow_locked: bool,
    pub recenter_active: bool,
    pub north_up_active: bool,
    pub rider_heading_rad: Option<f32>,
    pub presentation_band: MapPresentationBand,
    pub lod_mask_bits: u16,
    pub geometry_count: usize,
    pub lit_pixel_count: usize,
    pub pixel_hash: u64,
}

impl ParitySnapshot {
    pub fn from_output(output: &RuntimeFrameOutput, geometry_count: usize, pixels: &[u8]) -> Self {
        Self {
            camera_mode: output.camera.mode,
            zoom: output.camera.zoom,
            orientation_rad: output.camera.orientation_rad,
            follow_locked: output.camera.follow_locked,
            recenter_active: output.camera.recenter_active,
            north_up_active: output.overlay.north_up_active,
            rider_heading_rad: output.overlay.rider_heading_rad,
            presentation_band: output.map_query.presentation_band,
            lod_mask_bits: output.map_query.lod_mask.bits(),
            geometry_count,
            lit_pixel_count: pixels.iter().copied().filter(|value| *value > 0).count(),
            pixel_hash: pixel_hash(pixels),
        }
    }

    pub fn approx_eq(&self, other: &Self) -> bool {
        self.camera_mode == other.camera_mode
            && approx_f32(self.zoom, other.zoom)
            && approx_f32(self.orientation_rad, other.orientation_rad)
            && self.follow_locked == other.follow_locked
            && self.recenter_active == other.recenter_active
            && self.north_up_active == other.north_up_active
            && approx_option_f32(self.rider_heading_rad, other.rider_heading_rad)
            && self.presentation_band == other.presentation_band
            && self.lod_mask_bits == other.lod_mask_bits
            && self.geometry_count == other.geometry_count
            && self.lit_pixel_count == other.lit_pixel_count
            && self.pixel_hash == other.pixel_hash
    }
}

pub fn bridge_parity_frames() -> Vec<FixtureFrame> {
    vec![
        FixtureFrame {
            dt: Duration::from_millis(16),
            viewport: FIXTURE_VIEWPORT,
            gps: Some(helsinki_fix(24.94210, 5.0)),
            touch: Some(FixtureTouchFrame {
                sequence: 1,
                contacts: vec![touch_contact(1, TouchPhase::Started, 120.0, 140.0)],
            }),
        },
        FixtureFrame {
            dt: Duration::from_millis(16),
            viewport: FIXTURE_VIEWPORT,
            gps: Some(helsinki_fix(24.94220, 5.0)),
            touch: Some(FixtureTouchFrame {
                sequence: 2,
                contacts: vec![touch_contact(1, TouchPhase::Stationary, 120.0, 140.0)],
            }),
        },
        FixtureFrame {
            dt: Duration::from_millis(16),
            viewport: FIXTURE_VIEWPORT,
            gps: Some(helsinki_fix(24.94230, 5.0)),
            touch: Some(FixtureTouchFrame {
                sequence: 3,
                contacts: vec![],
            }),
        },
        FixtureFrame {
            dt: Duration::from_millis(16),
            viewport: FIXTURE_VIEWPORT,
            gps: Some(helsinki_fix(24.94240, 5.0)),
            touch: Some(FixtureTouchFrame {
                sequence: 4,
                contacts: vec![touch_contact(1, TouchPhase::Ended, 120.0, 140.0)],
            }),
        },
        FixtureFrame {
            dt: Duration::from_millis(16),
            viewport: FIXTURE_VIEWPORT,
            gps: Some(helsinki_fix(24.94250, 5.0)),
            touch: Some(FixtureTouchFrame {
                sequence: 5,
                contacts: vec![
                    touch_contact(1, TouchPhase::Started, 180.0, 220.0),
                    touch_contact(2, TouchPhase::Started, 260.0, 220.0),
                ],
            }),
        },
        FixtureFrame {
            dt: Duration::from_millis(16),
            viewport: FIXTURE_VIEWPORT,
            gps: Some(helsinki_fix(24.94260, 5.0)),
            touch: Some(FixtureTouchFrame {
                sequence: 6,
                contacts: vec![
                    touch_contact(1, TouchPhase::Moved, 170.0, 200.0),
                    touch_contact(2, TouchPhase::Moved, 280.0, 240.0),
                ],
            }),
        },
    ]
}

pub fn runtime_scenarios() -> Vec<FixtureScenario> {
    vec![
        FixtureScenario {
            name: "pan_recenter",
            frames: pan_recenter_frames(),
        },
        FixtureScenario {
            name: "pinch_rotate",
            frames: pinch_rotate_frames(),
        },
        FixtureScenario {
            name: "north_indicator_override",
            frames: north_indicator_tap_frames(),
        },
        FixtureScenario {
            name: "stopped_settle",
            frames: stopped_settle_frames(),
        },
        FixtureScenario {
            name: "gps_dropout",
            frames: gps_dropout_frames(),
        },
    ]
}

pub fn duplicate_contact_frame() -> FixtureFrame {
    FixtureFrame {
        dt: Duration::from_millis(16),
        viewport: FIXTURE_VIEWPORT,
        gps: Some(helsinki_fix(24.94210, 5.0)),
        touch: Some(FixtureTouchFrame {
            sequence: 99,
            contacts: vec![
                touch_contact(7, TouchPhase::Started, 10.0, 10.0),
                touch_contact(7, TouchPhase::Moved, 20.0, 20.0),
            ],
        }),
    }
}

fn pan_recenter_frames() -> Vec<FixtureFrame> {
    vec![
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94210, 0.0)),
            None,
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94310, 5.0)),
            None,
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94320, 5.0)),
            Some(FixtureTouchFrame {
                sequence: 10,
                contacts: vec![touch_contact(1, TouchPhase::Started, 160.0, 220.0)],
            }),
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94330, 5.0)),
            Some(FixtureTouchFrame {
                sequence: 11,
                contacts: vec![touch_contact(1, TouchPhase::Moved, 230.0, 220.0)],
            }),
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94340, 5.0)),
            Some(FixtureTouchFrame {
                sequence: 12,
                contacts: vec![touch_contact(1, TouchPhase::Ended, 230.0, 220.0)],
            }),
        ),
        frame(
            Duration::from_millis(500),
            Some(helsinki_fix(24.94350, 5.0)),
            None,
        ),
        frame(
            Duration::from_millis(1_300),
            Some(helsinki_fix(24.94360, 5.0)),
            None,
        ),
    ]
}

fn pinch_rotate_frames() -> Vec<FixtureFrame> {
    vec![
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94210, 0.0)),
            None,
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94310, 5.0)),
            None,
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94320, 5.0)),
            Some(FixtureTouchFrame {
                sequence: 20,
                contacts: vec![
                    touch_contact(1, TouchPhase::Started, 220.0, 320.0),
                    touch_contact(2, TouchPhase::Started, 320.0, 320.0),
                ],
            }),
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94330, 5.0)),
            Some(FixtureTouchFrame {
                sequence: 21,
                contacts: vec![
                    touch_contact(1, TouchPhase::Moved, 200.0, 280.0),
                    touch_contact(2, TouchPhase::Moved, 360.0, 360.0),
                ],
            }),
        ),
    ]
}

fn north_indicator_tap_frames() -> Vec<FixtureFrame> {
    vec![
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94210, 0.0)),
            None,
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94310, 5.0)),
            None,
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94320, 5.0)),
            Some(FixtureTouchFrame {
                sequence: 30,
                contacts: vec![touch_contact(1, TouchPhase::Started, 745.0, 55.0)],
            }),
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94330, 5.0)),
            Some(FixtureTouchFrame {
                sequence: 31,
                contacts: vec![touch_contact(1, TouchPhase::Ended, 745.0, 55.0)],
            }),
        ),
        frame(
            Duration::from_millis(1_100),
            Some(helsinki_fix(24.94340, 5.0)),
            None,
        ),
    ]
}

fn stopped_settle_frames() -> Vec<FixtureFrame> {
    vec![
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94210, 0.0)),
            None,
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94310, 5.0)),
            None,
        ),
        frame(
            Duration::from_millis(500),
            Some(helsinki_fix(24.94311, 0.0)),
            None,
        ),
        frame(
            Duration::from_millis(1_500),
            Some(helsinki_fix(24.94311, 0.0)),
            None,
        ),
    ]
}

fn gps_dropout_frames() -> Vec<FixtureFrame> {
    vec![
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94210, 0.0)),
            None,
        ),
        frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94310, 5.0)),
            None,
        ),
        frame(Duration::from_millis(500), None, None),
        frame(Duration::from_millis(1_100), None, None),
    ]
}

fn frame(dt: Duration, gps: Option<GpsSample>, touch: Option<FixtureTouchFrame>) -> FixtureFrame {
    FixtureFrame {
        dt,
        viewport: FIXTURE_VIEWPORT,
        gps,
        touch,
    }
}

fn touch_contact(id: u64, phase: TouchPhase, x_px: f32, y_px: f32) -> FixtureTouchContact {
    FixtureTouchContact {
        id,
        phase,
        position: ScreenPoint::new(x_px, y_px),
        pressure: Some(0.5),
    }
}

fn helsinki_fix(lon_deg: f64, speed_mps: f32) -> GpsSample {
    GpsSample {
        lat_deg: 60.17442,
        lon_deg,
        speed_mps,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(4.0),
    }
}

fn approx_f32(left: f32, right: f32) -> bool {
    (left - right).abs() <= FLOAT_EPSILON
}

fn approx_option_f32(left: Option<f32>, right: Option<f32>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => approx_f32(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn pixel_hash(pixels: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for pixel in pixels {
        hash ^= u64::from(*pixel);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}
