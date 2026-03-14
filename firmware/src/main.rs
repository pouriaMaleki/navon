#[cfg(target_os = "espidf")]
fn main() {
    firmware::esp_idf::run_device_main().expect("firmware device entrypoint should build");
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
