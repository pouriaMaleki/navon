use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const MAP_SOURCE_DIR: &str = "map-src";
const MAP_DATA_DIR: &str = "map-data";
const STANDARD_MAP_FILE: &str = "map-data/city.svm";
const GENERATED_MAP_RS: &str = "render-core-wasm/src/generated_map.rs";
const DEVICE_BUNDLE_DIR: &str = "device-bundle";
const FIRMWARE_ELF: &str = "target/xtensa-esp32-none-elf/debug/esp32-hello";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        return Err("usage: cargo run -p xtask -- <emu|prepare-map|bundle-device|deploy-device> [--release] [--port <tty>]".to_owned());
    };

    match cmd.as_str() {
        "emu" => {
            let release = args.any(|a| a == "--release");
            run_emu(release)
        }
        "prepare-map" => run_prepare_map(),
        "bundle-device" => run_bundle_device(),
        "deploy-device" => run_deploy_device(args.collect()),
        _ => Err(format!("unknown command: {cmd}")),
    }
}

fn run_emu(release: bool) -> Result<(), String> {
    let web_dir = Path::new("emulator/web");
    if !web_dir.exists() {
        return Err("emulator/web not found (run from repository root)".to_owned());
    }

    run_prepare_map()?;

    let mut wasm_pack = Command::new("wasm-pack");
    wasm_pack
        .arg("build")
        .arg("render-core-wasm")
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg("../emulator/web/wasm-pkg")
        .arg(if release { "--release" } else { "--dev" });
    run_cmd(wasm_pack, "wasm-pack build failed")?;

    let npm = pick_package_manager();
    run_cmd(
        cmd_in(&npm, &["install"], web_dir),
        "package install failed",
    )?;
    let script = if release { "preview" } else { "dev" };
    run_cmd(
        cmd_in(&npm, &["run", script], web_dir),
        &format!("failed to run npm script {script}"),
    )
}

fn run_prepare_map() -> Result<(), String> {
    fs::create_dir_all(MAP_SOURCE_DIR).map_err(|e| format!("failed to ensure {MAP_SOURCE_DIR}: {e}"))?;
    fs::create_dir_all(MAP_DATA_DIR).map_err(|e| format!("failed to ensure {MAP_DATA_DIR}: {e}"))?;

    if let Some(mbtiles) = discover_mbtiles(Path::new(MAP_SOURCE_DIR))? {
        let mut convert = Command::new("cargo");
        convert
            .arg("run")
            .arg("-p")
            .arg("map-vector-cli")
            .arg("--")
            .arg("convert-mbtiles")
            .arg("--input")
            .arg(&mbtiles)
            .arg("--output")
            .arg(STANDARD_MAP_FILE)
            .arg("--target-zoom")
            .arg("16");
        run_cmd(convert, "city map conversion failed")?;

        let mut emit = Command::new("cargo");
        emit.arg("run")
            .arg("-p")
            .arg("map-vector-cli")
            .arg("--")
            .arg("emit-rust-window")
            .arg("--input")
            .arg(STANDARD_MAP_FILE)
            .arg("--output")
            .arg(GENERATED_MAP_RS)
            .arg("--center-lat")
            .arg("60.17442")
            .arg("--center-lon")
            .arg("24.94210")
            .arg("--player-lat")
            .arg("60.173851")
            .arg("--player-lon")
            .arg("24.937951")
            .arg("--view-tiles")
            .arg("3.0");
        run_cmd(emit, "window map rust generation failed")?;

        eprintln!("xtask: map prepared from {}", mbtiles.display());
    } else {
        write_generated_empty()?;
        eprintln!("xtask: no *.mbtiles found in /work/{MAP_SOURCE_DIR}; using empty generated map");
    }
    Ok(())
}

fn run_bundle_device() -> Result<(), String> {
    run_prepare_map()?;

    let mut build = Command::new("bash");
    build
        .arg("-lc")
        .arg(". $HOME/export-esp.sh && cargo build -p esp32-hello");
    run_cmd(build, "firmware build failed")?;

    fs::create_dir_all(DEVICE_BUNDLE_DIR)
        .map_err(|e| format!("failed to create {DEVICE_BUNDLE_DIR}: {e}"))?;

    let fw_dest = Path::new(DEVICE_BUNDLE_DIR).join("firmware.elf");
    fs::copy(FIRMWARE_ELF, &fw_dest)
        .map_err(|e| format!("failed to copy firmware to bundle: {e}"))?;

    let map_dest = Path::new(DEVICE_BUNDLE_DIR).join("city.svm");
    fs::copy(STANDARD_MAP_FILE, &map_dest)
        .map_err(|e| format!("failed to copy map to bundle: {e}"))?;

    eprintln!(
        "xtask: device bundle ready at {} (firmware.elf + city.svm)",
        DEVICE_BUNDLE_DIR
    );
    Ok(())
}

fn run_deploy_device(args: Vec<String>) -> Result<(), String> {
    run_bundle_device()?;

    let mut port: Option<String> = None;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                port = args.get(i).cloned();
            }
            other => return Err(format!("unknown deploy arg: {other}")),
        }
        i += 1;
    }

    let port = port.ok_or("deploy-device requires --port <tty>")?;

    if !command_exists("espflash") {
        return Err("espflash is not installed; install espflash to use deploy-device".to_owned());
    }

    let mut flash = Command::new("espflash");
    flash
        .arg("flash")
        .arg("--monitor")
        .arg("--port")
        .arg(port)
        .arg(FIRMWARE_ELF);
    run_cmd(flash, "espflash deploy failed")
}

fn discover_mbtiles(dir: &Path) -> Result<Option<PathBuf>, String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|e| format!("failed to read {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "mbtiles"))
        .collect::<Vec<_>>();
    entries.sort();
    Ok(entries.into_iter().next())
}

fn write_generated_empty() -> Result<(), String> {
    let out = r#"use esp32_screen_render_core::{Line, WorldBounds};

pub const HAS_MAP: bool = false;
pub const MAP_SOURCE: &str = "none";
pub const MAP_ZOOM: i32 = 0;
pub const MAP_CENTER_LAT: f64 = 0.0;
pub const MAP_CENTER_LON: f64 = 0.0;
pub const MAP_BOUNDS: WorldBounds = WorldBounds { min_x: 0, max_x: 10000, min_y: 0, max_y: 10000 };
pub const MAP_PLAYER: esp32_screen_render_core::WorldPoint = esp32_screen_render_core::WorldPoint { x: 5000, y: 5000 };
pub const MAP_LINES: &[Line] = &[];
"#;
    fs::write(GENERATED_MAP_RS, out).map_err(|e| format!("failed writing empty map: {e}"))
}

fn pick_package_manager() -> String {
    for bin in ["bun", "pnpm", "npm"] {
        if Command::new("sh")
            .arg("-lc")
            .arg(format!("command -v {bin} >/dev/null 2>&1"))
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return bin.to_owned();
        }
    }
    "npm".to_owned()
}

fn command_exists(bin: &str) -> bool {
    Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {bin} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn cmd_in(bin: &str, args: &[&str], cwd: &Path) -> Command {
    let mut cmd = Command::new(bin);
    cmd.current_dir(cwd);
    cmd.args(args);
    cmd
}

fn run_cmd(mut cmd: Command, err: &str) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("{err}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{err}: exit status {status}"))
    }
}
