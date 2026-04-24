mod helsinki_water_posts;

use flate2::read::{GzDecoder, ZlibDecoder};
use geo_types::{Geometry, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon};
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
const MAX_WEB_MERCATOR_LAT_DEG: f64 = 85.051_128_78;
const DEFAULT_TARGET_ZOOM: i32 = 16;
const DEFAULT_MAX_SEGMENTS: usize = 5_000_000;
const DEFAULT_PROFILE: ConvertProfile = ConvertProfile::Bike;
const FEATURE_ARTERIAL_ROAD: u8 = 1;
const FEATURE_STREET_ROAD: u8 = 2;
const FEATURE_BIKE_ROUTE_MAIN: u8 = 3;
const FEATURE_BIKE_ROUTE_LOCAL: u8 = 4;
const FEATURE_FOOTPATH: u8 = 5;
const FEATURE_BUILDING_OUTLINE: u8 = 6;
const FEATURE_BIKE_PARKING: u8 = 7;
const FEATURE_BIKE_REPAIR: u8 = 8;
const FEATURE_SUPERMARKET: u8 = 9;
const FEATURE_RESTAURANT: u8 = 10;
const FEATURE_CAFE: u8 = 11;
const FEATURE_WATER: u8 = 12;
const FEATURE_WC: u8 = 13;
const GEOMETRY_POLYLINE: u8 = 0;
const GEOMETRY_POINT: u8 = 1;

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
    geometry_kind: u8,
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
        "shrink-svm" => {
            let cfg = parse_shrink_args(args.collect())?;
            let map = read_standard_map(&cfg.input)?;
            let shrunk = shrink_standard_map(map, cfg.max_segments);
            write_standard_map(&cfg.output, &shrunk)?;
            eprintln!(
                "map-vector-cli: wrote shrunk city map {} segments to {}",
                shrunk.segments.len(),
                cfg.output.display()
            );
            Ok(())
        }
        _ => Err(usage().to_owned()),
    }
}

fn usage() -> &'static str {
    "usage:\n  map-vector-cli convert-mbtiles --input <file.mbtiles> --output <city.svm> [--target-zoom <i32>] [--max-segments <usize>] [--profile <bike|all>]\n  map-vector-cli shrink-svm --input <city.svm> --output <city-small.svm> --max-segments <usize>"
}

struct ShrinkArgs {
    input: PathBuf,
    output: PathBuf,
    max_segments: usize,
}

fn parse_shrink_args(args: Vec<String>) -> Result<ShrinkArgs, String> {
    let mut input = None;
    let mut output = None;
    let mut max_segments: Option<usize> = None;
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
            "--max-segments" => {
                i += 1;
                max_segments = Some(parse_usize(args.get(i), "--max-segments")?);
            }
            other => return Err(format!("unknown arg: {other}")),
        }
        i += 1;
    }
    Ok(ShrinkArgs {
        input: input.ok_or("missing --input")?,
        output: output.ok_or("missing --output")?,
        max_segments: max_segments.ok_or("missing --max-segments")?,
    })
}

/// Keeps segments in priority order: lower `road_class` first (arterial
/// roads and dedicated bike-route spines before residential footpaths),
/// then by segment index for determinism. Truncates at `max_segments`.
fn shrink_standard_map(mut map: StandardMap, max_segments: usize) -> StandardMap {
    map.segments.sort_by_key(|s| (s.road_class, s.geometry_kind));
    if map.segments.len() > max_segments {
        map.segments.truncate(max_segments);
    }
    map
}

fn read_standard_map(path: &Path) -> Result<StandardMap, String> {
    let bytes =
        fs::read(path).map_err(|e| format!("failed reading {}: {e}", path.display()))?;
    parse_standard_map(&bytes)
}

