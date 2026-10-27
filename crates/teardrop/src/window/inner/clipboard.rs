use super::*;
// ------------------------------------------------------------- clipboard

fn clipboard_is_private() -> bool {
    unsafe {
        let fmt = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        fmt != 0 && IsClipboardFormatAvailable(fmt).is_ok()
    }
}

pub(super) fn read_clipboard_text(owner: HWND) -> Option<String> {
    unsafe {
        if OpenClipboard(owner).is_err() {
            return None;
        }
        let text = (|| {
            if clipboard_is_private() {
                return None;
            }
            let h: HANDLE = GetClipboardData(CF_UNICODETEXT_U32).ok()?;
            let hg = HGLOBAL(h.0);
            let ptr = GlobalLock(hg) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let mut len = 0usize;
            while len < 8192 && *ptr.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)).trim().to_string();
            let _ = GlobalUnlock(hg);
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        })();
        let _ = CloseClipboard();
        text
    }
}

pub(super) fn write_clipboard_text(owner: HWND, text: &str) -> bool {
    let data = wide(text);
    unsafe {
        if OpenClipboard(owner).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let ok = (|| {
            let h = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2).ok()?;
            let p = GlobalLock(h) as *mut u16;
            if p.is_null() {
                return None;
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
            let _ = GlobalUnlock(h);
            SetClipboardData(CF_UNICODETEXT_U32, HANDLE(h.0)).ok()?;
            Some(())
        })()
        .is_some();
        let _ = CloseClipboard();
        ok
    }
}
