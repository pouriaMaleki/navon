use map_runtime::EmbeddedMapSource;
use render_core::raster::Framebuffer;
use runtime_core::RuntimeCore;
use runtime_core::api::{RuntimeConfig, RuntimeFrameOutput, RuntimeInputFrame};
use runtime_core::map::MapSource;

use crate::output_bridge::JsFrameState;

pub struct RuntimeRenderBridge<S> {
    config: RuntimeConfig,
    runtime: RuntimeCore,
    map_source: S,
    framebuffer: Framebuffer,
    last_output: RuntimeFrameOutput,
}

impl<S> RuntimeRenderBridge<S>
where
    S: MapSource,
{
    pub fn with_map_source(config: RuntimeConfig, map_source: S) -> Self {
        let viewport = config.viewport_size;
        Self {
            config: config.clone(),
            runtime: RuntimeCore::new(config),
            map_source,
            framebuffer: Framebuffer::new(viewport.width_px, viewport.height_px),
            last_output: RuntimeFrameOutput::default(),
        }
    }

    pub fn reset(&mut self) {
        self.runtime = RuntimeCore::new(self.config.clone());
        self.framebuffer.resize(
            self.config.viewport_size.width_px,
            self.config.viewport_size.height_px,
        );
        self.framebuffer.clear(0);
        self.last_output = RuntimeFrameOutput::default();
    }

    pub fn step(&mut self, input: RuntimeInputFrame) -> JsFrameState {
        let viewport = input.viewport_size.unwrap_or(self.config.viewport_size);
        self.framebuffer
            .resize(viewport.width_px, viewport.height_px);
        let output = self.runtime.step(input);
        let geometry = self.map_source.query(&output.map_query);
        render_core::render_frame(
            render_core::RenderScene {
                config: self.runtime.config(),
                output: &output,
                geometry: &geometry,
            },
            &mut self.framebuffer,
        );
        self.last_output = output;
        JsFrameState::from_output(&self.last_output, &geometry)
    }

    pub fn pixels(&self) -> &[u8] {
        self.framebuffer.pixels()
    }

    pub fn width(&self) -> u32 {
        self.framebuffer.width()
    }

    pub fn height(&self) -> u32 {
        self.framebuffer.height()
    }

    pub fn last_output(&self) -> &RuntimeFrameOutput {
        &self.last_output
    }
}

pub type AdapterState = RuntimeRenderBridge<EmbeddedMapSource>;

impl AdapterState {
    pub fn new(config: RuntimeConfig) -> Self {
        Self::with_map_source(config, EmbeddedMapSource::default())
    }
}

impl Default for AdapterState {
    fn default() -> Self {
        Self::new(RuntimeConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use runtime_core::api::{
        CameraMode, GeometryCandidate, GpsSample, MapLayer, MapPolylineCandidate, MapQueryResult,
        RuntimeConfig, RuntimeInputFrame, ViewportSize, WorldPoint,
    };

    use super::*;

    #[derive(Debug, Default, Clone)]
    struct FixtureMapSource;

    impl MapSource for FixtureMapSource {
        fn query(&self, _spec: &runtime_core::api::MapQuerySpec) -> MapQueryResult {
            MapQueryResult {
                geometry: vec![GeometryCandidate::Polyline(MapPolylineCandidate {
                    layer: MapLayer::MajorRoad,
                    points: vec![WorldPoint::new(-50.0, 0.0), WorldPoint::new(50.0, 0.0)],
                })],
            }
        }
    }

    #[test]
    fn bridge_steps_runtime_and_renders_pixels() {
        let mut bridge =
            RuntimeRenderBridge::with_map_source(RuntimeConfig::default(), FixtureMapSource);
        let input = RuntimeInputFrame::new(std::time::Duration::from_millis(16))
            .with_viewport(ViewportSize::new(128, 128))
            .with_gps(GpsSample {
                lat_deg: 60.17442,
                lon_deg: 24.94210,
                speed_mps: 4.0,
                course_rad: Some(0.0),
                horizontal_accuracy_m: Some(5.0),
            });

        let snapshot = bridge.step(input);

        assert_eq!(snapshot.camera_mode, "riding");
        assert!(bridge.pixels().iter().copied().any(|value| value > 0));
        assert_eq!(bridge.last_output().camera.mode, CameraMode::Riding);
    }
}
