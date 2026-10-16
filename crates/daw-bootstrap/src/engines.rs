//! A factory of independent engines: glues the transport and the audio engine, which `daw-render` must not know about.

use std::sync::Arc;

use daw_contracts::{EngineFactory, EngineInstance};
use daw_engine::create_engine;
use daw_transport::create_transport;

pub struct OfflineEngines;

impl EngineFactory for OfflineEngines {
    fn create(&self) -> EngineInstance {
        let (transport, clock) = create_transport();
        let parts = create_engine(clock, Vec::new());
        EngineInstance { renderer: parts.renderer, dsp: Arc::clone(&parts.link) as _, snapshots: parts.link, transport }
    }
}
