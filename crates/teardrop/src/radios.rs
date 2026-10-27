//! Radio control for the Quick page (Windows 10/11): Wi-Fi and Bluetooth
//! on/off, the WLAN network list + connect, and the Bluetooth device list
//! + pairing.
//!
//! Two API surfaces do the work:
//! - WinRT `Windows.Devices.Radios` toggles both radios and `Devices.Bluetooth`
//!   enumerates/pairs devices — async, needs an MTA thread.
//! - `wlanapi.dll` scans networks and connects (saved profile, or a generated
//!   temporary profile carrying the password typed into the island).
//!
//! Both are slow/blocking calls, so a worker thread owns them: it re-reads
//! state every few seconds into a snapshot the UI thread copies, and commands
//! (toggle, connect, pair, rescan) arrive over a channel.

use std::sync::{mpsc, Mutex, OnceLock};
use teardrop_core::{BtDev, WifiNet};

/// Which radio a toggle request targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioKind {
    Wifi,
    Bluetooth,
}

/// Last known radio state; `None` = hardware missing or access denied.
#[derive(Debug, Clone, Default)]
pub struct RadioSnapshot {
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
    /// Connected WLAN SSID, only while Wi-Fi is on and associated.
    pub ssid: Option<String>,
    /// Networks in range, sorted connected-first then by signal.
    pub wifi_nets: Vec<WifiNet>,
    /// Paired + (while scanning) nearby Bluetooth devices.
    pub bt_devs: Vec<BtDev>,
}

enum Cmd {
    Toggle(RadioKind),
    Refresh,
    /// Kick a WLAN scan so the next read has fresh networks.
    ScanWifi,
    /// Join a network; `key` = Some(password) for secured, no-profile nets.
    ConnectWifi {
        ssid: String,
        key: Option<String>,
    },
    DisconnectWifi,
    /// Include unpaired devices in the device reads (inquiry is costly).
    ScanBt(bool),
    /// Pair with a discovered device by `DeviceInformation::Id`.
    PairBt(String),
}

static SNAP: Mutex<RadioSnapshot> = Mutex::new(RadioSnapshot { wifi: None, bluetooth: None, ssid: None, wifi_nets: Vec::new(), bt_devs: Vec::new() });
static CMDS: OnceLock<mpsc::Sender<Cmd>> = OnceLock::new();
static NOTIFY: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Register the callback that wakes the UI when the snapshot changes.
pub fn set_notify(f: impl Fn() + Send + Sync + 'static) {
    let _ = NOTIFY.set(Box::new(f));
}

/// The last published radio state.
pub fn snapshot() -> RadioSnapshot {
    SNAP.lock().unwrap().clone()
}

fn send(cmd: Cmd) {
    if let Some(tx) = CMDS.get() {
        let _ = tx.send(cmd);
    }
}

/// Flip a radio; the worker applies it and re-reads the states.
pub fn toggle(kind: RadioKind) {
    send(Cmd::Toggle(kind));
}

/// Re-read the radios now instead of waiting out the poll interval.
pub fn nudge() {
    send(Cmd::Refresh);
}

/// Ask the driver for a fresh network list.
pub fn scan_wifi() {
    send(Cmd::ScanWifi);
}

/// Join `ssid`; `key` is the password for secured networks without a saved
/// profile (`None` joins a stored-profile or open network).
pub fn connect_wifi(ssid: &str, key: Option<String>) {
    send(Cmd::ConnectWifi { ssid: ssid.to_string(), key });
}

pub fn disconnect_wifi() {
    send(Cmd::DisconnectWifi);
}

/// Pair with the device `id` (from the snapshot's `BtDev::id`).
pub fn pair_bt(id: &str) {
    send(Cmd::PairBt(id.to_string()));
}

/// Turn live device discovery on/off (the Bluetooth inquiry is expensive,
/// so it only runs while the tile's device list is open).
pub fn scan_bt(on: bool) {
    send(Cmd::ScanBt(on));
}

