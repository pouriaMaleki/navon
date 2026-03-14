mod app;
mod board_config;
mod display;
mod framebuffer;
mod gestures;
mod gps;
mod input_bridge;
mod logging;
mod map_source;
mod power;
mod touch;

fn main() {
    let mut app = app::App::default();
    let first_fix = gps::GpsInput {
        lat_deg: 60.17442,
        lon_deg: 24.94210,
        speed_mps: 0.0,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(4.0),
    };
    let second_fix = gps::GpsInput {
        lon_deg: 24.94310,
        speed_mps: 5.0,
        ..first_fix
    };

    app.step_frame(std::time::Duration::from_millis(16), Some(first_fix), None)
        .expect("initial firmware frame should build");
    let frame = app
        .step_frame(std::time::Duration::from_millis(16), Some(second_fix), None)
        .expect("moving firmware frame should build");

    println!(
        "firmware host slice: frame={} mode={:?} geometry={} lit_pixels={}",
        frame.output.frame_index,
        frame.output.camera.mode,
        frame.geometry_count,
        frame.lit_pixel_count
    );
}
