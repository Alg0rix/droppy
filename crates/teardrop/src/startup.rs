//! Start-with-Windows toggle via HKCU\...\Run (per-user, no admin).

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::ERROR_SUCCESS;
#[cfg(windows)]
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_READ, KEY_SET_VALUE, REG_SZ, REG_VALUE_TYPE,
};

#[cfg(windows)]
const RUN_KEY: &[u16] = &[
    0x53, 0x4F, 0x46, 0x54, 0x57, 0x41, 0x52, 0x45, 0x5C, 0x4D, 0x69, 0x63, 0x72, 0x6F, 0x73, 0x6F,
    0x66, 0x74, 0x5C, 0x57, 0x69, 0x6E, 0x64, 0x6F, 0x77, 0x73, 0x5C, 0x43, 0x75, 0x72, 0x72,
    0x65, 0x6E, 0x74, 0x56, 0x65, 0x72, 0x73, 0x69, 0x6F, 0x6E, 0x5C, 0x52, 0x75, 0x6E, 0x00,
];
#[cfg(windows)]
/// "Droppy"
const VALUE: &[u16] = &[0x44, 0x72, 0x6F, 0x70, 0x70, 0x79, 0x00];
#[cfg(windows)]
/// "Teardrop" — the value name before the rename.
const OLD_VALUE: &[u16] = &[0x54, 0x65, 0x61, 0x72, 0x64, 0x72, 0x6F, 0x70, 0x00];

#[cfg(windows)]
fn exe_quoted() -> Vec<u16> {
    let exe = std::env::current_exe()
        .map(|p| format!("\"{}\"", p.display()))
        .unwrap_or_default();
    exe.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub fn enabled() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(RUN_KEY.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        ) != ERROR_SUCCESS
        {
            return false;
        }
        let mut buf = [0u16; 512];
        let mut len: u32 = (buf.len() * 2) as u32;
        let mut ty = REG_VALUE_TYPE(0);
        let ok = windows::Win32::System::Registry::RegQueryValueExW(
            hkey,
            PCWSTR::from_raw(VALUE.as_ptr()),
            None,
            Some(&mut ty),
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut len),
        ) == ERROR_SUCCESS;
        let _ = RegCloseKey(hkey);
        ok
    }
}

#[cfg(windows)]
pub fn set(on: bool) -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(RUN_KEY.as_ptr()),
            0,
            KEY_SET_VALUE | KEY_READ,
            &mut hkey,
        ) != ERROR_SUCCESS
        {
            return false;
        }
        let ok = if on {
            let v = exe_quoted();
            let bytes: &[u8] =
                std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2);
            RegSetValueExW(hkey, PCWSTR::from_raw(VALUE.as_ptr()), 0, REG_SZ, Some(bytes))
                == ERROR_SUCCESS
        } else {
            let r = RegDeleteValueW(hkey, PCWSTR::from_raw(VALUE.as_ptr()));
            r == ERROR_SUCCESS || r.0 == 2 // ERROR_FILE_NOT_FOUND tolerable
        };
        let _ = RegCloseKey(hkey);
        ok
    }
}

/// Carry a pre-rename "Teardrop" startup entry over to "Droppy", pointing at
/// this exe, so Start-with-Windows survives the rename. Returns whether it
/// migrated anything.
#[cfg(windows)]
pub fn migrate_legacy() -> bool {
    migrate_in(RUN_KEY)
}

#[cfg(windows)]
fn migrate_in(key: &[u16]) -> bool {
    use windows::Win32::System::Registry::RegQueryValueExW;
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR::from_raw(key.as_ptr()), 0, KEY_SET_VALUE | KEY_READ, &mut hkey) != ERROR_SUCCESS {
            return false;
        }
        let mut len = 0u32;
        let had_old = RegQueryValueExW(hkey, PCWSTR::from_raw(OLD_VALUE.as_ptr()), None, None, None, Some(&mut len)) == ERROR_SUCCESS;
        if had_old {
            let v = exe_quoted();
            let bytes: &[u8] = std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2);
            let set = RegSetValueExW(hkey, PCWSTR::from_raw(VALUE.as_ptr()), 0, REG_SZ, Some(bytes)) == ERROR_SUCCESS;
            if set {
                let _ = RegDeleteValueW(hkey, PCWSTR::from_raw(OLD_VALUE.as_ptr()));
            }
        }
        let _ = RegCloseKey(hkey);
        had_old
    }
}

#[cfg(not(windows))]
pub fn migrate_legacy() -> bool {
    false
}

#[cfg(not(windows))]
pub fn enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set(_on: bool) -> bool {
    false
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows::Win32::System::Registry::{RegCreateKeyExW, RegDeleteKeyW, RegQueryValueExW, REG_OPTION_NON_VOLATILE};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn has(hkey: HKEY, name: &[u16]) -> bool {
        unsafe { RegQueryValueExW(hkey, PCWSTR::from_raw(name.as_ptr()), None, None, None, None) == ERROR_SUCCESS }
    }

    /// Runs against a scratch key under HKCU, never the real Run key.
    #[test]
    fn old_startup_entry_becomes_the_new_one() {
        let key = wide(&format!(r"Software\DroppyTest\Run{}", std::process::id()));
        unsafe {
            let mut hkey = HKEY::default();
            assert_eq!(
                RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR::from_raw(key.as_ptr()), 0, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE | KEY_READ, None, &mut hkey, None),
                ERROR_SUCCESS
            );
            // Nothing to do while only the new name (or nothing) is present.
            assert!(!migrate_in(&key));
            let old = wide(r#""C:\Old\teardrop.exe""#);
            let bytes = std::slice::from_raw_parts(old.as_ptr() as *const u8, old.len() * 2);
            assert_eq!(RegSetValueExW(hkey, PCWSTR::from_raw(OLD_VALUE.as_ptr()), 0, REG_SZ, Some(bytes)), ERROR_SUCCESS);

            assert!(migrate_in(&key), "old entry migrated");
            assert!(has(hkey, VALUE), "new Droppy entry written");
            assert!(!has(hkey, OLD_VALUE), "old Teardrop entry removed");
            assert!(!migrate_in(&key), "idempotent");

            let _ = RegCloseKey(hkey);
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR::from_raw(key.as_ptr()));
        }
    }
}
