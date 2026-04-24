use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

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
        Cli::Stub { command } => Err(format!(
            "{command} is not implemented yet; available commands: prepare-map, emu, bundle-device, deploy-device"
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

    run_command(CommandSpec::cargo_build_firmware(workspace, release))?;
    run_command(CommandSpec::espflash_save_image(workspace, release))?;

    println!(
        "\ndevice image ready: {}\nflash it from a host with USB access:\n  espflash flash --monitor {}",
        workspace.device_image_path(release).display(),
        workspace.device_image_path(release).display(),
    );
    Ok(())
}

fn run_deploy_device(workspace: &Workspace, port: &str, release: bool) -> Result<(), String> {
    ensure_tool("ldproxy")?;
    ensure_tool("espflash")?;
    workspace.ensure_device_prerequisites()?;

    run_command(CommandSpec::cargo_build_firmware(workspace, release))?;
    run_command(CommandSpec::espflash_flash(workspace, port, release))
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
  cargo xtask deploy-device --port <PORT> [--debug]"
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Cli {
    Emu { release: bool },
    BundleDevice { release: bool },
    DeployDevice { port: String, release: bool },
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
        "prepare-map" => Ok(Cli::Stub {
            command: command.clone(),
        }),
        "help" | "--help" | "-h" => Ok(Cli::Help),
        other => Err(format!(
            "unknown command `{other}`; supported commands are prepare-map, emu, bundle-device, deploy-device"
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
    map_data: PathBuf,
    xtask_tmp: PathBuf,
    firmware_crate: PathBuf,
    device_target: PathBuf,
    device_out_dir: PathBuf,
    esp_export_script: PathBuf,
}

const DEVICE_RUST_TARGET: &str = "riscv32imafc-esp-espidf";
const DEVICE_CHIP: &str = "esp32p4";

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
        Ok(Self {
            emulator_web: root.join("emulator/web"),
            wasm_pkg: root.join("emulator/web/wasm-pkg"),
            wasm_crate: root.join("render-core-wasm"),
            map_data: root.join("map-data/city.svm"),
            xtask_tmp: root.join(".xtask/tmp"),
            firmware_crate: root.join("firmware"),
            device_target: root.join("target").join(DEVICE_RUST_TARGET),
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
        let args = vec![
            OsString::from("save-image"),
            OsString::from("--chip"),
            OsString::from(DEVICE_CHIP),
            OsString::from("--merge"),
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
            xtask_tmp: PathBuf::from("/work/.xtask/tmp"),
            firmware_crate: PathBuf::from("/work/firmware"),
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
