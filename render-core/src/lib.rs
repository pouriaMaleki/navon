#![no_std]

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
            self.travel_heading_rad = libm::atan2f(self.filtered_motion_dx, self.filtered_motion_dy);
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
        let from = world_to_screen(line.from, view.bounds, frame.width, frame.height);
        let to = world_to_screen(line.to, view.bounds, frame.width, frame.height);
        draw_line(
            frame,
            from,
            to,
            line.intensity,
            line.thickness.max(1) as i32,
        );
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
        draw_line(
            frame,
            (x0, y0),
            (x1, y1),
            line.intensity,
            line.thickness.max(1) as i32,
        );
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

fn draw_player(frame: &mut FrameBuffer<'_>, player: WorldPoint, bounds: WorldBounds) {
    let (x, y) = world_to_screen(player, bounds, frame.width, frame.height);
    draw_player_screen(frame, (x, y), 0.0, false);
}

fn draw_player_screen(
    frame: &mut FrameBuffer<'_>,
    p: (i32, i32),
    heading_rad: f32,
    riding_mode: bool,
) {
    let (x, y) = p;
    if riding_mode {
        // Riding marker: brighter glow + forward pointer.
        stamp_circle(frame, x, y, 9, 205);
        stamp_circle(frame, x, y, 6, 238);
        stamp_circle(frame, x, y, 3, 252);

        let nx = libm::sinf(heading_rad);
        let ny = -libm::cosf(heading_rad);
        for step in 1..=8 {
            let fx = x + (nx * (step as f32) * 1.25) as i32;
            let fy = y + (ny * (step as f32) * 1.25) as i32;
            let value = if step < 4 { 252 } else { 238 };
            stamp_circle(frame, fx, fy, 1, value);
        }
    } else {
        // Stopped marker: larger static marker for easy map readability.
        stamp_circle(frame, x, y, 8, 215);
        stamp_circle(frame, x, y, 5, 242);
        stamp_circle(frame, x, y, 2, 255);
    }
}

fn draw_north_indicator(frame: &mut FrameBuffer<'_>, map_heading_rad: f32) {
    let (cx, cy, r) = north_indicator_geometry(frame.width, frame.height);
    // Fill backdrop with explicit writes so it remains visible over bright geometry.
    fill_circle(frame, cx, cy, r, 18);
    stamp_circle(frame, cx, cy, r, 96);
    stamp_circle(frame, cx, cy, (r - 2).max(1), 24);

    // Arrow points towards world north relative to current map rotation.
    let angle = -core::f32::consts::FRAC_PI_2 + map_heading_rad;
    let nx = libm::cosf(angle);
    let ny = libm::sinf(angle);
    let tip_x = cx + (nx * (r as f32 - 4.0)) as i32;
    let tip_y = cy + (ny * (r as f32 - 4.0)) as i32;
    draw_line(frame, (cx, cy), (tip_x, tip_y), 245, 1);

    let left = angle + 2.55;
    let right = angle - 2.55;
    let wing = (r as f32 * 0.45).max(3.0);
    let lx = tip_x + (libm::cosf(left) * wing) as i32;
    let ly = tip_y + (libm::sinf(left) * wing) as i32;
    let rx = tip_x + (libm::cosf(right) * wing) as i32;
    let ry = tip_y + (libm::sinf(right) * wing) as i32;
    draw_line(frame, (tip_x, tip_y), (lx, ly), 245, 1);
    draw_line(frame, (tip_x, tip_y), (rx, ry), 245, 1);
}

fn north_indicator_geometry(width: usize, height: usize) -> (i32, i32, i32) {
    let min_side = width.min(height) as i32;
    let frame_cx = (width / 2) as i32;
    let frame_cy = (height / 2) as i32;
    let frame_r = (min_side / 2) - 8;
    let indicator_r = (min_side / 12).clamp(24, 36);
    let radial = (frame_r - indicator_r - 10).max(indicator_r + 1);
    // 45-degree placement in top-right area, always inside round screen mask.
    let offset = (radial as f32 * 0.70710677) as i32;
    (frame_cx + offset, frame_cy - offset, indicator_r)
}

struct CameraTransform {
    zoom: f32,
    center_x: f32,
    center_y: f32,
    cos_h: f32,
    sin_h: f32,
    sx_scale: f32,
    sy_scale: f32,
    anchor_x_px: f32,
    anchor_y_px: f32,
}

impl CameraTransform {
    fn new(view: &CameraView, width: usize, height: usize) -> Self {
        let zoom = if view.zoom < 0.2 { 0.2 } else { view.zoom };
        let half_w = (view.base_bounds.width().max(1) as f32) / (2.0 * zoom);
        let half_h = (view.base_bounds.height().max(1) as f32) / (2.0 * zoom);
        let anchor_x = view.player_anchor_x.clamp(0.2, 0.8);
        let anchor_y = view.player_anchor_y.clamp(0.2, 0.86);
        Self {
            zoom,
            center_x: view.center.x as f32,
            center_y: view.center.y as f32,
            cos_h: libm::cosf(view.heading_rad),
            sin_h: libm::sinf(view.heading_rad),
            sx_scale: (width as f32 * 0.5) / half_w,
            sy_scale: (height as f32 * 0.5) / half_h,
            anchor_x_px: width as f32 * anchor_x,
            anchor_y_px: height as f32 * anchor_y,
        }
    }

