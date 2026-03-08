use flate2::read::{GzDecoder, ZlibDecoder};
use geo_types::{Geometry, LineString, MultiLineString};
use mvt_reader::Reader;
use rusqlite::Connection;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"SVM1";
const VERSION: u16 = 1;
const TILE_EXTENT: f64 = 4096.0;
const DEFAULT_TARGET_ZOOM: i32 = 16;
const DEFAULT_MAX_SEGMENTS: usize = 1_500_000;
const MAP_BOUNDS_MIN: i16 = 0;
const MAP_BOUNDS_MAX: i16 = 10_000;

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
}

#[derive(Debug)]
struct EmitRustWindowArgs {
    input: PathBuf,
    output: PathBuf,
    center_lat: f64,
    center_lon: f64,
    player_lat: f64,
    player_lon: f64,
    view_tiles: f64,
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
        "emit-rust-window" => {
            let cfg = parse_emit_rust_window_args(args.collect())?;
            let map = read_standard_map(&cfg.input)?;
            write_rust_window_module(&cfg.output, &map, &cfg)?;
            eprintln!(
                "map-vector-cli: wrote windowed rust module to {}",
                cfg.output.display()
            );
            Ok(())
        }
        _ => Err(usage().to_owned()),
    }
}

fn usage() -> &'static str {
    "usage:\n  map-vector-cli convert-mbtiles --input <file.mbtiles> --output <city.svm> [--target-zoom <i32>] [--max-segments <usize>]\n  map-vector-cli emit-rust-window --input <city.svm> --output <generated_map.rs> --center-lat <f64> --center-lon <f64> --player-lat <f64> --player-lon <f64> [--view-tiles <f64>]"
}

fn parse_convert_args(args: Vec<String>) -> Result<ConvertArgs, String> {
    let mut input = None;
    let mut output = None;
    let mut target_zoom = DEFAULT_TARGET_ZOOM;
    let mut max_segments = DEFAULT_MAX_SEGMENTS;

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
            other => return Err(format!("unknown arg: {other}")),
        }
        i += 1;
    }

    Ok(ConvertArgs {
        input: input.ok_or("missing --input")?,
        output: output.ok_or("missing --output")?,
        target_zoom,
        max_segments,
    })
}

fn parse_emit_rust_window_args(args: Vec<String>) -> Result<EmitRustWindowArgs, String> {
    let mut input = None;
    let mut output = None;
    let mut center_lat = None;
    let mut center_lon = None;
    let mut player_lat = None;
    let mut player_lon = None;
    let mut view_tiles = 3.0;

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
            "--center-lat" => {
                i += 1;
                center_lat = Some(parse_f64(args.get(i), "--center-lat")?);
            }
            "--center-lon" => {
                i += 1;
                center_lon = Some(parse_f64(args.get(i), "--center-lon")?);
            }
            "--player-lat" => {
                i += 1;
                player_lat = Some(parse_f64(args.get(i), "--player-lat")?);
            }
            "--player-lon" => {
                i += 1;
                player_lon = Some(parse_f64(args.get(i), "--player-lon")?);
            }
            "--view-tiles" => {
                i += 1;
                view_tiles = parse_f64(args.get(i), "--view-tiles")?;
            }
            other => return Err(format!("unknown arg: {other}")),
        }
        i += 1;
    }

    Ok(EmitRustWindowArgs {
        input: input.ok_or("missing --input")?,
        output: output.ok_or("missing --output")?,
        center_lat: center_lat.ok_or("missing --center-lat")?,
        center_lon: center_lon.ok_or("missing --center-lon")?,
        player_lat: player_lat.ok_or("missing --player-lat")?,
        player_lon: player_lon.ok_or("missing --player-lon")?,
        view_tiles,
    })
}

