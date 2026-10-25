//! OLE drop target for the island.
//!
//! Unlike plain `WM_DROPFILES` (which only fires on release), an
//! `IDropTarget` is told when a drag enters and leaves, so the island can
//! react to a file being carried over it before it is dropped.

use windows::core::{implement, Result};
use windows::Win32::Foundation::{HWND, POINTL};
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, ReleaseStgMedium, CF_HDROP, CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

/// Pointer positions are screen pixels.
pub enum DropEvent {
    /// A drag carrying this many files entered the island.
    Enter(u32, i32, i32),
    /// The drag moved.
    Over(i32, i32),
    Leave,
    /// Whatever was dropped: file paths, and/or a text/link payload.
    Drop(DropPayload, i32, i32),
}

/// The data a drop carries: files, a URL, or plain text.
pub struct DropPayload {
    pub paths: Vec<String>,
    pub text: Option<String>,
}

#[implement(IDropTarget)]
pub struct DropTarget {
    hwnd: isize,
    cb: fn(HWND, DropEvent),
}

impl DropTarget {
    pub fn new(hwnd: HWND, cb: fn(HWND, DropEvent)) -> IDropTarget {
        DropTarget { hwnd: hwnd.0 as isize, cb }.into()
    }

    fn emit(&self, e: DropEvent) {
        (self.cb)(HWND(self.hwnd as *mut _), e);
    }
}

fn hdrop_format() -> FORMATETC {
    FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 }
}

fn fmt(cf: u32) -> FORMATETC {
    FORMATETC { cfFormat: cf as u16, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 }
}

fn inet_url_cf() -> u32 {
    unsafe { RegisterClipboardFormatW(windows::core::w!("UniformResourceLocatorW")) as u32 }
}

/// UTF-16 text in a TYMED_HGLOBAL medium.
fn hglobal_text(data: &IDataObject, cf: u32) -> Option<String> {
    let f = fmt(cf);
    unsafe {
        if data.QueryGetData(&f) != windows::Win32::Foundation::S_OK {
            return None;
        }
        let mut medium = data.GetData(&f).ok()?;
        let base = GlobalLock(medium.u.hGlobal);
        let mut s = String::new();
        if !base.is_null() {
            let ptr = base as *const u16;
            let mut n = 0usize;
            while *ptr.add(n) != 0 && n < 16384 { n += 1; }
            s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, n));
            let _ = GlobalUnlock(medium.u.hGlobal);
        }
        ReleaseStgMedium(&mut medium);
        (!s.is_empty()).then_some(s)
    }
}

/// File paths carried by a drag (empty when it holds something else).
fn paths_of(data: &IDataObject) -> Vec<String> {
    let fmt = hdrop_format();
    let mut out = Vec::new();
    unsafe {
        if data.QueryGetData(&fmt) != windows::Win32::Foundation::S_OK {
            return out;
        }
        let Ok(mut medium) = data.GetData(&fmt) else { return out };
        let hdrop = HDROP(medium.u.hGlobal.0);
        let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
        for i in 0..count.min(24) {
            let mut buf = [0u16; 520];
            let len = DragQueryFileW(hdrop, i, Some(&mut buf));
            if len > 0 {
                out.push(String::from_utf16_lossy(&buf[..len as usize]));
            }
        }
        ReleaseStgMedium(&mut medium);
    }
    out
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(&self, pdataobj: Option<&IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, pdweffect: *mut DROPEFFECT) -> Result<()> {
        let n = pdataobj.map(|d| {
            let files = paths_of(d).len();
            if files > 0 { files } else { usize::from(hglobal_text(d, CF_UNICODETEXT.0 as u32).is_some() || hglobal_text(d, inet_url_cf()).is_some()) }
        }).unwrap_or(0) as u32;
        unsafe { *pdweffect = if n > 0 { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        if n > 0 {
            self.emit(DropEvent::Enter(n, pt.x, pt.y));
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, pdweffect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *pdweffect = DROPEFFECT_COPY };
        self.emit(DropEvent::Over(pt.x, pt.y));
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        self.emit(DropEvent::Leave);
        Ok(())
    }

    fn Drop(&self, pdataobj: Option<&IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, pdweffect: *mut DROPEFFECT) -> Result<()> {
        let payload = pdataobj
            .map(|d| {
                let paths = paths_of(d);
                let text = if paths.is_empty() {
                    hglobal_text(d, inet_url_cf()).or_else(|| hglobal_text(d, CF_UNICODETEXT.0 as u32))
                } else {
                    None
                };
                DropPayload { paths, text }
            })
            .unwrap_or(DropPayload { paths: Vec::new(), text: None });
        let empty = payload.paths.is_empty() && payload.text.is_none();
        unsafe { *pdweffect = if empty { DROPEFFECT_NONE } else { DROPEFFECT_COPY } };
        self.emit(DropEvent::Drop(payload, pt.x, pt.y));
        Ok(())
    }
}
