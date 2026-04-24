#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[allow(unused_imports)]
use alloc::{vec, vec::Vec, string::{String, ToString}, boxed::Box, format};
#[allow(unused_imports)]
use num_traits::Float as _;
use hashbrown::HashSet;

use runtime_core::api::{
    GeometryCandidate, MapLayer, MapPointCandidate, MapPolylineCandidate, MapQueryResult,
    MapQuerySpec, WorldBounds, WorldPoint,
};
use runtime_core::map::MapSource;

const SVM_MAGIC: &[u8; 4] = b"SVM1";
const SVM_VERSION: u16 = 1;
const SVM_HEADER_LEN: usize = 30;
const SVM_SEGMENT_RECORD_LEN: usize = 24;
const TILE_EXTENT: f64 = 4096.0;
const EARTH_RADIUS_M: f64 = 6_378_137.0;
const EARTH_CIRCUMFERENCE_M: f64 = core::f64::consts::TAU * EARTH_RADIUS_M;
const HALF_EARTH_CIRCUMFERENCE_M: f64 = EARTH_CIRCUMFERENCE_M / 2.0;
const GRID_CELL_WORLD_UNITS: i32 = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SegmentRecord {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    road_class: u8,
    geometry_kind: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceBounds {
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
}

impl SourceBounds {
    fn from_segments(segments: &[SegmentRecord]) -> Self {
        let mut bounds = Self {
            min_x: i32::MAX,
            max_x: i32::MIN,
            min_y: i32::MAX,
            max_y: i32::MIN,
        };
        for segment in segments {
            bounds.min_x = bounds.min_x.min(segment.x1).min(segment.x2);
            bounds.max_x = bounds.max_x.max(segment.x1).max(segment.x2);
            bounds.min_y = bounds.min_y.min(segment.y1).min(segment.y2);
            bounds.max_y = bounds.max_y.max(segment.y1).max(segment.y2);
        }
        bounds
    }
}

#[derive(Debug, Clone)]
struct SpatialGrid {
    bounds: SourceBounds,
    cell_size: i32,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<u32>>,
}

impl SpatialGrid {
    fn new(bounds: SourceBounds, segments: &[SegmentRecord]) -> Self {
        let cell_size = GRID_CELL_WORLD_UNITS;
        let cols = ((bounds.max_x - bounds.min_x) / cell_size + 1).max(1) as usize;
        let rows = ((bounds.max_y - bounds.min_y) / cell_size + 1).max(1) as usize;
        let mut cells = vec![Vec::new(); cols * rows];
        for (index, segment) in segments.iter().copied().enumerate() {
            let segment_bounds = segment_bounds(segment);
            let min_col = ((segment_bounds.min_x - bounds.min_x) / cell_size).max(0) as usize;
            let max_col = ((segment_bounds.max_x - bounds.min_x) / cell_size).max(0) as usize;
            let min_row = ((segment_bounds.min_y - bounds.min_y) / cell_size).max(0) as usize;
            let max_row = ((segment_bounds.max_y - bounds.min_y) / cell_size).max(0) as usize;

            for row in min_row.min(rows - 1)..=max_row.min(rows - 1) {
                for col in min_col.min(cols - 1)..=max_col.min(cols - 1) {
                    cells[(row * cols) + col].push(index as u32);
                }
            }
        }

        Self {
            bounds,
            cell_size,
            cols,
            rows,
            cells,
        }
    }

    fn candidate_indices(&self, query_bounds: SourceBounds) -> Vec<u32> {
        if query_bounds.max_x < self.bounds.min_x
            || query_bounds.min_x > self.bounds.max_x
            || query_bounds.max_y < self.bounds.min_y
            || query_bounds.min_y > self.bounds.max_y
        {
            return Vec::new();
        }

        let min_col = ((query_bounds.min_x - self.bounds.min_x) / self.cell_size)
            .clamp(0, self.cols as i32 - 1) as usize;
        let max_col = ((query_bounds.max_x - self.bounds.min_x) / self.cell_size)
            .clamp(0, self.cols as i32 - 1) as usize;
        let min_row = ((query_bounds.min_y - self.bounds.min_y) / self.cell_size)
            .clamp(0, self.rows as i32 - 1) as usize;
        let max_row = ((query_bounds.max_y - self.bounds.min_y) / self.cell_size)
            .clamp(0, self.rows as i32 - 1) as usize;

        let mut seen = HashSet::new();
        let mut indices = Vec::new();
        for row in min_row..=max_row {
            for col in min_col..=max_col {
                for index in &self.cells[(row * self.cols) + col] {
                    if seen.insert(*index) {
                        indices.push(*index);
                    }
                }
            }
        }
        indices
    }
}

#[derive(Debug, Clone)]
pub struct EmbeddedMapSource {
    meters_per_world_unit: f64,
    segments: Vec<SegmentRecord>,
    grid: SpatialGrid,
}

impl Default for EmbeddedMapSource {
    fn default() -> Self {
        Self::from_svm_bytes(include_bytes!("../../map-data/city.svm"))
            .expect("embedded city.svm should be valid")
    }
}

impl EmbeddedMapSource {
    pub fn from_svm_bytes(bytes: &[u8]) -> Result<Self, String> {
        validate_svm_header(bytes)?;
        let source_zoom = read_i32(bytes, 8)?;
        let header_bounds = SourceBounds {
            min_x: read_i32(bytes, 12)?,
            max_x: read_i32(bytes, 16)?,
            min_y: read_i32(bytes, 20)?,
            max_y: read_i32(bytes, 24)?,
        };
        let source_name_len = read_u16(bytes, 28)? as usize;
        let segment_count_offset = SVM_HEADER_LEN
            .checked_add(source_name_len)
            .ok_or("svm header offset overflow")?;
        if bytes.len() < segment_count_offset + 4 {
            return Err(format!(
                "svm source name payload is truncated: expected {} bytes before segment table, got {}",
                segment_count_offset + 4,
                bytes.len()
            ));
        }
        let segment_count = read_u32(bytes, segment_count_offset)? as usize;
        let mut offset = segment_count_offset + 4;
        let segment_table_len = segment_count
            .checked_mul(SVM_SEGMENT_RECORD_LEN)
            .ok_or("svm segment table length overflow")?;
        let segment_table_end = offset
            .checked_add(segment_table_len)
            .ok_or("svm segment table offset overflow")?;
        if bytes.len() < segment_table_end {
            return Err(format!(
                "svm segment payload is truncated: expected {} bytes, got {}",
                segment_table_end,
                bytes.len()
            ));
        }
        let mut segments = Vec::with_capacity(segment_count);
        for _ in 0..segment_count {
            segments.push(SegmentRecord {
                x1: read_i32(bytes, offset)?,
                y1: read_i32(bytes, offset + 4)?,
                x2: read_i32(bytes, offset + 8)?,
                y2: read_i32(bytes, offset + 12)?,
                road_class: *bytes
                    .get(offset + 16)
                    .ok_or("unexpected eof while reading road_class")?,
                geometry_kind: *bytes
                    .get(offset + 17)
                    .ok_or("unexpected eof while reading geometry_kind")?,
            });
            offset += 24;
        }
        let bounds = if segments.is_empty() {
            header_bounds
        } else {
            SourceBounds::from_segments(&segments)
        };
        let meters_per_world_unit =
            EARTH_CIRCUMFERENCE_M / (TILE_EXTENT * 2.0_f64.powi(source_zoom));
        let grid = SpatialGrid::new(bounds, &segments);

        Ok(Self {
            meters_per_world_unit,
            segments,
            grid,
        })
    }

    #[cfg(test)]
    fn from_segments(source_zoom: i32, segments: Vec<SegmentRecord>) -> Self {
        let bounds = SourceBounds::from_segments(&segments);
        let meters_per_world_unit =
            EARTH_CIRCUMFERENCE_M / (TILE_EXTENT * 2.0_f64.powi(source_zoom));
        let grid = SpatialGrid::new(bounds, &segments);
        Self {
            meters_per_world_unit,
            segments,
            grid,
        }
    }

    fn query_bounds_in_source_units(&self, bounds: WorldBounds) -> SourceBounds {
        SourceBounds {
            min_x: meters_to_source_x(bounds.min.x_m, self.meters_per_world_unit),
            max_x: meters_to_source_x(bounds.max.x_m, self.meters_per_world_unit),
            min_y: meters_to_source_y(bounds.min.y_m, self.meters_per_world_unit),
            max_y: meters_to_source_y(bounds.max.y_m, self.meters_per_world_unit),
        }
    }
}

fn validate_svm_header(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < SVM_HEADER_LEN {
        return Err(format!(
            "svm header is truncated: expected at least {SVM_HEADER_LEN} bytes, got {}",
            bytes.len()
        ));
    }
    let magic = bytes.get(0..4).ok_or("svm magic is truncated")?;
    if magic != SVM_MAGIC.as_slice() {
        return Err(format!(
            "svm magic mismatch: expected {:?}, got {:?}",
            core::str::from_utf8(SVM_MAGIC).unwrap_or("SVM1"),
            String::from_utf8_lossy(magic)
        ));
    }
    let version = read_u16(bytes, 4)?;
    if version != SVM_VERSION {
        return Err(format!(
            "unsupported svm version {version}; expected {SVM_VERSION}"
        ));
    }
    Ok(())
}

impl MapSource for EmbeddedMapSource {
    fn query(&self, spec: &MapQuerySpec) -> MapQueryResult {
        let query_bounds = self.query_bounds_in_source_units(spec.bounds);
        let mut geometry = Vec::new();
        for index in self.grid.candidate_indices(query_bounds) {
            let segment = self.segments[index as usize];
            if !segment_intersects_bounds(segment, query_bounds) {
                continue;
            }
            let layer = map_layer_for_feature_class(segment.road_class);
            if !spec.lod_mask.contains(layer) {
                continue;
            }
            let start = WorldPoint::new(
                source_x_to_meters(segment.x1, self.meters_per_world_unit),
                source_y_to_meters(segment.y1, self.meters_per_world_unit),
            );
            let end = WorldPoint::new(
                source_x_to_meters(segment.x2, self.meters_per_world_unit),
                source_y_to_meters(segment.y2, self.meters_per_world_unit),
            );

            if segment.geometry_kind == 1 || layer.is_point() {
                geometry.push(GeometryCandidate::Point(MapPointCandidate {
                    layer,
                    position: start,
                }));
                continue;
            }

            geometry.push(GeometryCandidate::Polyline(MapPolylineCandidate {
                layer,
                points: vec![start, end],
            }));
        }
        MapQueryResult { geometry }
    }
}

fn segment_bounds(segment: SegmentRecord) -> SourceBounds {
    SourceBounds {
        min_x: segment.x1.min(segment.x2),
        max_x: segment.x1.max(segment.x2),
        min_y: segment.y1.min(segment.y2),
        max_y: segment.y1.max(segment.y2),
    }
}

fn segment_intersects_bounds(segment: SegmentRecord, bounds: SourceBounds) -> bool {
    let segment_bounds = segment_bounds(segment);
    !(segment_bounds.max_x < bounds.min_x
        || segment_bounds.min_x > bounds.max_x
        || segment_bounds.max_y < bounds.min_y
        || segment_bounds.min_y > bounds.max_y)
}

fn map_layer_for_feature_class(feature_class: u8) -> MapLayer {
    match feature_class {
        1 => MapLayer::ArterialRoad,
        2 => MapLayer::StreetRoad,
        3 => MapLayer::BikeRouteMain,
        4 => MapLayer::BikeRouteLocal,
        5 => MapLayer::Footpath,
        6 => MapLayer::BuildingOutline,
        7 => MapLayer::BikeParking,
        8 => MapLayer::BikeRepair,
        9 => MapLayer::Supermarket,
        10 => MapLayer::Restaurant,
        11 => MapLayer::Cafe,
        12 => MapLayer::Water,
        13 => MapLayer::Wc,
        _ => MapLayer::Footpath,
    }
}

fn source_x_to_meters(source_x: i32, meters_per_world_unit: f64) -> f64 {
    f64::from(source_x) * meters_per_world_unit - HALF_EARTH_CIRCUMFERENCE_M
}

fn source_y_to_meters(source_y: i32, meters_per_world_unit: f64) -> f64 {
    HALF_EARTH_CIRCUMFERENCE_M + f64::from(source_y) * meters_per_world_unit
}

fn meters_to_source_x(x_m: f64, meters_per_world_unit: f64) -> i32 {
    ((x_m + HALF_EARTH_CIRCUMFERENCE_M) / meters_per_world_unit).floor() as i32
}

fn meters_to_source_y(y_m: f64, meters_per_world_unit: f64) -> i32 {
    ((y_m - HALF_EARTH_CIRCUMFERENCE_M) / meters_per_world_unit).floor() as i32
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let end = offset + 2;
    bytes
        .get(offset..end)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| format!("unexpected eof at offset {offset}"))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let end = offset + 4;
    bytes
        .get(offset..end)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| format!("unexpected eof at offset {offset}"))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, String> {
    let end = offset + 4;
    bytes
        .get(offset..end)
        .and_then(|slice| slice.try_into().ok())
        .map(i32::from_le_bytes)
        .ok_or_else(|| format!("unexpected eof at offset {offset}"))
}

