use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use resvg::{tiny_skia, usvg};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let asset_dir = manifest_dir.join("assets/overlay");
    let poi_asset_dir = asset_dir.join("poi");
    let out_path =
        PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("overlay_assets.rs");
    let assets = [
        (
            "NORTH_INDICATOR_BASE",
            asset_dir.join("north_indicator_base.svg"),
        ),
        (
            "NORTH_INDICATOR_LOCKED_BASE",
            asset_dir.join("north_indicator_locked_base.svg"),
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
        ("POI_BIKE_PARKING", poi_asset_dir.join("bike-lock.svg")),
        ("POI_BIKE_REPAIR", poi_asset_dir.join("bike-repair.svg")),
        ("POI_SUPERMARKET", poi_asset_dir.join("grocery.svg")),
        ("POI_RESTAURANT", poi_asset_dir.join("restaurant.svg")),
        ("POI_CAFE", poi_asset_dir.join("restaurant.svg")),
        ("POI_WATER", poi_asset_dir.join("water.svg")),
        ("POI_WC", poi_asset_dir.join("wc.svg")),
    ];

    let mut generated = String::new();
    generated.push_str("use crate::raster::AlphaMask;\n\n");
    for (symbol, path) in assets {
        println!("cargo:rerun-if-changed={}", path.display());
        let asset = rasterize_svg(&path);
        let pixel_symbol = format!("{symbol}_PIXELS");
        write!(
            generated,
            "pub const {pixel_symbol}: &[u8] = &[{}];\npub const {symbol}: AlphaMask = AlphaMask::new({}, {}, {pixel_symbol});\n\n",
            asset
                .pixels
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            asset.width,
            asset.height,
        )
        .expect("generate asset code");
    }

    fs::write(out_path, generated).expect("write overlay assets");
}

#[derive(Debug)]
struct RasterizedAsset {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn rasterize_svg(path: &Path) -> RasterizedAsset {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed reading {}: {error}", path.display()));
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_str(&text, &options)
        .unwrap_or_else(|error| panic!("failed parsing {}: {error}", path.display()));
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .unwrap_or_else(|| panic!("failed allocating pixmap for {}", path.display()));
    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap_mut);

    let pixels = pixmap
        .data()
        .chunks_exact(4)
        .map(|rgba| rgba[3])
        .collect::<Vec<_>>();

    RasterizedAsset {
        width: size.width(),
        height: size.height(),
        pixels,
    }
}
