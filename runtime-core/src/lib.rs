#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

#[cfg(test)]
use core::sync::atomic::{AtomicUsize, Ordering};
#[cfg(test)]
use std::alloc::{GlobalAlloc, Layout, System};

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use esp32_screen_render_core::{
    CameraControllerInput, CameraControllerState, CameraMode, CameraView, Line, WorldBounds,
    WorldPoint, north_indicator_hit_test,
};
use heapless::Vec;

pub const MAX_GESTURE_EVENTS: usize = 8;

#[cfg(test)]
struct CountingGlobalAlloc;

#[cfg(test)]
static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
#[global_allocator]
static GLOBAL_ALLOCATOR: CountingGlobalAlloc = CountingGlobalAlloc;

#[cfg(test)]
// Safety: delegates to the system allocator and only adds atomic counters.
unsafe impl GlobalAlloc for CountingGlobalAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // Safety: forwarding allocation to system allocator with the provided layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Safety: forwarding deallocation to system allocator with pointer/layout pair.
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerClass {
    Critical,
    Major,
    Minor,
    Local,
    Detail,
}

impl LayerClass {
    const fn bit(self) -> u8 {
        match self {
            Self::Critical => 1 << 0,
            Self::Major => 1 << 1,
            Self::Minor => 1 << 2,
            Self::Local => 1 << 3,
            Self::Detail => 1 << 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LodMask(u8);

impl LodMask {
    pub const ALL: Self = Self(
        LayerClass::Critical.bit()
            | LayerClass::Major.bit()
            | LayerClass::Minor.bit()
            | LayerClass::Local.bit()
            | LayerClass::Detail.bit(),
    );

    pub const MAJOR_AND_UP: Self =
        Self(LayerClass::Critical.bit() | LayerClass::Major.bit() | LayerClass::Minor.bit());

    pub const CORE_ONLY: Self = Self(LayerClass::Critical.bit() | LayerClass::Major.bit());

    pub const fn allows(self, class: LayerClass) -> bool {
        (self.0 & class.bit()) != 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LodPolicy {
    pub zoom_thresholds: [f32; 4],
    pub masks: [LodMask; 5],
}

impl Default for LodPolicy {
    fn default() -> Self {
        Self {
            zoom_thresholds: [1.0, 2.0, 4.0, 8.0],
            masks: [
                LodMask::CORE_ONLY,
                LodMask::CORE_ONLY,
                LodMask::MAJOR_AND_UP,
                LodMask::ALL,
                LodMask::ALL,
            ],
        }
    }
}

impl LodPolicy {
    pub fn bucket_for_zoom(&self, zoom: f32) -> u8 {
        if zoom < self.zoom_thresholds[0] {
            0
        } else if zoom < self.zoom_thresholds[1] {
            1
        } else if zoom < self.zoom_thresholds[2] {
            2
        } else if zoom < self.zoom_thresholds[3] {
            3
        } else {
            4
        }
    }

    pub fn mask_for_bucket(&self, bucket: u8) -> LodMask {
        let idx = (bucket as usize).min(self.masks.len() - 1);
        self.masks[idx]
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct FrameTime {
    pub dt_ms: f32,
    pub total_ms: f32,
    pub tick: u64,
}

impl Default for FrameTime {
    fn default() -> Self {
        Self {
            dt_ms: 0.0,
            total_ms: 0.0,
            tick: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimeConfig {
    pub viewport: Viewport,
    pub base_bounds: WorldBounds,
    pub background: u8,
    pub initial_zoom: f32,
    pub player_anchor_x: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpsFixEvent {
    pub player: WorldPoint,
    pub heading_rad: f32,
    pub speed_mps: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GestureEvent {
    Pan { dx: f32, dy: f32 },
    Pinch { scale: f32 },
    Rotate { delta_rad: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TapEvent {
    pub nx: f32,
    pub ny: f32,
}

#[derive(Clone, Debug)]
pub struct RuntimeInputFrame {
    pub dt_ms: f32,
    pub gps_fix: Option<GpsFixEvent>,
    pub gestures: Vec<GestureEvent, MAX_GESTURE_EVENTS>,
    pub tap: Option<TapEvent>,
    pub request_north_up: bool,
}

impl Default for RuntimeInputFrame {
    fn default() -> Self {
        Self {
            dt_ms: 0.0,
            gps_fix: None,
            gestures: Vec::new(),
            tap: None,
            request_north_up: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MapQuerySpec {
    pub bounds: WorldBounds,
    pub zoom_bucket: u8,
    pub lod_mask: LodMask,
}

#[derive(Clone, Debug)]
pub struct RuntimeFrameOutput<const MAX_VISIBLE: usize> {
    pub frame_time: FrameTime,
    pub camera_view: CameraView,
    pub camera_mode: CameraMode,
    pub query: MapQuerySpec,
    pub visible_lines: Vec<Line, MAX_VISIBLE>,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Rider {
    pub player: WorldPoint,
    pub heading_rad: f32,
    pub speed_mps: f32,
}

#[derive(Component)]
pub struct Camera {
    pub controller: CameraControllerState,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct InteractionState {
    pub pan_dx: f32,
    pub pan_dy: f32,
    pub zoom_scale: f32,
    pub rotate_delta_rad: f32,
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FollowLock {
    pub active: bool,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct MapQuery {
    pub spec: MapQuerySpec,
}

#[derive(Resource, Clone)]
struct PendingInput(pub RuntimeInputFrame);

#[derive(Resource, Clone, Copy)]
struct RuntimeConfigResource(pub RuntimeConfig);

#[derive(Resource, Clone, Copy)]
struct LodPolicyResource(pub LodPolicy);

#[derive(Resource, Clone, Copy, Default)]
struct NorthUpRequest(pub bool);

#[derive(Resource, Clone, Copy)]
struct RuntimeOutputResource {
    pub frame_time: FrameTime,
    pub camera_view: CameraView,
    pub camera_mode: CameraMode,
    pub query: MapQuerySpec,
}

#[derive(SystemSet, Debug, Clone, Hash, PartialEq, Eq)]
enum RuntimeSet {
    InputIngest,
    MotionFusion,
    CameraPolicy,
    MapQuery,
    OutputBuild,
}

pub trait MapSource {
    fn bounds(&self) -> WorldBounds;
    fn query<const MAX_VISIBLE: usize>(
        &self,
        query: &MapQuerySpec,
        out: &mut Vec<Line, MAX_VISIBLE>,
    );
}

#[derive(Clone)]
pub struct ClassifiedLine {
    pub line: Line,
    pub class: LayerClass,
}

pub struct SliceMapSource<'a> {
    pub lines: &'a [ClassifiedLine],
    pub bounds: WorldBounds,
}

impl<'a> MapSource for SliceMapSource<'a> {
    fn bounds(&self) -> WorldBounds {
        self.bounds
    }

    fn query<const MAX_VISIBLE: usize>(
        &self,
        query: &MapQuerySpec,
        out: &mut Vec<Line, MAX_VISIBLE>,
    ) {
        out.clear();
        for entry in self.lines {
            if !query.lod_mask.allows(entry.class) {
                continue;
            }
            let line = entry.line;
            let min_x = line.from.x.min(line.to.x);
            let max_x = line.from.x.max(line.to.x);
            let min_y = line.from.y.min(line.to.y);
            let max_y = line.from.y.max(line.to.y);
            if max_x < query.bounds.min_x
                || min_x > query.bounds.max_x
                || max_y < query.bounds.min_y
                || min_y > query.bounds.max_y
            {
                continue;
            }
            if out.push(line).is_err() {
                break;
            }
        }
    }
}

pub struct Runtime<const MAX_VISIBLE: usize, M: MapSource> {
    world: World,
    schedule: Schedule,
    map_source: M,
    entity: Entity,
}

impl<const MAX_VISIBLE: usize, M: MapSource> Runtime<MAX_VISIBLE, M> {
    pub fn new(config: RuntimeConfig, map_source: M) -> Self {
        let mut world = World::new();
        let mut schedule = Schedule::default();

        schedule.configure_sets(
            (
                RuntimeSet::InputIngest,
                RuntimeSet::MotionFusion,
                RuntimeSet::CameraPolicy,
                RuntimeSet::MapQuery,
                RuntimeSet::OutputBuild,
            )
                .chain(),
        );

        schedule.add_systems(input_ingest_system.in_set(RuntimeSet::InputIngest));
        schedule.add_systems(motion_fusion_system.in_set(RuntimeSet::MotionFusion));
        schedule.add_systems(camera_policy_system.in_set(RuntimeSet::CameraPolicy));
        schedule.add_systems(map_query_system.in_set(RuntimeSet::MapQuery));
        schedule.add_systems(output_build_system.in_set(RuntimeSet::OutputBuild));

        let base_bounds = map_source.bounds();
        let rider = Rider {
            player: WorldPoint {
                x: (base_bounds.min_x as i32 + base_bounds.max_x as i32) as i16 / 2,
                y: (base_bounds.min_y as i32 + base_bounds.max_y as i32) as i16 / 2,
            },
            heading_rad: 0.0,
            speed_mps: 0.0,
        };

        let mut controller = CameraControllerState::new(config.initial_zoom);
        controller.update(
            CameraControllerInput {
                player: rider.player,
                rider_heading_rad: rider.heading_rad,
                speed_mps: rider.speed_mps,
                pan_dx: 0.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            0.0,
        );

        let query = MapQuerySpec {
            bounds: base_bounds,
            zoom_bucket: 0,
            lod_mask: LodMask::ALL,
        };

        let output_view = CameraView {
            center: rider.player,
            player: rider.player,
            heading_rad: 0.0,
            rider_heading_rad: 0.0,
            zoom: config.initial_zoom,
            base_bounds,
            background: config.background,
            player_anchor_x: config.player_anchor_x,
            player_anchor_y: 0.5,
            riding_mode: false,
            interaction_active: false,
        };

        let entity = world
            .spawn((
                rider,
                Camera { controller },
                InteractionState {
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                FollowLock::default(),
                MapQuery { spec: query },
            ))
            .id();

        world.insert_resource(PendingInput(RuntimeInputFrame::default()));
        world.insert_resource(FrameTime::default());
        world.insert_resource(RuntimeConfigResource(RuntimeConfig {
            viewport: config.viewport,
            base_bounds,
            ..config
        }));
        world.insert_resource(LodPolicyResource(LodPolicy::default()));
        world.insert_resource(NorthUpRequest::default());
        world.insert_resource(RuntimeOutputResource {
            frame_time: FrameTime::default(),
            camera_view: output_view,
            camera_mode: CameraMode::StoppedNorthUp,
            query,
        });

        let mut runtime = Self {
            world,
            schedule,
            map_source,
            entity,
        };
        let _ = runtime.step(RuntimeInputFrame::default());
        runtime
    }

    pub fn reset(&mut self) {
        let cfg = self.world.resource::<RuntimeConfigResource>().0;
        let map_bounds = self.map_source.bounds();
        let rider = Rider {
            player: WorldPoint {
                x: (map_bounds.min_x as i32 + map_bounds.max_x as i32) as i16 / 2,
                y: (map_bounds.min_y as i32 + map_bounds.max_y as i32) as i16 / 2,
            },
            heading_rad: 0.0,
            speed_mps: 0.0,
        };
        {
            let mut entity = self.world.entity_mut(self.entity);
            entity.insert(rider);
            entity.insert(InteractionState {
                pan_dx: 0.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            });
            entity.insert(FollowLock::default());
            entity.insert(MapQuery {
                spec: MapQuerySpec {
                    bounds: map_bounds,
                    zoom_bucket: 0,
                    lod_mask: LodMask::ALL,
                },
            });
            entity.insert(Camera {
                controller: CameraControllerState::new(cfg.initial_zoom),
            });
        }

        *self.world.resource_mut::<FrameTime>() = FrameTime::default();
        *self.world.resource_mut::<NorthUpRequest>() = NorthUpRequest::default();
        *self.world.resource_mut::<PendingInput>() = PendingInput(RuntimeInputFrame::default());

        let _ = self.step(RuntimeInputFrame::default());
    }

    pub fn step(&mut self, input: RuntimeInputFrame) -> RuntimeFrameOutput<MAX_VISIBLE> {
        {
            let mut frame_time = self.world.resource_mut::<FrameTime>();
            frame_time.dt_ms = input.dt_ms.max(0.0);
            frame_time.total_ms += frame_time.dt_ms;
            frame_time.tick = frame_time.tick.saturating_add(1);
        }
        *self.world.resource_mut::<PendingInput>() = PendingInput(input);

        self.schedule.run(&mut self.world);

        let out = *self.world.resource::<RuntimeOutputResource>();
        let mut visible_lines = Vec::<Line, MAX_VISIBLE>::new();
        self.map_source.query(&out.query, &mut visible_lines);

        RuntimeFrameOutput {
            frame_time: out.frame_time,
            camera_view: out.camera_view,
            camera_mode: out.camera_mode,
            query: out.query,
            visible_lines,
        }
    }

    pub fn set_lod_policy(&mut self, policy: LodPolicy) {
        self.world.insert_resource(LodPolicyResource(policy));
    }
}

fn input_ingest_system(
    cfg: Res<'_, RuntimeConfigResource>,
    mut pending: ResMut<'_, PendingInput>,
    mut north_up_request: ResMut<'_, NorthUpRequest>,
    mut query: Query<'_, '_, (&mut Rider, &mut InteractionState)>,
) {
    let frame = core::mem::take(&mut pending.0);
    let Ok((mut rider, mut interaction)) = query.single_mut() else {
        return;
    };

    interaction.pan_dx = 0.0;
    interaction.pan_dy = 0.0;
    interaction.zoom_scale = 1.0;
    interaction.rotate_delta_rad = 0.0;

    if let Some(gps) = frame.gps_fix {
        rider.player = gps.player;
        rider.heading_rad = gps.heading_rad;
        rider.speed_mps = gps.speed_mps;
    }

    for gesture in frame.gestures {
        match gesture {
            GestureEvent::Pan { dx, dy } => {
                interaction.pan_dx += dx;
                interaction.pan_dy += dy;
            }
            GestureEvent::Pinch { scale } => {
                if scale.is_finite() && scale > 0.0 {
                    interaction.zoom_scale *= scale;
                }
            }
            GestureEvent::Rotate { delta_rad } => {
                if delta_rad.is_finite() {
                    interaction.rotate_delta_rad += delta_rad;
                }
            }
        }
    }

    if frame.request_north_up {
        north_up_request.0 = true;
    }

    if let Some(tap) = frame.tap {
        let x = libm::roundf(tap.nx.clamp(0.0, 1.0) * cfg.0.viewport.width as f32) as i32;
        let y = libm::roundf(tap.ny.clamp(0.0, 1.0) * cfg.0.viewport.height as f32) as i32;
        if north_indicator_hit_test(cfg.0.viewport.width, cfg.0.viewport.height, x, y) {
            north_up_request.0 = true;
        }
    }
}

fn motion_fusion_system() {
    // Motion fusion is currently represented by camera-controller internals
    // (speed threshold + movement delta fallback). This system exists as an
    // explicit schedule stage boundary for future sensor fusion extensions.
}

fn camera_policy_system(
    frame_time: Res<'_, FrameTime>,
    mut north_up_request: ResMut<'_, NorthUpRequest>,
    mut query: Query<'_, '_, (&Rider, &mut Camera, &InteractionState, &mut FollowLock)>,
) {
    let Ok((rider, mut camera, interaction, mut follow_lock)) = query.single_mut() else {
        return;
    };

    if north_up_request.0 {
        camera.controller.request_north_up();
        north_up_request.0 = false;
    }

    let out = camera.controller.update(
        CameraControllerInput {
            player: rider.player,
            rider_heading_rad: rider.heading_rad,
            speed_mps: rider.speed_mps,
            pan_dx: interaction.pan_dx,
            pan_dy: interaction.pan_dy,
            zoom_scale: interaction.zoom_scale,
            rotate_delta_rad: interaction.rotate_delta_rad,
        },
        frame_time.dt_ms,
    );

    follow_lock.active = out.follow_player != rider.player;
}

fn map_query_system(
    lod_policy: Res<'_, LodPolicyResource>,
    cfg: Res<'_, RuntimeConfigResource>,
    mut query: Query<'_, '_, (&Camera, &mut MapQuery)>,
) {
    let Ok((camera, mut map_query)) = query.single_mut() else {
        return;
    };
    let out = camera.controller.output();
    let center = WorldPoint {
        x: (out.follow_player.x as f32 + out.pan_x) as i16,
        y: (out.follow_player.y as f32 + out.pan_y) as i16,
    };

    let zoom = out.zoom.max(0.1);
    let half_w = (cfg.0.base_bounds.width().max(1) as f32) / (2.0 * zoom);
    let half_h = (cfg.0.base_bounds.height().max(1) as f32) / (2.0 * zoom);
    let margin_w = (half_w * 1.6) as i32;
    let margin_h = (half_h * 1.6) as i32;

    let bucket = lod_policy.0.bucket_for_zoom(zoom);
    map_query.spec = MapQuerySpec {
        bounds: WorldBounds {
            min_x: (center.x as i32 - margin_w).clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            max_x: (center.x as i32 + margin_w).clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            min_y: (center.y as i32 - margin_h).clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            max_y: (center.y as i32 + margin_h).clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        },
        zoom_bucket: bucket,
        lod_mask: lod_policy.0.mask_for_bucket(bucket),
    };
}

fn output_build_system(
    frame_time: Res<'_, FrameTime>,
    cfg: Res<'_, RuntimeConfigResource>,
    query: Query<'_, '_, (&Rider, &Camera, &MapQuery)>,
    mut out: ResMut<'_, RuntimeOutputResource>,
) {
    let Ok((rider, camera, map_query)) = query.single() else {
        return;
    };
    let camera_out = camera.controller.output();
    let center = WorldPoint {
        x: (camera_out.follow_player.x as f32 + camera_out.pan_x) as i16,
        y: (camera_out.follow_player.y as f32 + camera_out.pan_y) as i16,
    };

    out.frame_time = *frame_time;
    out.camera_mode = camera_out.mode;
    out.query = map_query.spec;
    out.camera_view = CameraView {
        center,
        player: rider.player,
        heading_rad: camera_out.heading_rad,
        rider_heading_rad: camera_out.rider_heading_rad,
        zoom: camera_out.zoom,
        base_bounds: cfg.0.base_bounds,
        background: cfg.0.background,
        player_anchor_x: cfg.0.player_anchor_x,
        player_anchor_y: camera_out.player_anchor_y,
        riding_mode: camera_out.riding_mode,
        interaction_active: camera_out.interaction_active,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::boxed::Box;
    use std::time::Instant;
    use std::vec::Vec as StdVec;

    fn reset_alloc_counters() {
        ALLOC_CALLS.store(0, Ordering::Relaxed);
        ALLOC_BYTES.store(0, Ordering::Relaxed);
    }

    fn mk_runtime<const N: usize>() -> Runtime<N, SliceMapSource<'static>> {
        static LINES: [ClassifiedLine; 5] = [
            ClassifiedLine {
                line: Line {
                    from: WorldPoint { x: 100, y: 100 },
                    to: WorldPoint { x: 900, y: 100 },
                    intensity: 200,
                    thickness: 2,
                },
                class: LayerClass::Major,
            },
            ClassifiedLine {
                line: Line {
                    from: WorldPoint { x: 100, y: 100 },
                    to: WorldPoint { x: 100, y: 900 },
                    intensity: 160,
                    thickness: 1,
                },
                class: LayerClass::Minor,
            },
            ClassifiedLine {
                line: Line {
                    from: WorldPoint { x: 900, y: 100 },
                    to: WorldPoint { x: 900, y: 900 },
                    intensity: 160,
                    thickness: 1,
                },
                class: LayerClass::Local,
            },
            ClassifiedLine {
                line: Line {
                    from: WorldPoint { x: 500, y: 500 },
                    to: WorldPoint { x: 700, y: 700 },
                    intensity: 255,
                    thickness: 1,
                },
                class: LayerClass::Detail,
            },
            ClassifiedLine {
                line: Line {
                    from: WorldPoint { x: 200, y: 200 },
                    to: WorldPoint { x: 800, y: 800 },
                    intensity: 255,
                    thickness: 1,
                },
                class: LayerClass::Critical,
            },
        ];

        Runtime::new(
            RuntimeConfig {
                viewport: Viewport {
                    width: 800,
                    height: 800,
                },
                base_bounds: WorldBounds {
                    min_x: 0,
                    max_x: 1000,
                    min_y: 0,
                    max_y: 1000,
                },
                background: 12,
                initial_zoom: 1.0,
                player_anchor_x: 0.5,
            },
            SliceMapSource {
                lines: &LINES,
                bounds: WorldBounds {
                    min_x: 0,
                    max_x: 1000,
                    min_y: 0,
                    max_y: 1000,
                },
            },
        )
    }

    fn mk_dense_runtime<const N: usize>() -> Runtime<N, SliceMapSource<'static>> {
        let mut lines: StdVec<ClassifiedLine> = StdVec::with_capacity(3200);
        for idx in 0..3200 {
            let x0 = ((idx * 37) % 3800 + 100) as i16;
            let y0 = ((idx * 53) % 3800 + 100) as i16;
            let x1 = ((idx * 97 + 640) % 3800 + 100) as i16;
            let y1 = ((idx * 71 + 420) % 3800 + 100) as i16;
            let class = match idx % 5 {
                0 => LayerClass::Critical,
                1 => LayerClass::Major,
                2 => LayerClass::Minor,
                3 => LayerClass::Local,
                _ => LayerClass::Detail,
            };
            let thickness = if idx % 9 == 0 {
                3
            } else if idx % 4 == 0 {
                2
            } else {
                1
            };
            lines.push(ClassifiedLine {
                line: Line {
                    from: WorldPoint { x: x0, y: y0 },
                    to: WorldPoint { x: x1, y: y1 },
                    intensity: 120 + (idx % 120) as u8,
                    thickness,
                },
                class,
            });
        }
        let leaked = Box::leak(lines.into_boxed_slice());
        Runtime::new(
            RuntimeConfig {
                viewport: Viewport {
                    width: 800,
                    height: 800,
                },
                base_bounds: WorldBounds {
                    min_x: 0,
                    max_x: 4000,
                    min_y: 0,
                    max_y: 4000,
                },
                background: 12,
                initial_zoom: 1.0,
                player_anchor_x: 0.5,
            },
            SliceMapSource {
                lines: leaked,
                bounds: WorldBounds {
                    min_x: 0,
                    max_x: 4000,
                    min_y: 0,
                    max_y: 4000,
                },
            },
        )
    }

    #[test]
    fn camera_transitions_to_stopped_north_up() {
        let mut runtime = mk_runtime::<32>();
        let mut p = WorldPoint { x: 200, y: 200 };

        for _ in 0..10 {
            p.x += 6;
            let _ = runtime.step(RuntimeInputFrame {
                dt_ms: 120.0,
                gps_fix: Some(GpsFixEvent {
                    player: p,
                    heading_rad: 1.0,
                    speed_mps: 2.5,
                }),
                ..RuntimeInputFrame::default()
            });
        }
        let moving_out = runtime.step(RuntimeInputFrame {
            dt_ms: 120.0,
            gps_fix: Some(GpsFixEvent {
                player: p,
                heading_rad: 1.0,
                speed_mps: 2.5,
            }),
            ..RuntimeInputFrame::default()
        });
        assert_eq!(moving_out.camera_mode, CameraMode::Riding);

        let mut out = runtime.step(RuntimeInputFrame {
            dt_ms: 120.0,
            gps_fix: Some(GpsFixEvent {
                player: p,
                heading_rad: 1.0,
                speed_mps: 0.0,
            }),
            ..RuntimeInputFrame::default()
        });
        for _ in 0..20 {
            out = runtime.step(RuntimeInputFrame {
                dt_ms: 120.0,
                gps_fix: Some(GpsFixEvent {
                    player: p,
                    heading_rad: 1.0,
                    speed_mps: 0.0,
                }),
                ..RuntimeInputFrame::default()
            });
        }
        assert_eq!(out.camera_mode, CameraMode::StoppedNorthUp);
        assert!(out.camera_view.heading_rad.abs() < 0.35);
    }

    #[test]
    fn pan_lock_keeps_rider_anchor_stable() {
        let mut runtime = mk_runtime::<32>();
        let mut p = WorldPoint { x: 500, y: 500 };

        for _ in 0..8 {
            p.x += 4;
            let _ = runtime.step(RuntimeInputFrame {
                dt_ms: 120.0,
                gps_fix: Some(GpsFixEvent {
                    player: p,
                    heading_rad: 0.4,
                    speed_mps: 2.0,
                }),
                ..RuntimeInputFrame::default()
            });
        }

        let mut input = RuntimeInputFrame {
            dt_ms: 120.0,
            gps_fix: Some(GpsFixEvent {
                player: p,
                heading_rad: 0.4,
                speed_mps: 2.0,
            }),
            ..RuntimeInputFrame::default()
        };
        let _ = input
            .gestures
            .push(GestureEvent::Pan { dx: 24.0, dy: -6.0 });
        let out = runtime.step(input);
        let anchor = out.camera_view.player_anchor_y;

        let moved = WorldPoint { x: 650, y: 620 };
        let out2 = runtime.step(RuntimeInputFrame {
            dt_ms: 120.0,
            gps_fix: Some(GpsFixEvent {
                player: moved,
                heading_rad: 0.4,
                speed_mps: 2.0,
            }),
            ..RuntimeInputFrame::default()
        });
        assert!((out2.camera_view.player_anchor_y - anchor).abs() < 0.01);
        assert_eq!(out2.camera_view.player, moved);
    }

    #[test]
    fn lod_filter_changes_visible_lines_by_zoom_bucket() {
        let mut runtime = mk_runtime::<32>();

        let out_far = runtime.step(RuntimeInputFrame {
            dt_ms: 120.0,
            gps_fix: Some(GpsFixEvent {
                player: WorldPoint { x: 500, y: 500 },
                heading_rad: 0.0,
                speed_mps: 0.0,
            }),
            ..RuntimeInputFrame::default()
        });

        let mut input = RuntimeInputFrame {
            dt_ms: 120.0,
            gps_fix: Some(GpsFixEvent {
                player: WorldPoint { x: 500, y: 500 },
                heading_rad: 0.0,
                speed_mps: 0.0,
            }),
            ..RuntimeInputFrame::default()
        };
        for _ in 0..12 {
            let _ = input.gestures.push(GestureEvent::Pinch { scale: 1.4 });
        }
        let out_near = runtime.step(input);

        assert!(out_far.visible_lines.len() <= out_near.visible_lines.len());
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Snapshot {
        mode: u8,
        heading_milli: i32,
        zoom_milli: i32,
        anchor_milli: i32,
        center_x: i16,
        center_y: i16,
        visible_lines: usize,
    }

    fn quantize(v: f32) -> i32 {
        libm::roundf(v * 1000.0) as i32
    }

    fn snapshot<const N: usize>(out: &RuntimeFrameOutput<N>) -> Snapshot {
        let mode = match out.camera_mode {
            CameraMode::Riding => 0,
            CameraMode::StoppedNorthUp => 1,
            CameraMode::TemporaryNorthUp => 2,
        };
        Snapshot {
            mode,
            heading_milli: quantize(out.camera_view.heading_rad),
            zoom_milli: quantize(out.camera_view.zoom),
            anchor_milli: quantize(out.camera_view.player_anchor_y),
            center_x: out.camera_view.center.x,
            center_y: out.camera_view.center.y,
            visible_lines: out.visible_lines.len(),
        }
    }

    #[test]
    fn replay_trace_is_deterministic() {
        let mut trace: Vec<RuntimeInputFrame, 64> = Vec::new();

        let mut x = 250_i16;
        for _ in 0..8 {
            x += 8;
            let _ = trace.push(RuntimeInputFrame {
                dt_ms: 120.0,
                gps_fix: Some(GpsFixEvent {
                    player: WorldPoint { x, y: 320 },
                    heading_rad: 0.7,
                    speed_mps: 2.4,
                }),
                ..RuntimeInputFrame::default()
            });
        }

        for _ in 0..10 {
            let mut frame = RuntimeInputFrame {
                dt_ms: 120.0,
                gps_fix: Some(GpsFixEvent {
                    player: WorldPoint { x, y: 320 },
                    heading_rad: 0.7,
                    speed_mps: 0.0,
                }),
                ..RuntimeInputFrame::default()
            };
            let _ = frame
                .gestures
                .push(GestureEvent::Pan { dx: 16.0, dy: -5.0 });
            let _ = trace.push(frame);
        }

        let mut frame = RuntimeInputFrame {
            dt_ms: 120.0,
            gps_fix: Some(GpsFixEvent {
                player: WorldPoint { x, y: 320 },
                heading_rad: 0.7,
                speed_mps: 0.0,
            }),
            request_north_up: true,
            ..RuntimeInputFrame::default()
        };
        let _ = frame.gestures.push(GestureEvent::Pinch { scale: 1.25 });
        let _ = frame
            .gestures
            .push(GestureEvent::Rotate { delta_rad: 0.25 });
        let _ = trace.push(frame);

        let mut runtime_a = mk_runtime::<32>();
        let mut snapshots_a: Vec<Snapshot, 64> = Vec::new();
        for frame in trace.iter().cloned() {
            let out = runtime_a.step(frame);
            let _ = snapshots_a.push(snapshot(&out));
        }

        let mut runtime_b = mk_runtime::<32>();
        let mut snapshots_b: Vec<Snapshot, 64> = Vec::new();
        for frame in trace.iter().cloned() {
            let out = runtime_b.step(frame);
            let _ = snapshots_b.push(snapshot(&out));
        }

        assert_eq!(snapshots_a, snapshots_b);
    }

    #[test]
    #[ignore = "profiling"]
    fn perf_sanity_representative_map_load() {
        let mut runtime = mk_dense_runtime::<1024>();
        let mut px = 1900_i16;
        let mut py = 2100_i16;

        for _ in 0..64 {
            let _ = runtime.step(RuntimeInputFrame {
                dt_ms: 16.0,
                gps_fix: Some(GpsFixEvent {
                    player: WorldPoint { x: px, y: py },
                    heading_rad: 0.35,
                    speed_mps: 3.0,
                }),
                ..RuntimeInputFrame::default()
            });
        }

        let frame_count = 2000_u32;
        let start = Instant::now();
        for i in 0..frame_count {
            px = 1900 + ((i as i16 * 9) % 600);
            py = 2100 + ((i as i16 * 11) % 600);
            let mut frame = RuntimeInputFrame {
                dt_ms: 16.0,
                gps_fix: Some(GpsFixEvent {
                    player: WorldPoint { x: px, y: py },
                    heading_rad: 0.6,
                    speed_mps: 3.8,
                }),
                ..RuntimeInputFrame::default()
            };
            if i % 14 == 0 {
                let _ = frame.gestures.push(GestureEvent::Pan { dx: 8.0, dy: -3.0 });
            }
            if i % 22 == 0 {
                let _ = frame.gestures.push(GestureEvent::Pinch { scale: 1.02 });
            }
            if i % 30 == 0 {
                let _ = frame
                    .gestures
                    .push(GestureEvent::Rotate { delta_rad: 0.015 });
            }
            let _ = runtime.step(frame);
        }
        let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
        let avg_ms = elapsed_ms / frame_count as f32;

        assert!(
            avg_ms < 4.0,
            "runtime perf sanity exceeded budget: avg_ms={avg_ms:.3}"
        );
    }

    #[test]
    #[ignore = "profiling"]
    fn allocation_profile_hot_step_after_warmup() {
        let mut runtime = mk_dense_runtime::<1024>();
        let mut p = WorldPoint { x: 1900, y: 2100 };

        for _ in 0..96 {
            p.x += 3;
            p.y += 2;
            let mut frame = RuntimeInputFrame {
                dt_ms: 16.0,
                gps_fix: Some(GpsFixEvent {
                    player: p,
                    heading_rad: 0.5,
                    speed_mps: 3.4,
                }),
                ..RuntimeInputFrame::default()
            };
            let _ = frame.gestures.push(GestureEvent::Pan { dx: 3.0, dy: -1.0 });
            let _ = runtime.step(frame);
        }

        reset_alloc_counters();
        for i in 0..1200 {
            p.x += 1;
            let mut frame = RuntimeInputFrame {
                dt_ms: 16.0,
                gps_fix: Some(GpsFixEvent {
                    player: p,
                    heading_rad: 0.55,
                    speed_mps: 3.2,
                }),
                ..RuntimeInputFrame::default()
            };
            if i % 40 == 0 {
                let _ = frame.gestures.push(GestureEvent::Pinch { scale: 0.99 });
            }
            let _ = runtime.step(frame);
        }

        let alloc_calls = ALLOC_CALLS.load(Ordering::Relaxed);
        let alloc_bytes = ALLOC_BYTES.load(Ordering::Relaxed);
        assert!(
            alloc_calls < 4000,
            "unexpected allocation pressure in hot step: calls={alloc_calls}, bytes={alloc_bytes}"
        );
    }
}