/// Start the worker thread (idempotent).
pub fn start() {
    static UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if UP.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    #[cfg(windows)]
    {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let _ = CMDS.set(tx);
        std::thread::Builder::new().name("radios".into()).spawn(move || imp::run(rx)).ok();
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::time::Duration;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice};
    use windows::Devices::Enumeration::{DeviceInformation, DevicePairingResultStatus};
    use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind as WinKind, RadioState};
    use windows::Networking::Connectivity::NetworkInformation;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::WiFi::{
        dot11_BSS_type_infrastructure, wlan_connection_mode_profile, wlan_connection_mode_temporary_profile, WlanCloseHandle, WlanConnect, WlanDisconnect, WlanEnumInterfaces,
        WlanFreeMemory, WlanGetAvailableNetworkList, WlanOpenHandle, WlanScan, DOT11_AUTH_ALGORITHM, DOT11_AUTH_ALGO_80211_OPEN, DOT11_AUTH_ALGO_80211_SHARED_KEY,
        DOT11_AUTH_ALGO_RSNA_PSK, DOT11_AUTH_ALGO_WPA3, DOT11_AUTH_ALGO_WPA3_SAE, DOT11_AUTH_ALGO_WPA_PSK, WLAN_AVAILABLE_NETWORK_CONNECTED, WLAN_AVAILABLE_NETWORK_HAS_PROFILE,
        WLAN_AVAILABLE_NETWORK_LIST, WLAN_CONNECTION_PARAMETERS, WLAN_INTERFACE_INFO_LIST,
    };
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    const POLL: Duration = Duration::from_secs(4);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    // ------------------------------------------------------------ radios

    fn read_radios() -> RadioSnapshot {
        let mut s = RadioSnapshot::default();
        let Ok(op) = Radio::GetRadiosAsync() else { return s };
        let Ok(list) = op.get() else { return s };
        for r in list {
            let (Ok(kind), Ok(state)) = (r.Kind(), r.State()) else { continue };
            let on = state == RadioState::On;
            match kind {
                WinKind::WiFi => s.wifi = Some(on),
                WinKind::Bluetooth => s.bluetooth = Some(on),
                _ => {}
            }
        }
        if s.wifi == Some(true) {
            s.ssid = NetworkInformation::GetInternetConnectionProfile()
                .and_then(|p| p.WlanConnectionProfileDetails())
                .and_then(|d| d.GetConnectedSsid())
                .ok()
                .map(|h| h.to_string())
                .filter(|s| !s.is_empty());
        }
        s
    }

    fn set_radio(kind: RadioKind) {
        let want_kind = match kind {
            RadioKind::Wifi => WinKind::WiFi,
            RadioKind::Bluetooth => WinKind::Bluetooth,
        };
        let Ok(list) = Radio::GetRadiosAsync().and_then(|op| op.get()) else { return };
        for r in list {
            if r.Kind() != Ok(want_kind) {
                continue;
            }
            let cur = r.State().unwrap_or(RadioState::Off);
            let next = if cur == RadioState::On { RadioState::Off } else { RadioState::On };
            let _ = r.SetStateAsync(next).and_then(|op| op.get());
            return;
        }
    }

    // ------------------------------------------------------------ wlan

    /// wlanapi handle + first wireless interface guid (None = no Wi-Fi hw).
    fn wlan() -> Option<(HANDLE, windows::core::GUID)> {
        unsafe {
            let mut ver = 0u32;
            let mut h = HANDLE::default();
            if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
                return None;
            }
            let mut list = std::ptr::null_mut::<WLAN_INTERFACE_INFO_LIST>();
            let mut guid = None;
            if WlanEnumInterfaces(h, None, &mut list) == 0 && !list.is_null() {
                let l = &*list;
                if l.dwNumberOfItems > 0 {
                    guid = Some(l.InterfaceInfo[0].InterfaceGuid);
                }
            }
            if !list.is_null() {
                WlanFreeMemory(list as *const _);
            }
            match guid {
                Some(g) => Some((h, g)),
                None => {
                    let _ = WlanCloseHandle(h, None);
                    None
                }
            }
        }
    }

    /// Call `f` with the `WLAN_AVAILABLE_NETWORK` whose SSID matches `ssid`.
    fn with_net<R>(ssid: &str, f: impl FnOnce(&windows::Win32::NetworkManagement::WiFi::WLAN_AVAILABLE_NETWORK) -> R) -> Option<R> {
        let (h, guid) = wlan()?;
        unsafe {
            let mut list = std::ptr::null_mut::<WLAN_AVAILABLE_NETWORK_LIST>();
            let mut out = None;
            if WlanGetAvailableNetworkList(h, &guid, 0, None, &mut list) == 0 && !list.is_null() {
                let l = &*list;
                let nets = std::slice::from_raw_parts(l.Network.as_ptr(), l.dwNumberOfItems.min(64) as usize);
                for n in nets {
                    let len = (n.dot11Ssid.uSSIDLength as usize).min(32);
                    if String::from_utf8_lossy(&n.dot11Ssid.ucSSID[..len]) == ssid {
                        out = Some(f(n));
                        break;
                    }
                }
                WlanFreeMemory(list as *const _);
            }
            let _ = WlanCloseHandle(h, None);
            out
        }
    }

    fn wifi_networks() -> Vec<WifiNet> {
        let Some((h, guid)) = wlan() else { return Vec::new() };
        let mut out: Vec<WifiNet> = Vec::new();
        unsafe {
            let mut list = std::ptr::null_mut::<WLAN_AVAILABLE_NETWORK_LIST>();
            if WlanGetAvailableNetworkList(h, &guid, 0, None, &mut list) == 0 && !list.is_null() {
                let l = &*list;
                let nets = std::slice::from_raw_parts(l.Network.as_ptr(), l.dwNumberOfItems.min(64) as usize);
                for n in nets {
                    let len = (n.dot11Ssid.uSSIDLength as usize).min(32);
                    let ssid = String::from_utf8_lossy(&n.dot11Ssid.ucSSID[..len]).trim().to_string();
                    if ssid.is_empty() || out.iter().any(|w| w.ssid == ssid) {
                        continue;
                    }
                    out.push(WifiNet {
                        ssid,
                        quality: n.wlanSignalQuality.min(100) as u8,
                        secured: n.bSecurityEnabled.as_bool(),
                        connected: n.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED != 0,
                        has_profile: n.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE != 0,
                    });
                }
                WlanFreeMemory(list as *const _);
            }
            let _ = WlanCloseHandle(h, None);
        }
        out.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.quality.cmp(&a.quality)));
        out
    }

    /// Kick a WLAN scan; results land in the network list ~1 s later.
    fn wifi_scan() {
        let Some((h, guid)) = wlan() else { return };
        unsafe {
            let _ = WlanScan(h, &guid, None, None, None);
            let _ = WlanCloseHandle(h, None);
        }
        // Give the radio a beat to finish the sweep before the re-read.
        std::thread::sleep(Duration::from_millis(1400));
    }

    fn wifi_disconnect() {
        let Some((h, guid)) = wlan() else { return };
        unsafe {
            let _ = WlanDisconnect(h, &guid, None);
            let _ = WlanCloseHandle(h, None);
        }
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
    }

    /// `(authentication, encryption)` pair for a WPA profile XML, or None for
    /// schemes a plain password can't join (enterprise, OWE).
    fn auth_xml(algo: DOT11_AUTH_ALGORITHM) -> Option<(&'static str, &'static str)> {
        if algo == DOT11_AUTH_ALGO_80211_OPEN {
            Some(("open", "none"))
        } else if algo == DOT11_AUTH_ALGO_80211_SHARED_KEY {
            Some(("shared", "WEP"))
        } else if algo == DOT11_AUTH_ALGO_WPA_PSK {
            Some(("WPAPSK", "TKIP"))
        } else if algo == DOT11_AUTH_ALGO_RSNA_PSK {
            Some(("WPA2PSK", "AES"))
        } else if algo == DOT11_AUTH_ALGO_WPA3_SAE || algo == DOT11_AUTH_ALGO_WPA3 {
            Some(("WPA3SAE", "AES"))
        } else {
            None
        }
    }

    /// A `wlan_connection_mode_temporary_profile` document for `ssid`.
    fn profile_xml(ssid: &str, auth: &str, enc: &str, key: Option<&str>) -> String {
        let e = xml_escape(ssid);
        let shared = key
            .map(|k| format!("<sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>{}</keyMaterial></sharedKey>", xml_escape(k)))
            .unwrap_or_default();
        format!(
            "<?xml version=\"1.0\"?><WLANProfile xmlns=\"http://www.microsoft.com/networking/WLAN/profile/v1\"><name>{e}</name><SSIDConfig><SSID><name>{e}</name></SSID></SSIDConfig><connectionType>ESS</connectionType><connectionMode>manual</connectionMode><MSM><security><authEncryption><authentication>{auth}</authentication><encryption>{enc}</encryption><useOneX>false</useOneX></authEncryption>{shared}</security></MSM></WLANProfile>"
        )
    }

    /// Join `ssid`. Tries the saved profile first, then a generated temporary
    /// profile (open networks need no key, WPA2/WPA3 take `key`).
    fn wifi_connect(ssid: &str, key: Option<String>) -> bool {
        let Some((h, guid)) = wlan() else { return false };
        // Profile name and auth algorithm come from the live network entry.
        let found = with_net(ssid, |n| {
            let end = n.strProfileName.iter().position(|&c| c == 0).unwrap_or(256);
            let name: Vec<u16> = n.strProfileName[..end].to_vec();
            (n.dwFlags, name, n.dot11DefaultAuthAlgorithm)
        });
        let Some((flags, prof, algo)) = found else {
            let _ = unsafe { WlanCloseHandle(h, None) };
            return false;
        };
        let has_profile = flags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE != 0;
        let (mode, profile) = if has_profile && !prof.is_empty() {
            (wlan_connection_mode_profile, prof)
        } else {
            let Some((auth, enc)) = auth_xml(algo) else {
                let _ = unsafe { WlanCloseHandle(h, None) };
                return false;
            };
            // An open network without a stored profile still needs the XML,
            // just without a <sharedKey>.
            let k = if auth == "open" { None } else { key.as_deref() };
            let mut w = wide(&profile_xml(ssid, auth, enc, k));
            (wlan_connection_mode_temporary_profile, std::mem::take(&mut w))
        };
        let mut pw = profile;
        pw.push(0);
        let params = WLAN_CONNECTION_PARAMETERS {
            wlanConnectionMode: mode,
            strProfile: PCWSTR::from_raw(pw.as_ptr()),
            pDot11Ssid: std::ptr::null_mut(),
            pDesiredBssidList: std::ptr::null_mut(),
            dot11BssType: dot11_BSS_type_infrastructure,
            dwFlags: 0,
        };
        let rc = unsafe { WlanConnect(h, &guid, &params, None) };
        unsafe {
            let _ = WlanCloseHandle(h, None);
        }
        rc == 0
    }

    // ----------------------------------------------------------- devices

    fn bt_connected(id: &str) -> bool {
        let hid = HSTRING::from(id);
        if let Ok(d) = BluetoothDevice::FromIdAsync(&hid).and_then(|op| op.get()) {
            return d.ConnectionStatus() == Ok(BluetoothConnectionStatus::Connected);
        }
        if let Ok(d) = BluetoothLEDevice::FromIdAsync(&hid).and_then(|op| op.get()) {
            return d.ConnectionStatus() == Ok(BluetoothConnectionStatus::Connected);
        }
        false
    }

    /// Paired devices (classic + LE), plus nearby unpaired ones while the
    /// device list is open (the unpaired selector runs a real inquiry).
    fn bt_devices(scan_unpaired: bool) -> Vec<BtDev> {
        let mut out: Vec<BtDev> = Vec::new();
        let mut sels = vec![BluetoothDevice::GetDeviceSelector().ok(), BluetoothLEDevice::GetDeviceSelector().ok()];
        if scan_unpaired {
            sels.push(BluetoothDevice::GetDeviceSelectorFromPairingState(false).ok());
            sels.push(BluetoothLEDevice::GetDeviceSelectorFromPairingState(false).ok());
        }
        for sel in sels.into_iter().flatten() {
            let Ok(all) = DeviceInformation::FindAllAsyncAqsFilter(&sel).and_then(|op| op.get()) else { continue };
            for info in all {
                let Ok(id) = info.Id() else { continue };
                let id = id.to_string();
                if out.iter().any(|d| d.id == id) {
                    continue;
                }
                let name = info.Name().map(|h| h.to_string()).unwrap_or_else(|_| "Device".into());
                let paired = info.Pairing().and_then(|p| p.IsPaired()).unwrap_or(false);
                out.push(BtDev { id, name, paired, connected: false });
            }
        }
        for d in out.iter_mut().filter(|d| d.paired) {
            d.connected = bt_connected(&d.id);
        }
        out.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.paired.cmp(&a.paired)).then(a.name.cmp(&b.name)));
        out
    }

    fn bt_pair(id: &str) -> bool {
        let Ok(di) = DeviceInformation::CreateFromIdAsync(&HSTRING::from(id)).and_then(|op| op.get()) else { return false };
        let Ok(res) = di.Pairing().and_then(|p| p.PairAsync()).and_then(|op| op.get()) else { return false };
        matches!(res.Status(), Ok(DevicePairingResultStatus::Paired) | Ok(DevicePairingResultStatus::AlreadyPaired))
    }

    // ------------------------------------------------------------ worker

    fn read_all(scan_bt: bool) -> RadioSnapshot {
        let mut s = read_radios();
        s.wifi_nets = if s.wifi == Some(true) { wifi_networks() } else { Vec::new() };
        s.bt_devs = if s.bluetooth == Some(true) { bt_devices(scan_bt) } else { Vec::new() };
        s
    }

    /// Publish a snapshot when it changed, waking the UI for a redraw.
    fn publish(s: RadioSnapshot) {
        let changed = {
            let mut g = SNAP.lock().unwrap();
            let changed = g.wifi != s.wifi || g.bluetooth != s.bluetooth || g.ssid != s.ssid || g.wifi_nets != s.wifi_nets || g.bt_devs != s.bt_devs;
            *g = s;
            changed
        };
        if changed {
            if let Some(f) = NOTIFY.get() {
                f();
            }
        }
    }

    pub fn run(rx: mpsc::Receiver<Cmd>) {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        if Radio::RequestAccessAsync().and_then(|op| op.get()) != Ok(RadioAccessStatus::Allowed) {
            // No radio control on this machine/sku: the tiles stay hidden.
            return;
        }
        let mut scan_bt = false;
        publish(read_all(scan_bt));
        loop {
            let mut ops = Vec::new();
            match rx.recv_timeout(POLL) {
                Ok(cmd) => ops.push(cmd),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
            while let Ok(cmd) = rx.try_recv() {
                ops.push(cmd);
            }
            for op in ops {
                match op {
                    Cmd::Toggle(k) => set_radio(k),
                    Cmd::ScanWifi => wifi_scan(),
                    Cmd::ConnectWifi { ssid, key } => {
                        let _ = wifi_connect(&ssid, key);
                    }
                    Cmd::DisconnectWifi => wifi_disconnect(),
                    Cmd::ScanBt(on) => scan_bt = on,
                    Cmd::PairBt(id) => {
                        let _ = bt_pair(&id);
                    }
                    Cmd::Refresh => {}
                }
            }
            publish(read_all(scan_bt));
        }
    }
}
