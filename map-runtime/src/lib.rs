use std::collections::HashSet;

use runtime_core::api::{
    GeometryCandidate, MapLayer, MapPolylineCandidate, MapQueryResult, MapQuerySpec, WorldBounds,
    WorldPoint,
};
use runtime_core::map::MapSource;

const TILE_EXTENT: f64 = 4096.0;
const EARTH_RADIUS_M: f64 = 6_378_137.0;
const EARTH_CIRCUMFERENCE_M: f64 = std::f64::consts::TAU * EARTH_RADIUS_M;
const HALF_EARTH_CIRCUMFERENCE_M: f64 = EARTH_CIRCUMFERENCE_M / 2.0;
const GRID_CELL_WORLD_UNITS: i32 = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SegmentRecord {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    road_class: u8,
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
        let source_zoom = read_i32(bytes, 8)?;
        let header_bounds = SourceBounds {
            min_x: read_i32(bytes, 12)?,
            max_x: read_i32(bytes, 16)?,
            min_y: read_i32(bytes, 20)?,
            max_y: read_i32(bytes, 24)?,
        };
        let source_name_len = read_u16(bytes, 28)? as usize;
        let segment_count_offset = 30 + source_name_len;
        let segment_count = read_u32(bytes, segment_count_offset)? as usize;
        let mut offset = segment_count_offset + 4;
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

impl MapSource for EmbeddedMapSource {
    fn query(&self, spec: &MapQuerySpec) -> MapQueryResult {
        let query_bounds = self.query_bounds_in_source_units(spec.bounds);
        let mut geometry = Vec::new();
        for index in self.grid.candidate_indices(query_bounds) {
            let segment = self.segments[index as usize];
            if !segment_intersects_bounds(segment, query_bounds) {
                continue;
            }
            let layer = map_layer_for_road_class(segment.road_class);
            if !spec.lod_mask.contains(layer) {
                continue;
            }
            geometry.push(GeometryCandidate::Polyline(MapPolylineCandidate {
                layer,
                points: vec![
                    WorldPoint::new(
                        source_x_to_meters(segment.x1, self.meters_per_world_unit),
                        source_y_to_meters(segment.y1, self.meters_per_world_unit),
                    ),
                    WorldPoint::new(
                        source_x_to_meters(segment.x2, self.meters_per_world_unit),
                        source_y_to_meters(segment.y2, self.meters_per_world_unit),
                    ),
                ],
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

fn map_layer_for_road_class(road_class: u8) -> MapLayer {
    match road_class {
        1 | 2 => MapLayer::MajorRoad,
        3 => MapLayer::MinorRoad,
        _ => MapLayer::Path,
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
    use runtime_core::api::{LodMask, MapLayer, MapQuerySpec, ZoomBucket};

    use super::*;

    fn spec(bounds: WorldBounds) -> MapQuerySpec {
        MapQuerySpec::new(
            WorldPoint::ORIGIN,
            bounds,
            1.0,
            15.5,
            ZoomBucket::Detail,
            LodMask::from_layers(&[MapLayer::MajorRoad, MapLayer::MinorRoad, MapLayer::Path]),
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
                },
                SegmentRecord {
                    x1: 50_000,
                    y1: -50_000,
                    x2: 52_000,
                    y2: -50_000,
                    road_class: 3,
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
}