#[cfg(test)]
mod tests {
    use runtime_core::api::{
        GpsSample, LodMask, MapLayer, MapPointCandidate, MapPresentationBand, MapQuerySpec,
    };
    use runtime_core::motion::project_gps_to_world;

    use super::*;

    fn encoded_map_with_payload(
        name_len: u16,
        name_payload: &[u8],
        segment_count: u32,
        segment_payload: &[u8],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(SVM_MAGIC);
        bytes.extend_from_slice(&SVM_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&16_i32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&name_len.to_le_bytes());
        bytes.extend_from_slice(name_payload);
        bytes.extend_from_slice(&segment_count.to_le_bytes());
        bytes.extend_from_slice(segment_payload);
        bytes
    }

    fn spec(bounds: WorldBounds) -> MapQuerySpec {
        MapQuerySpec::new(
            WorldPoint::ORIGIN,
            bounds,
            1.0,
            15.5,
            MapPresentationBand::CloseDetail,
            LodMask::from_layers(&[
                MapLayer::ArterialRoad,
                MapLayer::StreetRoad,
                MapLayer::BikeRouteMain,
                MapLayer::BikeRouteLocal,
                MapLayer::Footpath,
                MapLayer::BikeParking,
                MapLayer::BikeRepair,
                MapLayer::Supermarket,
                MapLayer::Restaurant,
                MapLayer::Cafe,
                MapLayer::Water,
                MapLayer::Wc,
            ]),
        )
    }