fn parse_f64(v: Option<&String>, flag: &str) -> Result<f64, String> {
    v.ok_or_else(|| format!("missing value for {flag}"))?
        .parse::<f64>()
        .map_err(|e| format!("invalid {flag}: {e}"))
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
             WHERE zoom_level = ?1",
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
            if !should_use_layer(&lname) {
                continue;
            }
            let road_class = classify_road(&lname);

            let feats = reader
                .get_features_as::<f32>(layer.layer_index)
                .map_err(|e| format!("feature decode failed: {e}"))?;
            for feature in feats {
                push_geometry_segments(&feature.geometry, tx, xyz_ty, road_class, &mut segments);
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

fn write_rust_window_module(path: &Path, map: &StandardMap, cfg: &EmitRustWindowArgs) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("failed to create rust output dir: {e}"))?;
    }

    let (center_x, center_y) = lonlat_to_world(cfg.center_lon, cfg.center_lat, map.source_zoom);
    let (player_x, player_y) = lonlat_to_world(cfg.player_lon, cfg.player_lat, map.source_zoom);
    let half_tile_world = ((cfg.view_tiles / 2.0) * TILE_EXTENT).round() as i32;
    let camera = Bounds {
        min_x: center_x - half_tile_world,
        max_x: center_x + half_tile_world,
        min_y: center_y - half_tile_world,
        max_y: center_y + half_tile_world,
    };

    let mut selected = Vec::new();
    for s in &map.segments {
        if segment_intersects_bounds(*s, camera) {
            let (x1, y1) = normalize_to_map_space(s.x1, s.y1, camera);
            let (x2, y2) = normalize_to_map_space(s.x2, s.y2, camera);
            if x1 == x2 && y1 == y2 {
                continue;
            }
            let intensity = match s.road_class {
                1 => 245,
                2 => 230,
                3 => 215,
                _ => 190,
            };
            let thickness = if s.road_class <= 2 { 2 } else { 1 };
            selected.push((x1, y1, x2, y2, intensity, thickness));
        }
    }

    let player = normalize_to_map_space(player_x, player_y, camera);

    let mut out = String::new();
    out.push_str("use esp32_screen_render_core::{Line, WorldBounds, WorldPoint};\n\n");
    out.push_str("pub const HAS_MAP: bool = true;\n");
    out.push_str(&format!("pub const MAP_SOURCE: &str = {:?};\n", map.source_name));
    out.push_str(&format!("pub const MAP_ZOOM: i32 = {};\n", map.source_zoom));
    out.push_str(&format!("pub const MAP_CENTER_LAT: f64 = {:.6};\n", cfg.center_lat));
    out.push_str(&format!("pub const MAP_CENTER_LON: f64 = {:.6};\n", cfg.center_lon));
    out.push_str("pub const MAP_BOUNDS: WorldBounds = WorldBounds { min_x: 0, max_x: 10000, min_y: 0, max_y: 10000 };\n");
    out.push_str(&format!(
        "pub const MAP_PLAYER: WorldPoint = WorldPoint {{ x: {}, y: {} }};\n",
        player.0, player.1
    ));
    out.push_str("pub const MAP_LINES: &[Line] = &[\n");
    for (x1, y1, x2, y2, intensity, thickness) in selected {
        out.push_str(&format!(
            "    Line {{ from: WorldPoint {{ x: {}, y: {} }}, to: WorldPoint {{ x: {}, y: {} }}, intensity: {}, thickness: {} }},\n",
            x1, y1, x2, y2, intensity, thickness
        ));
    }
    out.push_str("];\n");

    fs::write(path, out).map_err(|e| format!("failed writing {}: {e}", path.display()))
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

fn read_standard_map(path: &Path) -> Result<StandardMap, String> {
    let data = fs::read(path).map_err(|e| format!("failed reading {}: {e}", path.display()))?;
    let mut i = 0usize;

    if data.len() < 4 || &data[0..4] != MAGIC {
        return Err("invalid SVM file magic".to_owned());
    }
    i += 4;

    let version = read_u16(&data, &mut i)?;
    if version != VERSION {
        return Err(format!("unsupported SVM version: {version}"));
    }
    let _flags = read_u16(&data, &mut i)?;
    let source_zoom = read_i32(&data, &mut i)?;
    let bounds = Bounds {
        min_x: read_i32(&data, &mut i)?,
        max_x: read_i32(&data, &mut i)?,
        min_y: read_i32(&data, &mut i)?,
        max_y: read_i32(&data, &mut i)?,
    };

    let source_len = read_u16(&data, &mut i)? as usize;
    if i + source_len > data.len() {
        return Err("invalid source length in SVM".to_owned());
    }
    let source_name = String::from_utf8(data[i..i + source_len].to_vec())
        .map_err(|e| format!("invalid utf8 in source name: {e}"))?;
    i += source_len;

    let segment_count = read_u32(&data, &mut i)? as usize;
    let mut segments = Vec::with_capacity(segment_count);
    for _ in 0..segment_count {
        let x1 = read_i32(&data, &mut i)?;
        let y1 = read_i32(&data, &mut i)?;
        let x2 = read_i32(&data, &mut i)?;
        let y2 = read_i32(&data, &mut i)?;
        let road_class = read_u8(&data, &mut i)?;
        let lane_count = read_u8(&data, &mut i)?;
        let _reserved = read_u16(&data, &mut i)?;
        let attr_id = read_u32(&data, &mut i)?;
        segments.push(Segment {
            x1,
            y1,
            x2,
            y2,
            road_class,
            lane_count,
            attr_id,
        });
    }

    Ok(StandardMap {
        source_name,
        source_zoom,
        bounds,
        segments,
    })
}

