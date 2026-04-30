use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

mod i18n;

fn main() {
    if let Err(error) = run(env::args().skip(1).collect()) {
        eprintln!("xtask error: {error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let workspace = Workspace::new()?;
    match parse_cli(&args)? {
        Cli::Emu { release } => run_emu(&workspace, release),
        Cli::BundleDevice { release } => run_bundle_device(&workspace, release),
        Cli::DeployDevice { port, release } => run_deploy_device(&workspace, &port, release),
        Cli::CheckEspHalP4 => run_check_esp_hal_p4(),
        Cli::BuildC6Slave => run_build_c6_slave(&workspace),
        Cli::I18n { args } => i18n::run(&args, &workspace.root),
        Cli::Stub { command } => Err(format!(
            "{command} is not implemented yet; available commands: prepare-map, emu, bundle-device, deploy-device, check-esp-hal-p4, build-c6-slave, i18n-gen, i18n-sync"
        )),
        Cli::Help => {
            print_help();
            Ok(())
        }
    }
}

fn run_emu(workspace: &Workspace, release: bool) -> Result<(), String> {
    ensure_tool("wasm-pack")?;
    ensure_tool("npm")?;
    workspace.ensure_emu_prerequisites()?;

    run_command(CommandSpec::wasm_pack_build(workspace, release))?;
    run_command(CommandSpec::npm_dev(workspace))
}

fn run_bundle_device(workspace: &Workspace, release: bool) -> Result<(), String> {
    ensure_tool("ldproxy")?;
    ensure_tool("espflash")?;
    workspace.ensure_device_prerequisites()?;

    // esp-idf-sys is the crate that drives the CMake/Ninja compile of the
    // C side (ESP-IDF + our local components). Cargo doesn't track our
    // local component sources as inputs to esp-idf-sys, so an edit to e.g.
    // `firmware/components/hosted_ble/hosted_ble.c` quietly links against
    // the stale object from a previous build. Detect when any file under
    // `firmware/components/` is newer than esp-idf-sys's last invocation
    // timestamp and force-clean esp-idf-sys so the next cargo invocation
    // re-runs CMake.
    invalidate_esp_idf_sys_if_components_changed(workspace, release)?;

    run_command(CommandSpec::cargo_build_firmware(workspace, release))?;
    run_command(CommandSpec::espflash_save_image(workspace, release))?;

    // Truncate the full image to MAP_PARTITION_FLASH_OFFSET bytes so that
    // write-bin at 0x0 stops before the map_data partition. espflash fails
    // to write 32 MB in one shot (stub timeout); two separate writes work.
    let app_image = workspace.device_app_image_path(release);
    {
        use std::io::Read as _;
        let mut src = std::fs::File::open(workspace.device_image_path(release))
            .map_err(|e| format!("failed to open image for app copy: {e}"))?;
        let mut buf = vec![0u8; MAP_PARTITION_FLASH_OFFSET];
        src.read_exact(&mut buf)
            .map_err(|e| format!("image shorter than map offset 0x{MAP_PARTITION_FLASH_OFFSET:x}: {e}"))?;
        std::fs::write(&app_image, &buf)
            .map_err(|e| format!("failed to write app image: {e}"))?;
    }

    let app = workspace.device_app_image_path(release);
    let map = &workspace.device_map_data;
    let app_mb = std::fs::metadata(&app).map(|m| m.len()).unwrap_or(0) / 1_048_576;
    let map_mb = std::fs::metadata(map).map(|m| m.len()).unwrap_or(0) / 1_048_576;
    println!(
        "\ndevice image ready.\n\
         \nOPTION A: load map from SD card (carry a larger map without reflashing).\n\
         Format an SD card as FAT32, copy any svm file to the card root as `map.svm`,\n\
         insert it, and flash only the firmware:\n  \
         espflash write-bin --chip esp32p4 --port <PORT> 0x0 {}\n  \
         (then power-cycle; firmware reads /sdcard/map.svm read-only at boot)\n\
         \nOPTION B: bundle map into flash (no SD card needed).\n  \
         # Step 1 — firmware (~{} MB):\n  \
         espflash write-bin --chip esp32p4 --port <PORT> 0x0 {}\n  \
         # Step 2 — map (~{} MB):\n  \
         espflash write-bin --chip esp32p4 --port <PORT> 0x{MAP_PARTITION_FLASH_OFFSET:x} {}",
        app.display(),
        app_mb,
        app.display(),
        map_mb,
        map.display(),
    );
    Ok(())
}

/// Flash offset of the `map_data` partition defined in firmware/partitions.csv.
const MAP_PARTITION_FLASH_OFFSET: usize = 0x400000;


fn run_deploy_device(workspace: &Workspace, port: &str, release: bool) -> Result<(), String> {
    ensure_tool("ldproxy")?;
    ensure_tool("espflash")?;
    workspace.ensure_device_prerequisites()?;

    run_command(CommandSpec::cargo_build_firmware(workspace, release))?;
    run_command(CommandSpec::espflash_flash(workspace, port, release))
}

/// Pre-flight check for migrating the firmware to `esp-hal` + `embassy`.
/// Prints whether each of the crates we depend on in the esp-rs ecosystem
/// has published an `esp32p4` feature. All must report YES before it is
/// worth revisiting the migration (see the plan doc for rationale).
fn run_check_esp_hal_p4() -> Result<(), String> {
    const CRATES: &[&str] = &[
        "esp-hal",
        "esp-hal-embassy",
        "esp-println",
        "esp-backtrace",
        "esp-bootloader-esp-idf",
    ];
    let mut all_present = true;
    println!("Checking esp-rs ecosystem for published ESP32-P4 support:\n");
    for krate in CRATES {
        let spec = CommandSpec {
            program: OsString::from("cargo"),
            args: vec![OsString::from("info"), OsString::from(*krate)],
            cwd: PathBuf::from("/"),
            env: BTreeMap::new(),
        };
        let output = Command::new(&spec.program)
            .args(&spec.args)
            .output()
            .map_err(|error| format!("failed to run `{}`: {error}", spec.display()))?;
        if !output.status.success() {
            println!(" ✗ {:<24} cargo info failed", krate);
            all_present = false;
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let has_p4_feature = stdout
            .lines()
            .any(|line| line.trim_start().starts_with("esp32p4"));
        if has_p4_feature {
            println!(" ✓ {:<24} esp32p4 feature PRESENT", krate);
        } else {
            println!(" ✗ {:<24} esp32p4 feature MISSING", krate);
            all_present = false;
        }
    }
    println!();
    if all_present {
        println!("All crates advertise esp32p4 — migration to esp-hal may be viable.");
        println!("Verify with a scratch `cargo fetch` before committing any plan.");
        Ok(())
    } else {
        Err("esp-rs ecosystem does not yet have complete ESP32-P4 support; \
             stay on esp-idf-svc and re-run this check periodically."
            .to_owned())
    }
}

fn ensure_tool(tool: &str) -> Result<(), String> {
    let path = env::var_os("PATH").ok_or("PATH is not set".to_owned())?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(tool);
        if candidate.is_file() {
            return Ok(());
        }

        if cfg!(windows) {
            for extension in ["exe", "cmd", "bat"] {
                if dir.join(format!("{tool}.{extension}")).is_file() {
                    return Ok(());
                }
            }
        }
    }

    Err(format!("required tool `{tool}` was not found on PATH"))
}

fn run_command(spec: CommandSpec) -> Result<(), String> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .envs(&spec.env)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = command
        .status()
        .map_err(|error| format!("failed to start `{}`: {error}", spec.display()))?;
    ensure_success(spec, status)
}

fn ensure_success(spec: CommandSpec, status: ExitStatus) -> Result<(), String> {
    if status.success() {
        Ok(())
    } else {
        Err(match status.code() {
            Some(code) => format!("`{}` exited with status code {code}", spec.display()),
            None => format!("`{}` terminated by signal", spec.display()),
        })
    }
}

fn print_help() {
    println!(
        "\
xtask commands:
  cargo xtask emu [--release]
  cargo xtask prepare-map
  cargo xtask bundle-device [--debug]
  cargo xtask deploy-device --port <PORT> [--debug]
  cargo xtask build-c6-slave       # builds esp_hosted slave fw for the on-board ESP32-C6
  cargo xtask check-esp-hal-p4     # checks whether esp-hal ecosystem has P4 support yet
  cargo xtask i18n-gen [--check]   # regenerate per-platform localization outputs
  cargo xtask i18n-sync --locale <code> [--dry-run] [--budget-usd <N>]
                                   # call OpenAI to fill missing translations"
    );
}

/// Build the `esp_hosted` slave firmware for the on-board ESP32-C6.
///
/// The slave project is unpacked under
/// `<target>/.../esp-idf-sys-*/out/managed_components/espressif__esp_hosted/slave`
/// after a successful `bundle-device` run. We point cmake at it with
/// `IDF_TARGET=esp32c6`, merge in the SDIO transport sdkconfig, and let
/// the ESP-IDF cmake build do the rest. The resulting flat image is
/// dropped at `.xtask/c6-slave/c6-slave-merged.bin` for flashing onto
/// the C6 over its UART.
fn run_build_c6_slave(workspace: &Workspace) -> Result<(), String> {
    ensure_tool("ldproxy")?;
    ensure_tool("espflash")?;

    let slave_src = locate_c6_slave_source(workspace).ok_or_else(|| {
        format!(
            "could not locate the esp_hosted slave project under {}; \
             run `cargo xtask bundle-device` first so the managed component is unpacked",
            workspace.device_target.display()
        )
    })?;

    let out_dir = workspace.root.join(".xtask/c6-slave");
    let build_dir = out_dir.join("build");
    std::fs::create_dir_all(&build_dir)
        .map_err(|error| format!("failed to create {}: {error}", build_dir.display()))?;

    let env = c6_slave_build_env(workspace);
    let toolchain = workspace
        .root
        .join(".embuild/espressif/esp-idf/v5.4.2/tools/cmake/toolchain-esp32c6.cmake");
    let python = workspace
        .root
        .join(".embuild/espressif/python_env/idf5.4_py3.11_env/bin/python");
    let sdkconfig_defaults = format!(
        "{};{};{}",
        slave_src.join("sdkconfig.defaults").display(),
        slave_src.join("sdkconfig.defaults.esp32c6").display(),
        slave_src.join("sdkconfig.ci.sdio").display(),
    );

    println!("configuring esp_hosted slave build at {}", build_dir.display());
    run_command(CommandSpec {
        program: OsString::from("cmake"),
        args: vec![
            OsString::from("-G"),
            OsString::from("Ninja"),
            slave_src.clone().into_os_string(),
            OsString::from(format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain.display())),
            OsString::from(format!("-DSDKCONFIG_DEFAULTS={sdkconfig_defaults}")),
            OsString::from(format!("-DPYTHON={}", python.display())),
        ],
        cwd: build_dir.clone(),
        env: env.clone(),
    })?;

    println!("compiling esp_hosted slave (target esp32c6) — this takes a couple of minutes");
    run_command(CommandSpec {
        program: OsString::from("ninja"),
        args: vec![OsString::from("-C"), build_dir.clone().into_os_string()],
        cwd: build_dir.clone(),
        env,
    })?;

    let elf = build_dir.join("network_adapter.elf");
    let bootloader = build_dir.join("bootloader/bootloader.bin");
    let partition_table = build_dir.join("partition_table/partition-table.bin");
    let merged = out_dir.join("c6-slave-merged.bin");

    run_command(CommandSpec {
        program: OsString::from("espflash"),
        args: vec![
            OsString::from("save-image"),
            OsString::from("--chip"),
            OsString::from("esp32c6"),
            OsString::from("--merge"),
            OsString::from("--flash-size"),
            OsString::from("4mb"),
            OsString::from("--partition-table"),
            partition_table.into_os_string(),
            OsString::from("--bootloader"),
            bootloader.into_os_string(),
            elf.into_os_string(),
            merged.clone().into_os_string(),
        ],
        cwd: workspace.root.clone(),
        env: BTreeMap::new(),
    })?;

    let bytes = std::fs::metadata(&merged).map(|m| m.len()).unwrap_or(0);
    println!(
        "\nesp32-c6 slave image ready: {}  ({} KB)\n\
         \nFlash with espflash on the host that has the C6 UART connected:\n  \
         espflash write-bin --chip esp32c6 --port <PORT> 0x0 c6-slave-merged.bin\n\
         \nThe C6's UART is exposed on a separate connector from the P4's USB-JTAG \
         (see the Waveshare 3.4C kit schematic). Power-cycle after flashing; the \
         next P4 boot should report `Host BT Support: Enabled` and \
         `hosted-ble: route-sync GATT server online`.",
        merged.display(),
        bytes / 1024,
    );
    Ok(())
}

