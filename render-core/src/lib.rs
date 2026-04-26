#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[allow(unused_imports)]
use alloc::{vec, vec::Vec, string::String, boxed::Box, format};
#[allow(unused_imports)]
use num_traits::Float as _;

pub mod camera_view;
pub mod overlay;
pub mod raster;
pub mod style;
pub mod visibility;

use runtime_core::api::{MapQueryResult, RuntimeConfig, RuntimeFrameOutput, ViewportSize};

use camera_view::CameraView;
use overlay::draw_overlay;
use raster::{Framebuffer, Pixel};
use style::RenderStyle;
use visibility::clip_segment_to_viewport;

#[derive(Debug, Clone, PartialEq)]
pub struct RenderScene<'a> {
    pub config: &'a RuntimeConfig,
    pub output: &'a RuntimeFrameOutput,
    pub geometry: &'a MapQueryResult,
}

pub fn render_frame<P: Pixel>(scene: RenderScene<'_>, framebuffer: &mut Framebuffer<P>) {
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
                // Sub-pixel cull: at zoomed-out views many basemap
                // segments project to a single pixel. Skip them — the
                // alternative is a single `stamp_circle` whose 1-pixel
                // dot is barely visible against road density anyway,
                // and it costs us a per-segment `clip + stamp` walk.
                // Worst case: a high-zoom-out frame has 20k+ candidate
                // segments collapsing here, saving ~10 ms aggregate.
                if from_screen.x_px.round() == to_screen.x_px.round()
                    && from_screen.y_px.round() == to_screen.y_px.round()
                {
                    continue;
                }
                if let Some((clip_from, clip_to)) =
                    clip_segment_to_viewport(from_screen, to_screen, viewport)
                {
                    framebuffer.draw_line(clip_from, clip_to, stroke.color, stroke.thickness_px);
                }
            }
        }
    }

    render_route_overlay(
        &camera_view,
        viewport,
        &style,
        &scene.output.route,
        framebuffer,
    );

    render_points(&camera_view, viewport, &style, scene.geometry, framebuffer);

    draw_overlay(
        scene.config,
        &scene.output.camera,
        &scene.output.overlay,
        &scene.output.route,
        viewport,
        meters_per_pixel,
        framebuffer,
    );
}

fn render_route_overlay<P: Pixel>(
    camera_view: &CameraView,
    viewport: ViewportSize,
    style: &RenderStyle,
    route: &runtime_core::route::RouteRenderState,
    framebuffer: &mut Framebuffer<P>,
) {
    draw_route_polyline(
        camera_view,
        viewport,
        &route.completed_geometry_world,
        style.completed_route_backdrop,
        style.completed_route_line,
        framebuffer,
    );
    draw_route_polyline(
        camera_view,
        viewport,
        &route.remaining_geometry_world,
        style.remaining_route_backdrop,
        style.remaining_route_line,
        framebuffer,
    );
}

fn draw_route_polyline<P: Pixel>(
    camera_view: &CameraView,
    viewport: ViewportSize,
    route_geometry_world: &[runtime_core::api::WorldPoint],
    backdrop: crate::style::StrokeStyle,
    line: crate::style::StrokeStyle,
    framebuffer: &mut Framebuffer<P>,
) {
    for segment in route_geometry_world.windows(2) {
        let [from, to] = segment else {
            continue;
        };
        let from_screen = camera_view.world_to_screen(*from);
        let to_screen = camera_view.world_to_screen(*to);
        if let Some((clip_from, clip_to)) =
            clip_segment_to_viewport(from_screen, to_screen, viewport)
        {
            framebuffer.draw_line(clip_from, clip_to, backdrop.color, backdrop.thickness_px);
            framebuffer.draw_line(clip_from, clip_to, line.color, line.thickness_px);
        }
    }
}

fn render_points<P: Pixel>(
    camera_view: &CameraView,
    viewport: ViewportSize,
    style: &RenderStyle,
    geometry: &MapQueryResult,
    framebuffer: &mut Framebuffer<P>,
) {
    let mut points = geometry
        .geometry
        .iter()
        .filter_map(|candidate| match candidate {
            runtime_core::api::GeometryCandidate::Point(point) => {
                let point_style = style.point_for_layer(point.layer)?;
                let screen = camera_view.world_to_screen(point.position);
                if !point_within_viewport(screen, viewport, point_style.badge_radius_px) {
                    return None;
                }
                Some((point.layer, point_style, screen))
            }
            runtime_core::api::GeometryCandidate::Polyline(_) => None,
        })
        .collect::<Vec<_>>();

    points.sort_by_key(|(layer, _, _)| point_priority(*layer));

    let mut accepted = Vec::new();
    for (layer, point_style, screen) in points {
        if accepted.iter().any(
            |(existing, existing_layer, spacing): &(
                runtime_core::api::ScreenPoint,
                runtime_core::api::MapLayer,
                f32,
            )| {
                let dx = existing.x_px - screen.x_px;
                let dy = existing.y_px - screen.y_px;
                let min_spacing = if *existing_layer == layer {
                    spacing.max(f32::from(point_style.min_spacing_px))
                } else {
                    8.0
                };
                (dx * dx) + (dy * dy) < min_spacing * min_spacing
            },
        ) {
            continue;
        }

        draw_poi_marker(framebuffer, screen, layer, point_style);

        accepted.push((screen, layer, f32::from(point_style.min_spacing_px)));
    }
}

