use map_runtime::EmbeddedMapSource;
use runtime_core::api::{MapQueryResult, MapQuerySpec};
use runtime_core::map::MapSource;

#[derive(Debug, Clone)]
pub struct MapSourceBridge {
    inner: EmbeddedMapSource,
}

impl MapSourceBridge {
    pub fn new(inner: EmbeddedMapSource) -> Self {
        Self { inner }
    }

    pub fn embedded(&self) -> &EmbeddedMapSource {
        &self.inner
    }
}

impl Default for MapSourceBridge {
    fn default() -> Self {
        Self::new(EmbeddedMapSource::default())
    }
}

impl MapSource for MapSourceBridge {
    fn query(&self, spec: &MapQuerySpec) -> MapQueryResult {
        self.inner.query(spec)
    }
}
