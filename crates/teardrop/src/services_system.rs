//! Real system snapshot sampler (Windows): memory, disk, battery, CPU.
//!
//! Memory via `GlobalMemoryStatusEx`, disk via `GetDiskFreeSpaceExW` on the
//! system drive, battery via `GetSystemPowerStatus`, CPU via `GetSystemTimes`
//! deltas. All best-effort: failures yield the last good value, never panic.

#[cfg(windows)]
pub mod sys {
    use teardrop_core::SystemSnapshot;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::Win32::System::Power::GetSystemPowerStatus;
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows::Win32::System::Threading::GetSystemTimes;

    fn filetime_to_u64(ft: &FILETIME) -> u64 {
        ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
    }

    #[derive(Debug, Default)]
    pub struct CpuTracker {
        last_idle: u64,
        last_kernel: u64,
        last_user: u64,
        pct: f32,
        armed: bool,
    }

    impl CpuTracker {
        pub fn sample(&mut self) -> f32 {
            unsafe {
                let mut idle = FILETIME::default();
                let mut kernel = FILETIME::default();
                let mut user = FILETIME::default();
                if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_err() {
                    return self.pct;
                }
                let (i, k, u) = (filetime_to_u64(&idle), filetime_to_u64(&kernel), filetime_to_u64(&user));
                if !self.armed {
                    self.last_idle = i;
                    self.last_kernel = k;
                    self.last_user = u;
                    self.armed = true;
                    return self.pct;
                }
                let idle_d = i.saturating_sub(self.last_idle);
                let kern_d = k.saturating_sub(self.last_kernel);
                let user_d = u.saturating_sub(self.last_user);
                self.last_idle = i;
                self.last_kernel = k;
                self.last_user = u;
                let total = kern_d + user_d;
                if total == 0 {
                    return self.pct;
                }
                // Kernel includes idle on Windows; busy = total - idle.
                let busy = total.saturating_sub(idle_d);
                self.pct = (busy as f32 / total as f32 * 100.0).clamp(0.0, 100.0);
                self.pct
            }
        }
    }

    pub fn sample_all(cpu: &mut CpuTracker, prev: &SystemSnapshot) -> SystemSnapshot {
        let mut out = prev.clone();
        out.cpu_pct = cpu.sample();
        unsafe {
            let mut mem = MEMORYSTATUSEX::default();
            mem.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut mem).is_ok() && mem.ullTotalPhys > 0 {
                let total = mem.ullTotalPhys as f32 / 1_073_741_824.0;
                let avail = mem.ullAvailPhys as f32 / 1_073_741_824.0;
                out.mem_total_gb = total;
                out.mem_used_gb = (total - avail).max(0.0);
            }
            // System drive (where %SystemRoot% lives, usually C:\).
            let sysdrive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
            let root = format!("{}\\", sysdrive);
            let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let mut free_avail: u64 = 0;
            let mut total: u64 = 0;
            let mut total_free: u64 = 0;
            if GetDiskFreeSpaceExW(PCWSTR::from_raw(wide.as_ptr()), Some(&mut free_avail), Some(&mut total), Some(&mut total_free)).is_ok() {
                if total > 0 {
                    out.disk_total_gb = total as f32 / 1_073_741_824.0;
                    out.disk_used_gb = (total.saturating_sub(total_free)) as f32 / 1_073_741_824.0;
                }
            }
            let mut st = std::mem::zeroed::<windows::Win32::System::Power::SYSTEM_POWER_STATUS>();
            if GetSystemPowerStatus(&mut st).is_ok() {
                if st.BatteryLifePercent <= 100 {
                    out.battery_pct = Some(st.BatteryLifePercent);
                }
                // Bit 8 (0x08) = charging.
                out.charging = (st.BatteryFlag & 8) != 0;
            }
        }
        out
    }
}

#[cfg(not(windows))]
pub mod sys {
    use teardrop_core::SystemSnapshot;
    #[derive(Debug, Default)]
    pub struct CpuTracker;
    impl CpuTracker {
        pub fn sample(&mut self) -> f32 {
            0.0
        }
    }
    pub fn sample_all(_cpu: &mut CpuTracker, prev: &SystemSnapshot) -> SystemSnapshot {
        prev.clone()
    }
}