fn draw_poi_marker<P: Pixel>(
    framebuffer: &mut Framebuffer<P>,
    screen: runtime_core::api::ScreenPoint,
    layer: runtime_core::api::MapLayer,
    point_style: crate::style::PointStyle,
) {
    framebuffer.stamp_circle(
        screen.x_px.round() as i32,
        screen.y_px.round() as i32,
        point_style.badge_radius_px,
        point_style.badge_color,
    );
    framebuffer.draw_mask(screen, poi_asset_for_layer(layer), point_style.icon_color);
}

fn poi_asset_for_layer(layer: runtime_core::api::MapLayer) -> crate::raster::AlphaMask {
    match layer {
        runtime_core::api::MapLayer::BikeParking => crate::overlay::assets::POI_BIKE_PARKING,
        runtime_core::api::MapLayer::BikeRepair => crate::overlay::assets::POI_BIKE_REPAIR,
        runtime_core::api::MapLayer::Supermarket => crate::overlay::assets::POI_SUPERMARKET,
        runtime_core::api::MapLayer::Restaurant => crate::overlay::assets::POI_RESTAURANT,
        runtime_core::api::MapLayer::Cafe => crate::overlay::assets::POI_CAFE,
        runtime_core::api::MapLayer::Water => crate::overlay::assets::POI_WATER,
        runtime_core::api::MapLayer::Wc => crate::overlay::assets::POI_WC,
        _ => crate::overlay::assets::POI_BIKE_PARKING,
    }
}

fn point_within_viewport(
    screen: runtime_core::api::ScreenPoint,
    viewport: ViewportSize,
    radius_px: u8,
) -> bool {
    let margin = f32::from(radius_px) + 1.0;
    screen.x_px >= -margin
        && screen.y_px >= -margin
        && screen.x_px <= viewport.width_px as f32 + margin
        && screen.y_px <= viewport.height_px as f32 + margin
}

fn point_priority(layer: runtime_core::api::MapLayer) -> u8 {
    match layer {
        runtime_core::api::MapLayer::BikeRepair => 0,
        runtime_core::api::MapLayer::BikeParking => 1,
        runtime_core::api::MapLayer::Water => 2,
        runtime_core::api::MapLayer::Wc => 3,
        runtime_core::api::MapLayer::Supermarket => 4,
        runtime_core::api::MapLayer::Cafe => 5,
        runtime_core::api::MapLayer::Restaurant => 6,
        _ => 7,
    }
}

