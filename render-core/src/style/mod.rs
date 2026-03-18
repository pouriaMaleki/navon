use runtime_core::api::MapLayer;

use crate::raster::Color;

pub const COLOR_BACKGROUND_CANVAS: Color = Color::new(0x05, 0x0B, 0x12);
pub const COLOR_SURFACE_BASE: Color = Color::new(0x05, 0x1E, 0x24);
pub const COLOR_SURFACE_ELEVATED: Color = Color::new(0x10, 0x13, 0x2B);
pub const COLOR_BORDER_STRONG: Color = Color::new(0x10, 0x3B, 0x48);
pub const COLOR_ACCENT_PRIMARY: Color = Color::new(0x12, 0xA3, 0xA3);
pub const COLOR_ACCENT_HIGHLIGHT: Color = Color::new(0xD7, 0xFF, 0x3F);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrokeStyle {
    pub color: Color,
    pub thickness_px: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderStyle {
    pub background_color: Color,
    pub arterial_road: StrokeStyle,
    pub street_road: StrokeStyle,
    pub bike_route_main: StrokeStyle,
    pub bike_route_local: StrokeStyle,
    pub footpath: StrokeStyle,
    pub building_outline: StrokeStyle,
    pub rider_fill_color: Color,
    pub rider_heading_color: Color,
    pub north_indicator_active_color: Color,
    pub north_indicator_acquisition_color: Color,
    pub north_indicator_idle_color: Color,
    pub north_indicator_locked_color: Color,
    pub north_indicator_ring_color: Color,
    pub north_indicator_ack_color: Color,
}

impl RenderStyle {
    pub fn stroke_for_layer(self, layer: MapLayer) -> StrokeStyle {
        match layer {
            MapLayer::ArterialRoad => self.arterial_road,
            MapLayer::StreetRoad => self.street_road,
            MapLayer::BikeRouteMain => self.bike_route_main,
            MapLayer::BikeRouteLocal => self.bike_route_local,
            MapLayer::Footpath => self.footpath,
            MapLayer::BuildingOutline => self.building_outline,
            MapLayer::RiderOverlay => self.arterial_road,
        }
    }
}

impl Default for RenderStyle {
    fn default() -> Self {
        Self {
            background_color: COLOR_BACKGROUND_CANVAS,
            arterial_road: StrokeStyle {
                color: COLOR_ACCENT_PRIMARY,
                thickness_px: 3,
            },
            street_road: StrokeStyle {
                color: COLOR_ACCENT_PRIMARY,
                thickness_px: 2,
            },
            bike_route_main: StrokeStyle {
                color: COLOR_ACCENT_PRIMARY,
                thickness_px: 2,
            },
            bike_route_local: StrokeStyle {
                color: COLOR_BORDER_STRONG,
                thickness_px: 1,
            },
            footpath: StrokeStyle {
                color: COLOR_SURFACE_BASE,
                thickness_px: 1,
            },
            building_outline: StrokeStyle {
                color: COLOR_SURFACE_ELEVATED,
                thickness_px: 1,
            },
            rider_fill_color: COLOR_ACCENT_HIGHLIGHT,
            rider_heading_color: COLOR_BORDER_STRONG,
            north_indicator_active_color: COLOR_BORDER_STRONG,
            north_indicator_acquisition_color: COLOR_SURFACE_ELEVATED,
            north_indicator_idle_color: COLOR_SURFACE_BASE,
            north_indicator_locked_color: COLOR_ACCENT_PRIMARY,
            north_indicator_ring_color: COLOR_BORDER_STRONG,
            north_indicator_ack_color: COLOR_ACCENT_PRIMARY,
        }
    }
}