/// If anything under `firmware/components/` is newer than esp-idf-sys's
/// last build timestamp, force-clean it so the next cargo build re-runs
/// CMake/Ninja for the C side.
fn invalidate_esp_idf_sys_if_components_changed(
    workspace: &Workspace,
    release: bool,
) -> Result<(), String> {
    let components_dir = workspace.firmware_crate.join("components");
    let Some(latest_source_mtime) = newest_mtime_in(&components_dir) else {
        return Ok(()); // no components dir or it's empty — nothing to track
    };

    let profile = if release { "release" } else { "debug" };
    let build_root = workspace.device_target.join(profile).join("build");
    let Ok(entries) = std::fs::read_dir(&build_root) else {
        return Ok(()); // first build — cargo will compile esp-idf-sys anyway
    };
    let mut needs_clean = false;
    let mut any_esp_idf_sys = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !name_str.starts_with("esp-idf-sys-") {
            continue;
        }
        any_esp_idf_sys = true;
        let invoked = entry.path().join("invoked.timestamp");
        let Ok(meta) = std::fs::metadata(&invoked) else {
            continue;
        };
        let Ok(invoked_mtime) = meta.modified() else {
            continue;
        };
        if latest_source_mtime > invoked_mtime {
            needs_clean = true;
            break;
        }
    }
    if !any_esp_idf_sys || !needs_clean {
        return Ok(());
    }

    println!(
        "components/ changed since last esp-idf-sys build — forcing C rebuild"
    );
    let mut args = vec![
        OsString::from("clean"),
        OsString::from("-p"),
        OsString::from("esp-idf-sys"),
        OsString::from("--target"),
        OsString::from(DEVICE_RUST_TARGET),
    ];
    if release {
        args.push(OsString::from("--release"));
    }
    run_command(CommandSpec {
        program: OsString::from("cargo"),
        args,
        cwd: workspace.firmware_crate.clone(),
        env: BTreeMap::new(),
    })
}

