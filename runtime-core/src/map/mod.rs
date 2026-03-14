use crate::api::{
    CameraStateSnapshot, LodMask, MapLayer, MapQueryResult, MapQuerySpec, ViewportSize,
    WorldBounds, ZoomBucket,
};

pub trait MapSource {
    fn query(&self, spec: &MapQuerySpec) -> MapQueryResult;
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
    let zoom_bucket = zoom_bucket_for(camera.zoom);
    MapQuerySpec::new(
        camera.center_world,
        WorldBounds::from_center(camera.center_world, half_width_m, half_height_m),
        meters_per_pixel,
        camera.zoom,
        zoom_bucket,
        lod_mask_for(zoom_bucket),
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

pub fn zoom_bucket_for(zoom: f32) -> ZoomBucket {
    if zoom >= 15.5 {
        ZoomBucket::Detail
    } else if zoom >= 13.5 {
        ZoomBucket::Neighborhood
    } else {
        ZoomBucket::Overview
    }
}

pub fn lod_mask_for(bucket: ZoomBucket) -> LodMask {
    match bucket {
        ZoomBucket::Detail => LodMask::from_layers(&[
            MapLayer::MajorRoad,
            MapLayer::MinorRoad,
            MapLayer::Path,
            MapLayer::RiderOverlay,
        ]),
        ZoomBucket::Neighborhood => LodMask::from_layers(&[
            MapLayer::MajorRoad,
            MapLayer::MinorRoad,
            MapLayer::RiderOverlay,
        ]),
        ZoomBucket::Overview => {
            LodMask::from_layers(&[MapLayer::MajorRoad, MapLayer::RiderOverlay])
        }
    }
}
