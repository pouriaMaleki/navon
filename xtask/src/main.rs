use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

const MAP_SOURCE_DIR: &str = "map-src";
const MAP_DATA_DIR: &str = "map-data";
const STANDARD_MAP_FILE: &str = "map-data/city.svm";
const PREPARE_MAP_STAMP_FILE: &str = "map-data/.prepare-map.stamp";
const GENERATED_MAP_RS: &str = "render-core-wasm/src/generated_map.rs";
const DEVICE_BUNDLE_DIR: &str = "device-bundle";
const FIRMWARE_ELF: &str = "target/xtensa-esp32-none-elf/debug/esp32-hello";
const MAP_TARGET_ZOOM: i32 = 16;
const MAP_PROFILE: &str = "bike";
const MAP_CENTER_LAT: &str = "60.17442";
const MAP_CENTER_LON: &str = "24.94210";
const MAP_PLAYER_LAT: &str = "60.173851";
const MAP_PLAYER_LON: &str = "24.937951";
const MAP_VIEW_TILES: &str = "20.0";

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
    if !command_exists("wasm-pack") {
        return Err(
            "wasm-pack is not installed; install with `cargo install wasm-pack` or rerun devcontainer post-create".to_owned(),
        );
    }

    run_prepare_map()?;

    let mut wasm_pack = Command::new("wasm-pack");
    wasm_pack
        .env("CARGO_HOME", "/usr/local/cargo")
        .arg("build")
        .arg("render-core-wasm")
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg("../emulator/web/wasm-pkg")
        .arg(if release { "--release" } else { "--dev" });
    run_cmd(wasm_pack, "wasm-pack build failed")?;

    let npm = pick_package_manager()?;
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
    fs::create_dir_all(MAP_SOURCE_DIR)
        .map_err(|e| format!("failed to ensure {MAP_SOURCE_DIR}: {e}"))?;
    fs::create_dir_all(MAP_DATA_DIR)
        .map_err(|e| format!("failed to ensure {MAP_DATA_DIR}: {e}"))?;

    if let Some(mbtiles) = discover_mbtiles(Path::new(MAP_SOURCE_DIR))? {
        if should_skip_prepare_map(&mbtiles)? {
            eprintln!("xtask: map unchanged; skipping prepare-map");
            return Ok(());
        }

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
            .arg(MAP_TARGET_ZOOM.to_string())
            .arg("--profile")
            .arg(MAP_PROFILE);
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
            .arg(MAP_CENTER_LAT)
            .arg("--center-lon")
            .arg(MAP_CENTER_LON)
            .arg("--player-lat")
            .arg(MAP_PLAYER_LAT)
            .arg("--player-lon")
            .arg(MAP_PLAYER_LON)
            .arg("--view-tiles")
            .arg(MAP_VIEW_TILES);
        run_cmd(emit, "window map rust generation failed")?;
        write_prepare_map_stamp(&mbtiles)?;

        eprintln!("xtask: map prepared from {}", mbtiles.display());
    } else {
        write_generated_empty()?;
        clear_prepare_map_stamp()?;
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
pub const MAP_WORLD_MIN_X: i32 = 0;
pub const MAP_WORLD_MAX_X: i32 = 1;
pub const MAP_WORLD_MIN_Y: i32 = 0;
pub const MAP_WORLD_MAX_Y: i32 = 1;
pub const MAP_BOUNDS: WorldBounds = WorldBounds { min_x: 0, max_x: 10000, min_y: 0, max_y: 10000 };
pub const MAP_PLAYER: esp32_screen_render_core::WorldPoint = esp32_screen_render_core::WorldPoint { x: 5000, y: 5000 };
pub const MAP_LINES: &[Line] = &[];
"#;
    fs::write(GENERATED_MAP_RS, out).map_err(|e| format!("failed writing empty map: {e}"))
}

