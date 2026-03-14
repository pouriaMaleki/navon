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
  cargo xtask bundle-device
  cargo xtask deploy-device --port <PORT>"
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Cli {
    Emu { release: bool },
    Stub { command: String },
    Help,
}

fn parse_cli(args: &[String]) -> Result<Cli, String> {
    let Some(command) = args.first() else {
        return Ok(Cli::Help);
    };

    match command.as_str() {
        "emu" => parse_emu_args(&args[1..]),
        "prepare-map" | "bundle-device" | "deploy-device" => Ok(Cli::Stub {
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Workspace {
    root: PathBuf,
    emulator_web: PathBuf,
    wasm_pkg: PathBuf,
    wasm_crate: PathBuf,
    map_data: PathBuf,
    xtask_tmp: PathBuf,
}

impl Workspace {
    fn new() -> Result<Self, String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("xtask manifest should live inside the workspace".to_owned())?
            .to_path_buf();
        Ok(Self {
            emulator_web: root.join("emulator/web"),
            wasm_pkg: root.join("emulator/web/wasm-pkg"),
            wasm_crate: root.join("render-core-wasm"),
            map_data: root.join("map-data/city.svm"),
            xtask_tmp: root.join(".xtask/tmp"),
            root,
        })
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

    #[test]
    fn builds_expected_wasm_pack_command() {
        let workspace = Workspace {
            root: PathBuf::from("/work"),
            emulator_web: PathBuf::from("/work/emulator/web"),
            wasm_pkg: PathBuf::from("/work/emulator/web/wasm-pkg"),
            wasm_crate: PathBuf::from("/work/render-core-wasm"),
            map_data: PathBuf::from("/work/map-data/city.svm"),
            xtask_tmp: PathBuf::from("/work/.xtask/tmp"),
        };

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
        let workspace = Workspace {
            root: PathBuf::from("/work"),
            emulator_web: PathBuf::from("/work/emulator/web"),
            wasm_pkg: PathBuf::from("/work/emulator/web/wasm-pkg"),
            wasm_crate: PathBuf::from("/work/render-core-wasm"),
            map_data: PathBuf::from("/work/map-data/city.svm"),
            xtask_tmp: PathBuf::from("/work/.xtask/tmp"),
        };

        let spec = CommandSpec::npm_dev(&workspace);

        assert_eq!(spec.program, OsString::from("npm"));
        assert_eq!(
            spec.args,
            vec![OsString::from("run"), OsString::from("dev")]
        );
        assert_eq!(spec.cwd, PathBuf::from("/work/emulator/web"));
        assert!(spec.env.is_empty());
    }
}
