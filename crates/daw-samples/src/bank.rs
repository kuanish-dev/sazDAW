//! `SampleBank`: "keep decoded samples in memory" for instruments and the engine.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use daw_contracts::{SampleData, SampleId, SampleStore, sync::lock};

#[derive(Default)]
pub struct SampleBank {
    items: Mutex<HashMap<SampleId, Arc<SampleData>>>,
}

impl SampleBank {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SampleStore for SampleBank {
    fn get(&self, id: SampleId) -> Option<Arc<SampleData>> {
        lock(&self.items).get(&id).cloned()
    }

    fn insert(&self, id: SampleId, data: Arc<SampleData>) {
        lock(&self.items).insert(id, data);
    }

    fn remove(&self, id: SampleId) {
        lock(&self.items).remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_stores_and_removes() {
        let bank = SampleBank::new();
        let data = Arc::new(SampleData { sample_rate: 48_000, channels: 1, frames: vec![0.5; 4] });
        bank.insert(7, Arc::clone(&data));
        assert_eq!(bank.get(7), Some(data));
        bank.remove(7);
        assert!(bank.get(7).is_none());
    }
}
