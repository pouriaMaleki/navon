use flate2::read::{GzDecoder, ZlibDecoder};
use geo_types::{Geometry, LineString, MultiLineString, MultiPolygon, Polygon};
use mvt_reader::Reader;
use mvt_reader::feature::{Feature, Value};
use rusqlite::Connection;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"SVM1";
const VERSION: u16 = 1;
const TILE_EXTENT: f64 = 4096.0;
const DEFAULT_TARGET_ZOOM: i32 = 16;
const DEFAULT_MAX_SEGMENTS: usize = 5_000_000;
const DEFAULT_PROFILE: ConvertProfile = ConvertProfile::Bike;
const FEATURE_ARTERIAL_ROAD: u8 = 1;
const FEATURE_STREET_ROAD: u8 = 2;
const FEATURE_BIKE_ROUTE_MAIN: u8 = 3;
const FEATURE_BIKE_ROUTE_LOCAL: u8 = 4;
const FEATURE_FOOTPATH: u8 = 5;
const FEATURE_BUILDING_OUTLINE: u8 = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConvertProfile {
    Bike,
    All,
}

impl ConvertProfile {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.to_ascii_lowercase().as_str() {
            "bike" => Ok(Self::Bike),
            "all" => Ok(Self::All),
            other => Err(format!(
                "unsupported --profile value: {other} (expected bike|all)"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    road_class: u8,
    lane_count: u8,
    attr_id: u32,
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
}

#[derive(Debug)]
struct StandardMap {
    source_name: String,
    source_zoom: i32,
    bounds: Bounds,
    segments: Vec<Segment>,
}

#[derive(Debug)]
struct ConvertArgs {
    input: PathBuf,
    output: PathBuf,
    target_zoom: i32,
    max_segments: usize,
    profile: ConvertProfile,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("map-vector-cli error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        return Err(usage().to_owned());
    };

    match cmd.as_str() {
        "convert-mbtiles" => {
            let cfg = parse_convert_args(args.collect())?;
            let map = convert_mbtiles_to_standard(&cfg)?;
            write_standard_map(&cfg.output, &map)?;
            eprintln!(
                "map-vector-cli: wrote city map {} segments to {}",
                map.segments.len(),
                cfg.output.display()
            );
            Ok(())
        }
        _ => Err(usage().to_owned()),
    }
}

fn usage() -> &'static str {
    "usage:\n  map-vector-cli convert-mbtiles --input <file.mbtiles> --output <city.svm> [--target-zoom <i32>] [--max-segments <usize>] [--profile <bike|all>]"
}

fn parse_convert_args(args: Vec<String>) -> Result<ConvertArgs, String> {
    let mut input = None;
    let mut output = None;
    let mut target_zoom = DEFAULT_TARGET_ZOOM;
    let mut max_segments = DEFAULT_MAX_SEGMENTS;
    let mut profile = DEFAULT_PROFILE;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--input" => {
                i += 1;
                input = args.get(i).map(PathBuf::from);
            }
            "--output" => {
                i += 1;
                output = args.get(i).map(PathBuf::from);
            }
            "--target-zoom" => {
                i += 1;
                target_zoom = parse_i32(args.get(i), "--target-zoom")?;
            }
            "--max-segments" => {
                i += 1;
                max_segments = parse_usize(args.get(i), "--max-segments")?;
            }
            "--profile" => {
                i += 1;
                let raw = args.get(i).ok_or("missing value for --profile")?.as_str();
                profile = ConvertProfile::parse(raw)?;
            }
            other => return Err(format!("unknown arg: {other}")),
        }
        i += 1;
    }

    Ok(ConvertArgs {
        input: input.ok_or("missing --input")?,
        output: output.ok_or("missing --output")?,
        target_zoom,
        max_segments,
        profile,
    })
}

fn parse_i32(v: Option<&String>, flag: &str) -> Result<i32, String> {
    v.ok_or_else(|| format!("missing value for {flag}"))?
        .parse::<i32>()
        .map_err(|e| format!("invalid {flag}: {e}"))
}

fn parse_usize(v: Option<&String>, flag: &str) -> Result<usize, String> {
    v.ok_or_else(|| format!("missing value for {flag}"))?
        .parse::<usize>()
        .map_err(|e| format!("invalid {flag}: {e}"))
}

