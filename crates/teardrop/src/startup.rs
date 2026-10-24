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
const VALUE: &[u16] = &[0x54, 0x65, 0x61, 0x72, 0x64, 0x72, 0x6F, 0x70, 0x00];

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

#[cfg(not(windows))]
pub fn enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set(_on: bool) -> bool {
    false
}
