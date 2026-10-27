use super::*;
// ----------------------------------------------------------------- tray

pub(super) fn load_app_icon() -> Option<HICON> {
    unsafe {
        let inst = GetModuleHandleW(None).ok()?;
        let hinst: windows::Win32::Foundation::HINSTANCE = inst.into();
        let res = LoadImageW(hinst, windows::core::PCWSTR(1 as *const u16), IMAGE_ICON, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), IMAGE_FLAGS(0));
        if let Ok(handle) = res {
            if !handle.is_invalid() {
                return Some(HICON(handle.0));
            }
        }
        LoadIconW(hinst, windows::core::PCWSTR(1 as *const u16)).ok()
    }
}

fn make_tray_icon() -> Option<HICON> {
    if let Some(icon) = load_app_icon() {
        return Some(icon);
    }
    let mut s = Scene::new();
    s.circle(16.0, 16.0, 15.0, Fill::DGrad(Color::rgb(61, 200, 245), Color::rgb(60, 90, 220)));
    icons::draw(&mut s, icons::Icon::Drop, 16.0, 16.0, 20.0, Color::rgb(255, 255, 255));
    let (pw, ph, mut px) = render_offscreen(&s, (32.0, 32.0), 1.0)?;
    // Premultiplied -> straight alpha for the icon bitmap.
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as f32 / 255.0;
        if a > 0.0 {
            for c in &mut p[..3] {
                *c = ((*c as f32 / a).min(255.0)) as u8;
            }
        }
    }
    unsafe {
        let color = CreateBitmap(pw as i32, ph as i32, 1, 32, Some(px.as_ptr() as *const _));
        let mask_bits = vec![0u8; (pw as usize / 8) * ph as usize];
        let mask = CreateBitmap(pw as i32, ph as i32, 1, 1, Some(mask_bits.as_ptr() as *const _));
        let info = ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(color);
        let _ = DeleteObject(mask);
        icon
    }
}

pub(super) fn add_tray(hwnd: HWND) {
    unsafe {
        let hicon = load_app_icon().or_else(make_tray_icon).or_else(|| LoadIconW(None, IDI_APPLICATION).ok());
        let Some(hicon) = hicon else { return };
        let mut nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = TRAY_MSG;
        nid.hIcon = hicon;
        let tip = wide("Droppy");
        nid.szTip[..tip.len().min(127)].copy_from_slice(&tip[..tip.len().min(127)]);
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
}

pub(super) fn remove_tray(hwnd: HWND) {
    unsafe {
        let nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

pub(super) fn show_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let items: [(usize, &str); 3] = [(ID_TOGGLE, if is_open() { "Collapse island" } else { "Open island" }), (ID_SETTINGS, "Settings"), (ID_QUIT, "Quit Droppy")];
        for (id, label) in items {
            let l = wide(label);
            let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR::from_raw(l.as_ptr()));
        }
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        MENU_OPEN.set(true);
        let cmd = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, pt.x, pt.y, 0, hwnd, None);
        MENU_OPEN.set(false);
        let _ = DestroyMenu(menu);
        match cmd.0 as usize {
            ID_QUIT => {
                let _ = DestroyWindow(hwnd);
            }
            ID_TOGGLE => {
                if is_open() {
                    close_island(hwnd)
                } else {
                    open_island(hwnd, true)
                }
            }
            ID_SETTINGS => {
                VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    if vm.page != Page::Settings {
                        LAST_TAB.set(vm.page);
                        vm.page = Page::Settings;
                    }
                });
                open_island(hwnd, true);
            }
            _ => {}
        }
    }
}