fn convert_mbtiles_to_standard(cfg: &ConvertArgs) -> Result<StandardMap, String> {
    let conn = Connection::open(&cfg.input).map_err(|e| format!("failed to open mbtiles: {e}"))?;
    let z = select_zoom(&conn, cfg.target_zoom)?;

    let mut stmt = conn
        .prepare(
            "SELECT zoom_level, tile_column, tile_row, tile_data
             FROM tiles
             WHERE zoom_level = ?1
             ORDER BY tile_column ASC, tile_row ASC",
        )
        .map_err(|e| format!("prepare tiles query failed: {e}"))?;

    let mut rows = stmt
        .query(rusqlite::params![z])
        .map_err(|e| format!("tiles query failed: {e}"))?;

    let mut segments = Vec::new();
    while let Some(row) = rows.next().map_err(|e| format!("row read failed: {e}"))? {
        let zoom: i32 = row.get(0).map_err(|e| e.to_string())?;
        let tx: i32 = row.get(1).map_err(|e| e.to_string())?;
        let tms_ty: i32 = row.get(2).map_err(|e| e.to_string())?;
        let blob: Vec<u8> = row.get(3).map_err(|e| e.to_string())?;

        let xyz_ty = (1_i32 << zoom) - 1 - tms_ty;
        let tile_bytes = maybe_decompress(&blob)?;
        let reader = Reader::new(tile_bytes).map_err(|e| format!("tile decode failed: {e}"))?;
        let layers = reader
            .get_layer_metadata()
            .map_err(|e| format!("layer metadata failed: {e}"))?;

        for layer in layers {
            let lname = layer.name.to_ascii_lowercase();
            if !should_use_layer(&lname, cfg.profile) {
                continue;
            }
            let feats = reader
                .get_features_as::<f32>(layer.layer_index)
                .map_err(|e| format!("feature decode failed: {e}"))?;
            for feature in feats {
                if !should_use_feature(&lname, &feature, cfg.profile) {
                    continue;
                }
                let Some(feature_class) = classify_feature(&lname, &feature) else {
                    continue;
                };
                push_geometry_segments(&feature.geometry, tx, xyz_ty, feature_class, &mut segments);
                if segments.len() >= cfg.max_segments {
                    break;
                }
            }
            if segments.len() >= cfg.max_segments {
                break;
            }
        }
        if segments.len() >= cfg.max_segments {
            break;
        }
    }

    if segments.len() > cfg.max_segments {
        segments.truncate(cfg.max_segments);
    }
    let bounds = compute_bounds(&segments);
    let source_name = cfg
        .input
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_owned();

    eprintln!(
        "map-vector-cli: source={} zoom={} segments={} bounds=({}, {})..({}, {})",
        source_name,
        z,
        segments.len(),
        bounds.min_x,
        bounds.min_y,
        bounds.max_x,
        bounds.max_y
    );

    Ok(StandardMap {
        source_name,
        source_zoom: z,
        bounds,
        segments,
    })
}

fn write_standard_map(path: &Path, map: &StandardMap) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("failed to create output dir: {e}"))?;
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(MAGIC);
    write_u16(&mut buf, VERSION)?;
    write_u16(&mut buf, 0)?;
    write_i32(&mut buf, map.source_zoom)?;
    write_i32(&mut buf, map.bounds.min_x)?;
    write_i32(&mut buf, map.bounds.max_x)?;
    write_i32(&mut buf, map.bounds.min_y)?;
    write_i32(&mut buf, map.bounds.max_y)?;

    let src = map.source_name.as_bytes();
    if src.len() > u16::MAX as usize {
        return Err("source file name too long".to_owned());
    }
    write_u16(&mut buf, src.len() as u16)?;
    buf.extend_from_slice(src);

    write_u32(&mut buf, map.segments.len() as u32)?;
    for s in &map.segments {
        write_i32(&mut buf, s.x1)?;
        write_i32(&mut buf, s.y1)?;
        write_i32(&mut buf, s.x2)?;
        write_i32(&mut buf, s.y2)?;
        buf.push(s.road_class);
        buf.push(s.lane_count);
        write_u16(&mut buf, 0)?;
        write_u32(&mut buf, s.attr_id)?;
    }

    fs::write(path, buf).map_err(|e| format!("failed writing {}: {e}", path.display()))
}

