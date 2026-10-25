//! Bluetooth connection watcher (Windows): names newly connected devices and,
//! when the stack reports one, their battery level, for the "Connected" card.
//!
//! A small thread polls the set of connected Bluetooth devices (classic and
//! LE, via the WinRT device enumeration the Settings app uses) every few
//! seconds. The first poll only records a baseline, so devices that are
//! already connected when Teardrop starts never pop a card. The battery
//! percentage is read from the device's PnP properties (the value Windows
//! shows next to a paired headset) and may simply be absent.

use std::sync::{Condvar, Mutex, OnceLock};

/// A device that just connected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtEvent {
    pub name: String,
    /// 0..=100 when the device reports it.
    pub battery: Option<u8>,
}

static PENDING: Mutex<Vec<BtEvent>> = Mutex::new(Vec::new());
static NOTIFY: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static NUDGE: OnceLock<(Mutex<bool>, Condvar)> = OnceLock::new();

fn nudge_pair() -> &'static (Mutex<bool>, Condvar) {
    NUDGE.get_or_init(|| (Mutex::new(false), Condvar::new()))
}

/// Register the callback that wakes the UI when a device connects.
pub fn set_notify(f: impl Fn() + Send + Sync + 'static) {
    let _ = NOTIFY.set(Box::new(f));
}

/// Newest connection since the last call, if any.
pub fn take() -> Option<BtEvent> {
    PENDING.lock().unwrap().drain(..).last()
}

/// Check right now instead of waiting out the poll interval (a device-arrival
/// broadcast just hit).
pub fn nudge() {
    let (m, cv) = nudge_pair();
    *m.lock().unwrap() = true;
    cv.notify_all();
}

/// Start the watcher thread (idempotent).
pub fn start() {
    static UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if UP.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let _ = nudge_pair();
    #[cfg(windows)]
    std::thread::Builder::new().name("bluetooth".into()).spawn(imp::run).ok();
}

