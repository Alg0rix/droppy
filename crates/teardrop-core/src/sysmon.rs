//! Live system-monitor model for the System page: rolling histories plus the
//! latest CPU / GPU / memory / drive / network / battery readings. Sampling is
//! platform code (`teardrop::sysmon`); this module is the pure, testable shape
//! of what it produces and the formatting the page needs.
use std::collections::VecDeque;

/// Seconds of history every chart keeps (one sample per second).
pub const HISTORY: usize = 60;

/// Fixed-length ring of recent samples, oldest first.
#[derive(Debug, Clone, Default)]
pub struct Series {
    vals: VecDeque<f32>,
}

impl Series {
    pub fn push(&mut self, v: f32) {
        if self.vals.len() == HISTORY {
            self.vals.pop_front();
        }
        self.vals.push_back(if v.is_finite() { v.max(0.0) } else { 0.0 });
    }

    pub fn last(&self) -> f32 {
        self.vals.back().copied().unwrap_or(0.0)
    }

    pub fn peak(&self) -> f32 {
        self.vals.iter().copied().fold(0.0, f32::max)
    }

    pub fn avg(&self) -> f32 {
        if self.vals.is_empty() {
            0.0
        } else {
            self.vals.iter().sum::<f32>() / self.vals.len() as f32
        }
    }

    pub fn len(&self) -> usize {
        self.vals.len()
    }

    /// Samples oldest-first.
    pub fn iter(&self) -> impl Iterator<Item = f32> + '_ {
        self.vals.iter().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.vals.is_empty()
    }

    /// Samples oldest-first, left-padded with zeros to a full window so a
    /// chart always spans its whole width and fills in from the right.
    pub fn padded(&self) -> Vec<f32> {
        let mut out = vec![0.0; HISTORY - self.vals.len()];
        out.extend(self.vals.iter().copied());
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoreClass {
    Efficiency,
    Performance,
    #[default]
    Unknown,
}

/// A process's share of some resource (CPU %, GPU %, or memory in GB).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcLoad {
    pub name: String,
    /// Full image path (for the app icon); may be empty.
    pub path: String,
    pub value: f32,
}

#[derive(Debug, Clone, Default)]
pub struct CpuStats {
    pub name: String,
    pub total: f32,
    pub user: f32,
    pub system: f32,
    pub idle: f32,
    /// Per logical core, 0..100.
    pub cores: Vec<f32>,
    pub classes: Vec<CoreClass>,
    pub ghz: f32,
    pub temp_c: Option<f32>,
    pub physical: u32,
    pub history: Series,
}

