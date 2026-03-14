use firmware::app::App;
use firmware::board_config::BoardConfig;
use firmware::gps::GpsInput;
use firmware::input_bridge::InputBridge as FirmwareInputBridge;
use firmware::touch::{RawTouchContact, TouchInput};
use parity_fixtures::{
    FIXTURE_VIEWPORT, FixtureFrame, FixtureMapSource, FixtureScenario, ParitySnapshot,
    bridge_parity_frames, duplicate_contact_frame, runtime_scenarios,
};
use render_core_wasm::adapter::RuntimeRenderBridge;
use render_core_wasm::input_bridge::{BrowserTouchContact, InputBridge as WasmInputBridge};
use runtime_core::api::{RuntimeConfig, RuntimeInputFrame, TouchContactFrameError};

fn firmware_runtime_input(
    bridge: &FirmwareInputBridge,
    frame: &FixtureFrame,
) -> Result<RuntimeInputFrame, TouchContactFrameError> {
    bridge.frame_from_samples(
        frame.dt,
        frame.gps.map(|gps| GpsInput {
            lat_deg: gps.lat_deg,
            lon_deg: gps.lon_deg,
            speed_mps: gps.speed_mps,
            course_rad: gps.course_rad,
            horizontal_accuracy_m: gps.horizontal_accuracy_m,
        }),
        frame.touch.as_ref().map(|touch| TouchInput {
            sequence: touch.sequence,
            contacts: touch
                .contacts
                .iter()
                .copied()
                .map(|contact| RawTouchContact {
                    id: contact.id,
                    phase: contact.phase,
                    position: contact.position,
                    pressure: contact.pressure,
                })
                .collect(),
        }),
    )
}

fn wasm_runtime_input(frame: &FixtureFrame) -> Result<RuntimeInputFrame, TouchContactFrameError> {
    let bridge = WasmInputBridge;
    match frame.touch.as_ref() {
        Some(touch) => bridge.frame_from_browser(
            frame.dt,
            frame.viewport,
            frame.gps,
            touch.sequence,
            touch
                .contacts
                .iter()
                .copied()
                .map(|contact| BrowserTouchContact {
                    id: contact.id,
                    phase: contact.phase,
                    x_px: contact.position.x_px,
                    y_px: contact.position.y_px,
                    pressure: contact.pressure,
                })
                .collect(),
        ),
        None => {
            let frame_input = RuntimeInputFrame::new(frame.dt).with_viewport(frame.viewport);
            Ok(if let Some(gps) = frame.gps {
                frame_input.with_gps(gps)
            } else {
                frame_input
            })
        }
    }
}

fn runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        viewport_size: FIXTURE_VIEWPORT,
        ..RuntimeConfig::default()
    }
}

fn firmware_snapshot(
    app: &mut App<FixtureMapSource>,
    frame: &FixtureFrame,
) -> Result<ParitySnapshot, firmware::app::AppError> {
    let result = app.step_frame(
        frame.dt,
        frame.gps.map(|gps| GpsInput {
            lat_deg: gps.lat_deg,
            lon_deg: gps.lon_deg,
            speed_mps: gps.speed_mps,
            course_rad: gps.course_rad,
            horizontal_accuracy_m: gps.horizontal_accuracy_m,
        }),
        frame.touch.as_ref().map(|touch| TouchInput {
            sequence: touch.sequence,
            contacts: touch
                .contacts
                .iter()
                .copied()
                .map(|contact| RawTouchContact {
                    id: contact.id,
                    phase: contact.phase,
                    position: contact.position,
                    pressure: contact.pressure,
                })
                .collect(),
        }),
    )?;
    Ok(ParitySnapshot::from_output(
        &result.output,
        result.geometry_count,
        app.display().framebuffer().pixels(),
    ))
}

fn wasm_snapshot(
    bridge: &mut RuntimeRenderBridge<FixtureMapSource>,
    frame: &FixtureFrame,
) -> Result<ParitySnapshot, TouchContactFrameError> {
    let input = wasm_runtime_input(frame)?;
    let state = bridge.step(input);
    Ok(ParitySnapshot::from_output(
        bridge.last_output(),
        state.geometry_count,
        bridge.pixels(),
    ))
}

fn assert_scenario_parity(scenario: &FixtureScenario) {
    let board = BoardConfig::new(FIXTURE_VIEWPORT);
    let mut firmware_app = App::with_map_source(board, runtime_config(), FixtureMapSource);
    let mut wasm_bridge = RuntimeRenderBridge::with_map_source(runtime_config(), FixtureMapSource);

    for (index, frame) in scenario.frames.iter().enumerate() {
        let firmware = firmware_snapshot(&mut firmware_app, frame)
            .expect("firmware scenario frame should be valid");
        let wasm =
            wasm_snapshot(&mut wasm_bridge, frame).expect("wasm scenario frame should be valid");
        assert!(
            firmware.approx_eq(&wasm),
            "scenario={} frame={} firmware={firmware:?} wasm={wasm:?}",
            scenario.name,
            index
        );
    }
}

#[test]
fn firmware_and_wasm_bridges_normalize_drag_sequence_identically() {
    let firmware_bridge = FirmwareInputBridge::new(FIXTURE_VIEWPORT);
    let frames = bridge_parity_frames();

    for frame in &frames {
        let firmware = firmware_runtime_input(&firmware_bridge, frame).expect("firmware input");
        let wasm = wasm_runtime_input(frame).expect("wasm input");
        assert_eq!(firmware, wasm);
    }
}

#[test]
fn duplicate_contact_ids_fail_identically() {
    let firmware_bridge = FirmwareInputBridge::new(FIXTURE_VIEWPORT);
    let frame = duplicate_contact_frame();

    let firmware = firmware_runtime_input(&firmware_bridge, &frame);
    let wasm = wasm_runtime_input(&frame);

    assert_eq!(firmware, Err(TouchContactFrameError::DuplicateContactId(7)));
    assert_eq!(wasm, Err(TouchContactFrameError::DuplicateContactId(7)));
}

#[test]
fn runtime_query_render_parity_holds_across_scenarios() {
    for scenario in runtime_scenarios() {
        assert_scenario_parity(&scenario);
    }
}