fn write_u16<W: Write>(w: &mut W, v: u16) -> Result<(), String> {
    w.write_all(&v.to_le_bytes()).map_err(|e| e.to_string())
}

fn write_u32<W: Write>(w: &mut W, v: u32) -> Result<(), String> {
    w.write_all(&v.to_le_bytes()).map_err(|e| e.to_string())
}

fn write_i32<W: Write>(w: &mut W, v: i32) -> Result<(), String> {
    w.write_all(&v.to_le_bytes()).map_err(|e| e.to_string())
}

fn select_zoom(conn: &Connection, target: i32) -> Result<i32, String> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT zoom_level FROM tiles ORDER BY zoom_level")
        .map_err(|e| format!("prepare zoom list query failed: {e}"))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| format!("zoom list query failed: {e}"))?;
    let mut zooms = Vec::new();
    while let Some(row) = rows.next().map_err(|e| format!("zoom row failed: {e}"))? {
        zooms.push(row.get::<_, i32>(0).map_err(|e| e.to_string())?);
    }

    if let Some(z) = select_zoom_from_available(&zooms, target) {
        if z != target {
            eprintln!("map-vector-cli: requested zoom {target} not available, using {z}");
        }
        return Ok(z);
    }

    let mut stmt = conn
        .prepare("SELECT MIN(zoom_level) FROM tiles")
        .map_err(|e| format!("prepare zoom query failed: {e}"))?;
    stmt.query_row([], |row| row.get::<_, i32>(0))
        .map_err(|e| format!("zoom query failed: {e}"))
}

fn select_zoom_from_available(zooms: &[i32], target: i32) -> Option<i32> {
    zooms
        .iter()
        .copied()
        .filter(|z| *z <= target)
        .max()
        .or_else(|| zooms.iter().copied().min())
}

fn should_use_layer(layer: &str, profile: ConvertProfile) -> bool {
    match profile {
        ConvertProfile::All => [
            "transport",
            "road",
            "street",
            "highway",
            "path",
            "rail",
            "cycle",
            "building",
        ]
        .iter()
        .any(|k| layer.contains(k)),
        ConvertProfile::Bike => [
            "transport",
            "road",
            "street",
            "highway",
            "path",
            "rail",
            "cycle",
            "building",
        ]
        .iter()
        .any(|k| layer.contains(k)),
    }
}

fn should_use_feature(layer_name: &str, feature: &Feature<f32>, profile: ConvertProfile) -> bool {
    match profile {
        ConvertProfile::All => true,
        ConvertProfile::Bike => !is_bike_excluded_transport(layer_name, feature),
    }
}

fn is_bike_excluded_transport(layer_name: &str, feature: &Feature<f32>) -> bool {
    if layer_name.contains("water")
        || layer_name.contains("rail")
        || layer_name.contains("metro")
        || layer_name.contains("tram")
        || layer_name.contains("train")
        || layer_name.contains("subway")
    {
        return true;
    }

    let Some(props) = feature.properties.as_ref() else {
        return false;
    };

    let blocked = [
        "ferry",
        "boat",
        "ship",
        "water",
        "waterway",
        "seaway",
        "marine",
        "rail",
        "tram",
        "metro",
        "train",
        "subway",
        "light_rail",
    ];

    for (k, v) in props {
        let key = k.to_ascii_lowercase();
        if !matches!(
            key.as_str(),
            "class" | "subclass" | "type" | "route" | "network" | "transport"
        ) {
            continue;
        }

        let Some(value) = mvt_value_to_ascii(v) else {
            continue;
        };
        if blocked.iter().any(|needle| value.contains(needle)) {
            return true;
        }
    }

    false
}

fn mvt_value_to_ascii(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.to_ascii_lowercase()),
        Value::Float(n) => Some(n.to_string()),
        Value::Double(n) => Some(n.to_string()),
        Value::Int(n) => Some(n.to_string()),
        Value::UInt(n) => Some(n.to_string()),
        Value::SInt(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
    }
}

