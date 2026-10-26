//! Quick-settings page state: radios, levels and toggles.
//!
//! Pure view data — the Windows samplers (`radios`, `volume`, `brightness`)
//! fill it; `Option` fields are `None` when the hardware or control is absent
//! so the page can hide or grey out the tile instead of lying.
use serde::{Deserialize, Serialize};

/// One visible Wi-Fi network (from `WlanGetAvailableNetworkList`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WifiNet {
    pub ssid: String,
    /// Link quality 0..=100.
    pub quality: u8,
    /// Needs a key we don't have stored.
    pub secured: bool,
    /// Windows is associated with it right now.
    pub connected: bool,
    /// A saved profile exists — joining needs no password.
    pub has_profile: bool,
}

/// One Bluetooth device (paired or discovered nearby).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BtDev {
    /// `DeviceInformation::Id` — needed to pair.
    pub id: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuickState {
    /// Wi-Fi radio on? `None` = no Wi-Fi adapter (or access denied).
    pub wifi: Option<bool>,
    /// Connected WLAN SSID while `wifi` is on (`None` = not associated).
    pub wifi_ssid: Option<String>,
    /// Bluetooth radio on? `None` = no Bluetooth adapter.
    pub bluetooth: Option<bool>,
    /// Master output level 0..=100 and mute (always present on Windows).
    pub volume_pct: u8,
    pub muted: bool,
    /// Microphone muted? `None` = no capture endpoint.
    pub mic_muted: Option<bool>,
    /// Display brightness 0..=100, `None` = panel can't be read/controlled.
    pub brightness: Option<u8>,
    /// Visible Wi-Fi networks (only while the radio is on).
    pub wifi_nets: Vec<WifiNet>,
    /// Known / nearby Bluetooth devices (only while the radio is on).
    pub bt_devs: Vec<BtDev>,
}

impl QuickState {
    /// Status line under a radio tile: SSID while connected, plain on/off else.
    pub fn wifi_label(&self) -> String {
        match self.wifi {
            Some(true) => self.wifi_ssid.clone().unwrap_or_else(|| "On".to_string()),
            Some(false) => "Off".to_string(),
            None => "Not available".to_string(),
        }
    }

    pub fn bt_label(&self) -> String {
        match self.bluetooth {
            Some(true) => "On".to_string(),
            Some(false) => "Off".to_string(),
            None => "Not available".to_string(),
        }
    }
}

/// A click position `x` inside a slider hit rect `r` -> per-mille 0..=1000.
pub fn slider_permille(rx: f32, w: f32, x: f32) -> u16 {
    let f = ((x - rx) / w.max(1.0)).clamp(0.0, 1.0);
    (f * 1000.0).round() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_state() {
        let mut q = QuickState::default();
        assert_eq!(q.wifi_label(), "Not available");
        q.wifi = Some(false);
        assert_eq!(q.wifi_label(), "Off");
        q.wifi = Some(true);
        assert_eq!(q.wifi_label(), "On");
        q.wifi_ssid = Some("Home-5G".into());
        assert_eq!(q.wifi_label(), "Home-5G");
        assert_eq!(q.bt_label(), "Not available");
    }

    #[test]
    fn slider_maps_and_clamps() {
        assert_eq!(slider_permille(10.0, 100.0, 10.0), 0);
        assert_eq!(slider_permille(10.0, 100.0, 60.0), 500);
        assert_eq!(slider_permille(10.0, 100.0, 110.0), 1000);
        assert_eq!(slider_permille(10.0, 100.0, -5.0), 0);
    }
}
