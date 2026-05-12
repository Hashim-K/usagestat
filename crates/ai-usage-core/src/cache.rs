use crate::UsageSnapshot;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct UsageCache {
    snapshots: BTreeMap<String, UsageSnapshot>,
}

impl UsageCache {
    pub fn upsert(&mut self, snapshot: UsageSnapshot) {
        self.snapshots
            .insert(snapshot.provider_id.clone(), snapshot);
    }

    pub fn get(&self, provider_id: &str) -> Option<&UsageSnapshot> {
        self.snapshots.get(provider_id)
    }

    pub fn list(&self) -> Vec<UsageSnapshot> {
        self.snapshots.values().cloned().collect()
    }
}