fn classify_feature(layer_name: &str, feature: &Feature<f32>) -> Option<u8> {
    let class_text = classification_text(layer_name, feature);
    if class_text.contains("building") {
        return Some(FEATURE_BUILDING_OUTLINE);
    }

    if contains_any(&class_text, &["cycle", "bike", "bicycle"]) {
        if contains_any(
            &class_text,
            &[
                "trunk",
                "primary",
                "secondary",
                "route",
                "network",
                "ncn",
                "rcn",
            ],
        ) {
            return Some(FEATURE_BIKE_ROUTE_MAIN);
        }
        return Some(FEATURE_BIKE_ROUTE_LOCAL);
    }

    if contains_any(
        &class_text,
        &[
            "footway",
            "foot",
            "pedestrian",
            "track",
            "trail",
            "steps",
            "walk",
            "path",
        ],
    ) {
        return Some(FEATURE_FOOTPATH);
    }

    if contains_any(
        &class_text,
        &[
            "motorway",
            "highway",
            "trunk",
            "primary",
            "secondary",
            "tertiary",
            "arterial",
        ],
    ) {
        return Some(FEATURE_ARTERIAL_ROAD);
    }

    if contains_any(
        &class_text,
        &[
            "road",
            "street",
            "residential",
            "service",
            "living_street",
            "unclassified",
            "transport",
        ],
    ) {
        return Some(FEATURE_STREET_ROAD);
    }

    None
}

fn push_geometry_segments(
    geometry: &Geometry<f32>,
    tx: i32,
    ty: i32,
    feature_class: u8,
    out: &mut Vec<Segment>,
) {
    match geometry {
        Geometry::LineString(ls) => push_linestring(ls, tx, ty, feature_class, out),
        Geometry::MultiLineString(MultiLineString(lines)) => {
            for ls in lines {
                push_linestring(ls, tx, ty, feature_class, out);
            }
        }
        Geometry::Polygon(polygon) => push_polygon_outline(polygon, tx, ty, feature_class, out),
        Geometry::MultiPolygon(MultiPolygon(polygons)) => {
            for polygon in polygons {
                push_polygon_outline(polygon, tx, ty, feature_class, out);
            }
        }
        _ => {}
    }
}

fn push_linestring(
    ls: &LineString<f32>,
    tx: i32,
    ty: i32,
    feature_class: u8,
    out: &mut Vec<Segment>,
) {
    if ls.0.len() < 2 {
        return;
    }
    for win in ls.0.windows(2) {
        let a = tile_coord_to_world(tx, ty, win[0].x, win[0].y);
        let b = tile_coord_to_world(tx, ty, win[1].x, win[1].y);
        if a == b {
            continue;
        }
        out.push(Segment {
            x1: a.0,
            y1: a.1,
            x2: b.0,
            y2: b.1,
            road_class: feature_class,
            lane_count: 0,
            attr_id: 0,
        });
    }
}

fn push_polygon_outline(
    polygon: &Polygon<f32>,
    tx: i32,
    ty: i32,
    feature_class: u8,
    out: &mut Vec<Segment>,
) {
    push_linestring(polygon.exterior(), tx, ty, feature_class, out);
}

fn classification_text(layer_name: &str, feature: &Feature<f32>) -> String {
    let mut parts = vec![layer_name.to_ascii_lowercase()];
    if let Some(props) = feature.properties.as_ref() {
        for (key, value) in props {
            let key = key.to_ascii_lowercase();
            if !matches!(
                key.as_str(),
                "class"
                    | "subclass"
                    | "type"
                    | "route"
                    | "network"
                    | "transport"
                    | "kind"
                    | "highway"
                    | "category"
            ) {
                continue;
            }
            if let Some(value) = mvt_value_to_ascii(value) {
                parts.push(value);
            }
        }
    }
    parts.join(" ")
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn tile_coord_to_world(tx: i32, ty: i32, x: f32, y: f32) -> (i32, i32) {
    let wx = (tx as f64 * TILE_EXTENT + x as f64).round() as i32;
    let wy = -((ty as f64 * TILE_EXTENT + y as f64).round() as i32);
    (wx, wy)
}

fn maybe_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        let mut out = Vec::new();
        let mut gz = GzDecoder::new(data);
        gz.read_to_end(&mut out)
            .map_err(|e| format!("gzip decode failed: {e}"))?;
        return Ok(out);
    }
    if data.len() >= 2 && data[0] == 0x78 {
        let mut out = Vec::new();
        let mut z = ZlibDecoder::new(data);
        if z.read_to_end(&mut out).is_ok() {
            return Ok(out);
        }
    }
    Ok(data.to_vec())
}