fn newest_mtime_in(dir: &Path) -> Option<std::time::SystemTime> {
    fn walk(dir: &Path, newest: &mut Option<std::time::SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                walk(&path, newest);
            } else if let Ok(mtime) = meta.modified() {
                if newest.map_or(true, |existing| mtime > existing) {
                    *newest = Some(mtime);
                }
            }
        }
    }
    let mut newest = None;
    walk(dir, &mut newest);
    newest
}

fn locate_c6_slave_source(workspace: &Workspace) -> Option<PathBuf> {
    // The managed component lives under
    // `<target>/<profile>/build/esp-idf-sys-<hash>/out/managed_components/...`.
    // The hash changes whenever esp-idf-sys's inputs change, so glob the
    // build dir for the slave path rather than hardcoding it.
    for profile in ["release", "debug"] {
        let build_root = workspace.device_target.join(profile).join("build");
        let Ok(entries) = std::fs::read_dir(&build_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let candidate = entry.path().join(
                "out/managed_components/espressif__esp_hosted/slave/CMakeLists.txt",
            );
            if candidate.is_file() {
                return Some(candidate.parent().unwrap().to_path_buf());
            }
        }
    }
    None
}

/// Build env for the esp_hosted slave cmake project. Mirrors the env
/// `embuild` sets up for the P4 build, but explicitly switches
/// `IDF_TARGET` to esp32c6 and skips `LIBCLANG_PATH` (the slave is pure
/// C, no bindgen).
fn c6_slave_build_env(workspace: &Workspace) -> BTreeMap<OsString, OsString> {
    let mut env = BTreeMap::new();
    let idf_path = workspace
        .root
        .join(".embuild/espressif/esp-idf/v5.4.2");
    let idf_tools = workspace.root.join(".embuild/espressif");
    env.insert(OsString::from("IDF_PATH"), idf_path.into_os_string());
    env.insert(OsString::from("IDF_TOOLS_PATH"), idf_tools.clone().into_os_string());
    env.insert(OsString::from("IDF_TARGET"), OsString::from("esp32c6"));
    env.insert(OsString::from("IDF_COMPONENT_MANAGER"), OsString::from("1"));
    env.insert(
        OsString::from("ESP_ROM_ELF_DIR"),
        idf_tools.join("tools/esp-rom-elfs/20241011/").into_os_string(),
    );

    // PATH — embuild's tool dirs in the same order it uses for the P4 build.
    let extra_path = [
        idf_tools.join("python_env/idf5.4_py3.11_env/bin"),
        idf_tools.join("tools/cmake/3.30.2/bin"),
        idf_tools.join("tools/ninja/1.12.1"),
        idf_tools.join("tools/riscv32-esp-elf/esp-14.2.0_20241119/riscv32-esp-elf/bin"),
        idf_tools.join("tools/esp-clang/esp-18.1.2_20240912/esp-clang/bin"),
    ];
    let existing = env::var_os("PATH").unwrap_or_default();
    let mut combined = OsString::new();
    for (i, dir) in extra_path.iter().enumerate() {
        if i > 0 {
            combined.push(":");
        }
        combined.push(dir.as_os_str());
    }
    if !existing.is_empty() {
        combined.push(":");
        combined.push(&existing);
    }
    env.insert(OsString::from("PATH"), combined);
    env
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Cli {
    Emu { release: bool },
    BundleDevice { release: bool },
    DeployDevice { port: String, release: bool },
    CheckEspHalP4,
    BuildC6Slave,
    I18n { args: Vec<String> },
    Stub { command: String },
    Help,
}

fn parse_cli(args: &[String]) -> Result<Cli, String> {
    let Some(command) = args.first() else {
        return Ok(Cli::Help);
    };

    match command.as_str() {
        "emu" => parse_emu_args(&args[1..]),
        "bundle-device" => parse_bundle_device_args(&args[1..]),
        "deploy-device" => parse_deploy_device_args(&args[1..]),
        "check-esp-hal-p4" => Ok(Cli::CheckEspHalP4),
        "build-c6-slave" => Ok(Cli::BuildC6Slave),
        // i18n-gen / i18n-sync / i18n-extract — dispatch into the i18n module.
        c if c.starts_with("i18n-") => Ok(Cli::I18n { args: args.to_vec() }),
        "prepare-map" => Ok(Cli::Stub {
            command: command.clone(),
        }),
        "help" | "--help" | "-h" => Ok(Cli::Help),
        other => Err(format!(
            "unknown command `{other}`; supported commands are prepare-map, emu, bundle-device, deploy-device, i18n-gen, i18n-sync"
        )),
    }
}

fn parse_emu_args(args: &[String]) -> Result<Cli, String> {
    let mut release = false;

    for arg in args {
        match arg.as_str() {
            "--release" => release = true,
            "--help" | "-h" => return Ok(Cli::Help),
            other => return Err(format!("unsupported `cargo xtask emu` argument `{other}`")),
        }
    }

    Ok(Cli::Emu { release })
}

fn parse_bundle_device_args(args: &[String]) -> Result<Cli, String> {
    let mut release = true;
    for arg in args {
        match arg.as_str() {
            "--release" => release = true,
            "--debug" => release = false,
            "--help" | "-h" => return Ok(Cli::Help),
            other => {
                return Err(format!(
                    "unsupported `cargo xtask bundle-device` argument `{other}`"
                ));
            }
        }
    }
    Ok(Cli::BundleDevice { release })
}

fn parse_deploy_device_args(args: &[String]) -> Result<Cli, String> {
    let mut release = true;
    let mut port: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--release" => release = true,
            "--debug" => release = false,
            "--port" => {
                port = Some(
                    iter.next()
                        .ok_or_else(|| "`--port` requires a value".to_owned())?
                        .clone(),
                );
            }
            "--help" | "-h" => return Ok(Cli::Help),
            other if other.starts_with("--port=") => {
                port = Some(other.trim_start_matches("--port=").to_owned());
            }
            other => {
                return Err(format!(
                    "unsupported `cargo xtask deploy-device` argument `{other}`"
                ));
            }
        }
    }
    let port = port.ok_or_else(|| {
        "`cargo xtask deploy-device` requires `--port <PORT>` (e.g. --port /dev/ttyACM0)"
            .to_owned()
    })?;
    Ok(Cli::DeployDevice { port, release })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Workspace {
    root: PathBuf,
    emulator_web: PathBuf,
    wasm_pkg: PathBuf,
    wasm_crate: PathBuf,
    /// Full city.svm used by the emulator / wasm build.
    map_data: PathBuf,
    /// SVM bundled into the device flash image (`map_data` partition).
    /// Stays under the 12 MB partition and within the PSRAM budget for
    /// peak parse memory. SD card (looking for `/sdcard/city.svm`) can
    /// override this at boot.
    device_map_data: PathBuf,
    xtask_tmp: PathBuf,
    firmware_crate: PathBuf,
    partitions_csv: PathBuf,
    device_target: PathBuf,
    device_out_dir: PathBuf,
    esp_export_script: PathBuf,
}