    #[test]
    fn query_returns_segments_inside_bounds_and_excludes_outside() {
        let source = EmbeddedMapSource::from_segments(
            14,
            vec![
                SegmentRecord {
                    x1: 1_000,
                    y1: -1_000,
                    x2: 2_000,
                    y2: -1_000,
                    road_class: 1,
                    geometry_kind: 0,
                },
                SegmentRecord {
                    x1: 50_000,
                    y1: -50_000,
                    x2: 52_000,
                    y2: -50_000,
                    road_class: 3,
                    geometry_kind: 0,
                },
            ],
        );
        let meters_per_unit = source.meters_per_world_unit;
        let bounds = WorldBounds {
            min: WorldPoint::new(
                source_x_to_meters(500, meters_per_unit),
                source_y_to_meters(-2_500, meters_per_unit),
            ),
            max: WorldPoint::new(
                source_x_to_meters(2_500, meters_per_unit),
                source_y_to_meters(-500, meters_per_unit),
            ),
        };

        let result = source.query(&spec(bounds));

        assert_eq!(result.geometry.len(), 1);
    }

    #[test]
    fn query_keeps_edge_touching_segments() {
        let source = EmbeddedMapSource::from_segments(
            14,
            vec![SegmentRecord {
                x1: 1_000,
                y1: -1_000,
                x2: 2_000,
                y2: -1_000,
                road_class: 1,
                geometry_kind: 0,
            }],
        );
        let meters_per_unit = source.meters_per_world_unit;
        let bounds = WorldBounds {
            min: WorldPoint::new(
                source_x_to_meters(2_000, meters_per_unit),
                source_y_to_meters(-1_100, meters_per_unit),
            ),
            max: WorldPoint::new(
                source_x_to_meters(3_000, meters_per_unit),
                source_y_to_meters(-900, meters_per_unit),
            ),
        };

        let result = source.query(&spec(bounds));

        assert_eq!(result.geometry.len(), 1);
    }

