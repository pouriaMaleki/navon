#[allow(unused_imports)]
use num_traits::Float as _;
use crate::api::{
    CameraStateSnapshot, LodMask, MapLayer, MapPresentationBand, MapQueryResult, MapQuerySpec,
    ViewportSize, WorldBounds,
};

pub trait MapSource {
    fn query(&mut self, spec: &MapQuerySpec) -> MapQueryResult;
}

pub fn build_query(camera: &CameraStateSnapshot, viewport_size: ViewportSize) -> MapQuerySpec {
    let meters_per_pixel = meters_per_pixel_for_zoom(camera.zoom);
    let local_half_width_m = f64::from(viewport_size.width_px) * meters_per_pixel / 2.0;
    let local_half_height_m = f64::from(viewport_size.height_px) * meters_per_pixel / 2.0;
    let (half_width_m, half_height_m) = rotated_query_half_extents(
        local_half_width_m,
        local_half_height_m,
        camera.orientation_rad,
    );
    let presentation_band = presentation_band_for(camera.zoom);
    MapQuerySpec::new(
        camera.center_world,
        WorldBounds::from_center(camera.center_world, half_width_m, half_height_m),
        meters_per_pixel,
        camera.zoom,
        presentation_band,
        lod_mask_for(presentation_band),
    )
}

fn rotated_query_half_extents(
    local_half_width_m: f64,
    local_half_height_m: f64,
    orientation_rad: f32,
) -> (f64, f64) {
    let sin_theta = f64::from(orientation_rad).sin().abs();
    let cos_theta = f64::from(orientation_rad).cos().abs();
    let world_half_width_m = (cos_theta * local_half_width_m) + (sin_theta * local_half_height_m);
    let world_half_height_m = (sin_theta * local_half_width_m) + (cos_theta * local_half_height_m);
    (world_half_width_m, world_half_height_m)
}

pub fn meters_per_pixel_for_zoom(zoom: f32) -> f64 {
    156_543.033_92 / 2.0_f64.powf(f64::from(zoom))
}

pub fn presentation_band_for(zoom: f32) -> MapPresentationBand {
    if zoom >= 16.5 {
        MapPresentationBand::CloseDetail
    } else if zoom >= 14.5 {
        MapPresentationBand::RideDetail
    } else if zoom >= 12.5 {
        MapPresentationBand::NetworkOverview
    } else {
        MapPresentationBand::DistrictOverview
    }
}

pub fn lod_mask_for(band: MapPresentationBand) -> LodMask {
    match band {
        MapPresentationBand::CloseDetail => LodMask::from_layers(&[
            MapLayer::ArterialRoad,
            MapLayer::StreetRoad,
            MapLayer::BikeRouteMain,
            MapLayer::BikeRouteLocal,
            MapLayer::Footpath,
            MapLayer::BuildingOutline,
            MapLayer::BikeParking,
            MapLayer::BikeRepair,
            MapLayer::Supermarket,
            MapLayer::Restaurant,
            MapLayer::Cafe,
            MapLayer::Water,
            MapLayer::Wc,
            MapLayer::RiderOverlay,
        ]),
        MapPresentationBand::RideDetail => LodMask::from_layers(&[
            MapLayer::ArterialRoad,
            MapLayer::StreetRoad,
            MapLayer::BikeRouteMain,
            MapLayer::BikeRouteLocal,
            MapLayer::BuildingOutline,
            MapLayer::RiderOverlay,
        ]),
        MapPresentationBand::NetworkOverview => LodMask::from_layers(&[
            MapLayer::ArterialRoad,
            MapLayer::BikeRouteMain,
            MapLayer::RiderOverlay,
        ]),
        MapPresentationBand::DistrictOverview => LodMask::from_layers(&[
            MapLayer::ArterialRoad,
            MapLayer::BikeRouteMain,
            MapLayer::RiderOverlay,
        ]),
    }
}
