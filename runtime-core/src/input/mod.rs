pub mod staging {
    use bevy_ecs::prelude::Resource;

    use crate::api::RuntimeInputFrame;

    #[derive(Debug, Clone, Resource, Default)]
    pub struct PendingInput {
        pub frame: RuntimeInputFrame,
    }
}

pub mod gestures {
    #[derive(Debug, Default)]
    pub struct GestureState;
}

pub mod taps {
    #[derive(Debug, Default)]
    pub struct TapState;
}