    #[test]
    fn rejects_invalid_magic() {
        let mut bytes = encoded_map_with_payload(0, &[], 0, &[]);
        bytes[0..4].copy_from_slice(b"BAD!");

        let error = EmbeddedMapSource::from_svm_bytes(&bytes).expect_err("invalid magic");

        assert!(error.contains("magic mismatch"));
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut bytes = encoded_map_with_payload(0, &[], 0, &[]);
        bytes[4..6].copy_from_slice(&2_u16.to_le_bytes());

        let error = EmbeddedMapSource::from_svm_bytes(&bytes).expect_err("unsupported version");

        assert!(error.contains("unsupported svm version"));
    }

    #[test]
    fn rejects_truncated_header() {
        let error = EmbeddedMapSource::from_svm_bytes(b"SVM1").expect_err("truncated header");

        assert!(error.contains("header is truncated"));
    }

    #[test]
    fn rejects_truncated_name_payload() {
        let bytes = encoded_map_with_payload(4, b"abc", 0, &[]);

        let error = EmbeddedMapSource::from_svm_bytes(&bytes).expect_err("truncated name");

        assert!(error.contains("source name payload is truncated"));
    }

    #[test]
    fn rejects_truncated_segment_payload() {
        let bytes = encoded_map_with_payload(0, &[], 1, &[]);

        let error =
            EmbeddedMapSource::from_svm_bytes(&bytes).expect_err("truncated segment payload");

        assert!(error.contains("segment payload is truncated"));
    }

