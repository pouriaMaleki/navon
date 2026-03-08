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
    pub zoom: f32,
    pub base_bounds: WorldBounds,
    pub background: u8,
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
        draw_line(frame, from, to, line.intensity, line.thickness.max(1) as i32);
    }
    draw_player(frame, view.player, view.bounds);
}

pub fn render_minimap_camera(frame: &mut FrameBuffer<'_>, lines: &[Line], view: &CameraView) {
    frame.clear(view.background);
    let zoom = if view.zoom < 0.2 { 0.2 } else { view.zoom };
    let half_w = (view.base_bounds.width().max(1) as f32) / (2.0 * zoom);
    let half_h = (view.base_bounds.height().max(1) as f32) / (2.0 * zoom);
    // Extra margin keeps line edges visible during pan/rotation while rejecting far geometry.
    let mx = (half_w * 1.6) as i32;
    let my = (half_h * 1.6) as i32;
    let cmin_x = view.center.x as i32 - mx;
    let cmax_x = view.center.x as i32 + mx;
    let cmin_y = view.center.y as i32 - my;
    let cmax_y = view.center.y as i32 + my;

    for line in lines {
        let min_x = (line.from.x.min(line.to.x)) as i32;
        let max_x = (line.from.x.max(line.to.x)) as i32;
        let min_y = (line.from.y.min(line.to.y)) as i32;
        let max_y = (line.from.y.max(line.to.y)) as i32;
        if max_x < cmin_x || min_x > cmax_x || max_y < cmin_y || min_y > cmax_y {
            continue;
        }
        let from = world_to_screen_camera(line.from, view, frame.width, frame.height);
        let to = world_to_screen_camera(line.to, view, frame.width, frame.height);
        draw_line(frame, from, to, line.intensity, line.thickness.max(1) as i32);
    }
    let p = world_to_screen_camera(view.player, view, frame.width, frame.height);
    draw_player_screen(frame, p);
}

fn draw_player(frame: &mut FrameBuffer<'_>, player: WorldPoint, bounds: WorldBounds) {
    let (x, y) = world_to_screen(player, bounds, frame.width, frame.height);
    draw_player_screen(frame, (x, y));
}

fn draw_player_screen(frame: &mut FrameBuffer<'_>, p: (i32, i32)) {
    let (x, y) = p;
    for dy in -2..=2 {
        for dx in -2..=2 {
            if dx * dx + dy * dy <= 4 {
                frame.set_pixel_checked(x + dx, y + dy, 255);
            }
        }
    }
}

fn world_to_screen_camera(
    p: WorldPoint,
    view: &CameraView,
    width: usize,
    height: usize,
) -> (i32, i32) {
    let zoom = if view.zoom < 0.2 { 0.2 } else { view.zoom };
    let dx = (p.x - view.center.x) as f32;
    let dy = (p.y - view.center.y) as f32;
    let cos_h = libm::cosf(view.heading_rad);
    let sin_h = libm::sinf(view.heading_rad);

    // Rotate world by -heading so forward direction stays near screen north.
    let rx = dx * cos_h + dy * sin_h;
    let ry = -dx * sin_h + dy * cos_h;

    let half_w = (view.base_bounds.width().max(1) as f32) / (2.0 * zoom);
    let half_h = (view.base_bounds.height().max(1) as f32) / (2.0 * zoom);
    let sx = ((rx / half_w) * (width as f32 * 0.5)) + (width as f32 * 0.5);
    let sy = ((-ry / half_h) * (height as f32 * 0.5)) + (height as f32 * 0.5);
    (sx as i32, sy as i32)
}

fn world_to_screen(
    p: WorldPoint,
    bounds: WorldBounds,
    width: usize,
    height: usize,
) -> (i32, i32) {
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
        assert_eq!(sum, 14500818);
    }
}