    fn world_to_screen(&self, p: WorldPoint) -> (i32, i32) {
        let dx = p.x as f32 - self.center_x;
        let dy = p.y as f32 - self.center_y;
        let rx = dx * self.cos_h + dy * self.sin_h;
        let ry = -dx * self.sin_h + dy * self.cos_h;
        let sx = (rx * self.sx_scale) + self.anchor_x_px;
        let sy = (-ry * self.sy_scale) + self.anchor_y_px;
        (sx as i32, sy as i32)
    }

    fn player_anchor_screen(&self) -> (i32, i32) {
        (self.anchor_x_px as i32, self.anchor_y_px as i32)
    }
}

fn compute_out_code(x: i32, y: i32, xmin: i32, xmax: i32, ymin: i32, ymax: i32) -> u8 {
    let mut code = 0_u8;
    if x < xmin {
        code |= 1;
    } else if x > xmax {
        code |= 2;
    }
    if y < ymin {
        code |= 4;
    } else if y > ymax {
        code |= 8;
    }
    code
}

fn clip_line_to_rect(
    mut x0: i32,
    mut y0: i32,
    mut x1: i32,
    mut y1: i32,
    xmin: i32,
    xmax: i32,
    ymin: i32,
    ymax: i32,
) -> Option<(i32, i32, i32, i32)> {
    let mut out0 = compute_out_code(x0, y0, xmin, xmax, ymin, ymax);
    let mut out1 = compute_out_code(x1, y1, xmin, xmax, ymin, ymax);

    loop {
        if out0 == 0 && out1 == 0 {
            return Some((x0, y0, x1, y1));
        }
        if (out0 & out1) != 0 {
            return None;
        }
        let out = if out0 != 0 { out0 } else { out1 };
        let dx = x1 - x0;
        let dy = y1 - y0;
        let (x, y) = if (out & 8) != 0 {
            if dy == 0 {
                return None;
            }
            (x0 + (dx * (ymax - y0)) / dy, ymax)
        } else if (out & 4) != 0 {
            if dy == 0 {
                return None;
            }
            (x0 + (dx * (ymin - y0)) / dy, ymin)
        } else if (out & 2) != 0 {
            if dx == 0 {
                return None;
            }
            (xmax, y0 + (dy * (xmax - x0)) / dx)
        } else {
            if dx == 0 {
                return None;
            }
            (xmin, y0 + (dy * (xmin - x0)) / dx)
        };

        if out == out0 {
            x0 = x;
            y0 = y;
            out0 = compute_out_code(x0, y0, xmin, xmax, ymin, ymax);
        } else {
            x1 = x;
            y1 = y;
            out1 = compute_out_code(x1, y1, xmin, xmax, ymin, ymax);
        }
    }
}

fn world_to_screen(p: WorldPoint, bounds: WorldBounds, width: usize, height: usize) -> (i32, i32) {
    let bw = bounds.width().max(1) as i32;
    let bh = bounds.height().max(1) as i32;
    let px = (p.x - bounds.min_x) as i32;
    let py = (p.y - bounds.min_y) as i32;

    let sx = (px * (width.saturating_sub(1) as i32)) / bw;
    let sy = ((bh - py) * (height.saturating_sub(1) as i32)) / bh;
    (sx, sy)
}

fn draw_line(frame: &mut FrameBuffer<'_>, from: (i32, i32), to: (i32, i32), value: u8, r: i32) {
    let (mut x0, mut y0) = from;
    let (x1, y1) = to;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        stamp_circle(frame, x0, y0, r, value);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn stamp_circle(frame: &mut FrameBuffer<'_>, cx: i32, cy: i32, r: i32, value: u8) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                frame.set_pixel_checked(cx + dx, cy + dy, value);
            }
        }
    }
}

fn fill_circle(frame: &mut FrameBuffer<'_>, cx: i32, cy: i32, r: i32, value: u8) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy > r * r {
                continue;
            }
            let x = cx + dx;
            let y = cy + dy;
            if x < 0 || y < 0 {
                continue;
            }
            let ux = x as usize;
            let uy = y as usize;
            if ux >= frame.width || uy >= frame.height {
                continue;
            }
            let idx = uy * frame.width + ux;
            frame.pixels[idx] = value;
        }
    }
}

fn apply_device_style(frame: &mut FrameBuffer<'_>) {
    let cx = (frame.width / 2) as i32;
    let cy = (frame.height / 2) as i32;
    let radius = ((frame.width.min(frame.height) as i32) / 2) - 8;
    let inner = (radius - 6).max(1);

    for y in 0..frame.height as i32 {
        for x in 0..frame.width as i32 {
            let dx = x - cx;
            let dy = y - cy;
            let d2 = dx * dx + dy * dy;
            let idx = (y as usize) * frame.width + (x as usize);
            if d2 > radius * radius {
                frame.pixels[idx] = 0;
            } else if d2 >= inner * inner {
                frame.pixels[idx] = frame.pixels[idx].max(210);
            }
        }
    }

    for y in (cy - radius)..(cy - radius + 14) {
        for x in (cx - 2)..=(cx + 2) {
            frame.set_pixel_checked(x, y, 255);
        }
    }
}

fn normalize_angle(mut angle: f32) -> f32 {
    while angle > core::f32::consts::PI {
        angle -= 2.0 * core::f32::consts::PI;
    }
    while angle < -core::f32::consts::PI {
        angle += 2.0 * core::f32::consts::PI;
    }
    angle
}

fn slew_angle(current: f32, target: f32, max_step: f32) -> f32 {
    let delta = normalize_angle(target - current);
    let clamped = delta.clamp(-max_step, max_step);
    normalize_angle(current + clamped)
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
        assert_eq!(sum, 14864873);
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
