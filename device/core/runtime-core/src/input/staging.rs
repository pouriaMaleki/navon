use bevy_ecs::prelude::Resource;

use crate::api::{RuntimeInputFrame, TapEvent};

use super::gestures::DerivedGesture;

#[derive(Debug, Clone, Resource, Default)]
pub struct PendingInput {
    pub frame: RuntimeInputFrame,
}

#[derive(Debug, Clone, Resource, Default)]
pub struct DerivedInputState {
    pub gesture: DerivedGesture,
    pub tap: Option<TapEvent>,
}
