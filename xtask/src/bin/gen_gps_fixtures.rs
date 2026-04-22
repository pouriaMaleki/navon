//! Parse Garmin GPX files in docs/sample-gps/ and emit deterministic
//! stream.jsonl + route.geojson fixtures under parity-fixtures/data/<name>/.
//!
//! Run: cargo run -p xtask --bin gen-gps-fixtures
//!
//! Re-running yields byte-identical output — the generator is the only place
//! that parses GPX so tests can load pre-baked JSON without pulling an XML
//! dependency into every platform.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const EARTH_RADIUS_M: f64 = 6_378_137.0;

fn main() {
    if let Err(error) = run() {
        eprintln!("gen-gps-fixtures error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let workspace = workspace_root()?;
    let source = workspace.join("docs/sample-gps/activity_22605379291.gpx");
    let output_dir = workspace.join("parity-fixtures/data/helsinki-gravel");

    let gpx_bytes = fs::read(&source)
        .map_err(|error| format!("failed to read {}: {error}", source.display()))?;
    let gpx_text = std::str::from_utf8(&gpx_bytes)
        .map_err(|error| format!("gpx is not utf-8: {error}"))?;

    let track = parse_gpx_track(gpx_text)?;
    if track.len() < 2 {
        return Err(format!("track has {} points, need at least 2", track.len()));
    }

    let samples = compute_samples(&track);
    let route = decimate_for_route(&track, 150);

    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("failed to create {}: {error}", output_dir.display()))?;

    write_stream_jsonl(&output_dir.join("stream.jsonl"), &samples)?;
    write_route_geojson(&output_dir.join("route.geojson"), &route)?;
    write_scenarios_toml(&output_dir.join("scenarios.toml"), &samples)?;
    write_readme(&output_dir.join("README.md"))?;

    println!(
        "wrote {} samples, {} route points to {}",
        samples.len(),
        route.len(),
        output_dir.display()
    );
    Ok(())
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest = env::var_os("CARGO_MANIFEST_DIR")
        .ok_or("CARGO_MANIFEST_DIR is not set".to_owned())?;
    Path::new(&manifest)
        .parent()
        .map(Path::to_path_buf)
        .ok_or("xtask must live inside the workspace".to_owned())
}

#[derive(Debug, Clone, Copy)]
struct TrackPoint {
    lat_deg: f64,
    lon_deg: f64,
    time_ms: i64,
}

#[derive(Debug, Clone)]
struct GpsSampleOut {
    lat_deg: f64,
    lon_deg: f64,
    speed_mps: f64,
    course_rad: f64,
    horizontal_accuracy_m: f64,
    time_offset_ms: i64,
}

fn parse_gpx_track(xml: &str) -> Result<Vec<TrackPoint>, String> {
    let mut points = Vec::new();
    let mut cursor = 0;
    while let Some(start) = xml[cursor..].find("<trkpt") {
        let abs_start = cursor + start;
        let tag_close = xml[abs_start..]
            .find('>')
            .ok_or("malformed trkpt tag")?;
        let tag = &xml[abs_start..abs_start + tag_close];
        let lat = attr_value(tag, "lat")?;
        let lon = attr_value(tag, "lon")?;
        let end_tag = xml[abs_start..]
            .find("</trkpt>")
            .ok_or("unterminated trkpt")?;
        let inner = &xml[abs_start + tag_close + 1..abs_start + end_tag];
        let time = extract_text(inner, "<time>", "</time>")
            .ok_or("trkpt missing <time>")?;
        points.push(TrackPoint {
            lat_deg: lat
                .parse::<f64>()
                .map_err(|e| format!("bad lat '{lat}': {e}"))?,
            lon_deg: lon
                .parse::<f64>()
                .map_err(|e| format!("bad lon '{lon}': {e}"))?,
            time_ms: parse_iso8601_ms(time)?,
        });
        cursor = abs_start + end_tag + "</trkpt>".len();
    }
    Ok(points)
}

fn attr_value(tag: &str, name: &str) -> Result<String, String> {
    let key = format!("{name}=\"");
    let start = tag
        .find(&key)
        .ok_or_else(|| format!("attribute {name} not found in {tag}"))?;
    let after = &tag[start + key.len()..];
    let end = after
        .find('"')
        .ok_or_else(|| format!("unterminated attribute {name}"))?;
    Ok(after[..end].to_owned())
}

fn extract_text<'a>(source: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = source.find(open)?;
    let after = &source[start + open.len()..];
    let end = after.find(close)?;
    Some(&after[..end])
}

