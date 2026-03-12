pub(crate) fn normalize_angle(mut angle: f32) -> f32 {
    while angle > core::f32::consts::PI {
        angle -= 2.0 * core::f32::consts::PI;
    }
    while angle < -core::f32::consts::PI {
        angle += 2.0 * core::f32::consts::PI;
    }
    angle
}

pub(crate) fn slew_angle(current: f32, target: f32, max_step: f32) -> f32 {
    let delta = normalize_angle(target - current);
    let clamped = delta.clamp(-max_step, max_step);
    normalize_angle(current + clamped)
}
