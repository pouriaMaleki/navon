pub mod camera_view;
pub mod math;
pub mod overlay;
pub mod raster;
pub mod style;
pub mod visibility;

use runtime_core::api::{MapQueryResult, RuntimeConfig, RuntimeFrameOutput, ViewportSize};

use camera_view::CameraView;
use overlay::draw_overlay;
use raster::Framebuffer;
use style::RenderStyle;
use visibility::clip_segment_to_viewport;

#[derive(Debug, Clone, PartialEq)]
pub struct RenderScene<'a> {
    pub config: &'a RuntimeConfig,
    pub output: &'a RuntimeFrameOutput,
    pub geometry: &'a MapQueryResult,
}

pub fn render_frame(scene: RenderScene<'_>, framebuffer: &mut Framebuffer) {
    let viewport = ViewportSize::new(framebuffer.width(), framebuffer.height());
    let camera_view = CameraView::new(viewport, &scene.output.camera);
    let style = RenderStyle::default();

    framebuffer.clear(style.background_intensity);

    for candidate in &scene.geometry.geometry {
        if let runtime_core::api::GeometryCandidate::Polyline(polyline) = candidate {
            let stroke = style.stroke_for_layer(polyline.layer);
            for segment in polyline.points.windows(2) {
                let [from, to] = segment else {
                    continue;
                };
                let from_screen = camera_view.world_to_screen(*from);
                let to_screen = camera_view.world_to_screen(*to);
                if let Some((clip_from, clip_to)) =
                    clip_segment_to_viewport(from_screen, to_screen, viewport)
                {
                    framebuffer.draw_line(
                        clip_from,
                        clip_to,
                        stroke.intensity,
                        stroke.thickness_px,
                    );
                }
            }
        }
    }

    draw_overlay(
        scene.config,
        &scene.output.camera,
        &scene.output.overlay,
        viewport,
        framebuffer,
    );
}

#[cfg(test)]
mod tests {
    use runtime_core::api::{
        CameraMode, CameraStateSnapshot, MapLayer, MapPolylineCandidate, NormalizedScreenPoint,
        OverlayState, RuntimeConfig, RuntimeFrameOutput, WorldPoint,
    };

    use super::*;

    fn sample_output() -> RuntimeFrameOutput {
        RuntimeFrameOutput {
            frame_index: 1,
            camera: CameraStateSnapshot {
                mode: CameraMode::Riding,
                focus_world: WorldPoint::new(0.0, 0.0),
                center_world: WorldPoint::new(0.0, 0.0),
                zoom: 15.5,
                orientation_rad: 0.0,
                rider_anchor: NormalizedScreenPoint::CENTER,
                follow_locked: false,
                recenter_active: false,
            },
            overlay: OverlayState {
                north_indicator_visible: true,
                north_up_active: false,
                rider_heading_rad: Some(0.0),
            },
            ..RuntimeFrameOutput::default()
        }
    }

    #[test]
    fn renders_visible_geometry_and_overlay_deterministically() {
        let config = RuntimeConfig::default();
        let output = sample_output();
        let geometry = MapQueryResult {
            geometry: vec![runtime_core::api::GeometryCandidate::Polyline(
                MapPolylineCandidate {
                    layer: MapLayer::MajorRoad,
                    points: vec![WorldPoint::new(-40.0, 0.0), WorldPoint::new(40.0, 0.0)],
                },
            )],
        };
        let scene = RenderScene {
            config: &config,
            output: &output,
            geometry: &geometry,
        };
        let mut first = Framebuffer::new(128, 128);
        let mut second = Framebuffer::new(128, 128);

        render_frame(scene.clone(), &mut first);
        render_frame(scene, &mut second);

        assert_eq!(first.pixels(), second.pixels());
        assert!(
            first
                .pixels()
                .iter()
                .copied()
                .any(|value| value == RenderStyle::default().major_road.intensity)
        );
        assert!(first.pixels().iter().copied().any(|value| value > 240));
    }
}
