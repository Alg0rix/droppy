//! System-monitor sampler (Windows): feeds the System page.
//!
//! A single background thread samples once a second. The cheap readings
//! (CPU, memory, disk and network throughput, battery) always run so charts
//! are already full when the page opens; the expensive ones (GPU engines,
//! per-process CPU / memory / GPU) only run while the page is on screen.
//! Everything comes from documented Win32 APIs: PDH performance counters,
//! `GlobalMemoryStatusEx` / `GetPerformanceInfo`, `GetSystemPowerStatus`,
//! DXGI adapter info and a Toolhelp process walk. Failures leave a field at
//! its last/default value — nothing here panics or blocks the UI.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use teardrop_core::sysmon::{SysTab, SysView};

static VIEW: OnceLock<Mutex<SysView>> = OnceLock::new();
static REV: AtomicU64 = AtomicU64::new(0);
/// 0 = page not showing; else 1 + SysTab index.
static ACTIVE: AtomicU8 = AtomicU8::new(0);
static WAKE: OnceLock<(Mutex<bool>, Condvar)> = OnceLock::new();
static NOTIFY: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

fn view() -> &'static Mutex<SysView> {
    VIEW.get_or_init(|| Mutex::new(SysView::default()))
}

fn wake() -> &'static (Mutex<bool>, Condvar) {
    WAKE.get_or_init(|| (Mutex::new(false), Condvar::new()))
}

/// Latest sample (cloned; call only when `revision()` changed).
pub fn snapshot() -> SysView {
    view().lock().unwrap().clone()
}

/// Bumps once per finished sample.
pub fn revision() -> u64 {
    REV.load(Ordering::Relaxed)
}

/// Register a callback that wakes the UI after each sample.
pub fn set_notify(f: impl Fn() + Send + Sync + 'static) {
    let _ = NOTIFY.set(Box::new(f));
}

/// Tell the sampler which tab is on screen (None = page closed). Opening the
/// page triggers an immediate sample instead of waiting out the second.
pub fn set_active(tab: Option<SysTab>) {
    let v = tab.map_or(0, |t| 1 + SysTab::ALL.iter().position(|&x| x == t).unwrap_or(0) as u8);
    if ACTIVE.swap(v, Ordering::Relaxed) == 0 && v != 0 {
        let (m, cv) = wake();
        *m.lock().unwrap() = true;
        cv.notify_all();
    }
}