fn compute_bounds(segments: &[Segment]) -> Bounds {
    if segments.is_empty() {
        return Bounds {
            min_x: 0,
            max_x: 0,
            min_y: 0,
            max_y: 0,
        };
    }

    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    for s in segments {
        min_x = min_x.min(s.x1).min(s.x2);
        max_x = max_x.max(s.x1).max(s.x2);
        min_y = min_y.min(s.y1).min(s.y2);
        max_y = max_y.max(s.y1).max(s.y2);
    }

    Bounds {
        min_x,
        max_x,
        min_y,
        max_y,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConvertProfile, FEATURE_ARTERIAL_ROAD, FEATURE_BIKE_ROUTE_LOCAL, FEATURE_BUILDING_OUTLINE,
        FEATURE_FOOTPATH, classify_feature, select_zoom_from_available, usage,
    };
    use geo_types::{Geometry, LineString};
    use mvt_reader::feature::{Feature, Value};
    use std::collections::HashMap;

    #[test]
    fn select_zoom_prefers_nearest_below_or_equal_target() {
        let zooms = [12, 14, 16, 17];
        assert_eq!(select_zoom_from_available(&zooms, 16), Some(16));
        assert_eq!(select_zoom_from_available(&zooms, 15), Some(14));
    }

    #[test]
    fn select_zoom_falls_back_to_min_when_target_too_low() {
        let zooms = [10, 12, 13];
        assert_eq!(select_zoom_from_available(&zooms, 8), Some(10));
    }

    #[test]
    fn select_zoom_handles_empty_zoom_list() {
        let zooms: [i32; 0] = [];
        assert_eq!(select_zoom_from_available(&zooms, 10), None);
    }

    #[test]
    fn usage_only_advertises_convert_mbtiles() {
        assert!(usage().contains("convert-mbtiles"));
        assert!(!usage().contains("emit-rust-window"));
    }

    #[test]
    fn classify_building_from_layer_name() {
        let feature = sample_feature([("class", Value::String("building".into()))]);
        assert_eq!(
            classify_feature("building", &feature),
            Some(FEATURE_BUILDING_OUTLINE)
        );
    }

    #[test]
    fn classify_bike_local_from_cycle_terms() {
        let feature = sample_feature([("class", Value::String("cycleway".into()))]);
        assert_eq!(
            classify_feature("transportation", &feature),
            Some(FEATURE_BIKE_ROUTE_LOCAL)
        );
    }

    #[test]
    fn classify_arterial_and_footpath() {
        let arterial = sample_feature([("class", Value::String("primary".into()))]);
        assert_eq!(
            classify_feature("transportation", &arterial),
            Some(FEATURE_ARTERIAL_ROAD)
        );

        let footpath = sample_feature([("class", Value::String("footway".into()))]);
        assert_eq!(
            classify_feature("transportation", &footpath),
            Some(FEATURE_FOOTPATH)
        );
    }

    #[test]
    fn bike_profile_keeps_building_layers() {
        assert!(super::should_use_layer("building", ConvertProfile::Bike));
    }

    #[test]
    fn bike_profile_excludes_rail_and_tram_transport() {
        let rail = sample_feature([("class", Value::String("rail".into()))]);
        assert!(super::is_bike_excluded_transport("transportation", &rail));

        let tram = sample_feature([("class", Value::String("tram".into()))]);
        assert!(super::is_bike_excluded_transport("transportation", &tram));
    }

    fn sample_feature<const N: usize>(entries: [(&str, Value); N]) -> Feature<f32> {
        Feature {
            geometry: Geometry::LineString(LineString::new(vec![])),
            id: None,
            properties: Some(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value))
                    .collect::<HashMap<_, _>>(),
            ),
        }
    }
}
