use std::env;
use std::path::Path;
use std::process::{Command, ExitCode};

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
        return Err("usage: cargo run -p xtask -- <emu|emu-release>".to_owned());
    };

    match cmd.as_str() {
        "emu" => {
            let release = args.any(|a| a == "--release");
            run_emu(release)
        }
        _ => Err(format!("unknown command: {cmd}")),
    }
}

fn run_emu(release: bool) -> Result<(), String> {
    let web_dir = Path::new("emulator/web");
    if !web_dir.exists() {
        return Err("emulator/web not found (run from repository root)".to_owned());
    }

    let build_mode = if release { "--release" } else { "--dev" };
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
        &format!("failed to run npm script {script} ({build_mode})"),
    )
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