/// Start the sampler thread (idempotent).
pub fn start() {
    static UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if UP.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = view();
    let _ = wake();
    #[cfg(windows)]
    std::thread::Builder::new().name("sysmon".into()).spawn(imp::run).ok();
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    use teardrop_core::sysmon::{CoreClass, ProcLoad, Volume};
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, FILETIME, MAX_PATH};
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetFileVersionInfoSizeW, GetFileVersionInfoW, GetLogicalDrives, VerQueryValueW,
    };
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue, PdhOpenQueryW,
        PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA,
    };
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, K32GetProcessMemoryInfo, PERFORMANCE_INFORMATION, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    use windows::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, GlobalMemoryStatusEx, RelationProcessorCore, MEMORYSTATUSEX, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };
    use windows::Win32::System::Threading::{GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};

    const GB: f32 = 1_073_741_824.0;

    // ------------------------------------------------------------- PDH

    struct Query(isize);

    impl Query {
        fn new() -> Option<Self> {
            let mut h = 0isize;
            (unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut h) } == 0).then_some(Self(h))
        }

        /// A counter handle, or 0 when this machine doesn't expose it.
        fn add(&self, path: &str) -> isize {
            let mut c = 0isize;
            if unsafe { PdhAddEnglishCounterW(self.0, &HSTRING::from(path), 0, &mut c) } == 0 {
                c
            } else {
                0
            }
        }

        fn collect(&self) -> bool {
            unsafe { PdhCollectQueryData(self.0) == 0 }
        }
    }

    impl Drop for Query {
        fn drop(&mut self) {
            unsafe {
                PdhCloseQuery(self.0);
            }
        }
    }

    fn value(c: isize) -> Option<f64> {
        if c == 0 {
            return None;
        }
        let mut v = PDH_FMT_COUNTERVALUE::default();
        unsafe {
            if PdhGetFormattedCounterValue(c, PDH_FMT_DOUBLE, None, &mut v) != 0 || v.CStatus > 1 {
                return None;
            }
            Some(v.Anonymous.doubleValue)
        }
    }

    /// (instance name, value) for a wildcard counter.
    fn array(c: isize) -> Vec<(String, f64)> {
        if c == 0 {
            return Vec::new();
        }
        unsafe {
            let (mut size, mut count) = (0u32, 0u32);
            if PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, None) != PDH_MORE_DATA || size == 0 {
                return Vec::new();
            }
            // u64 backing keeps the item array 8-byte aligned.
            let mut buf = vec![0u64; size as usize / 8 + 1];
            let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items)) != 0 {
                return Vec::new();
            }
            (0..count as usize)
                .filter_map(|i| {
                    let it = &*items.add(i);
                    (it.FmtValue.CStatus <= 1).then(|| (it.szName.to_string().unwrap_or_default(), it.FmtValue.Anonymous.doubleValue))
                })
                .collect()
        }
    }

    // ------------------------------------------------------ static facts

    fn cpu_name() -> String {
        unsafe {
            let mut buf = [0u16; 128];
            let mut len = (buf.len() * 2) as u32;
            let r = RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0"),
                w!("ProcessorNameString"),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len),
            );
            if r.is_err() {
                return String::new();
            }
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            let s = String::from_utf16_lossy(&buf[..end]);
            // "13th Gen Intel(R) Core(TM) i7-13700K" reads better without the marks.
            s.replace("(R)", "").replace("(TM)", "").replace("  ", " ").trim().to_string()
        }
    }

    /// (per-logical-core class, physical core count). Hybrid CPUs report
    /// distinct EfficiencyClass values; higher = faster cores.
    fn core_layout(logical: usize) -> (Vec<CoreClass>, u32) {
        let mut classes = vec![CoreClass::Unknown; logical];
        unsafe {
            let mut len = 0u32;
            let _ = GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len);
            if len == 0 {
                return (classes, 0);
            }
            let mut buf = vec![0u64; len as usize / 8 + 1];
            let base = buf.as_mut_ptr() as *mut u8;
            if GetLogicalProcessorInformationEx(RelationProcessorCore, Some(base.cast()), &mut len).is_err() {
                return (classes, 0);
            }
            // (efficiency class, group-0 mask) per physical core.
            let mut cores: Vec<(u8, usize)> = Vec::new();
            let mut off = 0usize;
            while off + std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>().min(16) <= len as usize {
                let info = &*(base.add(off) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX);
                if info.Size == 0 {
                    break;
                }
                let p = &info.Anonymous.Processor;
                if p.GroupCount > 0 && p.GroupMask[0].Group == 0 {
                    cores.push((p.EfficiencyClass, p.GroupMask[0].Mask));
                }
                off += info.Size as usize;
            }
            let (lo, hi) = (cores.iter().map(|c| c.0).min().unwrap_or(0), cores.iter().map(|c| c.0).max().unwrap_or(0));
            if hi > lo {
                for (class, mask) in &cores {
                    let kind = if *class == hi { CoreClass::Performance } else { CoreClass::Efficiency };
                    for (bit, slot) in classes.iter_mut().enumerate().take(usize::BITS as usize) {
                        if mask >> bit & 1 == 1 {
                            *slot = kind;
                        }
                    }
                }
            }
            (classes, cores.len() as u32)
        }
    }

    struct Gpu {
        name: String,
        luid: String,
        dedicated_gb: f32,
        shared_gb: f32,
    }

    /// The busiest hardware adapter (most dedicated memory; falls back to
    /// shared-memory iGPUs). Software renderers are skipped.
    fn find_gpu() -> Option<Gpu> {
        unsafe {
            let f: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
            let mut best: Option<Gpu> = None;
            let mut i = 0;
            while let Ok(a) = f.EnumAdapters1(i) {
                i += 1;
                let Ok(d) = a.GetDesc1() else { continue };
                if d.VendorId == 0x1414 || d.Flags & 2 != 0 {
                    continue;
                }
                let end = d.Description.iter().position(|&c| c == 0).unwrap_or(d.Description.len());
                let g = Gpu {
                    name: String::from_utf16_lossy(&d.Description[..end]),
                    luid: format!("luid_0x{:08X}_0x{:08X}", d.AdapterLuid.HighPart as u32, d.AdapterLuid.LowPart),
                    dedicated_gb: d.DedicatedVideoMemory as f32 / GB,
                    shared_gb: d.SharedSystemMemory as f32 / GB,
                };
                if best.as_ref().map_or(true, |b| g.dedicated_gb > b.dedicated_gb) {
                    best = Some(g);
                }
            }
            best
        }
    }

    // ---------------------------------------------------------- processes

    #[derive(Default)]
    struct ProcTracker {
        /// pid -> (cpu 100ns ticks at last sample)
        last: HashMap<u32, u64>,
        at: Option<Instant>,
        paths: HashMap<u32, String>,
        names: HashMap<String, String>,
        logical: f32,
    }

    struct ProcSample {
        pid: u32,
        exe: String,
        cpu: f32,
        mem_gb: f32,
    }

    fn ft(f: FILETIME) -> u64 {
        ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
    }

    impl ProcTracker {
        fn path_of(&mut self, pid: u32) -> String {
            if let Some(p) = self.paths.get(&pid) {
                return p.clone();
            }
            let path = unsafe {
                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                    .ok()
                    .map(|h| {
                        let mut buf = [0u16; MAX_PATH as usize * 2];
                        let mut n = buf.len() as u32;
                        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
                        let _ = CloseHandle(h);
                        if ok { String::from_utf16_lossy(&buf[..n as usize]) } else { String::new() }
                    })
                    .unwrap_or_default()
            };
            self.paths.insert(pid, path.clone());
            path
        }

        /// "Microsoft Edge" from the exe's version resource, else "msedge".
        fn friendly(&mut self, exe: &str, path: &str) -> String {
            if let Some(n) = self.names.get(exe) {
                return n.clone();
            }
            let name = file_description(path).unwrap_or_else(|| {
                let stem = exe.trim_end_matches(".exe").trim_end_matches(".EXE");
                let mut c = stem.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            });
            self.names.insert(exe.to_string(), name.clone());
            name
        }

        /// One pass over all processes: CPU % since the previous pass and the
        /// working set. Returns per-pid samples (the caller groups by exe).
        fn sample(&mut self) -> Vec<ProcSample> {
            let now = Instant::now();
            let wall = self.at.map(|t| now.duration_since(t).as_secs_f64() * 1e7).unwrap_or(0.0);
            self.at = Some(now);
            let mut out = Vec::new();
            let mut seen = HashMap::new();
            unsafe {
                let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
                let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
                let mut ok = Process32FirstW(snap, &mut e).is_ok();
                while ok {
                    let pid = e.th32ProcessID;
                    if pid > 4 {
                        let end = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                        let exe = String::from_utf16_lossy(&e.szExeFile[..end]);
                        if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                            let (mut c, mut x, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
                            let mut cpu = 0.0;
                            if GetProcessTimes(h, &mut c, &mut x, &mut k, &mut u).is_ok() {
                                let total = ft(k) + ft(u);
                                if let (Some(prev), true) = (self.last.get(&pid), wall > 0.0) {
                                    cpu = (total.saturating_sub(*prev) as f64 / (wall * self.logical.max(1.0) as f64) * 100.0) as f32;
                                }
                                seen.insert(pid, total);
                            }
                            let mut mc = PROCESS_MEMORY_COUNTERS::default();
                            mc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                            let mem = if K32GetProcessMemoryInfo(h, &mut mc, mc.cb).as_bool() { mc.WorkingSetSize as f32 / GB } else { 0.0 };
                            let _ = CloseHandle(h);
                            out.push(ProcSample { pid, exe, cpu: cpu.clamp(0.0, 100.0), mem_gb: mem });
                        }
                    }
                    ok = Process32NextW(snap, &mut e).is_ok();
                }
                let _ = CloseHandle(snap);
            }
            self.last = seen;
            self.paths.retain(|pid, _| self.last.contains_key(pid));
            out
        }
    }

    fn file_description(path: &str) -> Option<String> {
        if path.is_empty() {
            return None;
        }
        unsafe {
            let p = HSTRING::from(path);
            let size = GetFileVersionInfoSizeW(&p, None);
            if size == 0 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            GetFileVersionInfoW(&p, 0, size, buf.as_mut_ptr().cast()).ok()?;
            let (mut ptr, mut len) = (std::ptr::null_mut::<std::ffi::c_void>(), 0u32);
            let mut lang = (0x0409u16, 0x04B0u16);
            if VerQueryValueW(buf.as_ptr().cast(), w!("\\VarFileInfo\\Translation"), &mut ptr, &mut len).as_bool() && len >= 4 {
                let t = ptr as *const u16;
                lang = (*t, *t.add(1));
            }
            let sub = HSTRING::from(format!("\\StringFileInfo\\{:04x}{:04x}\\FileDescription", lang.0, lang.1));
            if !VerQueryValueW(buf.as_ptr().cast(), &sub, &mut ptr, &mut len).as_bool() || len == 0 {
                return None;
            }
            let s = std::slice::from_raw_parts(ptr as *const u16, len as usize);
            let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
            let name = String::from_utf16_lossy(&s[..end]).trim().to_string();
            (!name.is_empty()).then_some(name)
        }
    }

    // ------------------------------------------------------------ volumes

    fn volumes() -> Vec<Volume> {
        let mut out = Vec::new();
        unsafe {
            let mask = GetLogicalDrives();
            for i in 0..26u32 {
                if mask >> i & 1 == 0 {
                    continue;
                }
                let letter = (b'A' + i as u8) as char;
                let root = HSTRING::from(format!("{letter}:\\"));
                if GetDriveTypeW(&root) != 3 {
                    continue; // fixed disks only
                }
                let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
                if GetDiskFreeSpaceExW(&root, Some(&mut avail), Some(&mut total), Some(&mut free)).is_ok() && total > 0 {
                    out.push(Volume { label: format!("{letter}:"), used_gb: (total - free) as f32 / GB, total_gb: total as f32 / GB });
                }
            }
        }
        out
    }

    // ------------------------------------------------------------- sampler

    struct Counters {
        cpu_user: isize,
        cpu_priv: isize,
        cpu_idle: isize,
        cores: isize,
        perf: isize,
        mhz: isize,
        temp: isize,
        disk_r: isize,
        disk_w: isize,
        disk_busy: isize,
        net_rx: isize,
        net_tx: isize,
    }

    struct HeavyCounters {
        engines: isize,
        mem_ded: isize,
        mem_shared: isize,
    }

    pub fn run() {
        let Some(base) = Query::new() else { return };
        let c = Counters {
            cpu_user: base.add("\\Processor(_Total)\\% User Time"),
            cpu_priv: base.add("\\Processor(_Total)\\% Privileged Time"),
            cpu_idle: base.add("\\Processor(_Total)\\% Idle Time"),
            cores: base.add("\\Processor(*)\\% Processor Time"),
            perf: base.add("\\Processor Information(_Total)\\% Processor Performance"),
            mhz: base.add("\\Processor Information(_Total)\\Processor Frequency"),
            temp: base.add("\\Thermal Zone Information(*)\\Temperature"),
            disk_r: base.add("\\PhysicalDisk(_Total)\\Disk Read Bytes/sec"),
            disk_w: base.add("\\PhysicalDisk(_Total)\\Disk Write Bytes/sec"),
            disk_busy: base.add("\\PhysicalDisk(_Total)\\% Disk Time"),
            net_rx: base.add("\\Network Interface(*)\\Bytes Received/sec"),
            net_tx: base.add("\\Network Interface(*)\\Bytes Sent/sec"),
        };
        let heavy_q = Query::new();
        let h = heavy_q.as_ref().map(|q| HeavyCounters {
            engines: q.add("\\GPU Engine(*)\\Utilization Percentage"),
            mem_ded: q.add("\\GPU Adapter Memory(*)\\Dedicated Usage"),
            mem_shared: q.add("\\GPU Adapter Memory(*)\\Shared Usage"),
        });

        let logical = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        let (classes, physical) = core_layout(logical);
        let gpu = find_gpu();
        let mut procs = ProcTracker { logical: logical as f32, ..Default::default() };
        {
            let mut v = view().lock().unwrap();
            v.cpu.name = cpu_name();
            v.cpu.classes = classes;
            v.cpu.physical = physical;
            if let Some(g) = &gpu {
                v.gpu.name = g.name.clone();
                v.gpu.available = true;
                v.gpu.mem_total_gb = if g.dedicated_gb >= 0.5 { g.dedicated_gb } else { g.shared_gb / 2.0 };
            }
        }

        let mut tick = 0u64;
        let mut heavy_was_on = false;
        let (mut down_total, mut up_total) = (0u64, 0u64);
        let _ = base.collect(); // arm the rate counters
        loop {
            // Sleep a second, or less if the page just opened.
            {
                let (m, cv) = wake();
                let g = m.lock().unwrap();
                let (mut g, _) = cv.wait_timeout(g, Duration::from_secs(1)).unwrap();
                *g = false;
            }
            tick += 1;
            let active = ACTIVE.load(Ordering::Relaxed);
            let _ = base.collect();

            // CPU ------------------------------------------------------
            let user = value(c.cpu_user).unwrap_or(0.0) as f32;
            let sys = value(c.cpu_priv).unwrap_or(0.0) as f32;
            let idle = value(c.cpu_idle).unwrap_or(100.0) as f32;
            let total = (100.0 - idle).clamp(0.0, 100.0);
            let mut cores: Vec<(usize, f32)> = array(c.cores)
                .into_iter()
                .filter_map(|(n, v)| n.parse::<usize>().ok().map(|i| (i, v as f32)))
                .collect();
            cores.sort_by_key(|c| c.0);
            let ghz = match (value(c.mhz), value(c.perf)) {
                (Some(mhz), Some(p)) => (mhz * p / 100.0 / 1000.0) as f32,
                (Some(mhz), None) => (mhz / 1000.0) as f32,
                _ => 0.0,
            };
            let temp = array(c.temp)
                .into_iter()
                .map(|(_, k)| k - 273.15)
                .filter(|t| (10.0..120.0).contains(t))
                .fold(None, |m: Option<f64>, t| Some(m.map_or(t, |m| m.max(t))))
                .map(|t| t as f32);

            // Memory ---------------------------------------------------
            let mut mem = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
            let have_mem = unsafe { GlobalMemoryStatusEx(&mut mem).is_ok() };
            let mut pi = PERFORMANCE_INFORMATION { cb: std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32, ..Default::default() };
            let have_pi = unsafe { GetPerformanceInfo(&mut pi, pi.cb).is_ok() };
            let page = pi.PageSize as f32;

            // Network / disk -------------------------------------------
            let real_nic = |n: &str| {
                let l = n.to_ascii_lowercase();
                !(l.contains("loopback") || l.contains("isatap") || l.contains("teredo") || l.contains("pseudo"))
            };
            let rx = array(c.net_rx);
            let tx = array(c.net_tx);
            let down: f32 = rx.iter().filter(|(n, _)| real_nic(n)).map(|(_, v)| *v as f32).sum();
            let up: f32 = tx.iter().filter(|(n, _)| real_nic(n)).map(|(_, v)| *v as f32).sum();
            down_total += down as u64;
            up_total += up as u64;
            let adapter = rx
                .iter()
                .filter(|(n, _)| real_nic(n))
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(n, _)| n.clone())
                .unwrap_or_default();

            // Battery --------------------------------------------------
            let mut ps = SYSTEM_POWER_STATUS::default();
            let have_ps = unsafe { GetSystemPowerStatus(&mut ps).is_ok() };

            // Heavy: GPU + processes, only while the page shows ---------
            let heavy_on = active != 0;
            let mut gpu_util = None;
            let mut gpu_mem = None;
            let mut engines: Vec<(String, f32)> = Vec::new();
            let mut gpu_procs: HashMap<u32, f32> = HashMap::new();
            if let (true, Some(q), Some(hc), Some(g)) = (heavy_on, heavy_q.as_ref(), h.as_ref(), gpu.as_ref()) {
                let _ = q.collect();
                if heavy_was_on {
                    // Per engine-kind sum across processes; the busiest kind is
                    // the adapter's utilisation (what Task Manager shows).
                    let mut by_kind: HashMap<String, f32> = HashMap::new();
                    for (name, v) in array(hc.engines) {
                        if !name.contains(&g.luid) {
                            continue;
                        }
                        let kind = name.rsplit("engtype_").next().unwrap_or("").to_string();
                        *by_kind.entry(kind).or_default() += v as f32;
                        if let Some(pid) = name.strip_prefix("pid_").and_then(|r| r.split('_').next()).and_then(|p| p.parse::<u32>().ok()) {
                            *gpu_procs.entry(pid).or_default() += v as f32;
                        }
                    }
                    engines = by_kind.into_iter().map(|(k, v)| (engine_label(&k), v.min(100.0))).filter(|(_, v)| *v > 0.05).collect();
                    engines.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    gpu_util = Some(engines.iter().map(|e| e.1).fold(0.0, f32::max));
                    let pick = |arr: Vec<(String, f64)>| arr.into_iter().filter(|(n, _)| n.contains(&g.luid)).map(|(_, v)| v as f32).sum::<f32>() / GB;
                    gpu_mem = Some(if g.dedicated_gb >= 0.5 { pick(array(hc.mem_ded)) } else { pick(array(hc.mem_shared)) });
                }
            }
            let proc_samples = if heavy_on { procs.sample() } else { Vec::new() };
            heavy_was_on = heavy_on;

            // Group processes by exe --------------------------------------
            let mut by_exe: HashMap<String, (f32, f32, f32, u32)> = HashMap::new(); // cpu, mem, gpu, a pid
            for p in &proc_samples {
                let e = by_exe.entry(p.exe.to_ascii_lowercase()).or_insert((0.0, 0.0, 0.0, p.pid));
                e.0 += p.cpu;
                e.1 += p.mem_gb;
                e.2 += gpu_procs.get(&p.pid).copied().unwrap_or(0.0);
            }
            let mut top = |pick: fn(&(f32, f32, f32, u32)) -> f32, min: f32, n: usize| -> Vec<ProcLoad> {
                let mut v: Vec<(&String, &(f32, f32, f32, u32))> = by_exe.iter().filter(|(_, e)| pick(e) > min).collect();
                v.sort_by(|a, b| pick(b.1).partial_cmp(&pick(a.1)).unwrap_or(std::cmp::Ordering::Equal));
                v.truncate(n);
                v.into_iter()
                    .map(|(exe, e)| {
                        let path = procs.path_of(e.3);
                        ProcLoad { name: procs.friendly(exe, &path), path, value: pick(e) }
                    })
                    .collect()
            };
            let top_cpu = if active == 0 { Vec::new() } else { top(|e| e.0, 0.05, 8) };
            let top_mem = if active == 0 { Vec::new() } else { top(|e| e.1, 0.0, 8) };
            let top_gpu = if active == 0 { Vec::new() } else { top(|e| e.2, 0.05, 6) };

            // Publish ----------------------------------------------------
            {
                let mut v = view().lock().unwrap();
                v.loaded = tick >= 2;
                v.cpu.total = total;
                v.cpu.user = user.min(total);
                v.cpu.system = sys.min(total - v.cpu.user.min(total)).max(0.0);
                v.cpu.idle = idle;
                v.cpu.cores = cores.iter().map(|c| c.1.clamp(0.0, 100.0)).collect();
                v.cpu.ghz = ghz;
                v.cpu.temp_c = temp;
                v.cpu.history.push(total);

                if have_mem {
                    v.mem.total_gb = mem.ullTotalPhys as f32 / GB;
                    v.mem.available_gb = mem.ullAvailPhys as f32 / GB;
                    v.mem.used_gb = (v.mem.total_gb - v.mem.available_gb).max(0.0);
                }
                if have_pi {
                    v.mem.commit_used_gb = pi.CommitTotal as f32 * page / GB;
                    v.mem.commit_limit_gb = pi.CommitLimit as f32 * page / GB;
                    v.mem.cached_gb = pi.SystemCache as f32 * page / GB;
                }
                let mp = v.mem.pct();
                v.mem.history.push(mp);
                if active != 0 {
                    v.mem.procs = top_mem;
                    v.procs = top_cpu;
                    v.gpu.procs = top_gpu;
                }

                v.drives.read_bps = value(c.disk_r).unwrap_or(0.0) as f32;
                v.drives.write_bps = value(c.disk_w).unwrap_or(0.0) as f32;
                v.drives.busy = (value(c.disk_busy).unwrap_or(0.0) as f32).clamp(0.0, 100.0);
                let (r, wr) = (v.drives.read_bps, v.drives.write_bps);
                v.drives.read_hist.push(r);
                v.drives.write_hist.push(wr);
                if tick % 5 == 1 || v.drives.volumes.is_empty() {
                    v.drives.volumes = volumes();
                }

                v.net.down_bps = down;
                v.net.up_bps = up;
                v.net.down_hist.push(down);
                v.net.up_hist.push(up);
                v.net.session_down = down_total;
                v.net.session_up = up_total;
                if !adapter.is_empty() {
                    v.net.adapter = adapter;
                }

                if have_ps {
                    let none = ps.BatteryFlag & 128 != 0 || ps.BatteryFlag == 255;
                    v.battery.present = !none && ps.BatteryLifePercent <= 100;
                    v.battery.pct = if ps.BatteryLifePercent <= 100 { ps.BatteryLifePercent as f32 } else { 0.0 };
                    v.battery.charging = ps.BatteryFlag & 8 != 0;
                    v.battery.on_ac = ps.ACLineStatus == 1;
                    v.battery.mins_left = (ps.BatteryLifeTime != u32::MAX).then(|| ps.BatteryLifeTime / 60);
                    v.battery.saver = ps.SystemStatusFlag == 1;
                    let bp = v.battery.pct;
                    v.battery.history.push(bp);
                }

                if let Some(u) = gpu_util {
                    v.gpu.util = u;
                    v.gpu.engines = engines;
                    v.gpu.history.push(u);
                }
                if let Some(m) = gpu_mem {
                    v.gpu.mem_used_gb = m;
                    v.gpu.mem_history.push(m);
                }
            }
            REV.fetch_add(1, Ordering::Relaxed);
            // Only wake the UI when someone is looking at the page.
            if active != 0 {
                if let Some(n) = NOTIFY.get() {
                    n();
                }
            }
        }
    }

    /// "3D", "Copy", "VideoDecode" -> a label for the page.
    fn engine_label(kind: &str) -> String {
        match kind {
            "3D" => "3D".into(),
            "Copy" => "Copy".into(),
            "Compute_0" | "Compute" => "Compute".into(),
            "VideoDecode" => "Video decode".into(),
            "VideoEncode" => "Video encode".into(),
            "VideoProcessing" => "Video processing".into(),
            "Cuda" => "CUDA".into(),
            other => other.replace('_', " "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sampler runs against the real machine: after a few seconds the
    /// always-on readings must be sane. Skipped gracefully where counters
    /// are unavailable (headless CI images).
    #[cfg(windows)]
    #[test]
    fn real_machine_sample_is_sane() {
        start();
        set_active(Some(SysTab::Cpu));
        let t0 = std::time::Instant::now();
        while revision() < 4 && t0.elapsed().as_secs() < 12 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if revision() < 4 {
            eprintln!("sysmon produced no samples; skipping");
            return;
        }
        let v = snapshot();
        assert!(v.mem.total_gb > 0.5, "total memory {}", v.mem.total_gb);
        assert!(v.mem.used_gb > 0.0 && v.mem.used_gb <= v.mem.total_gb + 0.1);
        assert!((0.0..=100.0).contains(&v.cpu.total), "cpu {}", v.cpu.total);
        assert!(!v.cpu.cores.is_empty(), "per-core counters");
        assert!(v.cpu.cores.iter().all(|c| (0.0..=100.0).contains(c)));
        assert_eq!(v.cpu.history.len(), v.mem.history.len());
        eprintln!(
            "cpu {:.0}% ({}) cores={} ghz={:.2} temp={:?} gpu={} {:.0}% vram {:.1}/{:.1} GB net {:.0}/{:.0} B/s vols={} procs={} battery={} {:.0}%",
            v.cpu.total, v.cpu.name, v.cpu.cores.len(), v.cpu.ghz, v.cpu.temp_c, v.gpu.name, v.gpu.util, v.gpu.mem_used_gb, v.gpu.mem_total_gb,
            v.net.down_bps, v.net.up_bps, v.drives.volumes.len(), v.procs.len(), v.battery.present, v.battery.pct
        );
        set_active(None);
    }
}
