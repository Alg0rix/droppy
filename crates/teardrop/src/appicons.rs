//! Real app icons for launcher search hits: a .lnk's shell image extracted
//! once, converted to premultiplied BGRA through WIC, and cached for the
//! painter. The UI thread is STA (OleInitialize in window::run), so the
//! shell calls below are legal where layout runs.

use crate::ui::scene::Image;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Icons draw at ~18px logical (up to ~36px at 2x DPI); 48 is the shell's
/// standard "large" size and downscales cleanly.
const PX: i32 = 48;

static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<Image>>>>> = OnceLock::new();

/// Premultiplied-BGRA icon for the app `lnk` points at, or None when the
/// shell can't produce one (callers fall back to the letter tile).
pub fn icon_for(lnk: &str) -> Option<Arc<Image>> {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().unwrap().get(lnk) {
        return hit.clone();
    }
    let img = extract(lnk).map(Arc::new);
    cache.lock().unwrap().insert(lnk.to_string(), img.clone());
    img
}

/// File the icon should come from: the link's own IconLocation when set,
/// else the link target (no overlay arrow). Non-.lnk paths — files, folders,
/// `shell:`/::{GUID} parsing paths — go straight to the shell item factory.
fn icon_source(lnk: &str) -> String {
    if !lnk.to_ascii_lowercase().ends_with(".lnk") {
        return lnk.to_string();
    }
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
    use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink};
    unsafe {
        let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else {
            return lnk.to_string();
        };
        let Ok(pf) = link.cast::<IPersistFile>() else { return lnk.to_string() };
        if pf.Load(&HSTRING::from(lnk), STGM_READ).is_err() {
            return lnk.to_string();
        }
        // Custom icon location wins.
        let mut buf = [0u16; 260];
        let mut idx = 0;
        if link.GetIconLocation(&mut buf, &mut idx).is_ok() {
            let s = expand_env(&wide(&buf));
            if !s.is_empty() {
                return s;
            }
        }
        let mut buf = [0u16; 260];
        let mut fd = WIN32_FIND_DATAW::default();
        if link.GetPath(&mut buf, &mut fd, SLGP_RAWPATH.0 as u32).is_ok() {
            let s = expand_env(&wide(&buf));
            if !s.is_empty() {
                return s;
            }
        }
        lnk.to_string()
    }
}

/// `%windir%\...` -> `C:\WINDOWS\...` — SLGP_RAWPATH and IconLocation keep
/// environment variables unexpanded.
fn expand_env(s: &str) -> String {
    use windows::core::HSTRING;
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
    if !s.contains('%') {
        return s.to_string();
    }
    unsafe {
        let mut buf = [0u16; 2048];
        if ExpandEnvironmentStringsW(&HSTRING::from(s), Some(&mut buf)) == 0 {
            return s.to_string();
        }
        wide(&buf)
    }
}

fn wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn extract(lnk: &str) -> Option<Image> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{DeleteObject, HPALETTE};
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, IWICImagingFactory, IWICPalette, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
        WICBitmapUsePremultipliedAlpha, WICRect, GUID_WICPixelFormat32bppPBGRA,
    };
    use windows::Win32::System::Com::{CoCreateInstance, IBindCtx, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK};
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(icon_source(lnk)), None::<&IBindCtx>).ok()?;
        // No ICONONLY flag: files that have real thumbnails (images, docs)
        // get them, everything else falls back to its icon.
        let hbmp = item
            .cast::<IShellItemImageFactory>()
            .ok()?
            .GetImage(SIZE { cx: PX, cy: PX }, SIIGBF_BIGGERSIZEOK)
            .ok()?;
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        // Shell image-factory bitmaps are already premultiplied BGRA; WIC
        // normalizes whatever it gets to 32bppPBGRA for the D2D painter.
        let src = wic.CreateBitmapFromHBITMAP(hbmp, HPALETTE::default(), WICBitmapUsePremultipliedAlpha).ok();
        let _ = DeleteObject(hbmp);
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&src?, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None::<&IWICPalette>, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let (mut w, mut h) = (0, 0);
        conv.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        let stride = w * 4;
        let mut bgra = vec![0u8; (stride * h) as usize];
        conv.CopyPixels(&WICRect { X: 0, Y: 0, Width: w as i32, Height: h as i32 }, stride, &mut bgra).ok()?;
        Some(Image::new(w, h, bgra))
    }
}

#[cfg(test)]
mod tests {
    /// Extraction end-to-end on the machine's real Start Menu: the first
    /// indexed .lnk must yield a premultiplied bitmap with matching size.
    #[test]
    fn extracts_a_real_icon() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        }
        let Some((_, lnk)) = crate::launcher::installed_apps().first().cloned() else {
            return; // nothing installed — nothing to assert against
        };
        let img = super::icon_for(&lnk).expect("shell image for a Start Menu .lnk");
        assert!(img.w > 0 && img.h > 0);
        assert_eq!(img.bgra.len() as u32, img.w * img.h * 4);
        // Cache serves the same image back.
        assert_eq!(super::icon_for(&lnk).unwrap().id, img.id);

        // Cmd::Image through the real D2D painter must land pixels on canvas.
        let mut s = crate::ui::scene::Scene::new();
        let r = crate::ui::scene::Rect::new(4.0, 4.0, 32.0, 32.0);
        s.image(r, &img);
        let (pw, ph, px) = crate::render_d2d::d2d::render_offscreen(&s, (40.0, 40.0), 1.0).expect("offscreen render");
        let hit = (0..ph as usize).any(|y| {
            (0..pw as usize).any(|x| {
                let inside = x >= 4 && x < 36 && y >= 4 && y < 36;
                inside && px[(y * pw as usize + x) * 4 + 3] > 0
            })
        });
        assert!(hit, "icon drew no visible pixels");
    }
}