fn parse_standard_map(bytes: &[u8]) -> Result<StandardMap, String> {
    const HEADER_LEN: usize = 30;
    const RECORD_LEN: usize = 24;
    if bytes.len() < HEADER_LEN {
        return Err("svm input too short for header".to_owned());
    }
    if &bytes[0..4] != MAGIC {
        return Err("svm magic mismatch".to_owned());
    }
    let version = read_u16(bytes, 4)?;
    if version != VERSION {
        return Err(format!(
            "unexpected svm version: got {}, expected {}",
            version, VERSION
        ));
    }
    let source_zoom = read_i32(bytes, 8)?;
    let bounds = Bounds {
        min_x: read_i32(bytes, 12)?,
        max_x: read_i32(bytes, 16)?,
        min_y: read_i32(bytes, 20)?,
        max_y: read_i32(bytes, 24)?,
    };
    let name_len = read_u16(bytes, 28)? as usize;
    let segment_count_offset = HEADER_LEN
        .checked_add(name_len)
        .ok_or("svm name offset overflow")?;
    if bytes.len() < segment_count_offset + 4 {
        return Err("svm truncated before segment count".to_owned());
    }
    let source_name = std::str::from_utf8(&bytes[HEADER_LEN..HEADER_LEN + name_len])
        .map_err(|e| format!("svm source name is not utf-8: {e}"))?
        .to_owned();
    let segment_count = read_u32(bytes, segment_count_offset)? as usize;
    let mut offset = segment_count_offset + 4;
    let segment_bytes = segment_count
        .checked_mul(RECORD_LEN)
        .ok_or("svm segment count overflow")?;
    if bytes.len() < offset + segment_bytes {
        return Err("svm truncated before segment table end".to_owned());
    }
    let mut segments = Vec::with_capacity(segment_count);
    for _ in 0..segment_count {
        segments.push(Segment {
            x1: read_i32(bytes, offset)?,
            y1: read_i32(bytes, offset + 4)?,
            x2: read_i32(bytes, offset + 8)?,
            y2: read_i32(bytes, offset + 12)?,
            road_class: bytes[offset + 16],
            geometry_kind: bytes[offset + 17],
            attr_id: read_u32(bytes, offset + 20)?,
        });
        offset += RECORD_LEN;
    }
    Ok(StandardMap {
        source_name,
        source_zoom,
        bounds,
        segments,
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    bytes
        .get(offset..offset + 2)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| format!("svm: unexpected eof at offset {offset}"))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    bytes
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| format!("svm: unexpected eof at offset {offset}"))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, String> {
    bytes
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .map(i32::from_le_bytes)
        .ok_or_else(|| format!("svm: unexpected eof at offset {offset}"))
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

    let supplemental_segments = hardcoded_helsinki_water_post_segments(cfg.input.as_path(), z);
    if !supplemental_segments.is_empty() {
        let keep_len = cfg.max_segments.saturating_sub(supplemental_segments.len());
        if segments.len() > keep_len {
            segments.truncate(keep_len);
        }
        segments.extend(supplemental_segments);
    } else if segments.len() > cfg.max_segments {
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
        buf.push(s.geometry_kind);
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
            "poi",
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
            "poi",
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
    if !(layer_name.contains("transport")
        || layer_name.contains("road")
        || layer_name.contains("street")
        || layer_name.contains("highway")
        || layer_name.contains("path")
        || layer_name.contains("rail")
        || layer_name.contains("cycle"))
    {
        return false;
    }

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
    if let Some(poi_class) = classify_poi_feature(layer_name, &class_text) {
        return Some(poi_class);
    }

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

fn classify_poi_feature(layer_name: &str, class_text: &str) -> Option<u8> {
    if !layer_name.contains("poi") {
        return None;
    }

    if contains_any(class_text, &["bicycle_parking"])
        || (class_text.contains("parking") && contains_any(class_text, &["bicycle", "bike"]))
    {
        return Some(FEATURE_BIKE_PARKING);
    }

    if contains_any(
        class_text,
        &[
            "bicycle_repair_station",
            "repair_station",
            "repair station",
            "bike repair",
            "bicycle repair",
            "bicycle_rental",
            "bike rental",
            "bicycle shop",
            "bike shop",
            "cycle repair",
            "compressed_air",
            "air pump",
            "bike pump",
            "bicycle pump",
        ],
    ) || (contains_any(class_text, &["bicycle", "bike", "cycle"])
        && contains_any(
            class_text,
            &["repair", "rental", "service", "shop", "workshop", "bicycle"],
        ))
    {
        return Some(FEATURE_BIKE_REPAIR);
    }

    if contains_any(
        class_text,
        &["supermarket", "grocery", "convenience", "general"],
    ) {
        return Some(FEATURE_SUPERMARKET);
    }

    if contains_any(
        class_text,
        &[
            "restaurant",
            "fast_food",
            "fast food",
            "food_court",
            "food court",
        ],
    ) {
        return Some(FEATURE_RESTAURANT);
    }

    if contains_any(class_text, &["cafe", "bakery"]) {
        return Some(FEATURE_CAFE);
    }

    if contains_any(
        class_text,
        &[
            "drinking_water",
            "drinking water",
            "water_point",
            "water point",
            "water_tap",
            "water tap",
            "fountain",
            "spring",
        ],
    ) {
        return Some(FEATURE_WATER);
    }

    if contains_any(class_text, &["toilets", "toilet", "wc", "restroom"]) {
        return Some(FEATURE_WC);
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
        Geometry::Point(point) => push_point(point, tx, ty, feature_class, out),
        Geometry::MultiPoint(MultiPoint(points)) => {
            for point in points {
                push_point(point, tx, ty, feature_class, out);
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
            geometry_kind: GEOMETRY_POLYLINE,
            attr_id: 0,
        });
    }
}

fn push_point(point: &Point<f32>, tx: i32, ty: i32, feature_class: u8, out: &mut Vec<Segment>) {
    let world = tile_coord_to_world(tx, ty, point.x(), point.y());
    out.push(Segment {
        x1: world.0,
        y1: world.1,
        x2: world.0,
        y2: world.1,
        road_class: feature_class,
        geometry_kind: GEOMETRY_POINT,
        attr_id: 0,
    });
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

fn hardcoded_helsinki_water_post_segments(input: &Path, source_zoom: i32) -> Vec<Segment> {
    if !is_helsinki_source(input) {
        return Vec::new();
    }

    helsinki_water_posts::HELSINKI_WATER_POSTS_LON_LAT
        .iter()
        .copied()
        .map(|(lon_deg, lat_deg)| lon_lat_to_world_point(lon_deg, lat_deg, source_zoom))
        .map(|(x, y)| Segment {
            x1: x,
            y1: y,
            x2: x,
            y2: y,
            road_class: FEATURE_WATER,
            geometry_kind: GEOMETRY_POINT,
            attr_id: 0,
        })
        .collect()
}

fn is_helsinki_source(input: &Path) -> bool {
    input
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().contains("helsinki"))
        .unwrap_or(false)
}

fn lon_lat_to_world_point(lon_deg: f64, lat_deg: f64, source_zoom: i32) -> (i32, i32) {
    let scale = TILE_EXTENT * 2.0_f64.powi(source_zoom);
    let clamped_lat = lat_deg.clamp(-MAX_WEB_MERCATOR_LAT_DEG, MAX_WEB_MERCATOR_LAT_DEG);
    let lat_rad = clamped_lat.to_radians();
    let world_x = ((lon_deg + 180.0) / 360.0 * scale).round() as i32;
    let mercator_y =
        (1.0 - ((lat_rad.tan() + (1.0 / lat_rad.cos())).ln() / std::f64::consts::PI)) / 2.0;
    let world_y = -(mercator_y * scale).round() as i32;
    (world_x, world_y)
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
        ConvertProfile, FEATURE_ARTERIAL_ROAD, FEATURE_BIKE_PARKING, FEATURE_BIKE_REPAIR,
        FEATURE_BIKE_ROUTE_LOCAL, FEATURE_BUILDING_OUTLINE, FEATURE_CAFE, FEATURE_FOOTPATH,
        FEATURE_RESTAURANT, FEATURE_SUPERMARKET, FEATURE_WATER, FEATURE_WC, classify_feature,
        hardcoded_helsinki_water_post_segments, is_helsinki_source, select_zoom_from_available,
        usage,
    };
    use geo_types::{Geometry, LineString};
    use mvt_reader::feature::{Feature, Value};
    use std::collections::HashMap;
    use std::path::Path;

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
        assert!(super::should_use_layer("poi", ConvertProfile::Bike));
    }

    #[test]
    fn bike_profile_excludes_rail_and_tram_transport() {
        let rail = sample_feature([("class", Value::String("rail".into()))]);
        assert!(super::is_bike_excluded_transport("transportation", &rail));

        let tram = sample_feature([("class", Value::String("tram".into()))]);
        assert!(super::is_bike_excluded_transport("transportation", &tram));
    }

    #[test]
    fn classify_bike_poi_categories() {
        let parking = sample_feature([
            ("class", Value::String("parking".into())),
            ("subclass", Value::String("bicycle".into())),
        ]);
        assert_eq!(
            classify_feature("poi", &parking),
            Some(FEATURE_BIKE_PARKING)
        );

        let repair = sample_feature([("subclass", Value::String("bicycle_repair_station".into()))]);
        assert_eq!(classify_feature("poi", &repair), Some(FEATURE_BIKE_REPAIR));

        let bicycle = sample_feature([
            ("class", Value::String("bicycle".into())),
            ("subclass", Value::String("bicycle".into())),
        ]);
        assert_eq!(classify_feature("poi", &bicycle), Some(FEATURE_BIKE_REPAIR));
    }

    #[test]
    fn classify_essentials_poi_categories() {
        let supermarket = sample_feature([("subclass", Value::String("supermarket".into()))]);
        assert_eq!(
            classify_feature("poi", &supermarket),
            Some(FEATURE_SUPERMARKET)
        );

        let restaurant = sample_feature([("subclass", Value::String("restaurant".into()))]);
        assert_eq!(
            classify_feature("poi", &restaurant),
            Some(FEATURE_RESTAURANT)
        );

        let cafe = sample_feature([("subclass", Value::String("cafe".into()))]);
        assert_eq!(classify_feature("poi", &cafe), Some(FEATURE_CAFE));

        let water = sample_feature([("class", Value::String("drinking_water".into()))]);
        assert_eq!(classify_feature("poi", &water), Some(FEATURE_WATER));

        let water_park = sample_feature([("class", Value::String("water_park".into()))]);
        assert_eq!(classify_feature("poi", &water_park), None);

        let wc = sample_feature([("class", Value::String("toilets".into()))]);
        assert_eq!(classify_feature("poi", &wc), Some(FEATURE_WC));
    }

    #[test]
    fn hardcoded_water_posts_only_apply_to_helsinki_sources() {
        assert!(is_helsinki_source(Path::new(
            "osm_finland_helsinki.mbtiles"
        )));
        assert!(!is_helsinki_source(Path::new("osm_finland_turku.mbtiles")));
    }

    #[test]
    fn hardcoded_helsinki_water_posts_generate_point_segments() {
        let segments =
            hardcoded_helsinki_water_post_segments(Path::new("osm_finland_helsinki.mbtiles"), 14);
        assert_eq!(segments.len(), 67);
        assert!(
            segments
                .iter()
                .all(|segment| segment.road_class == FEATURE_WATER)
        );
        assert!(
            segments
                .iter()
                .all(|segment| segment.geometry_kind == super::GEOMETRY_POINT)
        );
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
