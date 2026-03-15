use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
enum Shape {
    Circle { cx: f32, cy: f32, r: f32 },
    Polygon { points: Vec<(f32, f32)> },
}

#[derive(Debug, Clone)]
struct SvgAsset {
    width: u32,
    height: u32,
    shapes: Vec<Shape>,
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let asset_dir = manifest_dir.join("assets/overlay");
    let out_path =
        PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("overlay_assets.rs");
    let assets = [
        (
            "NORTH_INDICATOR_BASE",
            asset_dir.join("north_indicator_base.svg"),
        ),
        (
            "NORTH_INDICATOR_NEEDLE",
            asset_dir.join("north_indicator_needle.svg"),
        ),
        (
            "RIDER_MARKER_RIDING",
            asset_dir.join("rider_marker_riding.svg"),
        ),
        (
            "RIDER_MARKER_STOPPED",
            asset_dir.join("rider_marker_stopped.svg"),
        ),
    ];

    let mut generated = String::new();
    generated.push_str("use crate::raster::AlphaMask;\n\n");
    for (symbol, path) in assets {
        println!("cargo:rerun-if-changed={}", path.display());
        let asset = parse_svg(&path);
        let pixels = rasterize_asset(&asset);
        let pixel_symbol = format!("{symbol}_PIXELS");
        write!(
            generated,
            "pub const {pixel_symbol}: &[u8] = &[{}];\npub const {symbol}: AlphaMask = AlphaMask::new({}, {}, {pixel_symbol});\n\n",
            pixels.iter().map(u8::to_string).collect::<Vec<_>>().join(", "),
            asset.width,
            asset.height,
        )
        .expect("generate asset code");
    }

    fs::write(out_path, generated).expect("write overlay assets");
}

fn parse_svg(path: &Path) -> SvgAsset {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed reading {}: {error}", path.display()));
    let svg_tag =
        find_tag(&text, "svg").unwrap_or_else(|| panic!("missing <svg> in {}", path.display()));
    let view_box = parse_attr(&svg_tag, "viewBox")
        .unwrap_or_else(|| panic!("missing viewBox in {}", path.display()));
    let mut view_box_parts = view_box.split_whitespace();
    let _min_x = view_box_parts
        .next()
        .and_then(|v| v.parse::<f32>().ok())
        .expect("viewBox min x");
    let _min_y = view_box_parts
        .next()
        .and_then(|v| v.parse::<f32>().ok())
        .expect("viewBox min y");
    let width = view_box_parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .expect("viewBox width");
    let height = view_box_parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .expect("viewBox height");

    let mut shapes = Vec::new();
    for tag in find_tags(&text, "circle") {
        let cx = parse_attr(&tag, "cx")
            .and_then(|v| v.parse::<f32>().ok())
            .expect("circle cx");
        let cy = parse_attr(&tag, "cy")
            .and_then(|v| v.parse::<f32>().ok())
            .expect("circle cy");
        let r = parse_attr(&tag, "r")
            .and_then(|v| v.parse::<f32>().ok())
            .expect("circle r");
        shapes.push(Shape::Circle { cx, cy, r });
    }
    for tag in find_tags(&text, "polygon") {
        let points = parse_attr(&tag, "points")
            .unwrap_or_else(|| panic!("polygon points missing in {}", path.display()));
        let parsed_points = points
            .split_whitespace()
            .map(|pair| {
                let (x, y) = pair.split_once(',').expect("polygon point pair");
                (
                    x.parse::<f32>().expect("polygon x"),
                    y.parse::<f32>().expect("polygon y"),
                )
            })
            .collect::<Vec<_>>();
        shapes.push(Shape::Polygon {
            points: parsed_points,
        });
    }

    SvgAsset {
        width,
        height,
        shapes,
    }
}

fn rasterize_asset(asset: &SvgAsset) -> Vec<u8> {
    let mut pixels = vec![0_u8; asset.width as usize * asset.height as usize];
    let sample_offsets = [
        (0.25_f32, 0.25_f32),
        (0.75, 0.25),
        (0.25, 0.75),
        (0.75, 0.75),
    ];

    for y in 0..asset.height {
        for x in 0..asset.width {
            let coverage = sample_offsets
                .iter()
                .filter(|(sx, sy)| {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;
                    asset
                        .shapes
                        .iter()
                        .any(|shape| shape_contains(shape, px, py))
                })
                .count() as u8;
            pixels[y as usize * asset.width as usize + x as usize] =
                ((u16::from(coverage) * 255) / 4) as u8;
        }
    }

    pixels
}

fn shape_contains(shape: &Shape, px: f32, py: f32) -> bool {
    match shape {
        Shape::Circle { cx, cy, r } => {
            let dx = px - cx;
            let dy = py - cy;
            (dx * dx) + (dy * dy) <= r * r
        }
        Shape::Polygon { points } => point_in_polygon(points, px, py),
    }
}

fn point_in_polygon(points: &[(f32, f32)], px: f32, py: f32) -> bool {
    let mut inside = false;
    let mut previous = *points.last().expect("polygon points");
    for &current in points {
        let intersects = ((current.1 > py) != (previous.1 > py))
            && (px
                < (previous.0 - current.0) * (py - current.1)
                    / ((previous.1 - current.1).max(f32::EPSILON))
                    + current.0);
        if intersects {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

fn find_tag(text: &str, tag_name: &str) -> Option<String> {
    find_tags(text, tag_name).into_iter().next()
}

fn find_tags(text: &str, tag_name: &str) -> Vec<String> {
    let needle = format!("<{tag_name}");
    let mut tags = Vec::new();
    let mut search = text;
    while let Some(start) = search.find(&needle) {
        let after_start = &search[start..];
        let Some(end) = after_start.find('>') else {
            break;
        };
        tags.push(after_start[..=end].to_owned());
        search = &after_start[end + 1..];
    }
    tags
}

fn parse_attr(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let remainder = &tag[start..];
    let end = remainder.find('"')?;
    Some(remainder[..end].to_owned())
}