fn segment_intersects_bounds(s: Segment, b: Bounds) -> bool {
    let min_x = s.x1.min(s.x2);
    let max_x = s.x1.max(s.x2);
    let min_y = s.y1.min(s.y2);
    let max_y = s.y1.max(s.y2);
    !(max_x < b.min_x || min_x > b.max_x || max_y < b.min_y || min_y > b.max_y)
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

fn read_u8(data: &[u8], i: &mut usize) -> Result<u8, String> {
    if *i + 1 > data.len() {
        return Err("unexpected EOF".to_owned());
    }
    let v = data[*i];
    *i += 1;
    Ok(v)
}

fn read_u16(data: &[u8], i: &mut usize) -> Result<u16, String> {
    if *i + 2 > data.len() {
        return Err("unexpected EOF".to_owned());
    }
    let mut b = [0u8; 2];
    b.copy_from_slice(&data[*i..*i + 2]);
    *i += 2;
    Ok(u16::from_le_bytes(b))
}

fn read_u32(data: &[u8], i: &mut usize) -> Result<u32, String> {
    if *i + 4 > data.len() {
        return Err("unexpected EOF".to_owned());
    }
    let mut b = [0u8; 4];
    b.copy_from_slice(&data[*i..*i + 4]);
    *i += 4;
    Ok(u32::from_le_bytes(b))
}

fn read_i32(data: &[u8], i: &mut usize) -> Result<i32, String> {
    if *i + 4 > data.len() {
        return Err("unexpected EOF".to_owned());
    }
    let mut b = [0u8; 4];
    b.copy_from_slice(&data[*i..*i + 4]);
    *i += 4;
    Ok(i32::from_le_bytes(b))
}

fn select_zoom(conn: &Connection, target: i32) -> Result<i32, String> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT zoom_level FROM tiles ORDER BY zoom_level")
        .map_err(|e| format!("prepare zoom list query failed: {e}"))?;
    let mut rows = stmt.query([]).map_err(|e| format!("zoom list query failed: {e}"))?;
    let mut zooms = Vec::new();
    while let Some(row) = rows.next().map_err(|e| format!("zoom row failed: {e}"))? {
        zooms.push(row.get::<_, i32>(0).map_err(|e| e.to_string())?);
    }

    if let Some(z) = zooms.iter().copied().filter(|z| *z <= target).max() {
        if z != target {
            eprintln!("map-vector-cli: requested zoom {} not available, using {}", target, z);
        }
        return Ok(z);
    }

    let mut stmt = conn
        .prepare("SELECT MAX(zoom_level) FROM tiles")
        .map_err(|e| format!("prepare zoom query failed: {e}"))?;
    stmt.query_row([], |row| row.get::<_, i32>(0))
        .map_err(|e| format!("zoom query failed: {e}"))
}

fn should_use_layer(layer: &str) -> bool {
    ["transport", "road", "street", "highway", "path", "rail"]
        .iter()
        .any(|k| layer.contains(k))
}

fn classify_road(layer: &str) -> u8 {
    if layer.contains("motorway") || layer.contains("highway") {
        1
    } else if layer.contains("trunk") || layer.contains("primary") {
        2
    } else if layer.contains("secondary") || layer.contains("tertiary") {
        3
    } else {
        4
    }
}

fn push_geometry_segments(
    geometry: &Geometry<f32>,
    tx: i32,
    ty: i32,
    road_class: u8,
    out: &mut Vec<Segment>,
) {
    match geometry {
        Geometry::LineString(ls) => push_linestring(ls, tx, ty, road_class, out),
        Geometry::MultiLineString(MultiLineString(lines)) => {
            for ls in lines {
                push_linestring(ls, tx, ty, road_class, out);
            }
        }
        _ => {}
    }
}

fn push_linestring(ls: &LineString<f32>, tx: i32, ty: i32, road_class: u8, out: &mut Vec<Segment>) {
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
            road_class,
            lane_count: 0,
            attr_id: 0,
        });
    }
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

fn lonlat_to_tile_xy(lon: f64, lat: f64, z: i32) -> (f64, f64) {
    let n = 2_f64.powi(z);
    let x = (lon + 180.0) / 360.0 * n;
    let lat_rad = lat.to_radians();
    let y = (1.0 - ((lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI)) / 2.0 * n;
    (x, y)
}

fn lonlat_to_world(lon: f64, lat: f64, z: i32) -> (i32, i32) {
    let (tx, ty) = lonlat_to_tile_xy(lon, lat, z);
    let wx = (tx * TILE_EXTENT).round() as i32;
    let wy = -(ty * TILE_EXTENT).round() as i32;
    (wx, wy)
}

fn normalize_to_map_space(x: i32, y: i32, bounds: Bounds) -> (i16, i16) {
    (
        normalize_axis(x, bounds.min_x, bounds.max_x),
        normalize_axis(y, bounds.min_y, bounds.max_y),
    )
}

fn normalize_axis(v: i32, min_v: i32, max_v: i32) -> i16 {
    let range = (max_v - min_v).max(1) as i64;
    let pos = (v - min_v) as i64;
    let scaled = (pos * (MAP_BOUNDS_MAX as i64)) / range;
    scaled.clamp(MAP_BOUNDS_MIN as i64, MAP_BOUNDS_MAX as i64) as i16
}
