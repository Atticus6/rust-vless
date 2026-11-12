use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Per-user traffic counters. Counters are in-memory and reset when the node
/// reports a batch to the master.
#[derive(Debug, Default)]
pub struct UserStats {
    up: AtomicU64,
    down: AtomicU64,
}

impl UserStats {
    pub fn add_up(&self, n: u64) {
        if n > 0 {
            self.up.fetch_add(n, Ordering::Relaxed);
        }
    }

    pub fn add_down(&self, n: u64) {
        if n > 0 {
            self.down.fetch_add(n, Ordering::Relaxed);
        }
    }

    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.up.load(Ordering::Relaxed),
            self.down.load(Ordering::Relaxed),
        )
    }

    /// Atomically take and reset the counters.
    pub fn take(&self) -> (u64, u64) {
        (
            self.up.swap(0, Ordering::Relaxed),
            self.down.swap(0, Ordering::Relaxed),
        )
    }
}

/// Thread-safe UUID registry shared by the VLESS handler and RPC config sync.
#[derive(Debug, Default)]
pub struct UserRegistry {
    users: RwLock<HashMap<Uuid, Arc<UserStats>>>,
}

impl UserRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn valid(&self, id: Uuid) -> bool {
        self.users
            .read()
            .expect("registry lock poisoned")
            .contains_key(&id)
    }

    pub fn stats_for(&self, id: Uuid) -> Option<Arc<UserStats>> {
        self.users
            .read()
            .expect("registry lock poisoned")
            .get(&id)
            .cloned()
    }

    pub fn list(&self) -> Vec<Uuid> {
        self.users
            .read()
            .expect("registry lock poisoned")
            .keys()
            .copied()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.users.read().expect("registry lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Replace the registry with `ids`, preserving counters for users that
    /// remain enabled. Returns (added, removed, total).
    pub fn replace_all(&self, ids: impl IntoIterator<Item = Uuid>) -> (usize, usize, usize) {
        let mut next: HashMap<Uuid, Arc<UserStats>> = HashMap::new();
        let mut added = 0usize;

        for id in ids {
            if next.contains_key(&id) {
                continue;
            }
            let stats = {
                let current = self.users.read().expect("registry lock poisoned");
                current.get(&id).cloned()
            };
            match stats {
                Some(existing) => {
                    next.insert(id, existing);
                }
                None => {
                    next.insert(id, Arc::new(UserStats::default()));
                    added += 1;
                }
            }
        }

        let mut guard = self.users.write().expect("registry lock poisoned");
        let removed = guard.keys().filter(|id| !next.contains_key(id)).count();
        let total = next.len();
        *guard = next;
        (added, removed, total)
    }

    /// Snapshot all counters without resetting them.
    pub fn snapshot_all(&self) -> Vec<(Uuid, u64, u64)> {
        let guard = self.users.read().expect("registry lock poisoned");
        guard
            .iter()
            .map(|(id, stats)| {
                let (up, down) = stats.snapshot();
                (*id, up, down)
            })
            .collect()
    }

    /// Take all counters, resetting every user to zero.
    pub fn take_all(&self) -> Vec<(Uuid, u64, u64)> {
        let guard = self.users.read().expect("registry lock poisoned");
        guard
            .iter()
            .map(|(id, stats)| {
                let (up, down) = stats.take();
                (*id, up, down)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn replace_preserves_existing_stats() {
        let reg = UserRegistry::new();
        reg.replace_all([id(1), id(2)]);
        reg.stats_for(id(1)).unwrap().add_up(10);

        let (added, removed, total) = reg.replace_all([id(1), id(3)]);
        assert_eq!((added, removed, total), (1, 1, 2));
        assert_eq!(reg.stats_for(id(1)).unwrap().snapshot().0, 10);
        assert!(!reg.valid(id(2)));
        assert!(reg.valid(id(3)));
    }

    #[test]
    fn take_resets_counters() {
        let reg = UserRegistry::new();
        reg.replace_all([id(1)]);
        let stats = reg.stats_for(id(1)).unwrap();
        stats.add_up(7);
        stats.add_down(3);
        assert_eq!(reg.take_all(), vec![(id(1), 7, 3)]);
        assert_eq!(stats.snapshot(), (0, 0));
    }
}