fn should_skip_prepare_map(mbtiles: &Path) -> Result<bool, String> {
    if !Path::new(STANDARD_MAP_FILE).is_file() || !Path::new(GENERATED_MAP_RS).is_file() {
        return Ok(false);
    }

    let stamp_path = Path::new(PREPARE_MAP_STAMP_FILE);
    let old = match fs::read_to_string(stamp_path) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };
    let now = prepare_map_fingerprint(mbtiles)?;
    Ok(old == now)
}

fn write_prepare_map_stamp(mbtiles: &Path) -> Result<(), String> {
    let fingerprint = prepare_map_fingerprint(mbtiles)?;
    fs::write(PREPARE_MAP_STAMP_FILE, fingerprint)
        .map_err(|e| format!("failed writing {PREPARE_MAP_STAMP_FILE}: {e}"))
}

fn clear_prepare_map_stamp() -> Result<(), String> {
    match fs::remove_file(PREPARE_MAP_STAMP_FILE) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("failed clearing {PREPARE_MAP_STAMP_FILE}: {e}")),
    }
}

fn prepare_map_fingerprint(mbtiles: &Path) -> Result<String, String> {
    let meta = fs::metadata(mbtiles).map_err(|e| {
        format!(
            "failed reading map source metadata {}: {e}",
            mbtiles.display()
        )
    })?;
    let len = meta.len();
    let modified_ns = system_time_to_unix_nanos(meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));

    Ok(format!(
        "v1\nsource={}\nsize={len}\nmodified_ns={modified_ns}\ntarget_zoom={MAP_TARGET_ZOOM}\nprofile={MAP_PROFILE}\ncenter_lat={MAP_CENTER_LAT}\ncenter_lon={MAP_CENTER_LON}\nplayer_lat={MAP_PLAYER_LAT}\nplayer_lon={MAP_PLAYER_LON}\nview_tiles={MAP_VIEW_TILES}\n",
        mbtiles.display()
    ))
}

fn system_time_to_unix_nanos(t: SystemTime) -> u128 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn pick_package_manager() -> Result<String, String> {
    if let Some(pm) = detect_package_manager() {
        return Ok(pm);
    }

    eprintln!("xtask: no JS package manager found; attempting local bun bootstrap...");
    install_bun_fallback()?;

    if let Some(pm) = detect_package_manager() {
        return Ok(pm);
    }

    Err(package_manager_fix_hint())
}

fn detect_package_manager() -> Option<String> {
    // Canonical preference order: npm, then pnpm, then bun.
    for bin in ["npm", "pnpm", "bun"] {
        if let Some(path) = command_path(bin) {
            return Some(path);
        }
    }

    let home = env::var("HOME").ok()?;
    let local_bun = Path::new(&home).join(".bun/bin/bun");
    if local_bun.is_file() {
        return Some(local_bun.to_string_lossy().into_owned());
    }
    None
}

fn command_path(bin: &str) -> Option<String> {
    let out = Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {bin}"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if path.is_empty() { None } else { Some(path) }
}

fn install_bun_fallback() -> Result<(), String> {
    let home = env::var("HOME").map_err(|_| "HOME is not set; cannot bootstrap bun".to_owned())?;
    let bun_install = format!("{home}/.bun");

    let mut cmd = Command::new("sh");
    cmd.arg("-lc")
        .arg("curl -fsSL https://bun.sh/install | bash")
        .env("BUN_INSTALL", bun_install)
        .env("CARGO_HOME", "/usr/local/cargo");
    run_cmd(cmd, "bun bootstrap failed")
}

fn package_manager_fix_hint() -> String {
    "no JS package manager found after bun bootstrap.\n\
install Node.js/npm (recommended) or bun manually:\n\
  - Debian/Ubuntu: sudo apt-get update && sudo apt-get install -y nodejs npm\n\
  - Bun: curl -fsSL https://bun.sh/install | bash\n\
if bun is installed at ~/.bun/bin, add it to PATH:\n\
  export PATH=\"$HOME/.bun/bin:$PATH\""
        .to_owned()
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