impl CpuStats {
    /// Mean load of the efficiency / performance cores, when the CPU is hybrid.
    pub fn class_load(&self, class: CoreClass) -> Option<f32> {
        let v: Vec<f32> = self.cores.iter().zip(&self.classes).filter(|(_, c)| **c == class).map(|(l, _)| *l).collect();
        (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
    }

    pub fn is_hybrid(&self) -> bool {
        self.classes.contains(&CoreClass::Efficiency) && self.classes.contains(&CoreClass::Performance)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GpuStats {
    pub name: String,
    pub available: bool,
    pub util: f32,
    pub mem_used_gb: f32,
    pub mem_total_gb: f32,
    pub temp_c: Option<f32>,
    /// Utilisation per engine kind (3D, Copy, Video Decode, ...), busiest first.
    pub engines: Vec<(String, f32)>,
    pub history: Series,
    pub mem_history: Series,
    /// Busiest processes by GPU %.
    pub procs: Vec<ProcLoad>,
}

impl GpuStats {
    pub fn mem_pct(&self) -> f32 {
        pct(self.mem_used_gb, self.mem_total_gb)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemStats {
    pub used_gb: f32,
    pub total_gb: f32,
    pub available_gb: f32,
    pub commit_used_gb: f32,
    pub commit_limit_gb: f32,
    pub cached_gb: f32,
    pub history: Series,
    /// Biggest processes by working set, in GB.
    pub procs: Vec<ProcLoad>,
}

impl MemStats {
    pub fn pct(&self) -> f32 {
        pct(self.used_gb, self.total_gb)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Volume {
    pub label: String,
    pub used_gb: f32,
    pub total_gb: f32,
}

impl Volume {
    pub fn pct(&self) -> f32 {
        pct(self.used_gb, self.total_gb)
    }
}

#[derive(Debug, Clone, Default)]
pub struct DriveStats {
    pub read_bps: f32,
    pub write_bps: f32,
    pub read_hist: Series,
    pub write_hist: Series,
    pub volumes: Vec<Volume>,
    /// Time the busiest disk spent busy, 0..100.
    pub busy: f32,
}

#[derive(Debug, Clone, Default)]
pub struct NetStats {
    pub adapter: String,
    pub down_bps: f32,
    pub up_bps: f32,
    pub down_hist: Series,
    pub up_hist: Series,
    /// Bytes moved since Droppy started sampling.
    pub session_down: u64,
    pub session_up: u64,
}

#[derive(Debug, Clone, Default)]
pub struct BatteryStats {
    pub present: bool,
    pub pct: f32,
    pub charging: bool,
    pub on_ac: bool,
    pub mins_left: Option<u32>,
    pub saver: bool,
    pub history: Series,
}

#[derive(Debug, Clone, Default)]
pub struct SysView {
    /// False until the first full sample has landed.
    pub loaded: bool,
    pub cpu: CpuStats,
    pub gpu: GpuStats,
    pub mem: MemStats,
    pub drives: DriveStats,
    pub net: NetStats,
    pub battery: BatteryStats,
    /// Busiest processes by CPU %.
    pub procs: Vec<ProcLoad>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SysTab {
    #[default]
    Cpu,
    Gpu,
    Memory,
    Drives,
    Network,
    Battery,
}

impl SysTab {
    pub const ALL: [SysTab; 6] = [SysTab::Cpu, SysTab::Gpu, SysTab::Memory, SysTab::Drives, SysTab::Network, SysTab::Battery];

    pub fn label(self) -> &'static str {
        match self {
            SysTab::Cpu => "CPU",
            SysTab::Gpu => "GPU",
            SysTab::Memory => "Memory",
            SysTab::Drives => "Drives",
            SysTab::Network => "Network",
            SysTab::Battery => "Battery",
        }
    }

    /// Next/previous tab, wrapping.
    pub fn step(self, d: i32) -> SysTab {
        let i = Self::ALL.iter().position(|&t| t == self).unwrap_or(0) as i32;
        Self::ALL[(i + d).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

fn pct(used: f32, total: f32) -> f32 {
    if total <= 0.0 {
        0.0
    } else {
        (used / total * 100.0).clamp(0.0, 100.0)
    }
}

/// "812 B/s", "1.5 KB/s", "12.4 MB/s", "1.20 GB/s".
pub fn fmt_rate(bps: f32) -> String {
    let b = bps.max(0.0);
    if b < 1000.0 {
        format!("{b:.0} B/s")
    } else if b < 1_000_000.0 {
        format!("{:.1} KB/s", b / 1e3)
    } else if b < 1_000_000_000.0 {
        format!("{:.1} MB/s", b / 1e6)
    } else {
        format!("{:.2} GB/s", b / 1e9)
    }
}

/// "850 MB", "12.4 GB", "1.8 TB" for a byte count.
pub fn fmt_bytes(n: u64) -> String {
    let b = n as f64;
    if b < 1e6 {
        format!("{:.0} KB", b / 1e3)
    } else if b < 1e9 {
        format!("{:.0} MB", b / 1e6)
    } else if b < 1e12 {
        format!("{:.1} GB", b / 1e9)
    } else {
        format!("{:.2} TB", b / 1e12)
    }
}

/// "6.3 GB" for a GB value (MB below 1 GB).
pub fn fmt_gb(gb: f32) -> String {
    if gb < 1.0 {
        format!("{:.0} MB", gb * 1024.0)
    } else if gb < 100.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{gb:.0} GB")
    }
}

/// "3 h 12 min", "47 min".
pub fn fmt_mins(m: u32) -> String {
    if m >= 60 {
        format!("{} h {:02} min", m / 60, m % 60)
    } else {
        format!("{m} min")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_is_a_bounded_ring_and_pads_left() {
        let mut s = Series::default();
        assert_eq!(s.padded().len(), HISTORY);
        for i in 0..(HISTORY + 10) {
            s.push(i as f32);
        }
        assert_eq!(s.len(), HISTORY);
        assert_eq!(s.last(), (HISTORY + 9) as f32);
        assert_eq!(s.padded()[0], 10.0, "oldest 10 samples dropped");
        let mut t = Series::default();
        t.push(5.0);
        let p = t.padded();
        assert_eq!((p[HISTORY - 1], p[0]), (5.0, 0.0));
    }

    #[test]
    fn series_rejects_garbage() {
        let mut s = Series::default();
        s.push(f32::NAN);
        s.push(-3.0);
        s.push(f32::INFINITY);
        assert_eq!((s.peak(), s.last()), (0.0, 0.0));
    }

    #[test]
    fn hybrid_class_loads() {
        let c = CpuStats {
            cores: vec![10.0, 30.0, 50.0, 70.0],
            classes: vec![CoreClass::Performance, CoreClass::Performance, CoreClass::Efficiency, CoreClass::Efficiency],
            ..Default::default()
        };
        assert!(c.is_hybrid());
        assert_eq!(c.class_load(CoreClass::Performance), Some(20.0));
        assert_eq!(c.class_load(CoreClass::Efficiency), Some(60.0));
        assert!(!CpuStats::default().is_hybrid());
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_rate(812.0), "812 B/s");
        assert_eq!(fmt_rate(1_500_000.0), "1.5 MB/s");
        assert_eq!(fmt_rate(2.5e9), "2.50 GB/s");
        assert_eq!(fmt_bytes(850_000_000), "850 MB");
        assert_eq!(fmt_gb(0.5), "512 MB");
        assert_eq!(fmt_gb(6.34), "6.3 GB");
        assert_eq!(fmt_mins(47), "47 min");
        assert_eq!(fmt_mins(192), "3 h 12 min");
    }

    #[test]
    fn tabs_step_and_wrap() {
        assert_eq!(SysTab::Cpu.step(-1), SysTab::Battery);
        assert_eq!(SysTab::Battery.step(1), SysTab::Cpu);
        assert_eq!(SysTab::Cpu.step(2), SysTab::Memory);
    }

    #[test]
    fn percentages_clamp() {
        assert_eq!(Volume { label: "C:".into(), used_gb: 50.0, total_gb: 200.0 }.pct(), 25.0);
        assert_eq!(MemStats::default().pct(), 0.0);
        assert_eq!(GpuStats { mem_used_gb: 9.0, mem_total_gb: 8.0, ..Default::default() }.mem_pct(), 100.0);
    }
}
