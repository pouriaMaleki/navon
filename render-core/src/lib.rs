pub mod camera_view;
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
    let meters_per_pixel = scene.output.map_query.meters_per_pixel;
    let camera_view = CameraView::new(viewport, &scene.output.camera, meters_per_pixel);
    let style = RenderStyle::default();

    framebuffer.clear(style.background_color);

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
                    framebuffer.draw_line(clip_from, clip_to, stroke.color, stroke.thickness_px);
                }
            }
        }
    }

    draw_overlay(
        scene.config,
        &scene.output.camera,
        &scene.output.overlay,
        viewport,
        meters_per_pixel,
        framebuffer,
    );
}

#[cfg(test)]
mod tests {
    use runtime_core::api::{
        CameraMode, CameraOrientationMode, CameraStateSnapshot, LodMask, MapLayer,
        MapPolylineCandidate, MapQuerySpec, NormalizedScreenPoint, OverlayState, RuntimeConfig,
        RuntimeFrameOutput, WorldPoint, ZoomBucket,
    };

    use super::*;

    fn sample_output() -> RuntimeFrameOutput {
        RuntimeFrameOutput {
            frame_index: 1,
            camera: CameraStateSnapshot {
                mode: CameraMode::Riding,
                orientation_mode: CameraOrientationMode::TravelUpAuto,
                focus_world: WorldPoint::new(0.0, 0.0),
                center_world: WorldPoint::new(0.0, 0.0),
                zoom: 15.5,
                orientation_rad: 0.0,
                north_preview_progress: None,
                compass_ack_progress: 0.0,
                rider_anchor: NormalizedScreenPoint::CENTER,
                follow_locked: false,
                recenter_active: false,
            },
            overlay: OverlayState {
                north_indicator_visible: true,
                north_up_active: false,
                rider_heading_rad: Some(0.0),
                north_preview_progress: None,
                compass_ack_progress: 0.0,
            },
            map_query: MapQuerySpec::new(
                WorldPoint::ORIGIN,
                runtime_core::api::WorldBounds::from_center(WorldPoint::ORIGIN, 64.0, 64.0),
                1.0,
                15.5,
                ZoomBucket::Detail,
                LodMask::from_layers(&[MapLayer::MajorRoad]),
            ),
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
                .chunks_exact(4)
                .any(|rgba| {
                    rgba[0] == RenderStyle::default().major_road.color.r
                        && rgba[1] == RenderStyle::default().major_road.color.g
                        && rgba[2] == RenderStyle::default().major_road.color.b
                })
        );
        assert!(first.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == RenderStyle::default().rider_fill_color.r
                && rgba[1] == RenderStyle::default().rider_fill_color.g
                && rgba[2] == RenderStyle::default().rider_fill_color.b
        }));
    }

    #[test]
    fn render_uses_runtime_supplied_query_scale() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult {
            geometry: vec![runtime_core::api::GeometryCandidate::Polyline(
                MapPolylineCandidate {
                    layer: MapLayer::MajorRoad,
                    points: vec![WorldPoint::new(-40.0, 0.0), WorldPoint::new(40.0, 0.0)],
                },
            )],
        };
        let mut near_output = sample_output();
        near_output.map_query.meters_per_pixel = 1.0;
        let mut far_output = sample_output();
        far_output.map_query.meters_per_pixel = 100.0;

        let mut near = Framebuffer::new(128, 128);
        let mut far = Framebuffer::new(128, 128);

        render_frame(
            RenderScene {
                config: &config,
                output: &near_output,
                geometry: &geometry,
            },
            &mut near,
        );
        render_frame(
            RenderScene {
                config: &config,
                output: &far_output,
                geometry: &geometry,
            },
            &mut far,
        );

        assert_ne!(near.pixels(), far.pixels());
    }

    #[test]
    fn riding_marker_rotation_changes_overlay_pixels() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult::default();
        let mut north_output = sample_output();
        north_output.overlay.rider_heading_rad = Some(0.0);
        let mut east_output = sample_output();
        east_output.overlay.rider_heading_rad = Some(std::f32::consts::FRAC_PI_2);

        let mut north = Framebuffer::new(128, 128);
        let mut east = Framebuffer::new(128, 128);

        render_frame(
            RenderScene {
                config: &config,
                output: &north_output,
                geometry: &geometry,
            },
            &mut north,
        );
        render_frame(
            RenderScene {
                config: &config,
                output: &east_output,
                geometry: &geometry,
            },
            &mut east,
        );

        assert_ne!(north.pixels(), east.pixels());
    }

    #[test]
    fn north_indicator_rotation_changes_overlay_pixels() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult::default();
        let mut north_up = sample_output();
        north_up.camera.orientation_rad = 0.0;
        let mut east_up = sample_output();
        east_up.camera.orientation_rad = std::f32::consts::FRAC_PI_2;

        let mut north = Framebuffer::new(128, 128);
        let mut east = Framebuffer::new(128, 128);

        render_frame(
            RenderScene {
                config: &config,
                output: &north_up,
                geometry: &geometry,
            },
            &mut north,
        );
        render_frame(
            RenderScene {
                config: &config,
                output: &east_up,
                geometry: &geometry,
            },
            &mut east,
        );

        assert_ne!(north.pixels(), east.pixels());
    }

    #[test]
    fn compass_visual_states_render_differently() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult::default();
        let mut preview_output = sample_output();
        preview_output.camera.orientation_mode = CameraOrientationMode::NorthPreview;
        preview_output.overlay.north_up_active = true;
        preview_output.overlay.north_preview_progress = Some(0.6);
        let mut locked_output = sample_output();
        locked_output.camera.orientation_mode = CameraOrientationMode::NorthLocked;
        locked_output.overlay.north_up_active = true;
        let mut acquisition_output = sample_output();
        acquisition_output.camera.orientation_mode = CameraOrientationMode::HeadingAcquisition;
        acquisition_output.overlay.north_up_active = true;
        let mut ack_output = sample_output();
        ack_output.camera.orientation_mode = CameraOrientationMode::StoppedNorthUp;
        ack_output.overlay.north_up_active = true;
        ack_output.overlay.compass_ack_progress = 0.8;

        let mut preview = Framebuffer::new(128, 128);
        let mut locked = Framebuffer::new(128, 128);
        let mut acquisition = Framebuffer::new(128, 128);
        let mut ack = Framebuffer::new(128, 128);

        render_frame(
            RenderScene {
                config: &config,
                output: &preview_output,
                geometry: &geometry,
            },
            &mut preview,
        );
        render_frame(
            RenderScene {
                config: &config,
                output: &locked_output,
                geometry: &geometry,
            },
            &mut locked,
        );
        render_frame(
            RenderScene {
                config: &config,
                output: &acquisition_output,
                geometry: &geometry,
            },
            &mut acquisition,
        );
        render_frame(
            RenderScene {
                config: &config,
                output: &ack_output,
                geometry: &geometry,
            },
            &mut ack,
        );

        assert_ne!(preview.pixels(), locked.pixels());
        assert_ne!(preview.pixels(), acquisition.pixels());
        assert_ne!(ack.pixels(), acquisition.pixels());
    }
}
