#![no_std]

mod math;
mod raster;
mod style;
mod visibility;

use math::{normalize_angle, slew_angle};
use raster::draw_line;
use style::{
    apply_device_style, draw_north_indicator, draw_player, draw_player_screen,
    line_style_dark_profile, north_indicator_geometry,
};
use visibility::{CameraTransform, clip_line_to_rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceProfile {
    pub id: &'static str,
    pub width: usize,
    pub height: usize,
}

pub const WAVESHARE_ESP32_P4_3_4: DeviceProfile = DeviceProfile {
    id: "waveshare-esp32-p4-3.4-800x800",
    width: 800,
    height: 800,
};

pub const WAVESHARE_ESP32_P4_4_0: DeviceProfile = DeviceProfile {
    id: "waveshare-esp32-p4-4.0-720x720",
    width: 720,
    height: 720,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldPoint {
    pub x: i16,
    pub y: i16,
}

#[derive(Clone, Copy, Debug)]
pub struct Line {
    pub from: WorldPoint,
    pub to: WorldPoint,
    pub intensity: u8,
    pub thickness: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct WorldBounds {
    pub min_x: i16,
    pub max_x: i16,
    pub min_y: i16,
    pub max_y: i16,
}

impl WorldBounds {
    pub const fn width(self) -> i16 {
        self.max_x - self.min_x
    }

    pub const fn height(self) -> i16 {
        self.max_y - self.min_y
    }
}

pub struct FrameBuffer<'a> {
    pub width: usize,
    pub height: usize,
    pub pixels: &'a mut [u8],
}

impl<'a> FrameBuffer<'a> {
    pub fn new(width: usize, height: usize, pixels: &'a mut [u8]) -> Self {
        assert_eq!(width * height, pixels.len());
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn clear(&mut self, value: u8) {
        self.pixels.fill(value);
    }

    pub fn set_pixel_checked(&mut self, x: i32, y: i32, value: u8) {
        if x < 0 || y < 0 {
            return;
        }
        let ux = x as usize;
        let uy = y as usize;
        if ux >= self.width || uy >= self.height {
            return;
        }
        let idx = uy * self.width + ux;
        self.pixels[idx] = self.pixels[idx].max(value);
    }
}

pub struct MinimapView {
    pub bounds: WorldBounds,
    pub background: u8,
    pub player: WorldPoint,
}

#[derive(Clone, Copy, Debug)]
pub struct CameraView {
    pub center: WorldPoint,
    pub player: WorldPoint,
    pub heading_rad: f32,
    pub rider_heading_rad: f32,
    pub zoom: f32,
    pub base_bounds: WorldBounds,
    pub background: u8,
    pub player_anchor_x: f32,
    pub player_anchor_y: f32,
    pub riding_mode: bool,
    pub interaction_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    Riding,
    StoppedNorthUp,
    TemporaryNorthUp,
}

#[derive(Clone, Copy, Debug)]
pub struct CameraControllerInput {
    pub player: WorldPoint,
    pub rider_heading_rad: f32,
    pub speed_mps: f32,
    pub pan_dx: f32,
    pub pan_dy: f32,
    pub zoom_scale: f32,
    pub rotate_delta_rad: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct CameraOutput {
    pub mode: CameraMode,
    pub heading_rad: f32,
    pub rider_heading_rad: f32,
    pub follow_player: WorldPoint,
    pub player_anchor_y: f32,
    pub riding_mode: bool,
    pub interaction_active: bool,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

pub struct CameraControllerState {
    mode: CameraMode,
    map_heading_rad: f32,
    rider_heading_rad: f32,
    travel_heading_rad: f32,
    filtered_motion_dx: f32,
    filtered_motion_dy: f32,
    has_filtered_motion: bool,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    idle_ms_since_pan: f32,
    moving_ms: f32,
    stopped_ms: f32,
    temporary_override_moving_ms: f32,
    current_anchor_y: f32,
    last_player: WorldPoint,
    has_last_player: bool,
    manual_heading_offset_rad: f32,
    pan_lock_active: bool,
    pan_lock_player: WorldPoint,
    follow_player: WorldPoint,
    pan_lock_anchor_y: f32,
}

const RIDING_ANCHOR_Y: f32 = 0.74;
const STOPPED_ANCHOR_Y: f32 = 0.50;
const ANCHOR_BLEND_MS: f32 = 280.0;
const STOP_DELAY_MS: f32 = 1800.0;
const RESUME_MOVING_MS: f32 = 420.0;
const TEMP_NORTH_RETURN_MOVING_MS: f32 = 1800.0;
const PAN_IDLE_MS: f32 = 1200.0;
const PAN_RECENTER_MS: f32 = 420.0;
const ACTIVE_INTERACTION_MS: f32 = 180.0;
const MOVING_DELTA_THRESHOLD: i32 = 2;
const MOVING_SPEED_THRESHOLD_MPS: f32 = 0.6;
const TRAVEL_VECTOR_BLEND_MS: f32 = 950.0;
const HEADING_SLEW_RAD_PER_SEC: f32 = 1.35;
const MIN_ZOOM: f32 = 0.8;
const MAX_ZOOM: f32 = 60.0;
const PAN_LOCK_RELEASE_EPSILON: f32 = 0.75;

impl CameraControllerState {
    pub fn new(initial_zoom: f32) -> Self {
        Self {
            mode: CameraMode::StoppedNorthUp,
            map_heading_rad: 0.0,
            rider_heading_rad: 0.0,
            travel_heading_rad: 0.0,
            filtered_motion_dx: 0.0,
            filtered_motion_dy: 0.0,
            has_filtered_motion: false,
            zoom: initial_zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            pan_x: 0.0,
            pan_y: 0.0,
            idle_ms_since_pan: 0.0,
            moving_ms: 0.0,
            stopped_ms: 0.0,
            temporary_override_moving_ms: 0.0,
            current_anchor_y: STOPPED_ANCHOR_Y,
            last_player: WorldPoint { x: 0, y: 0 },
            has_last_player: false,
            manual_heading_offset_rad: 0.0,
            pan_lock_active: false,
            pan_lock_player: WorldPoint { x: 0, y: 0 },
            follow_player: WorldPoint { x: 0, y: 0 },
            pan_lock_anchor_y: STOPPED_ANCHOR_Y,
        }
    }

    pub fn toggle_temporary_north_up(&mut self) {
        self.mode = if self.mode == CameraMode::TemporaryNorthUp {
            CameraMode::Riding
        } else {
            CameraMode::TemporaryNorthUp
        };
        self.temporary_override_moving_ms = 0.0;
        if self.mode == CameraMode::TemporaryNorthUp {
            self.manual_heading_offset_rad = 0.0;
        }
    }

    pub fn request_north_up(&mut self) {
        self.mode = CameraMode::TemporaryNorthUp;
        self.temporary_override_moving_ms = 0.0;
        self.manual_heading_offset_rad = 0.0;
    }

    pub fn update(&mut self, input: CameraControllerInput, dt_ms: f32) -> CameraOutput {
        let input_heading_rad = input.rider_heading_rad;
        let dt_sec = (dt_ms.max(0.0)) / 1000.0;
        self.manual_heading_offset_rad =
            normalize_angle(self.manual_heading_offset_rad + input.rotate_delta_rad);
        let zoom_scale = if input.zoom_scale.is_finite() && input.zoom_scale > 0.0 {
            input.zoom_scale
        } else {
            1.0
        };
        self.zoom = (self.zoom * zoom_scale).clamp(MIN_ZOOM, MAX_ZOOM);

        if input.pan_dx != 0.0 || input.pan_dy != 0.0 {
            if !self.pan_lock_active {
                self.pan_lock_active = true;
                self.pan_lock_player = input.player;
                self.pan_lock_anchor_y = self.current_anchor_y;
            }
            self.pan_x += input.pan_dx;
            self.pan_y += input.pan_dy;
            self.idle_ms_since_pan = 0.0;
        } else {
            self.idle_ms_since_pan += dt_ms;
        }
        if self.idle_ms_since_pan > PAN_IDLE_MS {
            let t = (dt_ms / PAN_RECENTER_MS).clamp(0.0, 1.0);
            self.pan_x *= 1.0 - t;
            self.pan_y *= 1.0 - t;
        }
        if self.pan_lock_active
            && self.idle_ms_since_pan > PAN_IDLE_MS
            && self.pan_x.abs() <= PAN_LOCK_RELEASE_EPSILON
            && self.pan_y.abs() <= PAN_LOCK_RELEASE_EPSILON
        {
            self.pan_lock_active = false;
        }
        if self.pan_lock_active {
            self.follow_player = self.pan_lock_player;
        } else {
            self.pan_lock_player = input.player;
            self.follow_player = input.player;
        }

        let motion_vector = if self.has_last_player {
            let dx = input.player.x as i32 - self.last_player.x as i32;
            let dy = input.player.y as i32 - self.last_player.y as i32;
            if dx.abs() + dy.abs() >= MOVING_DELTA_THRESHOLD {
                Some((dx as f32, dy as f32))
            } else {
                None
            }
        } else {
            self.has_last_player = true;
            None
        };
        let delta_moving = motion_vector.is_some();
        self.last_player = input.player;

        let moving = input.speed_mps >= MOVING_SPEED_THRESHOLD_MPS || delta_moving;
        if let Some((dx, dy)) = motion_vector {
            if self.has_filtered_motion {
                let t = (dt_ms / TRAVEL_VECTOR_BLEND_MS).clamp(0.0, 1.0);
                self.filtered_motion_dx += (dx - self.filtered_motion_dx) * t;
                self.filtered_motion_dy += (dy - self.filtered_motion_dy) * t;
            } else {
                self.filtered_motion_dx = dx;
                self.filtered_motion_dy = dy;
                self.has_filtered_motion = true;
            }
            self.travel_heading_rad =
                libm::atan2f(self.filtered_motion_dx, self.filtered_motion_dy);
        }
        self.rider_heading_rad = if moving {
            if self.has_filtered_motion {
                self.travel_heading_rad
            } else {
                input_heading_rad
            }
        } else {
            input_heading_rad
        };
        if moving {
            self.moving_ms += dt_ms;
            self.stopped_ms = 0.0;
        } else {
            self.stopped_ms += dt_ms;
            self.moving_ms = 0.0;
        }

        match self.mode {
            CameraMode::Riding => {
                if self.stopped_ms >= STOP_DELAY_MS {
                    self.mode = CameraMode::StoppedNorthUp;
                    self.manual_heading_offset_rad = 0.0;
                }
            }
            CameraMode::StoppedNorthUp => {
                if self.moving_ms >= RESUME_MOVING_MS {
                    self.mode = CameraMode::Riding;
                }
            }
            CameraMode::TemporaryNorthUp => {
                if moving {
                    self.temporary_override_moving_ms += dt_ms;
                } else {
                    self.temporary_override_moving_ms = 0.0;
                }
                if self.temporary_override_moving_ms >= TEMP_NORTH_RETURN_MOVING_MS {
                    self.mode = CameraMode::Riding;
                    self.temporary_override_moving_ms = 0.0;
                }
            }
        }

        let target_heading = match self.mode {
            CameraMode::Riding => {
                normalize_angle(-self.rider_heading_rad + self.manual_heading_offset_rad)
            }
            CameraMode::StoppedNorthUp | CameraMode::TemporaryNorthUp => 0.0,
        };
        self.map_heading_rad = slew_angle(
            self.map_heading_rad,
            target_heading,
            HEADING_SLEW_RAD_PER_SEC * dt_sec,
        );

        let target_anchor_y = if self.pan_lock_active {
            self.pan_lock_anchor_y
        } else {
            match self.mode {
                CameraMode::Riding => RIDING_ANCHOR_Y,
                CameraMode::StoppedNorthUp | CameraMode::TemporaryNorthUp => STOPPED_ANCHOR_Y,
            }
        };
        let a_blend = (dt_ms / ANCHOR_BLEND_MS).clamp(0.0, 1.0);
        self.current_anchor_y += (target_anchor_y - self.current_anchor_y) * a_blend;

        self.output()
    }

    pub fn output(&self) -> CameraOutput {
        CameraOutput {
            mode: self.mode,
            heading_rad: self.map_heading_rad,
            rider_heading_rad: self.rider_heading_rad,
            follow_player: self.follow_player,
            player_anchor_y: self.current_anchor_y,
            riding_mode: self.mode == CameraMode::Riding,
            interaction_active: self.idle_ms_since_pan < ACTIVE_INTERACTION_MS,
            zoom: self.zoom,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
        }
    }
}

impl Default for CameraControllerState {
    fn default() -> Self {
        Self::new(1.0)
    }
}

pub const SAMPLE_BOUNDS: WorldBounds = WorldBounds {
    min_x: 0,
    max_x: 1000,
    min_y: 0,
    max_y: 1000,
};

pub const SAMPLE_LINES: [Line; 10] = [
    Line {
        from: WorldPoint { x: 100, y: 100 },
        to: WorldPoint { x: 900, y: 100 },
        intensity: 210,
        thickness: 2,
    },
    Line {
        from: WorldPoint { x: 900, y: 100 },
        to: WorldPoint { x: 900, y: 900 },
        intensity: 210,
        thickness: 2,
    },
    Line {
        from: WorldPoint { x: 900, y: 900 },
        to: WorldPoint { x: 100, y: 900 },
        intensity: 210,
        thickness: 2,
    },
    Line {
        from: WorldPoint { x: 100, y: 900 },
        to: WorldPoint { x: 100, y: 100 },
        intensity: 210,
        thickness: 2,
    },
    Line {
        from: WorldPoint { x: 200, y: 200 },
        to: WorldPoint { x: 800, y: 800 },
        intensity: 255,
        thickness: 1,
    },
    Line {
        from: WorldPoint { x: 800, y: 200 },
        to: WorldPoint { x: 200, y: 800 },
        intensity: 255,
        thickness: 1,
    },
    Line {
        from: WorldPoint { x: 300, y: 500 },
        to: WorldPoint { x: 700, y: 500 },
        intensity: 180,
        thickness: 1,
    },
    Line {
        from: WorldPoint { x: 500, y: 300 },
        to: WorldPoint { x: 500, y: 700 },
        intensity: 180,
        thickness: 1,
    },
    Line {
        from: WorldPoint { x: 240, y: 240 },
        to: WorldPoint { x: 240, y: 760 },
        intensity: 150,
        thickness: 1,
    },
    Line {
        from: WorldPoint { x: 760, y: 240 },
        to: WorldPoint { x: 760, y: 760 },
        intensity: 150,
        thickness: 1,
    },
];

pub fn sample_player_for_tick(tick: u32) -> WorldPoint {
    WorldPoint {
        x: 180 + ((tick as i32 * 19) % 620) as i16,
        y: 180 + ((tick as i32 * 13) % 620) as i16,
    }
}

pub fn render_sample_device_style(frame: &mut FrameBuffer<'_>, player: WorldPoint) {
    let view = MinimapView {
        bounds: SAMPLE_BOUNDS,
        background: 18,
        player,
    };
    render_device_style(frame, &SAMPLE_LINES, &view);
}

pub fn render_device_style(frame: &mut FrameBuffer<'_>, lines: &[Line], view: &MinimapView) {
    render_minimap(frame, lines, view);
    apply_device_style(frame);
}

pub fn render_device_style_camera(frame: &mut FrameBuffer<'_>, lines: &[Line], view: &CameraView) {
    render_minimap_camera(frame, lines, view);
    apply_device_style(frame);
}

pub fn render_minimap(frame: &mut FrameBuffer<'_>, lines: &[Line], view: &MinimapView) {
    frame.clear(view.background);
    for line in lines {
        let from = raster::world_to_screen(line.from, view.bounds, frame.width, frame.height);
        let to = raster::world_to_screen(line.to, view.bounds, frame.width, frame.height);
        let (intensity, radius) = line_style_dark_profile(line.intensity, line.thickness, false);
        draw_line(frame, from, to, intensity, radius);
    }
    draw_player(frame, view.player, view.bounds);
    draw_north_indicator(frame, 0.0);
}

pub fn render_minimap_camera(frame: &mut FrameBuffer<'_>, lines: &[Line], view: &CameraView) {
    frame.clear(view.background);
    let transform = CameraTransform::new(view, frame.width, frame.height);
    let zoom = transform.zoom;
    let half_w = (view.base_bounds.width().max(1) as f32) / (2.0 * zoom);
    let half_h = (view.base_bounds.height().max(1) as f32) / (2.0 * zoom);
    // Extra margin keeps line edges visible during pan/rotation while rejecting far geometry.
    let mx = (half_w * 1.6) as i32;
    let my = (half_h * 1.6) as i32;
    let cmin_x = view.center.x as i32 - mx;
    let cmax_x = view.center.x as i32 + mx;
    let cmin_y = view.center.y as i32 - my;
    let cmax_y = view.center.y as i32 + my;

    for (idx, line) in lines.iter().enumerate() {
        let min_x = (line.from.x.min(line.to.x)) as i32;
        let max_x = (line.from.x.max(line.to.x)) as i32;
        let min_y = (line.from.y.min(line.to.y)) as i32;
        let max_y = (line.from.y.max(line.to.y)) as i32;
        if max_x < cmin_x || min_x > cmax_x || max_y < cmin_y || min_y > cmax_y {
            continue;
        }
        if view.interaction_active && line.thickness <= 1 && idx % 2 == 0 {
            continue;
        }
        let from = transform.world_to_screen(line.from);
        let to = transform.world_to_screen(line.to);
        let clip = clip_line_to_rect(
            from.0,
            from.1,
            to.0,
            to.1,
            0,
            (frame.width as i32) - 1,
            0,
            (frame.height as i32) - 1,
        );
        let Some((x0, y0, x1, y1)) = clip else {
            continue;
        };
        let (intensity, radius) =
            line_style_dark_profile(line.intensity, line.thickness, view.interaction_active);
        draw_line(frame, (x0, y0), (x1, y1), intensity, radius);
    }
    let p = transform.player_anchor_screen();
    draw_player_screen(frame, p, view.rider_heading_rad, view.riding_mode);
    draw_north_indicator(frame, view.heading_rad);
}

pub fn north_indicator_hit_test(width: usize, height: usize, x: i32, y: i32) -> bool {
    let (cx, cy, r) = north_indicator_geometry(width, height);
    let dx = x - cx;
    let dy = y - cy;
    (dx * dx) + (dy * dy) <= r * r
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    #[test]
    fn device_specs_match_target() {
        assert_eq!(WAVESHARE_ESP32_P4_3_4.width, 800);
        assert_eq!(WAVESHARE_ESP32_P4_3_4.height, 800);
        assert_eq!(WAVESHARE_ESP32_P4_4_0.width, 720);
        assert_eq!(WAVESHARE_ESP32_P4_4_0.height, 720);
    }

    #[test]
    fn style_mask_clears_corners() {
        let mut px = vec![0_u8; 800 * 800];
        let mut fb = FrameBuffer::new(800, 800, &mut px);
        render_sample_device_style(&mut fb, sample_player_for_tick(0));

        assert_eq!(fb.pixels[0], 0);
        assert_eq!(fb.pixels[799], 0);
        assert_eq!(fb.pixels[799 * 800], 0);
        assert_eq!(fb.pixels[800 * 800 - 1], 0);
    }

    #[test]
    fn deterministic_frame_checksum() {
        let mut px = vec![0_u8; 800 * 800];
        let mut fb = FrameBuffer::new(800, 800, &mut px);
        render_sample_device_style(&mut fb, sample_player_for_tick(42));

        let sum: u64 = fb.pixels.iter().map(|v| *v as u64).sum();
        assert_eq!(sum, 14661203);
    }

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn stationary_transitions_to_north_up() {
        let mut c = CameraControllerState::default();
        let mut player = WorldPoint { x: 500, y: 500 };
        for _ in 0..10 {
            player.x += 3;
            c.update(
                CameraControllerInput {
                    player,
                    rider_heading_rad: 1.2,
                    speed_mps: 2.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        assert_eq!(c.output().mode, CameraMode::Riding);

        for _ in 0..20 {
            c.update(
                CameraControllerInput {
                    player,
                    rider_heading_rad: 1.2,
                    speed_mps: 0.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        let out = c.output();
        assert_eq!(out.mode, CameraMode::StoppedNorthUp);
        assert!(out.heading_rad.abs() < 0.35);
    }

    #[test]
    fn moving_resumes_riding_after_dwell() {
        let mut c = CameraControllerState::default();
        let mut player = WorldPoint { x: 400, y: 400 };
        for _ in 0..20 {
            c.update(
                CameraControllerInput {
                    player,
                    rider_heading_rad: 0.0,
                    speed_mps: 0.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        assert_eq!(c.output().mode, CameraMode::StoppedNorthUp);

        for _ in 0..4 {
            player.y += 4;
            c.update(
                CameraControllerInput {
                    player,
                    rider_heading_rad: 0.8,
                    speed_mps: 2.2,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        assert_eq!(c.output().mode, CameraMode::Riding);
    }

    #[test]
    fn riding_heading_follows_movement_direction() {
        let mut c = CameraControllerState::default();
        let mut player = WorldPoint { x: 400, y: 400 };

        for _ in 0..8 {
            player.x += 5;
            c.update(
                CameraControllerInput {
                    player,
                    rider_heading_rad: 0.0,
                    speed_mps: 2.2,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }

        let out = c.output();
        assert_eq!(out.mode, CameraMode::Riding);
        assert!(out.heading_rad < -0.7);
        assert!(out.rider_heading_rad > 1.3 && out.rider_heading_rad < 1.9);
    }

    #[test]
    fn westward_travel_rotates_map_so_west_is_up() {
        let mut c = CameraControllerState::default();
        let mut player = WorldPoint { x: 600, y: 400 };

        for _ in 0..8 {
            player.x -= 5;
            c.update(
                CameraControllerInput {
                    player,
                    rider_heading_rad: 0.0,
                    speed_mps: 2.2,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }

        let out = c.output();
        assert_eq!(out.mode, CameraMode::Riding);
        assert!(out.heading_rad > 0.7);
        assert!(out.rider_heading_rad < -1.3 && out.rider_heading_rad > -1.9);
    }

    #[test]
    fn heading_response_is_smoothed_for_quick_direction_change() {
        let mut c = CameraControllerState::default();
        let mut player = WorldPoint { x: 400, y: 400 };

        for _ in 0..6 {
            player.x += 6;
            c.update(
                CameraControllerInput {
                    player,
                    rider_heading_rad: 0.0,
                    speed_mps: 2.4,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        let before = c.output().heading_rad;

        player.y += 6;
        c.update(
            CameraControllerInput {
                player,
                rider_heading_rad: 0.0,
                speed_mps: 2.4,
                pan_dx: 0.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            120.0,
        );
        let after = c.output().heading_rad;

        assert!(before < -0.25);
        assert!((after - before).abs() < 1.1);
    }

    #[test]
    fn temporary_override_expires_on_sustained_movement() {
        let mut c = CameraControllerState::default();
        let mut player = WorldPoint { x: 300, y: 300 };
        for _ in 0..6 {
            player.x += 5;
            c.update(
                CameraControllerInput {
                    player,
                    rider_heading_rad: 1.0,
                    speed_mps: 2.5,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        c.toggle_temporary_north_up();
        assert_eq!(c.output().mode, CameraMode::TemporaryNorthUp);

        for _ in 0..14 {
            player.x += 4;
            c.update(
                CameraControllerInput {
                    player,
                    rider_heading_rad: 1.0,
                    speed_mps: 2.5,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        assert_eq!(c.output().mode, CameraMode::TemporaryNorthUp);

        player.x += 4;
        c.update(
            CameraControllerInput {
                player,
                rider_heading_rad: 1.0,
                speed_mps: 2.5,
                pan_dx: 0.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            120.0,
        );
        assert_eq!(c.output().mode, CameraMode::Riding);
    }

    #[test]
    fn heading_blends_shortest_angle() {
        let mut c = CameraControllerState::default();
        let p = WorldPoint { x: 100, y: 100 };
        c.update(
            CameraControllerInput {
                player: p,
                rider_heading_rad: core::f32::consts::PI - 0.1,
                speed_mps: 2.0,
                pan_dx: 0.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            120.0,
        );
        let a = c.output().heading_rad;
        c.update(
            CameraControllerInput {
                player: WorldPoint { x: 104, y: 100 },
                rider_heading_rad: -core::f32::consts::PI + 0.1,
                speed_mps: 2.0,
                pan_dx: 0.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            120.0,
        );
        let b = c.output().heading_rad;
        assert!((b - a).abs() < 1.0);
    }

    #[test]
    fn anchor_transitions_between_modes() {
        let mut c = CameraControllerState::default();
        let mut p = WorldPoint { x: 200, y: 200 };
        for _ in 0..6 {
            p.x += 3;
            c.update(
                CameraControllerInput {
                    player: p,
                    rider_heading_rad: 0.5,
                    speed_mps: 1.8,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        assert!(c.output().player_anchor_y > 0.6);

        for _ in 0..24 {
            c.update(
                CameraControllerInput {
                    player: p,
                    rider_heading_rad: 0.5,
                    speed_mps: 0.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        assert!(approx(c.output().player_anchor_y, 0.5, 0.03));
    }

    #[test]
    fn pan_recenters_after_idle() {
        let mut c = CameraControllerState::default();
        let p = WorldPoint { x: 500, y: 500 };
        c.update(
            CameraControllerInput {
                player: p,
                rider_heading_rad: 0.0,
                speed_mps: 0.0,
                pan_dx: 40.0,
                pan_dy: -20.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            120.0,
        );
        let before = c.output();
        assert!(before.pan_x.abs() > 10.0);
        for _ in 0..4 {
            c.update(
                CameraControllerInput {
                    player: p,
                    rider_heading_rad: 0.0,
                    speed_mps: 0.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        let still = c.output();
        assert!(still.pan_x.abs() >= before.pan_x.abs() - 0.1);
        for _ in 0..20 {
            c.update(
                CameraControllerInput {
                    player: p,
                    rider_heading_rad: 0.0,
                    speed_mps: 0.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        let after = c.output();
        assert!(after.pan_x.abs() < before.pan_x.abs());
    }

    #[test]
    fn pan_lock_holds_follow_target_during_manual_pan() {
        let mut c = CameraControllerState::default();
        let pan_start = WorldPoint { x: 500, y: 500 };
        c.update(
            CameraControllerInput {
                player: pan_start,
                rider_heading_rad: 0.0,
                speed_mps: 2.0,
                pan_dx: 30.0,
                pan_dy: -10.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            120.0,
        );

        let moved = WorldPoint { x: 620, y: 590 };
        let out = c.update(
            CameraControllerInput {
                player: moved,
                rider_heading_rad: 0.0,
                speed_mps: 2.0,
                pan_dx: 0.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            120.0,
        );
        assert_eq!(out.follow_player, pan_start);
    }

    #[test]
    fn pan_lock_releases_after_recenter() {
        let mut c = CameraControllerState::default();
        let pan_start = WorldPoint { x: 450, y: 450 };
        c.update(
            CameraControllerInput {
                player: pan_start,
                rider_heading_rad: 0.0,
                speed_mps: 2.0,
                pan_dx: 40.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            120.0,
        );

        let moved = WorldPoint { x: 640, y: 640 };
        let mut out = c.output();
        for _ in 0..30 {
            out = c.update(
                CameraControllerInput {
                    player: moved,
                    rider_heading_rad: 0.0,
                    speed_mps: 2.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        assert!(out.pan_x.abs() <= PAN_LOCK_RELEASE_EPSILON);
        assert_eq!(out.follow_player, moved);
    }

    #[test]
    fn pan_lock_keeps_anchor_stable_while_mode_changes() {
        let mut c = CameraControllerState::default();
        let mut p = WorldPoint { x: 300, y: 300 };

        for _ in 0..8 {
            p.x += 5;
            c.update(
                CameraControllerInput {
                    player: p,
                    rider_heading_rad: 0.7,
                    speed_mps: 2.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }

        c.update(
            CameraControllerInput {
                player: p,
                rider_heading_rad: 0.7,
                speed_mps: 0.0,
                pan_dx: 20.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            120.0,
        );
        let locked_anchor = c.output().player_anchor_y;

        for _ in 0..24 {
            c.update(
                CameraControllerInput {
                    player: p,
                    rider_heading_rad: 0.7,
                    speed_mps: 0.0,
                    pan_dx: 0.1,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
            let out = c.output();
            assert!(approx(out.player_anchor_y, locked_anchor, 0.01));
        }
        assert_eq!(c.output().mode, CameraMode::StoppedNorthUp);
    }

    #[test]
    fn rotate_delta_changes_riding_heading() {
        let mut c = CameraControllerState::default();
        for i in 0..6 {
            c.update(
                CameraControllerInput {
                    player: WorldPoint {
                        x: 100 + i * 4,
                        y: 100,
                    },
                    rider_heading_rad: 0.0,
                    speed_mps: 2.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        let before = c.output().heading_rad;
        c.update(
            CameraControllerInput {
                player: WorldPoint { x: 130, y: 100 },
                rider_heading_rad: 0.0,
                speed_mps: 2.0,
                pan_dx: 0.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.8,
            },
            120.0,
        );
        let after = c.output().heading_rad;
        assert!(normalize_angle(after - before).abs() > 0.05);
    }

    #[test]
    fn request_north_up_rotates_towards_zero() {
        let mut c = CameraControllerState::default();
        let mut p = WorldPoint { x: 200, y: 200 };
        for _ in 0..8 {
            p.x += 5;
            c.update(
                CameraControllerInput {
                    player: p,
                    rider_heading_rad: 1.2,
                    speed_mps: 2.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        c.request_north_up();
        for _ in 0..8 {
            c.update(
                CameraControllerInput {
                    player: p,
                    rider_heading_rad: 1.2,
                    speed_mps: 2.0,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                120.0,
            );
        }
        let out = c.output();
        assert_eq!(out.mode, CameraMode::TemporaryNorthUp);
        assert!(out.heading_rad.abs() < 0.35);
    }

    #[test]
    fn north_indicator_hit_test_detects_center() {
        let w = WAVESHARE_ESP32_P4_3_4.width;
        let h = WAVESHARE_ESP32_P4_3_4.height;
        let (cx, cy, _) = north_indicator_geometry(w, h);
        assert!(north_indicator_hit_test(w, h, cx, cy));
    }
}
