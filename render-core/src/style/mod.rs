use runtime_core::api::MapLayer;

use crate::raster::Color;

pub const COLOR_BACKGROUND_CANVAS: Color = Color::new(0x05, 0x0B, 0x12);
pub const COLOR_SURFACE_INVERSE: Color = Color::new(0x00, 0x00, 0x00);
pub const COLOR_SURFACE_BASE: Color = Color::new(0x05, 0x1E, 0x24);
pub const COLOR_SURFACE_ELEVATED: Color = Color::new(0x10, 0x13, 0x2B);
pub const COLOR_BORDER_STRONG: Color = Color::new(0x10, 0x3B, 0x48);
pub const COLOR_ACCENT_PRIMARY: Color = Color::new(0x12, 0xA3, 0xA3);
pub const COLOR_ACCENT_HIGHLIGHT: Color = Color::new(0xD7, 0xFF, 0x3F);
pub const COLOR_TEXT_PRIMARY: Color = Color::new(0xFF, 0xFF, 0xFF);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrokeStyle {
    pub color: Color,
    pub thickness_px: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointStyle {
    pub badge_color: Color,
    pub icon_color: Color,
    pub badge_radius_px: u8,
    pub min_spacing_px: u8,
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
    pub bike_parking: PointStyle,
    pub bike_repair: PointStyle,
    pub supermarket: PointStyle,
    pub restaurant: PointStyle,
    pub cafe: PointStyle,
    pub water: PointStyle,
    pub wc: PointStyle,
    pub active_route_backdrop: StrokeStyle,
    pub active_route_line: StrokeStyle,
    pub rider_fill_color: Color,
    pub rider_heading_color: Color,
    pub north_indicator_active_color: Color,
    pub north_indicator_acquisition_color: Color,
    pub north_indicator_idle_color: Color,
    pub north_indicator_locked_color: Color,
    pub north_indicator_ring_color: Color,
    pub north_indicator_ack_color: Color,
    pub speed_panel_background_color: Color,
    pub speed_panel_text_color: Color,
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
            _ => self.arterial_road,
        }
    }

    pub fn point_for_layer(self, layer: MapLayer) -> Option<PointStyle> {
        match layer {
            MapLayer::BikeParking => Some(self.bike_parking),
            MapLayer::BikeRepair => Some(self.bike_repair),
            MapLayer::Supermarket => Some(self.supermarket),
            MapLayer::Restaurant => Some(self.restaurant),
            MapLayer::Cafe => Some(self.cafe),
            MapLayer::Water => Some(self.water),
            MapLayer::Wc => Some(self.wc),
            _ => None,
        }
    }
}

impl Default for RenderStyle {
    fn default() -> Self {
        let shared_poi_style = PointStyle {
            badge_color: COLOR_SURFACE_ELEVATED,
            icon_color: COLOR_ACCENT_PRIMARY,
            badge_radius_px: 8,
            min_spacing_px: 20,
        };
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
            bike_parking: shared_poi_style,
            bike_repair: shared_poi_style,
            supermarket: shared_poi_style,
            restaurant: shared_poi_style,
            cafe: shared_poi_style,
            water: shared_poi_style,
            wc: shared_poi_style,
            active_route_backdrop: StrokeStyle {
                color: COLOR_SURFACE_INVERSE,
                thickness_px: 6,
            },
            active_route_line: StrokeStyle {
                color: COLOR_ACCENT_HIGHLIGHT,
                thickness_px: 4,
            },
            rider_fill_color: COLOR_ACCENT_HIGHLIGHT,
            rider_heading_color: COLOR_BORDER_STRONG,
            north_indicator_active_color: COLOR_BORDER_STRONG,
            north_indicator_acquisition_color: COLOR_SURFACE_ELEVATED,
            north_indicator_idle_color: COLOR_SURFACE_BASE,
            north_indicator_locked_color: COLOR_ACCENT_PRIMARY,
            north_indicator_ring_color: COLOR_BORDER_STRONG,
            north_indicator_ack_color: COLOR_ACCENT_PRIMARY,
            speed_panel_background_color: COLOR_SURFACE_INVERSE,
            speed_panel_text_color: COLOR_TEXT_PRIMARY,
        }
    }
}