#[cfg(test)]
mod tests {
    use runtime_core::api::{
        CameraMode, CameraOrientationMode, CameraStateSnapshot, LodMask, MapLayer,
        MapPointCandidate, MapPolylineCandidate, MapPresentationBand, MapQuerySpec,
        NormalizedScreenPoint, OverlayState, RuntimeConfig, RuntimeFrameOutput, SpeedUnit,
        WorldPoint,
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
                speed_panel_visible: false,
                speed_display_value: 0,
                speed_unit: SpeedUnit::Kph,
                map_tiles_loading: false,
            },
            map_query: MapQuerySpec::new(
                WorldPoint::ORIGIN,
                runtime_core::api::WorldBounds::from_center(WorldPoint::ORIGIN, 64.0, 64.0),
                1.0,
                15.5,
                MapPresentationBand::CloseDetail,
                LodMask::from_layers(&[MapLayer::ArterialRoad]),
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
                    layer: MapLayer::ArterialRoad,
                    points: smallvec::smallvec![
                        WorldPoint::new(-40.0, 0.0),
                        WorldPoint::new(40.0, 0.0)
                    ],
                },
            )],
        };
        let scene = RenderScene {
            config: &config,
            output: &output,
            geometry: &geometry,
        };
        let mut first : Framebuffer = Framebuffer::new(128, 128);
        let mut second : Framebuffer = Framebuffer::new(128, 128);

        render_frame(scene.clone(), &mut first);
        render_frame(scene, &mut second);

        assert_eq!(first.pixels(), second.pixels());
        assert!(first.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == RenderStyle::default().arterial_road.color.r
                && rgba[1] == RenderStyle::default().arterial_road.color.g
                && rgba[2] == RenderStyle::default().arterial_road.color.b
        }));
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
                    layer: MapLayer::ArterialRoad,
                    points: smallvec::smallvec![
                        WorldPoint::new(-40.0, 0.0),
                        WorldPoint::new(40.0, 0.0)
                    ],
                },
            )],
        };
        let mut near_output = sample_output();
        near_output.map_query.meters_per_pixel = 1.0;
        let mut far_output = sample_output();
        far_output.map_query.meters_per_pixel = 100.0;

        let mut near : Framebuffer = Framebuffer::new(128, 128);
        let mut far : Framebuffer = Framebuffer::new(128, 128);

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
        east_output.overlay.rider_heading_rad = Some(core::f32::consts::FRAC_PI_2);

        let mut north : Framebuffer = Framebuffer::new(128, 128);
        let mut east : Framebuffer = Framebuffer::new(128, 128);

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
        east_up.camera.orientation_rad = core::f32::consts::FRAC_PI_2;

        let mut north : Framebuffer = Framebuffer::new(128, 128);
        let mut east : Framebuffer = Framebuffer::new(128, 128);

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

        let mut preview : Framebuffer = Framebuffer::new(128, 128);
        let mut locked : Framebuffer = Framebuffer::new(128, 128);
        let mut acquisition : Framebuffer = Framebuffer::new(128, 128);
        let mut ack : Framebuffer = Framebuffer::new(128, 128);

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

    #[test]
    fn speed_panel_renders_black_bottom_band_when_visible() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult::default();
        let mut output = sample_output();
        output.overlay.speed_panel_visible = true;
        output.overlay.speed_display_value = 22;
        output.overlay.speed_unit = SpeedUnit::Kph;
        let mut framebuffer : Framebuffer = Framebuffer::new(128, 128);

        render_frame(
            RenderScene {
                config: &config,
                output: &output,
                geometry: &geometry,
            },
            &mut framebuffer,
        );

        let panel_start = (framebuffer.height() as f32 * 0.75).round() as usize;
        let has_black_panel_pixel =
            framebuffer
                .pixels()
                .chunks_exact(4)
                .enumerate()
                .any(|(index, rgba)| {
                    let y = index / framebuffer.width() as usize;
                    y >= panel_start && rgba[0] == 0 && rgba[1] == 0 && rgba[2] == 0
                });

        assert!(has_black_panel_pixel);
    }

    #[test]
    fn speed_panel_unit_render_changes_pixels() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult::default();
        let mut kph_output = sample_output();
        kph_output.overlay.speed_panel_visible = true;
        kph_output.overlay.speed_display_value = 18;
        kph_output.overlay.speed_unit = SpeedUnit::Kph;
        let mut mph_output = kph_output.clone();
        mph_output.overlay.speed_unit = SpeedUnit::Mph;

        let mut kph : Framebuffer = Framebuffer::new(128, 128);
        let mut mph : Framebuffer = Framebuffer::new(128, 128);

        render_frame(
            RenderScene {
                config: &config,
                output: &kph_output,
                geometry: &geometry,
            },
            &mut kph,
        );
        render_frame(
            RenderScene {
                config: &config,
                output: &mph_output,
                geometry: &geometry,
            },
            &mut mph,
        );

        assert_ne!(kph.pixels(), mph.pixels());
    }

    #[test]
    fn poi_points_render_distinct_marker_pixels() {
        let config = RuntimeConfig::default();
        let output = sample_output();
        let empty_geometry = MapQueryResult::default();
        let geometry = MapQueryResult {
            geometry: vec![
                runtime_core::api::GeometryCandidate::Point(MapPointCandidate {
                    layer: MapLayer::BikeParking,
                    position: WorldPoint::new(0.0, 0.0),
                }),
                runtime_core::api::GeometryCandidate::Point(MapPointCandidate {
                    layer: MapLayer::BikeRepair,
                    position: WorldPoint::new(16.0, 12.0),
                }),
                runtime_core::api::GeometryCandidate::Point(MapPointCandidate {
                    layer: MapLayer::Restaurant,
                    position: WorldPoint::new(-16.0, -12.0),
                }),
            ],
        };
        let mut empty : Framebuffer = Framebuffer::new(128, 128);
        let mut framebuffer : Framebuffer = Framebuffer::new(128, 128);

        render_frame(
            RenderScene {
                config: &config,
                output: &output,
                geometry: &empty_geometry,
            },
            &mut empty,
        );
        render_frame(
            RenderScene {
                config: &config,
                output: &output,
                geometry: &geometry,
            },
            &mut framebuffer,
        );

        assert_ne!(empty.pixels(), framebuffer.pixels());
    }

    #[test]
    fn major_turn_alert_renders_banner_pixels() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult::default();
        let mut output = sample_output();
        output.route.upcoming_turn_alert = Some(runtime_core::route::UpcomingTurnAlert {
            kind: runtime_core::route::TurnAlertKind::Left,
            distance_remaining_m: 42.0,
            instruction_text: Some("Turn left".to_owned()),
        });

        let mut framebuffer : Framebuffer = Framebuffer::new(160, 160);
        render_frame(
            RenderScene {
                config: &config,
                output: &output,
                geometry: &geometry,
            },
            &mut framebuffer,
        );

        let style = RenderStyle::default();
        assert!(framebuffer.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == style.major_turn_banner_background_color.r
                && rgba[1] == style.major_turn_banner_background_color.g
                && rgba[2] == style.major_turn_banner_background_color.b
        }));
    }

    #[test]
    fn essential_alert_verbosity_hides_major_turn_banner() {
        let standard_config = RuntimeConfig::default();
        let mut essential_config = RuntimeConfig::default();
        essential_config.route_alert_verbosity = runtime_core::api::RouteAlertVerbosity::Essential;
        let geometry = MapQueryResult::default();
        let mut output = sample_output();
        output.route.upcoming_turn_alert = Some(runtime_core::route::UpcomingTurnAlert {
            kind: runtime_core::route::TurnAlertKind::Left,
            distance_remaining_m: 42.0,
            instruction_text: Some("Turn left".to_owned()),
        });

        let mut standard : Framebuffer = Framebuffer::new(160, 160);
        let mut essential : Framebuffer = Framebuffer::new(160, 160);
        render_frame(
            RenderScene {
                config: &standard_config,
                output: &output,
                geometry: &geometry,
            },
            &mut standard,
        );
        render_frame(
            RenderScene {
                config: &essential_config,
                output: &output,
                geometry: &geometry,
            },
            &mut essential,
        );

        assert_ne!(standard.pixels(), essential.pixels());
    }

    #[test]
    fn detailed_alert_verbosity_changes_major_turn_banner_layout() {
        let standard_config = RuntimeConfig::default();
        let mut detailed_config = RuntimeConfig::default();
        detailed_config.route_alert_verbosity = runtime_core::api::RouteAlertVerbosity::Detailed;
        let geometry = MapQueryResult::default();
        let mut output = sample_output();
        output.route.upcoming_turn_alert = Some(runtime_core::route::UpcomingTurnAlert {
            kind: runtime_core::route::TurnAlertKind::Right,
            distance_remaining_m: 42.0,
            instruction_text: Some("Turn right".to_owned()),
        });

        let mut standard : Framebuffer = Framebuffer::new(160, 160);
        let mut detailed : Framebuffer = Framebuffer::new(160, 160);
        render_frame(
            RenderScene {
                config: &standard_config,
                output: &output,
                geometry: &geometry,
            },
            &mut standard,
        );
        render_frame(
            RenderScene {
                config: &detailed_config,
                output: &output,
                geometry: &geometry,
            },
            &mut detailed,
        );

        assert_ne!(standard.pixels(), detailed.pixels());
    }

    #[test]
    fn reroute_request_renders_banner_pixels() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult::default();
        let mut output = sample_output();
        output.route.reroute_requested = true;
        output.route.off_route = true;

        let mut framebuffer : Framebuffer = Framebuffer::new(160, 160);
        render_frame(
            RenderScene {
                config: &config,
                output: &output,
                geometry: &geometry,
            },
            &mut framebuffer,
        );

        let style = RenderStyle::default();
        assert!(framebuffer.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == style.reroute_banner_background_color.r
                && rgba[1] == style.reroute_banner_background_color.g
                && rgba[2] == style.reroute_banner_background_color.b
        }));
    }

    #[test]
    fn off_route_alert_renders_warning_banner_pixels() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult::default();
        let mut output = sample_output();
        output.route.off_route = true;

        let mut framebuffer : Framebuffer = Framebuffer::new(160, 160);
        render_frame(
            RenderScene {
                config: &config,
                output: &output,
                geometry: &geometry,
            },
            &mut framebuffer,
        );

        let style = RenderStyle::default();
        assert!(framebuffer.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == style.off_route_banner_background_color.r
                && rgba[1] == style.off_route_banner_background_color.g
                && rgba[2] == style.off_route_banner_background_color.b
        }));
        assert!(framebuffer.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == style.off_route_banner_text_color.r
                && rgba[1] == style.off_route_banner_text_color.g
                && rgba[2] == style.off_route_banner_text_color.b
        }));
    }

    #[test]
    fn detailed_alert_banner_stays_visible_on_small_viewport() {
        let mut config = RuntimeConfig::default();
        config.route_alert_verbosity = runtime_core::api::RouteAlertVerbosity::Detailed;
        let geometry = MapQueryResult::default();
        let mut output = sample_output();
        output.camera.orientation_mode = runtime_core::api::CameraOrientationMode::NorthLocked;
        output.camera.orientation_rad = 1.2;
        output.route.upcoming_turn_alert = Some(runtime_core::route::UpcomingTurnAlert {
            kind: runtime_core::route::TurnAlertKind::Left,
            distance_remaining_m: 42.0,
            instruction_text: Some("Turn left".to_owned()),
        });

        let mut framebuffer : Framebuffer = Framebuffer::new(96, 96);
        render_frame(
            RenderScene {
                config: &config,
                output: &output,
                geometry: &geometry,
            },
            &mut framebuffer,
        );

        let style = RenderStyle::default();
        assert!(framebuffer.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == style.major_turn_banner_background_color.r
                && rgba[1] == style.major_turn_banner_background_color.g
                && rgba[2] == style.major_turn_banner_background_color.b
        }));
    }

    #[test]
    fn active_route_overlay_stays_visible_when_camera_rotates() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult::default();
        let mut output = sample_output();
        output.camera.orientation_mode = runtime_core::api::CameraOrientationMode::TravelUpAuto;
        output.camera.orientation_rad = 1.1;
        output.route.route_id = Some("demo-route".to_owned());
        output.route.revision = Some(1);
        output.route.geometry_world = vec![
            WorldPoint::new(-30.0, -20.0),
            WorldPoint::new(-5.0, 10.0),
            WorldPoint::new(25.0, 22.0),
        ];
        output.route.completed_geometry_world =
            vec![WorldPoint::new(-30.0, -20.0), WorldPoint::new(-5.0, 10.0)];
        output.route.remaining_geometry_world =
            vec![WorldPoint::new(-5.0, 10.0), WorldPoint::new(25.0, 22.0)];

        let mut framebuffer : Framebuffer = Framebuffer::new(128, 128);
        render_frame(
            RenderScene {
                config: &config,
                output: &output,
                geometry: &geometry,
            },
            &mut framebuffer,
        );

        let style = RenderStyle::default();
        assert!(framebuffer.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == style.completed_route_line.color.r
                && rgba[1] == style.completed_route_line.color.g
                && rgba[2] == style.completed_route_line.color.b
        }));
        assert!(framebuffer.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == style.remaining_route_line.color.r
                && rgba[1] == style.remaining_route_line.color.g
                && rgba[2] == style.remaining_route_line.color.b
        }));
    }

    #[test]
    fn active_route_overlay_renders_completed_and_remaining_pixels() {
        let config = RuntimeConfig::default();
        let geometry = MapQueryResult::default();
        let mut output = sample_output();
        output.route.route_id = Some("demo-route".to_owned());
        output.route.revision = Some(1);
        output.route.geometry_world = vec![
            WorldPoint::new(-30.0, -20.0),
            WorldPoint::new(-5.0, 10.0),
            WorldPoint::new(25.0, 22.0),
        ];
        output.route.completed_geometry_world =
            vec![WorldPoint::new(-30.0, -20.0), WorldPoint::new(-5.0, 10.0)];
        output.route.remaining_geometry_world =
            vec![WorldPoint::new(-5.0, 10.0), WorldPoint::new(25.0, 22.0)];

        let mut framebuffer : Framebuffer = Framebuffer::new(128, 128);
        render_frame(
            RenderScene {
                config: &config,
                output: &output,
                geometry: &geometry,
            },
            &mut framebuffer,
        );

        let style = RenderStyle::default();
        assert!(framebuffer.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == style.completed_route_line.color.r
                && rgba[1] == style.completed_route_line.color.g
                && rgba[2] == style.completed_route_line.color.b
        }));
        assert!(framebuffer.pixels().chunks_exact(4).any(|rgba| {
            rgba[0] == style.remaining_route_line.color.r
                && rgba[1] == style.remaining_route_line.color.g
                && rgba[2] == style.remaining_route_line.color.b
        }));
    }
}
