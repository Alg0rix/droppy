//! System snapshot types (CPU/mem/disk/battery) — sampling is platform code.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub cpu_pct: f32,
    pub mem_used_gb: f32,
    pub mem_total_gb: f32,
    pub disk_used_gb: f32,
    pub disk_total_gb: f32,
    pub battery_pct: Option<u8>,
    pub charging: bool,
}

impl SystemSnapshot {
    pub fn mem_pct(&self) -> f32 {
        if self.mem_total_gb <= 0.0 {
            0.0
        } else {
            (self.mem_used_gb / self.mem_total_gb * 100.0).clamp(0.0, 100.0)
        }
    }
    pub fn disk_pct(&self) -> f32 {
        if self.disk_total_gb <= 0.0 {
            0.0
        } else {
            (self.disk_used_gb / self.disk_total_gb * 100.0).clamp(0.0, 100.0)
        }
    }
}
