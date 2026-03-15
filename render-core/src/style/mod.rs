use runtime_core::api::MapLayer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrokeStyle {
    pub intensity: u8,
    pub thickness_px: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderStyle {
    pub background_intensity: u8,
    pub major_road: StrokeStyle,
    pub minor_road: StrokeStyle,
    pub path: StrokeStyle,
    pub rider_fill_intensity: u8,
    pub rider_heading_intensity: u8,
    pub north_indicator_active_intensity: u8,
    pub north_indicator_acquisition_intensity: u8,
    pub north_indicator_idle_intensity: u8,
    pub north_indicator_locked_intensity: u8,
    pub north_indicator_ring_intensity: u8,
    pub north_indicator_ack_intensity: u8,
}

impl RenderStyle {
    pub fn stroke_for_layer(self, layer: MapLayer) -> StrokeStyle {
        match layer {
            MapLayer::MajorRoad => self.major_road,
            MapLayer::MinorRoad => self.minor_road,
            MapLayer::Path => self.path,
            MapLayer::RiderOverlay => self.major_road,
        }
    }
}

impl Default for RenderStyle {
    fn default() -> Self {
        Self {
            background_intensity: 18,
            major_road: StrokeStyle {
                intensity: 220,
                thickness_px: 3,
            },
            minor_road: StrokeStyle {
                intensity: 184,
                thickness_px: 2,
            },
            path: StrokeStyle {
                intensity: 144,
                thickness_px: 1,
            },
            rider_fill_intensity: 255,
            rider_heading_intensity: 230,
            north_indicator_active_intensity: 240,
            north_indicator_acquisition_intensity: 184,
            north_indicator_idle_intensity: 120,
            north_indicator_locked_intensity: 255,
            north_indicator_ring_intensity: 208,
            north_indicator_ack_intensity: 220,
        }
    }
}
