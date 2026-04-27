use crate::framebuffer::RenderFramebuffer;
use render_core::RenderScene;
use runtime_core::api::{CameraStateSnapshot, MapQueryResult, RuntimeConfig, RuntimeFrameOutput, ViewportSize};
use runtime_core::map::{self, MapSource, meters_per_pixel_for_zoom};

/// Pixel dimensions of the oversized render cache. The 400 px margin on each
/// side of the 800×800 viewport gives about 4 frames of pan headroom at typical
/// finger speeds (~100 px/frame) before a cache miss triggers a re-render.
const WORLD_W: u32 = 1600;
const WORLD_H: u32 = 1600;

/// Pre-rendered basemap cache larger than the display viewport. While the
/// camera stays within the buffered region and orientation/zoom are unchanged,
/// pan frames are served by blitting a crop from this buffer — no vector
/// rasterization needed.
pub struct WorldBuffer {
    framebuffer: RenderFramebuffer,
    center: runtime_core::api::WorldPoint,
    zoom: f32,
    orientation_rad: f32,
    valid: bool,
    /// Geometry count from the last full render, returned on blit frames so
    /// callers can distinguish "world buffer populated" from "empty map".
    last_geometry_count: usize,
}

impl WorldBuffer {
    pub fn new() -> Self {
        Self {
            framebuffer: RenderFramebuffer::new(WORLD_W, WORLD_H),
            center: runtime_core::api::WorldPoint::ORIGIN,
            zoom: 0.0,
            orientation_rad: 0.0,
            valid: false,
            last_geometry_count: 0,
        }
    }

    /// Geometry count from the last full render. Callers may report this on
    /// blit frames so the metric reflects the buffered scene, not an empty query.
    pub fn last_geometry_count(&self) -> usize {
        self.last_geometry_count
    }

    /// Returns true when the world buffer can serve a viewport crop for `camera`
    /// without re-rendering: same zoom/orientation and camera center is within
    /// the buffered area.
    pub fn is_valid_for(&self, camera: &CameraStateSnapshot) -> bool {
        if !self.valid {
            return false;
        }
        if (self.zoom - camera.zoom).abs() > 1e-3 {
            return false;
        }
        // Allow up to ~0.06° of orientation drift before treating it as a
        // rotation gesture that requires a new render.
        if (self.orientation_rad - camera.orientation_rad).abs() > 0.001 {
            return false;
        }
        let mpp = meters_per_pixel_for_zoom(camera.zoom);
        let (cx, cy) = self.camera_offset_px(camera, mpp);
        let half_w = WORLD_W as f32 / 2.0;
        let half_h = WORLD_H as f32 / 2.0;
        let half_vp_w = 400.0_f32; // 800 / 2
        let half_vp_h = 400.0_f32;
        cx - half_vp_w >= -half_w
            && cx + half_vp_w <= half_w
            && cy - half_vp_h >= -half_h
            && cy + half_vp_h <= half_h
    }

    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Renders the oversized basemap (no screen-anchored UI) into the world
    /// buffer and marks it valid. On the same call, blits the current viewport
    /// crop into `display_fb` and returns the geometry count.
    ///
    /// Callers should invoke `render_ui` on `display_fb` afterwards to layer
    /// the speed panel, north indicator, and other screen-fixed overlays.
    pub fn render_and_blit<S: MapSource>(
        &mut self,
        map_source: &mut S,
        config: &RuntimeConfig,
        output: &RuntimeFrameOutput,
        display_fb: &mut RenderFramebuffer,
    ) -> usize {
        let world_viewport = ViewportSize::new(WORLD_W, WORLD_H);
        let oversized_query = map::build_query(&output.camera, world_viewport);
        let geometry = map_source.query(&oversized_query);
        let geometry_count = geometry.geometry.len();

        // Temporarily patch map_query so render_world uses the right bounds/mpp.
        let world_output = RuntimeFrameOutput {
            map_query: oversized_query,
            ..output.clone()
        };
        render_core::render_world(
            RenderScene {
                config,
                output: &world_output,
                geometry: &geometry,
            },
            &mut self.framebuffer,
        );

        self.center = output.camera.center_world;
        self.zoom = output.camera.zoom;
        self.orientation_rad = output.camera.orientation_rad;
        self.valid = true;
        self.last_geometry_count = geometry_count;

        self.blit_into(&output.camera, display_fb);
        geometry_count
    }

    /// Copies the viewport-sized crop from the world buffer into `dst`.
    /// Assumes `is_valid_for(camera)` is true.
    pub fn blit_into(&self, camera: &CameraStateSnapshot, dst: &mut RenderFramebuffer) {
        let mpp = meters_per_pixel_for_zoom(camera.zoom);
        let (cx, cy) = self.camera_offset_px(camera, mpp);

        // Top-left pixel in world buffer coordinates for this viewport.
        let tl_x = ((WORLD_W as f32 / 2.0) + cx - 400.0).round() as usize;
        let tl_y = ((WORLD_H as f32 / 2.0) + cy - 400.0).round() as usize;

        let src = self.framebuffer.pixels();
        let dst = dst.pixels_mut();
        let bpp = src.len() / (WORLD_W as usize * WORLD_H as usize);
        let src_stride = WORLD_W as usize * bpp;
        let dst_stride = 800 * bpp;

        for row in 0..800_usize {
            let src_start = (tl_y + row) * src_stride + tl_x * bpp;
            let dst_start = row * dst_stride;
            dst[dst_start..dst_start + dst_stride]
                .copy_from_slice(&src[src_start..src_start + dst_stride]);
        }
    }

    /// Offset of the camera center from the world buffer center, in display
    /// pixels, in the rotated (screen) coordinate frame.
    fn camera_offset_px(&self, camera: &CameraStateSnapshot, mpp: f64) -> (f32, f32) {
        let dx = camera.center_world.x_m - self.center.x_m;
        let dy = camera.center_world.y_m - self.center.y_m;
        let sin = f64::from(self.orientation_rad).sin();
        let cos = f64::from(self.orientation_rad).cos();
        let local_east = dx * cos - dy * sin;
        let local_north = dx * sin + dy * cos;
        ((local_east / mpp) as f32, -(local_north / mpp) as f32)
    }
}

/// Render only the screen-anchored UI overlays (speed panel, north indicator,
/// rider marker, turn banners) into `display_fb` from the latest `output`.
/// Does not clear the framebuffer — call after `blit_into` or `render_and_blit`.
pub fn render_ui_overlay(
    config: &RuntimeConfig,
    output: &RuntimeFrameOutput,
    geometry: &MapQueryResult,
    display_fb: &mut RenderFramebuffer,
) {
    render_core::render_ui(
        RenderScene {
            config,
            output,
            geometry,
        },
        display_fb,
    );
}
