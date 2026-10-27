//! Drag files out of the Shelf.
//!
//! A shelf tile starts a standard OLE drag with a `CF_HDROP` payload, so
//! Explorer, desktop, mail clients and any other drop target receive the real
//! file path. The source only offers copy/link: dragging a shelf entry never
//! moves or deletes the original file.

use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::core::{implement, Result, HRESULT};
use windows::Win32::Foundation::{
    GlobalFree, BOOL, DATA_S_SAMEFORMATETC, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, DV_E_DVASPECT, DV_E_FORMATETC, DV_E_LINDEX, DV_E_TYMED, E_INVALIDARG, E_NOTIMPL,
    E_OUTOFMEMORY, HGLOBAL, OLE_E_ADVISENOTSUPPORTED, POINT, S_FALSE, S_OK,
};
use windows::Win32::System::Com::{
    IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumFORMATETC_Impl, IEnumSTATDATA, DATADIR_GET, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{DoDragDrop, IDropSource, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::DROPFILES;

/// Start an OLE drag for one shelf file. Blocks until the user drops or
/// cancels, exactly like Explorer's own drag loop.
pub fn drag(path: &str) -> bool {
    let data: IDataObject = ShelfDataObject { paths: vec![path.to_string()] }.into();
    let source: windows::Win32::System::Ole::IDropSource = DropSource.into();
    let mut effect = DROPEFFECT(0);
    let hr = unsafe { DoDragDrop(&data, &source, DROPEFFECT_COPY | DROPEFFECT_LINK, &mut effect) };
    hr == DRAGDROP_S_DROP
}

fn hdrop_format() -> FORMATETC {
    FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 }
}

fn accepts(f: &FORMATETC) -> HRESULT {
    if f.cfFormat != CF_HDROP.0 {
        DV_E_FORMATETC
    } else if f.dwAspect != DVASPECT_CONTENT.0 {
        DV_E_DVASPECT
    } else if f.lindex != -1 {
        DV_E_LINDEX
    } else if f.tymed & TYMED_HGLOBAL.0 as u32 == 0 {
        DV_E_TYMED
    } else {
        S_OK
    }
}

/// A `DROPFILES` header followed by the double-null-terminated wide path list.
fn hdrop_payload(paths: &[String]) -> Vec<u8> {
    let mut units: Vec<u16> = Vec::new();
    for p in paths {
        units.extend(p.encode_utf16());
        units.push(0);
    }
    units.push(0);

    let header = std::mem::size_of::<DROPFILES>();
    let mut bytes = vec![0u8; header + units.len() * 2];
    let df = DROPFILES { pFiles: header as u32, pt: POINT::default(), fNC: BOOL(0), fWide: BOOL(1) };
    unsafe {
        std::ptr::write_unaligned(bytes.as_mut_ptr() as *mut DROPFILES, df);
        std::ptr::copy_nonoverlapping(units.as_ptr() as *const u8, bytes.as_mut_ptr().add(header), units.len() * 2);
    }
    bytes
}

#[implement(IDataObject)]
struct ShelfDataObject {
    paths: Vec<String>,
}

impl IDataObject_Impl for ShelfDataObject_Impl {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> Result<STGMEDIUM> {
        if pformatetcin.is_null() {
            return Err(E_INVALIDARG.into());
        }
        let f = unsafe { &*pformatetcin };
        let hr = accepts(f);
        if hr.is_err() {
            return Err(hr.into());
        }

        let bytes = hdrop_payload(&self.paths);
        let hglobal: HGLOBAL = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len())? };
        let ptr = unsafe { GlobalLock(hglobal) };
        if ptr.is_null() {
            unsafe {
                let _ = GlobalFree(hglobal);
            }
            return Err(E_OUTOFMEMORY.into());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
            let _ = GlobalUnlock(hglobal);
        }
        Ok(STGMEDIUM { tymed: TYMED_HGLOBAL.0 as u32, u: STGMEDIUM_0 { hGlobal: hglobal }, pUnkForRelease: ManuallyDrop::new(None) })
    }

    fn GetDataHere(&self, _pformatetc: *const FORMATETC, _pmedium: *mut STGMEDIUM) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        if pformatetc.is_null() {
            return E_INVALIDARG;
        }
        accepts(unsafe { &*pformatetc })
    }

    fn GetCanonicalFormatEtc(&self, pformatectin: *const FORMATETC, pformatectout: *mut FORMATETC) -> HRESULT {
        if pformatectin.is_null() || pformatectout.is_null() {
            return E_INVALIDARG;
        }
        unsafe {
            *pformatectout = *pformatectin;
        }
        DATA_S_SAMEFORMATETC
    }

    fn SetData(&self, _pformatetc: *const FORMATETC, _pmedium: *const STGMEDIUM, _frelease: BOOL) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> Result<IEnumFORMATETC> {
        if dwdirection != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        Ok(FormatEtcEnum { index: AtomicUsize::new(0) }.into())
    }

    fn DAdvise(&self, _pformatetc: *const FORMATETC, _advf: u32, _padvsink: Option<&IAdviseSink>) -> Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _dwconnection: u32) -> Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

#[implement(IEnumFORMATETC)]
struct FormatEtcEnum {
    index: AtomicUsize,
}