fn parse_iso8601_ms(text: &str) -> Result<i64, String> {
    // Expect: YYYY-MM-DDTHH:MM:SS(.sss)?Z
    let trimmed = text.trim();
    if !trimmed.ends_with('Z') {
        return Err(format!("timestamp not in UTC: {trimmed}"));
    }
    let body = &trimmed[..trimmed.len() - 1];
    let (date_part, time_part) = body
        .split_once('T')
        .ok_or_else(|| format!("missing T separator: {trimmed}"))?;
    let mut date_fields = date_part.split('-');
    let year: i64 = parse_field(&mut date_fields, "year")?;
    let month: i64 = parse_field(&mut date_fields, "month")?;
    let day: i64 = parse_field(&mut date_fields, "day")?;

    let (hms, frac_ms) = match time_part.split_once('.') {
        Some((h, frac)) => (h, parse_fraction_ms(frac)?),
        None => (time_part, 0_i64),
    };
    let mut time_fields = hms.split(':');
    let hour: i64 = parse_field(&mut time_fields, "hour")?;
    let minute: i64 = parse_field(&mut time_fields, "minute")?;
    let second: i64 = parse_field(&mut time_fields, "second")?;

    let days_since_epoch = civil_to_days(year, month, day);
    Ok(days_since_epoch * 86_400_000
        + hour * 3_600_000
        + minute * 60_000
        + second * 1_000
        + frac_ms)
}

fn parse_field<'a, I: Iterator<Item = &'a str>>(it: &mut I, name: &str) -> Result<i64, String> {
    let raw = it
        .next()
        .ok_or_else(|| format!("timestamp missing field {name}"))?;
    raw.parse::<i64>()
        .map_err(|e| format!("bad {name} '{raw}': {e}"))
}

fn parse_fraction_ms(frac: &str) -> Result<i64, String> {
    let digits: String = frac.chars().take(3).collect();
    let padded = format!("{:0<3}", digits);
    padded
        .parse::<i64>()
        .map_err(|e| format!("bad fractional seconds '{frac}': {e}"))
}

fn civil_to_days(y: i64, m: i64, d: i64) -> i64 {
    // Howard Hinnant's civil-from-days algorithm, inverted.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * if m > 2 { m - 3 } else { m + 9 } + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn compute_samples(points: &[TrackPoint]) -> Vec<GpsSampleOut> {
    let mut out = Vec::with_capacity(points.len());
    let base = points[0].time_ms;
    for (index, point) in points.iter().enumerate() {
        let (speed_mps, course_rad) = if index == 0 {
            (0.0, 0.0)
        } else {
            let previous = &points[index - 1];
            let distance = haversine_distance_m(
                previous.lat_deg,
                previous.lon_deg,
                point.lat_deg,
                point.lon_deg,
            );
            let dt_s = ((point.time_ms - previous.time_ms) as f64 / 1000.0).max(0.001);
            let speed = distance / dt_s;
            let course = bearing_rad(
                previous.lat_deg,
                previous.lon_deg,
                point.lat_deg,
                point.lon_deg,
            );
            (speed, course)
        };
        out.push(GpsSampleOut {
            lat_deg: point.lat_deg,
            lon_deg: point.lon_deg,
            speed_mps,
            course_rad,
            horizontal_accuracy_m: 5.0,
            time_offset_ms: point.time_ms - base,
        });
    }
    out
}

fn haversine_distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let to_rad = std::f64::consts::PI / 180.0;
    let dlat = (lat2 - lat1) * to_rad;
    let dlon = (lon2 - lon1) * to_rad;
    let a = (dlat / 2.0).sin().powi(2)
        + (lat1 * to_rad).cos() * (lat2 * to_rad).cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_M * c
}

fn bearing_rad(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let to_rad = std::f64::consts::PI / 180.0;
    let (phi1, phi2) = (lat1 * to_rad, lat2 * to_rad);
    let dlon = (lon2 - lon1) * to_rad;
    let y = dlon.sin() * phi2.cos();
    let x = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * dlon.cos();
    y.atan2(x)
}

fn decimate_for_route(points: &[TrackPoint], target_count: usize) -> Vec<TrackPoint> {
    if points.len() <= target_count {
        return points.to_vec();
    }
    let stride = points.len() as f64 / target_count as f64;
    let mut out = Vec::with_capacity(target_count + 1);
    let mut accum = 0.0;
    for (index, point) in points.iter().enumerate() {
        if index == 0 || index + 1 == points.len() {
            out.push(*point);
            continue;
        }
        accum += 1.0;
        if accum >= stride {
            out.push(*point);
            accum -= stride;
        }
    }
    out
}

