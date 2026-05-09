#[cfg(target_os = "espidf")]
fn main() {
    if let Err(error) = firmware::esp_idf::run_device_main() {
        eprintln!("firmware device entrypoint failed: {error}");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            eprintln!("firmware device entrypoint failed: {error}");
        }
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    let frame = firmware::platform::run_host_demo().expect("firmware host demo should build");
    println!(
        "firmware platform demo: frame={} mode={:?} geometry={} lit_pixels={}",
        frame.output.frame_index,
        frame.output.camera.mode,
        frame.geometry_count,
        frame.lit_pixel_count
    );
}
