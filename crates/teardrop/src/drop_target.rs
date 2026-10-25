//! OLE drop target for the island.
//!
//! Unlike plain `WM_DROPFILES` (which only fires on release), an
//! `IDropTarget` is told when a drag enters and leaves, so the island can
//! react to a file being carried over it before it is dropped.

use windows::core::{implement, Result, w};
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

/// The data a drop carries: files, a URL/text, or memory blobs
/// (virtual files from Outlook/zips, dropped images, HTML fragments).
pub struct DropPayload {
    pub paths: Vec<String>,
    pub text: Option<String>,
    pub blobs: Vec<(String, Vec<u8>)>,
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
    unsafe { RegisterClipboardFormatW(w!("UniformResourceLocatorW")) as u32 }
}

fn reg_cf(name: windows::core::PCWSTR) -> u32 {
    unsafe { RegisterClipboardFormatW(name) as u32 }
}

/// Raw bytes out of a TYMED_HGLOBAL medium (lindex for FileContents).
fn hglobal_bytes(data: &IDataObject, cf: u32, lindex: i32) -> Option<Vec<u8>> {
    let f = FORMATETC { cfFormat: cf as u16, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex, tymed: TYMED_HGLOBAL.0 as u32 };
    unsafe {
        if data.QueryGetData(&f) != windows::Win32::Foundation::S_OK {
            return None;
        }
        let mut medium = data.GetData(&f).ok()?;
        let base = GlobalLock(medium.u.hGlobal);
        let mut out = None;
        if !base.is_null() {
            let size = windows::Win32::System::Memory::GlobalSize(medium.u.hGlobal);
            out = Some(std::slice::from_raw_parts(base as *const u8, size).to_vec());
            let _ = GlobalUnlock(medium.u.hGlobal);
        }
        ReleaseStgMedium(&mut medium);
        out
    }
}

/// Virtual files (Outlook mail/attachments, zip windows, Teams): the drop
/// carries a FILEDESCRIPTORW list + per-index CFSTR_FILECONTENTS streams.
fn virtual_files(data: &IDataObject) -> Vec<(String, Vec<u8>)> {
    let fd_cf = reg_cf(windows::core::w!("FileGroupDescriptorW"));
    let fc_cf = reg_cf(windows::core::w!("FileContents"));
    let mut out = Vec::new();
    let Some(desc) = hglobal_bytes(data, fd_cf, -1) else { return out };
    // FILEDESCRIPTORW is 592 bytes: flags(4) .. cFileName[260](520) at +76.
    let n = u32::from_le_bytes(desc.get(0..4).unwrap_or(&[0; 4]).try_into().unwrap_or([0; 4])) as usize;
    for i in 0..n.min(24) {
        let off = 4 + i * 592;
        let name_bytes = desc.get(off + 76..off + 76 + 520).unwrap_or(&[]);
        let name = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(name_bytes.as_ptr() as *const u16, 260) });
        let name = name.trim_matches(char::from(0)).replace([char::from(47), char::from(92)], "_").trim().to_string();
        if name.is_empty() { continue; }
        if let Some(bytes) = hglobal_bytes(data, fc_cf, i as i32) {
            out.push((name, bytes));
        }
    }
    out
}

/// Dropped pixels: a PNG stream or a DIB (wrapped into a .bmp file header).
fn image_blobs(data: &IDataObject) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    if let Some(png) = hglobal_bytes(data, reg_cf(w!("PNG")), -1) {
        if png.len() > 8 && &png[1..4] == b"PNG" {
            out.push(("drop.png".to_string(), png));
            return out;
        }
    }
    if let Some(dib) = hglobal_bytes(data, 17, -1) {
        if dib.len() > 40 {
            let mut bmp = Vec::with_capacity(dib.len() + 14);
            let clr_used = u32::from_le_bytes(dib[32..36].try_into().unwrap_or([0; 4])) as usize;
            let off_bits = 14 + 40 + clr_used * 4;
            let size = (dib.len() + 14) as u32;
            let mut head = [0u8; 14];
            head[0] = 66; head[1] = 77;
            head[2..6].copy_from_slice(&size.to_le_bytes());

            head[10..14].copy_from_slice(&(off_bits as u32).to_le_bytes());
            bmp.extend_from_slice(&head);
            bmp.extend_from_slice(&dib);
            out.push(("drop.bmp".to_string(), bmp));
        }
    }
    out
}

/// First URL inside a CF_HTML payload, or the fragment text itself.
fn html_payload(data: &IDataObject) -> Option<String> {
    let raw = hglobal_bytes(data, reg_cf(w!("HTML Format")), -1)?;
    let s = String::from_utf8_lossy(&raw);
    if let Some(pos) = s.find("http") {
        let tail = &s[pos..];
        let end = tail.find(|c| c == char::from(34) || c == char::from(39) || c == char::from(32) || c == char::from(60)).map(|e| pos + e).unwrap_or(s.len().min(pos + 2048));
        if end > pos + 8 {
            return Some(s[pos..end].to_string());
        }
    }
    Some(s.chars().take(4000).collect())
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
            if files > 0 {
                files
            } else {
                virtual_files(d).len()
                    + image_blobs(d).len()
                    + usize::from(hglobal_text(d, CF_UNICODETEXT.0 as u32).is_some()
                        || hglobal_text(d, inet_url_cf()).is_some()
                        || html_payload(d).is_some())
            }
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
                let (text, blobs) = if paths.is_empty() {
                    let vf = virtual_files(d);
                    if !vf.is_empty() {
                        (None, vf)
                    } else {
                        let img = image_blobs(d);
                        if !img.is_empty() {
                            (None, img)
                        } else {
                            let t = hglobal_text(d, inet_url_cf())
                                .or_else(|| hglobal_text(d, CF_UNICODETEXT.0 as u32))
                                .or_else(|| html_payload(d));
                            (t, Vec::new())
                        }
                    }
                } else {
                    (None, Vec::new())
                };
                DropPayload { paths, text, blobs }
            })
            .unwrap_or(DropPayload { paths: Vec::new(), text: None, blobs: Vec::new() });
        let empty = payload.paths.is_empty() && payload.text.is_none() && payload.blobs.is_empty();
        unsafe { *pdweffect = if empty { DROPEFFECT_NONE } else { DROPEFFECT_COPY } };
        self.emit(DropEvent::Drop(payload, pt.x, pt.y));
        Ok(())
    }
}