fn write_stream_jsonl(path: &Path, samples: &[GpsSampleOut]) -> Result<(), String> {
    let mut text = String::with_capacity(samples.len() * 160);
    for sample in samples {
        text.push_str(&format!(
            "{{\"lat_deg\":{},\"lon_deg\":{},\"speed_mps\":{},\"course_rad\":{},\"accuracy_m\":{},\"t_ms\":{}}}\n",
            fmt_f(sample.lat_deg, 8),
            fmt_f(sample.lon_deg, 8),
            fmt_f(sample.speed_mps, 4),
            fmt_f(sample.course_rad, 4),
            fmt_f(sample.horizontal_accuracy_m, 1),
            sample.time_offset_ms,
        ));
    }
    fs::write(path, text).map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn write_route_geojson(path: &Path, points: &[TrackPoint]) -> Result<(), String> {
    let mut coords = String::new();
    for (index, point) in points.iter().enumerate() {
        if index > 0 {
            coords.push(',');
        }
        coords.push_str(&format!(
            "[{},{}]",
            fmt_f(point.lon_deg, 8),
            fmt_f(point.lat_deg, 8)
        ));
    }
    let json = format!(
        "{{\"type\":\"Feature\",\"properties\":{{\"name\":\"helsinki-gravel\"}},\"geometry\":{{\"type\":\"LineString\",\"coordinates\":[{coords}]}}}}\n",
    );
    fs::write(path, json).map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn write_scenarios_toml(path: &Path, samples: &[GpsSampleOut]) -> Result<(), String> {
    // Pick representative slices by scanning the stream for characteristic
    // patterns — start-of-ride, steady-motion, stop-at-lights, end-of-ride.
    let total_ms = samples.last().map(|s| s.time_offset_ms).unwrap_or(0);
    let start_moving = find_first_above(samples, 2.0).unwrap_or(0);
    let first_stop = find_first_stop(samples, start_moving).unwrap_or(start_moving + 30);
    let end_ms = total_ms;

    let text = format!(
        "# Named time slices into stream.jsonl. Offsets are ms from the first fix.\n\
\n\
[stationary_with_gps_noise]\n\
start_ms = 0\n\
end_ms = {start_ms}\n\
expect = \"mode stays stationary, camera centered, speed hidden\"\n\
\n\
[start_moving_then_stop]\n\
start_ms = {start_ms}\n\
end_ms = {stop_ms}\n\
expect = \"mode transitions stationary -> moving, speed becomes visible\"\n\
\n\
[on_route_whole_ride]\n\
start_ms = 0\n\
end_ms = {end_ms}\n\
expect = \"full ride playback; next-turn distances trend-monotonic\"\n\
\n\
[gps_dropout]\n\
start_ms = {start_ms}\n\
end_ms = {start_ms_plus_5s}\n\
# Note: scenarios that simulate dropout do so by zeroing emission in the loader,\n\
# not in the fixture itself. This slice is the reference window.\n\
expect = \"camera holds, overlay marks stale after gap\"\n\
",
        start_ms = samples[start_moving].time_offset_ms,
        stop_ms = samples[first_stop].time_offset_ms,
        end_ms = end_ms,
        start_ms_plus_5s = samples[start_moving].time_offset_ms + 5_000,
    );
    fs::write(path, text).map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn find_first_above(samples: &[GpsSampleOut], speed_mps: f64) -> Option<usize> {
    samples.iter().position(|s| s.speed_mps > speed_mps)
}

fn find_first_stop(samples: &[GpsSampleOut], after: usize) -> Option<usize> {
    // Look for a dwell where speed < 0.5 mps for 3+ consecutive samples.
    let mut run = 0;
    for (index, sample) in samples.iter().enumerate().skip(after) {
        if sample.speed_mps < 0.5 {
            run += 1;
            if run >= 3 {
                return Some(index);
            }
        } else {
            run = 0;
        }
    }
    None
}

fn write_readme(path: &Path) -> Result<(), String> {
    let text = "# helsinki-gravel fixture\n\n\
Generated from docs/sample-gps/activity_22605379291.gpx by xtask's gen-gps-fixtures binary.\n\n\
- stream.jsonl — one GPS sample per line; all platforms replay this file.\n\
- route.geojson — decimated polyline used as the baked route plan.\n\
- scenarios.toml — named time slices for scenario-based tests.\n\n\
Re-running the generator produces byte-identical output. Do not hand-edit.\n";
    fs::write(path, text).map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn fmt_f(value: f64, decimals: usize) -> String {
    // Deterministic formatting — strip trailing zeros beyond the first decimal so
    // regeneration is stable across locales (we don't use locale-aware formatters).
    let rounded = format!("{value:.*}", decimals);
    if !rounded.contains('.') {
        return rounded;
    }
    let trimmed = rounded.trim_end_matches('0');
    let trimmed = trimmed.trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}
