use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use crate::agent;
use anyhow::Result;

/// Simple cached probe manager. Not production hardened — illustrative only.
pub struct ProbeManager {
    uri: String,
    timeout_secs: u64,
    cache_ttl: Duration,
    cache: Mutex<HashMap<String, (String, Instant)>>,
}

impl ProbeManager {
    pub fn new(uri: String, timeout: Duration, cache_ttl: Duration) -> Result<Self> {
        Ok(Self {
            uri,
            timeout_secs: timeout.as_secs(),
            cache_ttl,
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Get OS string for a VM, using cache if fresh.
    pub fn get_os(&self, vm: &str) -> Result<Option<String>> {
        {
            let c = self.cache.lock().unwrap();
            if let Some((val, ts)) = c.get(vm) {
                if ts.elapsed() < self.cache_ttl {
                    return Ok(Some(val.clone()));
                }
            }
        }

        // 1) guest-get-osinfo
        if let Ok(Some(s)) = agent::try_guest_get_osinfo(vm, self.timeout_secs) {
            self.store_cache(vm, &s);
            return Ok(Some(s));
        }

        // 2) guest-get-os
        if let Ok(Some(s)) = agent::try_guest_get_os(vm, self.timeout_secs) {
            self.store_cache(vm, &s);
            return Ok(Some(s));
        }

        // 3) guest-exec fallback (not implemented here; call into utils/virsh)
        Ok(None)
    }

    fn store_cache(&self, vm: &str, val: &str) {
        let mut c = self.cache.lock().unwrap();
        c.insert(vm.to_string(), (val.to_string(), Instant::now()));
    }
}
