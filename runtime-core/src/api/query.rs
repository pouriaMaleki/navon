#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPoint {
    pub x_m: f64,
    pub y_m: f64,
}

impl WorldPoint {
    pub const ORIGIN: Self = Self { x_m: 0.0, y_m: 0.0 };

    pub const fn new(x_m: f64, y_m: f64) -> Self {
        Self { x_m, y_m }
    }

    pub const fn translate(self, dx_m: f64, dy_m: f64) -> Self {
        Self {
            x_m: self.x_m + dx_m,
            y_m: self.y_m + dy_m,
        }
    }
}

impl Default for WorldPoint {
    fn default() -> Self {
        Self::ORIGIN
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBounds {
    pub min: WorldPoint,
    pub max: WorldPoint,
}

impl WorldBounds {
    pub fn from_center(center: WorldPoint, half_width_m: f64, half_height_m: f64) -> Self {
        Self {
            min: center.translate(-half_width_m, -half_height_m),
            max: center.translate(half_width_m, half_height_m),
        }
    }
}

impl Default for WorldBounds {
    fn default() -> Self {
        Self {
            min: WorldPoint::ORIGIN,
            max: WorldPoint::ORIGIN,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapPresentationBand {
    CloseDetail,
    RideDetail,
    NetworkOverview,
    DistrictOverview,
}

impl Default for MapPresentationBand {
    fn default() -> Self {
        Self::RideDetail
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapLayer {
    ArterialRoad,
    StreetRoad,
    BikeRouteMain,
    BikeRouteLocal,
    Footpath,
    BuildingOutline,
    RiderOverlay,
}

impl MapLayer {
    const fn bit(self) -> u16 {
        match self {
            Self::ArterialRoad => 1 << 0,
            Self::StreetRoad => 1 << 1,
            Self::BikeRouteMain => 1 << 2,
            Self::BikeRouteLocal => 1 << 3,
            Self::Footpath => 1 << 4,
            Self::BuildingOutline => 1 << 5,
            Self::RiderOverlay => 1 << 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LodMask(u16);

impl LodMask {
    pub const NONE: Self = Self(0);

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn contains(self, layer: MapLayer) -> bool {
        self.0 & layer.bit() != 0
    }

    pub const fn with_layer(self, layer: MapLayer) -> Self {
        Self(self.0 | layer.bit())
    }

    pub fn from_layers(layers: &[MapLayer]) -> Self {
        layers
            .iter()
            .copied()
            .fold(Self::NONE, |mask, layer| mask.with_layer(layer))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapQuerySpec {
    pub center: WorldPoint,
    pub bounds: WorldBounds,
    pub meters_per_pixel: f64,
    pub zoom: f32,
    pub presentation_band: MapPresentationBand,
    pub lod_mask: LodMask,
}

impl MapQuerySpec {
    pub fn new(
        center: WorldPoint,
        bounds: WorldBounds,
        meters_per_pixel: f64,
        zoom: f32,
        presentation_band: MapPresentationBand,
        lod_mask: LodMask,
    ) -> Self {
        Self {
            center,
            bounds,
            meters_per_pixel,
            zoom,
            presentation_band,
            lod_mask,
        }
    }
}

impl Default for MapQuerySpec {
    fn default() -> Self {
        Self {
            center: WorldPoint::ORIGIN,
            bounds: WorldBounds::default(),
            meters_per_pixel: 1.0,
            zoom: 0.0,
            presentation_band: MapPresentationBand::RideDetail,
            lod_mask: LodMask::NONE,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapPolylineCandidate {
    pub layer: MapLayer,
    pub points: Vec<WorldPoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapPointCandidate {
    pub layer: MapLayer,
    pub position: WorldPoint,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GeometryCandidate {
    Polyline(MapPolylineCandidate),
    Point(MapPointCandidate),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapQueryResult {
    pub geometry: Vec<GeometryCandidate>,
}