impl IEnumFORMATETC_Impl for FormatEtcEnum_Impl {
    fn Next(&self, celt: u32, rgelt: *mut FORMATETC, pceltfetched: *mut u32) -> HRESULT {
        if celt == 0 {
            return S_OK;
        }
        if rgelt.is_null() {
            return E_INVALIDARG;
        }
        let mut n = 0usize;
        while n < celt as usize {
            if self.index.fetch_add(1, Ordering::Relaxed) > 0 {
                break;
            }
            unsafe {
                *rgelt.add(n) = hdrop_format();
            }
            n += 1;
        }
        if !pceltfetched.is_null() {
            unsafe {
                *pceltfetched = n as u32;
            }
        }
        if n == celt as usize {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, celt: u32) -> Result<()> {
        let skipped = self.index.load(Ordering::Relaxed);
        let left = 1usize.saturating_sub(skipped);
        if celt as usize <= left {
            self.index.fetch_add(celt as usize, Ordering::Relaxed);
            Ok(())
        } else {
            self.index.store(1, Ordering::Relaxed);
            Err(S_FALSE.into())
        }
    }

    fn Reset(&self) -> Result<()> {
        self.index.store(0, Ordering::Relaxed);
        Ok(())
    }

    fn Clone(&self) -> Result<IEnumFORMATETC> {
        Ok(FormatEtcEnum { index: AtomicUsize::new(self.index.load(Ordering::Relaxed)) }.into())
    }
}

#[implement(IDropSource)]
struct DropSource;

impl windows::Win32::System::Ole::IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fescapepressed.as_bool() {
            windows::Win32::Foundation::DRAGDROP_S_CANCEL
        } else if grfkeystate.0 & MK_LBUTTON.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_is_a_wide_double_null_path_list() {
        let paths = vec!["C:\\Work\\a.txt".to_string(), "C:\\Work\\b.txt".to_string()];
        let bytes = hdrop_payload(&paths);
        let header = unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const DROPFILES) };
        assert_eq!(header.pFiles as usize, std::mem::size_of::<DROPFILES>());
        assert!(header.fWide.as_bool());
        assert!(!header.fNC.as_bool());

        let units: Vec<u16> = bytes[header.pFiles as usize..].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let mut found = Vec::new();
        let mut start = 0;
        for (i, ch) in units.iter().enumerate() {
            if *ch == 0 {
                if i > start {
                    found.push(String::from_utf16(&units[start..i]).unwrap());
                }
                start = i + 1;
            }
        }
        assert_eq!(found, paths);
        assert_eq!(&units[units.len() - 2..], &[0, 0]);
    }

    #[test]
    fn get_data_hands_back_the_paths() {
        let paths = vec!["C:\\Work\\a.txt".to_string()];
        let obj: IDataObject = ShelfDataObject { paths: paths.clone() }.into();
        let f = hdrop_format();
        let mut medium = unsafe { obj.GetData(&f) }.expect("GetData for CF_HDROP");
        assert_eq!(medium.tymed, TYMED_HGLOBAL.0 as u32);

        let hglobal = unsafe { medium.u.hGlobal };
        let size = unsafe { windows::Win32::System::Memory::GlobalSize(hglobal) };
        assert!(size > 0, "the medium owns a real HGLOBAL");
        let ptr = unsafe { GlobalLock(hglobal) } as *const u8;
        assert!(!ptr.is_null());
        let got = unsafe { std::slice::from_raw_parts(ptr, size) }.to_vec();
        unsafe {
            let _ = GlobalUnlock(hglobal);
            windows::Win32::System::Ole::ReleaseStgMedium(&mut medium);
        }

        assert_eq!(got, hdrop_payload(&paths));
    }

    #[test]
    fn query_accepts_only_hdrop_hglobal() {
        let mut f = hdrop_format();
        assert!(accepts(&f).is_ok());
        f.cfFormat = 1;
        assert_eq!(accepts(&f), DV_E_FORMATETC);
        f = hdrop_format();
        f.tymed = 0;
        assert_eq!(accepts(&f), DV_E_TYMED);
        f = hdrop_format();
        f.lindex = 0;
        assert_eq!(accepts(&f), DV_E_LINDEX);
        f = hdrop_format();
        f.dwAspect = 2;
        assert_eq!(accepts(&f), DV_E_DVASPECT);
    }

    #[test]
    fn format_enumerator_yields_hdrop_once() {
        let e: IEnumFORMATETC = FormatEtcEnum { index: AtomicUsize::new(0) }.into();
        let mut f = [FORMATETC::default(); 1];
        let mut n = 0u32;

        assert_eq!(unsafe { e.Next(&mut f, Some(&mut n)) }, S_OK);
        assert_eq!(n, 1);
        assert_eq!(f[0].cfFormat, CF_HDROP.0);
        assert_eq!(f[0].tymed, TYMED_HGLOBAL.0 as u32);

        assert_eq!(unsafe { e.Next(&mut f, Some(&mut n)) }, S_FALSE);
        assert_eq!(n, 0, "a second read reports nothing left");

        assert!(unsafe { e.Reset() }.is_ok());
        assert_eq!(unsafe { e.Next(&mut f, None) }, S_OK, "reset rewinds the enumerator");
    }
}