const DEVICE_RUST_TARGET: &str = "riscv32imafc-esp-espidf";
const DEVICE_CHIP: &str = "esp32p4";
// Waveshare ESP32-P4-Module-DEV-KIT has 32 MB SPI flash. espflash defaults
// to 4 MB, which collides with our 16 MB factory partition in partitions.csv.
const DEVICE_FLASH_SIZE: &str = "32mb";

impl Workspace {
    fn new() -> Result<Self, String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("xtask manifest should live inside the workspace".to_owned())?
            .to_path_buf();
        let esp_export_script = env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".config/esp/export-esp.sh"))
            .unwrap_or_else(|| PathBuf::from("/home/vscode/.config/esp/export-esp.sh"));
        // Honor `CARGO_TARGET_DIR` when it is set (the devcontainer sets it
        // to /work/target/devcontainer so builds don't collide with the host
        // user's target dir on a bind mount).
        let cargo_target_dir = env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target"));
        Ok(Self {
            emulator_web: root.join("emulator/web"),
            wasm_pkg: root.join("emulator/web/wasm-pkg"),
            wasm_crate: root.join("render-core-wasm"),
            map_data: root.join("map-data/city.svm"),
            device_map_data: root.join("map-data/city-small.svm"),
            xtask_tmp: root.join(".xtask/tmp"),
            firmware_crate: root.join("firmware"),
            partitions_csv: root.join("firmware/partitions.csv"),
            device_target: cargo_target_dir.join(DEVICE_RUST_TARGET),
            device_out_dir: root.join(".xtask/device"),
            esp_export_script,
            root,
        })
    }

    fn firmware_elf_path(&self, release: bool) -> PathBuf {
        self.device_target
            .join(if release { "release" } else { "debug" })
            .join("firmware")
    }

    fn device_image_path(&self, release: bool) -> PathBuf {
        self.device_out_dir.join(format!(
            "firmware-{}.bin",
            if release { "release" } else { "debug" }
        ))
    }

    fn device_app_image_path(&self, release: bool) -> PathBuf {
        self.device_out_dir.join(format!(
            "firmware-{}-app.bin",
            if release { "release" } else { "debug" }
        ))
    }

    fn ensure_device_prerequisites(&self) -> Result<(), String> {
        if !self.firmware_crate.join("Cargo.toml").is_file() {
            return Err(format!(
                "missing firmware manifest at {}",
                self.firmware_crate.join("Cargo.toml").display()
            ));
        }
        if !self.firmware_crate.join(".cargo/config.toml").is_file() {
            return Err(format!(
                "missing firmware cargo config at {}; bring-up requires the \
                 riscv32imafc-esp-espidf target to be wired",
                self.firmware_crate.join(".cargo/config.toml").display()
            ));
        }
        if !self.esp_export_script.is_file() {
            return Err(format!(
                "missing Espressif export script at {}; run `espup install \
                 --targets esp32p4 --esp-riscv-gcc` to produce it",
                self.esp_export_script.display()
            ));
        }
        if !self.device_map_data.is_file() {
            return Err(format!(
                "missing device map at {}; regenerate it from the full city.svm \
                 using map-vector-cli shrink-svm",
                self.device_map_data.display()
            ));
        }
        std::fs::create_dir_all(&self.device_out_dir).map_err(|error| {
            format!(
                "failed to create device output dir {}: {error}",
                self.device_out_dir.display()
            )
        })?;
        Ok(())
    }

    fn ensure_emu_prerequisites(&self) -> Result<(), String> {
        if !self.map_data.is_file() {
            return Err(format!(
                "missing map data at {}; generate it before starting the emulator",
                self.map_data.display()
            ));
        }
        if !self.emulator_web.join("package.json").is_file() {
            return Err(format!(
                "missing emulator package manifest at {}",
                self.emulator_web.join("package.json").display()
            ));
        }
        if !self.emulator_web.join("node_modules").is_dir() {
            return Err(format!(
                "missing emulator dependencies at {}; run `npm install` in {}",
                self.emulator_web.join("node_modules").display(),
                self.emulator_web.display()
            ));
        }
        std::fs::create_dir_all(&self.xtask_tmp)
            .map_err(|error| format!("failed to create {}: {error}", self.xtask_tmp.display()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandSpec {
    program: OsString,
    args: Vec<OsString>,
    cwd: PathBuf,
    env: BTreeMap<OsString, OsString>,
}

impl CommandSpec {
    fn wasm_pack_build(workspace: &Workspace, release: bool) -> Self {
        let mut args = vec![
            OsString::from("build"),
            workspace.wasm_crate.clone().into_os_string(),
            OsString::from("--target"),
            OsString::from("web"),
            OsString::from("--out-dir"),
            workspace.wasm_pkg.clone().into_os_string(),
            OsString::from("--out-name"),
            OsString::from("render_core_wasm"),
        ];
        if release {
            args.push(OsString::from("--release"));
        }

        Self {
            program: OsString::from("wasm-pack"),
            args,
            cwd: workspace.root.clone(),
            env: BTreeMap::from([(
                OsString::from("TMPDIR"),
                workspace.xtask_tmp.clone().into_os_string(),
            )]),
        }
    }

    fn npm_dev(workspace: &Workspace) -> Self {
        Self {
            program: OsString::from("npm"),
            args: vec![OsString::from("run"), OsString::from("dev")],
            cwd: workspace.emulator_web.clone(),
            env: BTreeMap::new(),
        }
    }

    fn cargo_build_firmware(workspace: &Workspace, release: bool) -> Self {
        let mut args = vec![
            OsString::from("+nightly"),
            OsString::from("build"),
            OsString::from("-p"),
            OsString::from("firmware"),
            OsString::from("--target"),
            OsString::from(DEVICE_RUST_TARGET),
            OsString::from("-Zbuild-std=std,panic_abort"),
        ];
        if release {
            args.push(OsString::from("--release"));
        }
        Self {
            program: OsString::from("cargo"),
            args,
            cwd: workspace.firmware_crate.clone(),
            env: device_build_env(workspace),
        }
    }

    fn espflash_save_image(workspace: &Workspace, release: bool) -> Self {
        let elf = workspace.firmware_elf_path(release);
        let out = workspace.device_image_path(release);
        // `save-image --merge` defaults to the esp-idf 4 MB factory partition
        // table. We use the custom table in firmware/partitions.csv which adds
        // the 15 MB map_data partition. The map is no longer embedded in the
        // ELF; bundle_map_partition() appends city-small.svm to the image
        // afterwards at MAP_PARTITION_FLASH_OFFSET.
        let args = vec![
            OsString::from("save-image"),
            OsString::from("--chip"),
            OsString::from(DEVICE_CHIP),
            OsString::from("--merge"),
            OsString::from("--flash-size"),
            OsString::from(DEVICE_FLASH_SIZE),
            OsString::from("--partition-table"),
            workspace.partitions_csv.clone().into_os_string(),
            elf.into_os_string(),
            out.into_os_string(),
        ];
        Self {
            program: OsString::from("espflash"),
            args,
            cwd: workspace.root.clone(),
            env: device_build_env(workspace),
        }
    }

    fn espflash_flash(workspace: &Workspace, port: &str, release: bool) -> Self {
        let elf = workspace.firmware_elf_path(release);
        let args = vec![
            OsString::from("flash"),
            OsString::from("--chip"),
            OsString::from(DEVICE_CHIP),
            OsString::from("--port"),
            OsString::from(port),
            OsString::from("--partition-table"),
            workspace.partitions_csv.clone().into_os_string(),
            OsString::from("--monitor"),
            elf.into_os_string(),
        ];
        Self {
            program: OsString::from("espflash"),
            args,
            cwd: workspace.root.clone(),
            env: device_build_env(workspace),
        }
    }

    fn display(&self) -> String {
        let program = self.program.to_string_lossy();
        let args = self
            .args
            .iter()
            .map(OsString::as_os_str)
            .map(shell_escape)
            .collect::<Vec<_>>()
            .join(" ");
        if args.is_empty() {
            program.into_owned()
        } else {
            format!("{program} {args}")
        }
    }
}

fn device_build_env(workspace: &Workspace) -> BTreeMap<OsString, OsString> {
    let mut env = BTreeMap::new();
    let (path_prepend, extras) = parse_esp_export_script(&workspace.esp_export_script);
    if !path_prepend.is_empty() {
        let existing = env::var_os("PATH").unwrap_or_default();
        let mut combined = OsString::from(path_prepend.join(":"));
        if !existing.is_empty() {
            combined.push(":");
            combined.push(&existing);
        }
        env.insert(OsString::from("PATH"), combined);
    }
    for (key, value) in extras {
        env.insert(OsString::from(key), OsString::from(value));
    }
    // bindgen needs libclang.so at run-time. firmware/.cargo/config.toml sets
    // this when cargo is invoked from inside the firmware crate, but xtask
    // runs cargo from the workspace root, where that file isn't picked up.
    // Mirror it here so `cargo xtask bundle-device` works from anywhere.
    let libclang =
        workspace.root.join("target/.embuild/espressif/tools/esp-clang/16.0.1-fe4f10a809/esp-clang/lib");
    if libclang.is_dir() {
        env.insert(OsString::from("LIBCLANG_PATH"), libclang.into_os_string());
    }
    env
}

fn parse_esp_export_script(path: &Path) -> (Vec<String>, Vec<(String, String)>) {
    let mut path_prepends = Vec::new();
    let mut extras = Vec::new();
    let Ok(content) = std::fs::read_to_string(path) else {
        return (path_prepends, extras);
    };
    for line in content.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("export ") else {
            continue;
        };
        let Some(eq) = rest.find('=') else {
            continue;
        };
        let key = rest[..eq].to_owned();
        let raw_value = rest[eq + 1..].trim_matches('"');
        if key == "PATH" {
            if let Some(prefix) = raw_value.strip_suffix(":$PATH") {
                path_prepends.push(prefix.to_owned());
            }
        } else {
            extras.push((key, raw_value.to_owned()));
        }
    }
    (path_prepends, extras)
}

fn shell_escape(value: &OsStr) -> String {
    let raw = value.to_string_lossy();
    if raw.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '/' | '-' | '_' | '.')
    }) {
        raw.into_owned()
    } else {
        format!("{raw:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_emu_release_flag() {
        let args = vec!["emu".to_owned(), "--release".to_owned()];

        let cli = parse_cli(&args).expect("emu args should parse");

        assert_eq!(cli, Cli::Emu { release: true });
    }

    #[test]
    fn rejects_unknown_emu_flag() {
        let args = vec!["emu".to_owned(), "--wat".to_owned()];

        let error = parse_cli(&args).expect_err("unknown emu flags should fail");

        assert!(error.contains("--wat"));
    }

    fn test_workspace() -> Workspace {
        Workspace {
            root: PathBuf::from("/work"),
            emulator_web: PathBuf::from("/work/emulator/web"),
            wasm_pkg: PathBuf::from("/work/emulator/web/wasm-pkg"),
            wasm_crate: PathBuf::from("/work/render-core-wasm"),
            map_data: PathBuf::from("/work/map-data/city.svm"),
            device_map_data: PathBuf::from("/work/map-data/city-small.svm"),
            xtask_tmp: PathBuf::from("/work/.xtask/tmp"),
            firmware_crate: PathBuf::from("/work/firmware"),
            partitions_csv: PathBuf::from("/work/firmware/partitions.csv"),
            device_target: PathBuf::from("/work/target/riscv32imafc-esp-espidf"),
            device_out_dir: PathBuf::from("/work/.xtask/device"),
            esp_export_script: PathBuf::from("/does-not-exist/export-esp.sh"),
        }
    }

    #[test]
    fn builds_expected_wasm_pack_command() {
        let workspace = test_workspace();

        let spec = CommandSpec::wasm_pack_build(&workspace, true);

        assert_eq!(spec.program, OsString::from("wasm-pack"));
        assert_eq!(
            spec.args,
            vec![
                OsString::from("build"),
                OsString::from("/work/render-core-wasm"),
                OsString::from("--target"),
                OsString::from("web"),
                OsString::from("--out-dir"),
                OsString::from("/work/emulator/web/wasm-pkg"),
                OsString::from("--out-name"),
                OsString::from("render_core_wasm"),
                OsString::from("--release"),
            ]
        );
        assert_eq!(spec.cwd, PathBuf::from("/work"));
        assert_eq!(
            spec.env.get(OsStr::new("TMPDIR")),
            Some(&OsString::from("/work/.xtask/tmp"))
        );
    }

    #[test]
    fn builds_expected_npm_dev_command() {
        let workspace = test_workspace();

        let spec = CommandSpec::npm_dev(&workspace);

        assert_eq!(spec.program, OsString::from("npm"));
        assert_eq!(
            spec.args,
            vec![OsString::from("run"), OsString::from("dev")]
        );
        assert_eq!(spec.cwd, PathBuf::from("/work/emulator/web"));
        assert!(spec.env.is_empty());
    }

    #[test]
    fn parses_bundle_device_release_flag_by_default() {
        let cli = parse_cli(&["bundle-device".to_owned()]).expect("bundle-device parse");
        assert_eq!(cli, Cli::BundleDevice { release: true });
    }

    #[test]
    fn parses_bundle_device_debug_flag() {
        let cli = parse_cli(&["bundle-device".to_owned(), "--debug".to_owned()])
            .expect("bundle-device --debug parse");
        assert_eq!(cli, Cli::BundleDevice { release: false });
    }

    #[test]
    fn parses_deploy_device_with_port() {
        let cli = parse_cli(&[
            "deploy-device".to_owned(),
            "--port".to_owned(),
            "/dev/ttyACM0".to_owned(),
        ])
        .expect("deploy-device parse");
        assert_eq!(
            cli,
            Cli::DeployDevice {
                port: "/dev/ttyACM0".to_owned(),
                release: true,
            }
        );
    }

    #[test]
    fn rejects_deploy_device_without_port() {
        let error = parse_cli(&["deploy-device".to_owned()])
            .expect_err("deploy-device without port should fail");
        assert!(error.contains("--port"));
    }

    #[test]
    fn builds_expected_firmware_build_command() {
        let workspace = test_workspace();

        let spec = CommandSpec::cargo_build_firmware(&workspace, true);

        assert_eq!(spec.program, OsString::from("cargo"));
        assert_eq!(
            spec.args,
            vec![
                OsString::from("+nightly"),
                OsString::from("build"),
                OsString::from("-p"),
                OsString::from("firmware"),
                OsString::from("--target"),
                OsString::from(DEVICE_RUST_TARGET),
                OsString::from("-Zbuild-std=std,panic_abort"),
                OsString::from("--release"),
            ]
        );
        assert_eq!(spec.cwd, PathBuf::from("/work/firmware"));
    }

    #[test]
    fn builds_expected_espflash_save_image_command() {
        let workspace = test_workspace();

        let spec = CommandSpec::espflash_save_image(&workspace, true);

        assert_eq!(spec.program, OsString::from("espflash"));
        assert_eq!(
            spec.args,
            vec![
                OsString::from("save-image"),
                OsString::from("--chip"),
                OsString::from(DEVICE_CHIP),
                OsString::from("--merge"),
                OsString::from("--flash-size"),
                OsString::from(DEVICE_FLASH_SIZE),
                OsString::from("--partition-table"),
                OsString::from("/work/firmware/partitions.csv"),
                OsString::from("/work/target/riscv32imafc-esp-espidf/release/firmware"),
                OsString::from("/work/.xtask/device/firmware-release.bin"),
            ]
        );
    }

    #[test]
    fn builds_expected_espflash_flash_command() {
        let workspace = test_workspace();

        let spec = CommandSpec::espflash_flash(&workspace, "/dev/ttyACM0", false);

        assert_eq!(spec.program, OsString::from("espflash"));
        assert_eq!(
            spec.args,
            vec![
                OsString::from("flash"),
                OsString::from("--chip"),
                OsString::from(DEVICE_CHIP),
                OsString::from("--port"),
                OsString::from("/dev/ttyACM0"),
                OsString::from("--partition-table"),
                OsString::from("/work/firmware/partitions.csv"),
                OsString::from("--monitor"),
                OsString::from("/work/target/riscv32imafc-esp-espidf/debug/firmware"),
            ]
        );
    }

    #[test]
    fn parses_esp_export_script_extracts_path_prepends_and_extras() {
        let dir = std::env::temp_dir().join("xtask-esp-export-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("export-esp.sh");
        std::fs::write(
            &path,
            "export LIBCLANG_PATH=\"/opt/clang/lib\"\nexport PATH=\"/opt/esp/bin:$PATH\"\n",
        )
        .unwrap();

        let (path_prepends, extras) = parse_esp_export_script(&path);

        assert_eq!(path_prepends, vec!["/opt/esp/bin".to_owned()]);
        assert_eq!(
            extras,
            vec![("LIBCLANG_PATH".to_owned(), "/opt/clang/lib".to_owned())]
        );
    }
}
