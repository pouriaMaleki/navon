use super::input::ScreenPoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureEventKind {
    Pan,
    Pinch,
    Rotate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TapEvent {
    pub position: ScreenPoint,
}
