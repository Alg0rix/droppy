//! OLE drop target for the island.
//!
//! Unlike plain `WM_DROPFILES` (which only fires on release), an
//! `IDropTarget` is told when a drag enters and leaves, so the island can
//! react to a file being carried over it before it is dropped.

use windows::core::{implement, Result};
use windows::Win32::Foundation::{HWND, POINTL};
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, ReleaseStgMedium, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

pub enum DropEvent {
    /// A drag carrying this many files entered the island.
    Enter(u32),
    Leave,
    Drop(Vec<String>),
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
    fn DragEnter(&self, pdataobj: Option<&IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, pdweffect: *mut DROPEFFECT) -> Result<()> {
        let n = pdataobj.map(|d| paths_of(d).len()).unwrap_or(0) as u32;
        unsafe { *pdweffect = if n > 0 { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        if n > 0 {
            self.emit(DropEvent::Enter(n));
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, pdweffect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *pdweffect = DROPEFFECT_COPY };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        self.emit(DropEvent::Leave);
        Ok(())
    }

    fn Drop(&self, pdataobj: Option<&IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, pdweffect: *mut DROPEFFECT) -> Result<()> {
        let list = pdataobj.map(paths_of).unwrap_or_default();
        unsafe { *pdweffect = if list.is_empty() { DROPEFFECT_NONE } else { DROPEFFECT_COPY } };
        self.emit(DropEvent::Drop(list));
        Ok(())
    }
}
