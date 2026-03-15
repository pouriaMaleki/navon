use std::time::Duration;

use bevy_ecs::prelude::{IntoScheduleConfigs, Res, ResMut, Resource, Schedule, SystemSet, World};

use crate::api::{MapQuerySpec, RuntimeConfig, RuntimeFrameOutput, RuntimeInputFrame};
use crate::camera::CameraState;
use crate::diagnostics;
use crate::input::contacts::ContactState;
use crate::input::gestures::GestureState;
use crate::input::staging::DerivedInputState;
use crate::input::staging::PendingInput;
use crate::input::taps::TapState;
use crate::map;
use crate::motion::{MotionIngestConfig, MotionState};
use crate::output;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScheduleSet {
    InputIngest,
    MotionFusion,
    CameraPolicy,
    MapQuery,
    OutputBuild,
}

impl ScheduleSet {
    pub const fn ordered() -> [Self; 5] {
        [
            Self::InputIngest,
            Self::MotionFusion,
            Self::CameraPolicy,
            Self::MapQuery,
            Self::OutputBuild,
        ]
    }
}

#[derive(Resource, Debug, Clone)]
struct RuntimeConfigResource(pub RuntimeConfig);

#[derive(Resource, Debug, Clone, Default)]
struct FrameTime {
    dt: Duration,
    total: Duration,
    tick: u64,
}

#[derive(Resource, Debug, Clone, Default)]
struct MotionResource(pub MotionState);

#[derive(Resource, Debug, Clone, Default)]
struct CameraResource(pub CameraState);

#[derive(Resource, Debug, Clone, Default)]
struct QueryResource(pub MapQuerySpec);

#[derive(Resource, Debug, Clone, Default)]
struct OutputResource(pub RuntimeFrameOutput);

pub struct RuntimeRunner {
    world: World,
    schedule: Schedule,
}

impl RuntimeRunner {
    pub fn new(config: RuntimeConfig) -> Self {
        let mut world = World::new();
        world.insert_resource(RuntimeConfigResource(config.clone()));
        world.insert_resource(FrameTime::default());
        world.insert_resource(PendingInput::default());
        world.insert_resource(DerivedInputState::default());
        world.insert_resource(ContactState::default());
        world.insert_resource(GestureState::default());
        world.insert_resource(TapState::default());
        world.insert_resource(MotionResource::default());
        world.insert_resource(CameraResource(CameraState {
            zoom: config.zoom_bounds.default,
            ..CameraState::default()
        }));
        world.insert_resource(QueryResource::default());
        world.insert_resource(OutputResource::default());

        let mut schedule = Schedule::default();
        schedule.configure_sets(
            (
                ScheduleSet::InputIngest,
                ScheduleSet::MotionFusion,
                ScheduleSet::CameraPolicy,
                ScheduleSet::MapQuery,
                ScheduleSet::OutputBuild,
            )
                .chain(),
        );
        schedule.add_systems((
            input_ingest.in_set(ScheduleSet::InputIngest),
            motion_fusion.in_set(ScheduleSet::MotionFusion),
            camera_policy.in_set(ScheduleSet::CameraPolicy),
            map_query.in_set(ScheduleSet::MapQuery),
            output_build.in_set(ScheduleSet::OutputBuild),
        ));

        Self { world, schedule }
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.world.resource::<RuntimeConfigResource>().0
    }

    pub fn frame_index(&self) -> u64 {
        self.world.resource::<FrameTime>().tick
    }

    pub fn step(&mut self, input: RuntimeInputFrame) -> RuntimeFrameOutput {
        {
            let dt = input.dt;
            let mut frame_time = self.world.resource_mut::<FrameTime>();
            frame_time.dt = dt;
            frame_time.total += dt;
            frame_time.tick += 1;
        }

        self.world.insert_resource(PendingInput { frame: input });
        self.schedule.run(&mut self.world);
        self.world.resource::<OutputResource>().0.clone()
    }
}

fn input_ingest(
    frame_time: Res<FrameTime>,
    pending: Res<PendingInput>,
    mut config: ResMut<RuntimeConfigResource>,
    mut contacts: ResMut<ContactState>,
    mut gesture_state: ResMut<GestureState>,
    mut tap_state: ResMut<TapState>,
    mut derived_input: ResMut<DerivedInputState>,
) {
    if let Some(viewport_size) = pending
        .frame
        .viewport_size
        .filter(|viewport_size| !viewport_size.is_empty())
    {
        config.0.viewport_size = viewport_size;
    }

    contacts.stage(pending.frame.touch.as_ref());
    let gesture = gesture_state.derive(
        contacts.current.as_ref(),
        config.0.pan_deadzone_px,
        config.0.rotate_deadzone_rad,
    );
    let tap = tap_state.update(
        contacts.previous.as_ref(),
        contacts.current.as_ref(),
        frame_time.total,
        config.0.tap_max_duration,
        config.0.tap_max_travel_px,
    );
    derived_input.gesture = gesture;
    derived_input.tap = tap;
}

fn motion_fusion(
    frame_time: Res<FrameTime>,
    pending: Res<PendingInput>,
    config: Res<RuntimeConfigResource>,
    mut motion: ResMut<MotionResource>,
) {
    motion.0.ingest(
        pending.frame.gps,
        frame_time.dt,
        MotionIngestConfig {
            riding_speed_threshold_mps: config.0.riding_speed_threshold_mps,
            stopped_speed_threshold_mps: config.0.stopped_speed_threshold_mps,
            gps_loss_stop_timeout: config.0.gps_loss_stop_timeout,
            min_heading_displacement_m: config.0.min_heading_displacement_m,
            heading_filter_alpha: config.0.heading_filter_alpha,
        },
    );
}

fn camera_policy(
    frame_time: Res<FrameTime>,
    config: Res<RuntimeConfigResource>,
    motion: Res<MotionResource>,
    derived_input: Res<DerivedInputState>,
    mut camera: ResMut<CameraResource>,
) {
    camera.0.advance(
        &motion.0,
        &derived_input,
        frame_time.dt,
        config.0.viewport_size,
        &config.0,
    );
}

fn map_query(
    config: Res<RuntimeConfigResource>,
    camera: Res<CameraResource>,
    mut query: ResMut<QueryResource>,
) {
    query.0 = map::build_query(&camera.0.snapshot(&config.0), config.0.viewport_size);
}

fn output_build(
    frame_time: Res<FrameTime>,
    config: Res<RuntimeConfigResource>,
    pending: Res<PendingInput>,
    motion: Res<MotionResource>,
    camera: Res<CameraResource>,
    query: Res<QueryResource>,
    mut output_resource: ResMut<OutputResource>,
) {
    let camera_snapshot = camera.0.snapshot(&config.0);
    let diagnostics = config.0.diagnostics_enabled.then(|| {
        diagnostics::build_snapshot(
            frame_time.tick,
            config.0.viewport_size,
            &pending.frame,
            &camera_snapshot,
            &query.0,
        )
    });

    output_resource.0 = output::build_frame_output(
        frame_time.tick,
        camera_snapshot,
        query.0.clone(),
        diagnostics,
        motion.0.travel_heading_rad,
    );
}