    #[test]
    fn embedded_city_map_returns_geometry_for_helsinki_query() {
        let source = EmbeddedMapSource::default();
        let center = project_gps_to_world(GpsSample {
            lat_deg: 60.17442,
            lon_deg: 24.94210,
            speed_mps: 0.0,
            course_rad: None,
            horizontal_accuracy_m: None,
        });
        let bounds = WorldBounds::from_center(center, 300.0, 300.0);
        let spec = MapQuerySpec::new(
            center,
            bounds,
            1.0,
            15.5,
            MapPresentationBand::CloseDetail,
            LodMask::from_layers(&[
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
            ]),
        );

        let result = source.query(&spec);

        assert!(
            !result.geometry.is_empty(),
            "expected embedded city.svm to return geometry near Helsinki"
        );
    }

    #[test]
    fn embedded_city_map_returns_poi_points_for_helsinki_query() {
        let source = EmbeddedMapSource::default();
        let center = project_gps_to_world(GpsSample {
            lat_deg: 60.1699,
            lon_deg: 24.9384,
            speed_mps: 0.0,
            course_rad: None,
            horizontal_accuracy_m: None,
        });
        let bounds = WorldBounds::from_center(center, 1_200.0, 1_200.0);
        let spec = MapQuerySpec::new(
            center,
            bounds,
            1.0,
            16.8,
            MapPresentationBand::CloseDetail,
            LodMask::from_layers(&[
                MapLayer::BikeParking,
                MapLayer::BikeRepair,
                MapLayer::Supermarket,
                MapLayer::Restaurant,
                MapLayer::Cafe,
                MapLayer::Water,
                MapLayer::Wc,
            ]),
        );

        let result = source.query(&spec);

        assert!(
            result
                .geometry
                .iter()
                .any(|candidate| matches!(candidate, GeometryCandidate::Point(_))),
            "expected embedded city.svm to return POI points near Helsinki city center"
        );
    }

    #[test]
    fn query_decodes_point_records_into_point_geometry() {
        let source = EmbeddedMapSource::from_segments(
            14,
            vec![SegmentRecord {
                x1: 1_000,
                y1: -1_000,
                x2: 1_000,
                y2: -1_000,
                road_class: 7,
                geometry_kind: 1,
            }],
        );
        let meters_per_unit = source.meters_per_world_unit;
        let bounds = WorldBounds {
            min: WorldPoint::new(
                source_x_to_meters(500, meters_per_unit),
                source_y_to_meters(-1_500, meters_per_unit),
            ),
            max: WorldPoint::new(
                source_x_to_meters(1_500, meters_per_unit),
                source_y_to_meters(-500, meters_per_unit),
            ),
        };

        let result = source.query(&spec(bounds));

        assert_eq!(result.geometry.len(), 1);
        assert!(matches!(
            &result.geometry[0],
            GeometryCandidate::Point(MapPointCandidate {
                layer: MapLayer::BikeParking,
                ..
            })
        ));
    }
}
