use runtime_core::api::{MapQueryResult, MapQuerySpec};
use runtime_core::map::MapSource;

#[derive(Debug, Default)]
pub struct MapSourceBridge;

impl MapSource for MapSourceBridge {
    fn query(&self, _spec: &MapQuerySpec) -> MapQueryResult {
        MapQueryResult::default()
    }
}
