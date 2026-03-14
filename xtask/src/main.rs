use std::env;

fn main() {
    let command = env::args().nth(1).unwrap_or_else(|| "help".to_string());

    match command.as_str() {
        "prepare-map" | "emu" | "bundle-device" | "deploy-device" => {
            println!("xtask stub: {command} is not implemented yet");
        }
        _ => {
            eprintln!(
                "xtask stub: supported commands are prepare-map, emu, bundle-device, deploy-device"
            );
        }
    }
}
