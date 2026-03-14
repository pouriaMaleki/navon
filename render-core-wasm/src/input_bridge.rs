use std::time::Duration;

use serde::Deserialize;

use runtime_core::api::{
    GpsSample, RuntimeInputFrame, ScreenPoint, TouchContact, TouchContactFrame,
    TouchContactFrameError, TouchPhase, ViewportSize,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrowserTouchContact {
    pub id: u64,
    pub phase: TouchPhase,
    pub x_px: f32,
    pub y_px: f32,
    pub pressure: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InputBridge;

#[derive(Debug, Clone, Deserialize)]
struct JsonFrameInput {
    viewport: JsonViewportSize,
    #[serde(default)]
    gps: Option<JsonGpsSample>,
    #[serde(default)]
    touch: Option<JsonTouchFrame>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonViewportSize {
    #[serde(rename = "widthPx")]
    width_px: u32,
    #[serde(rename = "heightPx")]
    height_px: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonGpsSample {
    #[serde(rename = "latDeg")]
    lat_deg: f64,
    #[serde(rename = "lonDeg")]
    lon_deg: f64,
    #[serde(rename = "speedMps")]
    speed_mps: f32,
    #[serde(rename = "courseRad")]
    course_rad: Option<f32>,
    #[serde(rename = "horizontalAccuracyM")]
    horizontal_accuracy_m: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonTouchFrame {
    sequence: u64,
    contacts: Vec<JsonTouchContact>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonTouchContact {
    id: u64,
    phase: String,
    #[serde(rename = "xPx")]
    x_px: f32,
    #[serde(rename = "yPx")]
    y_px: f32,
    pressure: Option<f32>,
}

impl InputBridge {
    pub fn frame_from_json(
        &self,
        dt_ms: f64,
        frame_json: &str,
    ) -> Result<RuntimeInputFrame, String> {
        let json_frame: JsonFrameInput = serde_json::from_str(frame_json)
            .map_err(|error| format!("invalid frame json: {error}"))?;
        let viewport =
            ViewportSize::new(json_frame.viewport.width_px, json_frame.viewport.height_px);
        let gps = json_frame.gps.map(|gps| GpsSample {
            lat_deg: gps.lat_deg,
            lon_deg: gps.lon_deg,
            speed_mps: gps.speed_mps,
            course_rad: gps.course_rad,
            horizontal_accuracy_m: gps.horizontal_accuracy_m,
        });
        let contacts = json_frame
            .touch
            .map(|touch| {
                touch
                    .contacts
                    .into_iter()
                    .map(|contact| {
                        Ok(BrowserTouchContact {
                            id: contact.id,
                            phase: parse_touch_phase(&contact.phase)?,
                            x_px: contact.x_px,
                            y_px: contact.y_px,
                            pressure: contact.pressure,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
                    .map(|contacts| (touch.sequence, contacts))
            })
            .transpose()?;

        if let Some((sequence, contacts)) = contacts {
            self.frame_from_browser(
                Duration::from_secs_f64((dt_ms.max(0.0)) / 1000.0),
                viewport,
                gps,
                sequence,
                contacts,
            )
            .map_err(|error| format!("invalid touch frame: {error:?}"))
        } else {
            let frame = RuntimeInputFrame::new(Duration::from_secs_f64((dt_ms.max(0.0)) / 1000.0))
                .with_viewport(viewport);
            Ok(if let Some(gps) = gps {
                frame.with_gps(gps)
            } else {
                frame
            })
        }
    }

    pub fn frame_from_browser(
        &self,
        dt: Duration,
        viewport_size: ViewportSize,
        gps: Option<GpsSample>,
        sequence: u64,
        contacts: Vec<BrowserTouchContact>,
    ) -> Result<RuntimeInputFrame, TouchContactFrameError> {
        let touch = TouchContactFrame::new(
            sequence,
            contacts
                .into_iter()
                .map(|contact| TouchContact {
                    id: contact.id,
                    phase: contact.phase,
                    position: ScreenPoint::new(contact.x_px, contact.y_px),
                    pressure: contact.pressure,
                })
                .collect(),
        )?;

        let frame = RuntimeInputFrame::new(dt).with_viewport(viewport_size);
        let frame = if let Some(gps) = gps {
            frame.with_gps(gps)
        } else {
            frame
        };
        Ok(frame.with_touch(touch))
    }
}

fn parse_touch_phase(raw: &str) -> Result<TouchPhase, String> {
    match raw {
        "started" => Ok(TouchPhase::Started),
        "moved" => Ok(TouchPhase::Moved),
        "stationary" => Ok(TouchPhase::Stationary),
        "ended" => Ok(TouchPhase::Ended),
        "cancelled" => Ok(TouchPhase::Cancelled),
        _ => Err(format!("unsupported touch phase: {raw}")),
    }
}