/// Do the two names refer to the same device? PnP child nodes carry suffixes
/// ("Sound Buds Hands-Free"), so match on a prefix either way, ignoring case.
pub fn same_device(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim().to_lowercase(), b.trim().to_lowercase());
    !a.is_empty() && !b.is_empty() && (a == b || a.starts_with(&b) || b.starts_with(&a))
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;
    use windows::core::GUID;
    use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice};
    use windows::Devices::Enumeration::DeviceInformation;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDevicePropertyW, SETUP_DI_GET_CLASS_DEVS_FLAGS, SP_DEVINFO_DATA,
    };
    use windows::Win32::Devices::Properties::{DEVPROPKEY, DEVPROPTYPE};
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    const POLL: Duration = Duration::from_secs(3);
    /// Headsets report their battery a moment after the link comes up.
    const BATTERY_SETTLE: Duration = Duration::from_millis(1800);

    /// DEVPKEY_Bluetooth_Battery: percentage as a byte.
    const BATTERY_KEY: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0x104EA319_6EE2_4701_BD47_8DDBF425BBE5), pid: 2 };
    /// DEVPKEY_Device_FriendlyName.
    const NAME_KEY: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0xA45C254E_DF1C_4EFD_8020_67D146A850E0), pid: 14 };

    /// Names of the Bluetooth devices connected right now (classic + LE).
    fn connected() -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let selectors = [
            BluetoothDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected),
            BluetoothLEDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected),
        ];
        for sel in selectors.into_iter().flatten() {
            let Ok(op) = DeviceInformation::FindAllAsyncAqsFilter(&sel) else { continue };
            let Ok(all) = op.get() else { continue };
            for info in all {
                if let Ok(name) = info.Name() {
                    let name = name.to_string();
                    if !name.trim().is_empty() && !out.iter().any(|n| super::same_device(n, &name)) {
                        out.push(name);
                    }
                }
            }
        }
        out
    }

    /// Friendly name -> battery % for every Bluetooth PnP node that reports one.
    fn batteries() -> HashMap<String, u8> {
        let mut out = HashMap::new();
        for enumerator in ["BTHENUM", "BTHLE"] {
            unsafe {
                // DIGCF_PRESENT | DIGCF_ALLCLASSES
                let Ok(set) = SetupDiGetClassDevsW(None, &windows::core::HSTRING::from(enumerator), None, SETUP_DI_GET_CLASS_DEVS_FLAGS(0x22)) else { continue };
                let mut data = SP_DEVINFO_DATA { cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
                let mut i = 0;
                while SetupDiEnumDeviceInfo(set, i, &mut data).is_ok() {
                    i += 1;
                    let mut ty = DEVPROPTYPE(0);
                    let mut buf = [0u8; 8];
                    let mut need = 0u32;
                    if SetupDiGetDevicePropertyW(set, &data, &BATTERY_KEY, &mut ty, Some(&mut buf), Some(&mut need), 0).is_err() {
                        continue;
                    }
                    let pct = buf[0];
                    if pct > 100 {
                        continue;
                    }
                    let mut nbuf = [0u8; 512];
                    if SetupDiGetDevicePropertyW(set, &data, &NAME_KEY, &mut ty, Some(&mut nbuf), Some(&mut need), 0).is_ok() {
                        let w: Vec<u16> = nbuf[..(need as usize).min(nbuf.len())].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
                        let name = String::from_utf16_lossy(&w);
                        if !name.is_empty() {
                            out.insert(name, pct);
                        }
                    }
                }
                let _ = SetupDiDestroyDeviceInfoList(set);
            }
        }
        out
    }

    fn battery_for(name: &str) -> Option<u8> {
        batteries().into_iter().find(|(n, _)| super::same_device(n, name)).map(|(_, p)| p)
    }

    pub fn run() {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let mut known: Option<Vec<String>> = None; // None until the baseline poll
        let (m, cv) = nudge_pair();
        loop {
            let now = connected();
            if let Some(prev) = &known {
                let fresh: Vec<String> = now.iter().filter(|n| !prev.iter().any(|p| super::same_device(p, n))).cloned().collect();
                if !fresh.is_empty() {
                    std::thread::sleep(BATTERY_SETTLE);
                    let mut q = PENDING.lock().unwrap();
                    for name in fresh {
                        let battery = battery_for(&name);
                        q.push(BtEvent { name, battery });
                    }
                    drop(q);
                    if let Some(n) = NOTIFY.get() {
                        n();
                    }
                }
            }
            known = Some(now);
            let g = m.lock().unwrap();
            let (mut g, _) = cv.wait_timeout(g, POLL).unwrap();
            *g = false;
        }
    }

    #[cfg(test)]
    pub fn snapshot_for_test() -> (Vec<String>, Vec<(String, u8)>) {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        (connected(), batteries().into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_across_pnp_suffixes() {
        assert!(same_device("Sound Buds", "Sound Buds Hands-Free"));
        assert!(same_device("sound buds ", "SOUND BUDS"));
        assert!(!same_device("Sound Buds", "Keyboard K380"));
        assert!(!same_device("", "Sound Buds"));
    }

    #[test]
    fn events_drain_newest_last() {
        PENDING.lock().unwrap().clear();
        PENDING.lock().unwrap().push(BtEvent { name: "A".into(), battery: None });
        PENDING.lock().unwrap().push(BtEvent { name: "B".into(), battery: Some(80) });
        assert_eq!(take(), Some(BtEvent { name: "B".into(), battery: Some(80) }));
        assert_eq!(take(), None);
    }

    /// Talks to the real Bluetooth stack: must not crash, whatever is paired.
    #[cfg(windows)]
    #[test]
    fn real_stack_enumerates_without_crashing() {
        let (connected, batteries) = imp::snapshot_for_test();
        eprintln!("connected: {connected:?}  batteries: {batteries:?}");
    }
}
