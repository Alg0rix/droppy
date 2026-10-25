//! Direct2D painter: replays a [`Scene`] with antialiasing, gradients, soft
//! shadow and DirectWrite text.
//!
//! Two targets share one painter:
//! - [`Presenter`]: ID2D1DCRenderTarget -> premultiplied DIB -> UpdateLayeredWindow,
//!   giving the island real per-pixel alpha (soft shadow, no jagged region edge).
//! - [`render_offscreen`]: WIC bitmap, used by `--selftest` for pixel-true PNGs.

#[cfg(windows)]
pub mod d2d {
    use crate::ui::scene::{Align, Cmd, Color, Fill, Scene, Weight};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use windows::core::{w, Interface};
    use windows::Foundation::Numerics::Matrix3x2;
    use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
    use windows::Win32::Graphics::Direct2D::Common::{
        D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED,
        D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP,
        D2D1_PIXEL_FORMAT, D2D_POINT_2F, D2D_RECT_F, D2D_SIZE_U,
    };
    use windows::Win32::Graphics::Direct2D::{
        D2D1CreateFactory, ID2D1Bitmap, ID2D1Brush, ID2D1DCRenderTarget, ID2D1Factory, ID2D1LinearGradientBrush, ID2D1PathGeometry,
        ID2D1RenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
        D2D1_BITMAP_BRUSH_PROPERTIES, D2D1_BITMAP_PROPERTIES, D2D1_BRUSH_PROPERTIES, D2D1_CAP_STYLE_ROUND,
        D2D1_DASH_STYLE_DASH, D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP,
        D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_GAMMA_2_2, D2D1_LINE_JOIN_ROUND,
        D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
        D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
        D2D1_STROKE_STYLE_PROPERTIES, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    };
    use windows::Win32::Graphics::DirectWrite::{
        DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
        DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
        DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    };
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        D3D11_RESOURCE_MISC_GDI_COMPATIBLE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
    use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGISurface, IDXGISurface1};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

    fn d2d_color(c: Color) -> D2D1_COLOR_F {
        D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
    }

    fn pt(x: f32, y: f32) -> D2D_POINT_2F {
        D2D_POINT_2F { x, y }
    }

    fn rc(r: crate::ui::scene::Rect) -> D2D_RECT_F {
        D2D_RECT_F { left: r.x, top: r.y, right: r.x + r.w, bottom: r.y + r.h }
    }

    /// `layouts` key: string + format key + box w/h bits.
    type LayoutKey = (String, (u32, Weight, u8), u32, u32);
    /// `grads` key: target pointer + stop-colour bits.
    type GradKey = (usize, [u32; 4], [u32; 4]);

    /// Factories + caches shared by every target.
    pub struct Painter {
        factory: ID2D1Factory,
        dwrite: IDWriteFactory,
        fmts: RefCell<HashMap<(u32, Weight, u8), IDWriteTextFormat>>,
        /// Shaped text: layouts are device-independent so they survive
        /// render-target recreation.
        layouts: RefCell<HashMap<LayoutKey, IDWriteTextLayout>>,
        /// Uploaded bitmaps keyed by (render target, image id): a D2D bitmap is
        /// bound to the target that created it, so both halves of the key matter.
        bitmaps: RefCell<HashMap<(usize, u64), ID2D1Bitmap>>,
        /// Gradient brushes by target + colours; start/end points are re-set
        /// per use so one brush serves every bounds/orientation.
        grads: RefCell<HashMap<GradKey, ID2D1LinearGradientBrush>>,
        round: ID2D1StrokeStyle,
        dashed: ID2D1StrokeStyle,
    }

    impl Painter {
        pub fn new() -> Option<Self> {
            unsafe {
                let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
                let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
                let mk = |dash| {
                    let p = D2D1_STROKE_STYLE_PROPERTIES {
                        startCap: D2D1_CAP_STYLE_ROUND,
                        endCap: D2D1_CAP_STYLE_ROUND,
                        dashCap: D2D1_CAP_STYLE_ROUND,
                        lineJoin: D2D1_LINE_JOIN_ROUND,
                        miterLimit: 1.0,
                        dashStyle: dash,
                        dashOffset: 0.0,
                    };
                    factory.CreateStrokeStyle(&p, None)
                };
                let round = mk(D2D1_DASH_STYLE_SOLID).ok()?;
                let dashed = mk(D2D1_DASH_STYLE_DASH).ok()?;
                Some(Self {
                    factory,
                    dwrite,
                    fmts: RefCell::new(HashMap::new()),
                    layouts: RefCell::new(HashMap::new()),
                    bitmaps: RefCell::new(HashMap::new()),
                    grads: RefCell::new(HashMap::new()),
                    round,
                    dashed,
                })
            }
        }

        /// Shared key for the `fmts`/`layouts` caches.
        fn fmt_key(size: f32, weight: Weight, align: Align, italic: bool, mono: bool) -> (u32, Weight, u8) {
            ((size * 10.0) as u32, weight, align as u8 | (italic as u8) << 4 | (mono as u8) << 5)
        }

        fn format(&self, size: f32, weight: Weight, align: Align) -> Option<IDWriteTextFormat> {
            self.format_styled(size, weight, align, false, false)
        }

        fn format_styled(&self, size: f32, weight: Weight, align: Align, italic: bool, mono: bool) -> Option<IDWriteTextFormat> {
            let key = Self::fmt_key(size, weight, align, italic, mono);
            if let Some(f) = self.fmts.borrow().get(&key) {
                return Some(f.clone());
            }
            let wt = match weight {
                Weight::Regular => 400,
                Weight::Medium => 500,
                Weight::Semibold => 600,
                Weight::Bold => 700,
            };
            unsafe {
                let f = self
                    .dwrite
                    .CreateTextFormat(
                        if mono { w!("Consolas") } else { w!("Segoe UI") },
                        None,
                        DWRITE_FONT_WEIGHT(wt),
                        if italic { DWRITE_FONT_STYLE_ITALIC } else { DWRITE_FONT_STYLE_NORMAL },
                        DWRITE_FONT_STRETCH_NORMAL,
                        size,
                        w!("en-us"),
                    )
                    .ok()?;
                let _ = f.SetTextAlignment(match align {
                    Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                    Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                    Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
                });
                let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
                let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
                if let Ok(sign) = self.dwrite.CreateEllipsisTrimmingSign(&f) {
                    let _ = f.SetTrimming(&trim, &sign);
                }
                self.fmts.borrow_mut().insert(key, f.clone());
                Some(f)
            }
        }

        fn text_width(&self, s: &[u16], fmt: &IDWriteTextFormat) -> f32 {
            unsafe {
                let Ok(layout) = self.dwrite.CreateTextLayout(s, fmt, 4000.0, 200.0) else { return 0.0 };
                Self::layout_width(&layout)
            }
        }

        fn layout_width(layout: &IDWriteTextLayout) -> f32 {
            unsafe {
                let mut m = DWRITE_TEXT_METRICS::default();
                if layout.GetMetrics(&mut m).is_ok() {
                    m.widthIncludingTrailingWhitespace
                } else {
                    0.0
                }
            }
        }

        /// Shaped layout for a (string, format, box) triple; CreateTextLayout
        /// was the dominant cost inside Text commands.
        fn layout(&self, s: &str, wide: &[u16], fmt: &IDWriteTextFormat, key: (u32, Weight, u8), w: f32, h: f32) -> Option<IDWriteTextLayout> {
            let k: LayoutKey = (s.to_string(), key, w.to_bits(), h.to_bits());
            if let Some(l) = self.layouts.borrow().get(&k) {
                return Some(l.clone());
            }
            unsafe {
                let l = self.dwrite.CreateTextLayout(wide, fmt, w.max(1.0), h.max(1.0)).ok()?;
                let mut map = self.layouts.borrow_mut();
                if map.len() >= 1024 {
                    map.clear();
                }
                map.insert(k, l.clone());
                Some(l)
            }
        }

        fn path(&self, pts: &[(f32, f32)], closed: bool) -> Option<ID2D1PathGeometry> {
            if pts.len() < 2 {
                return None;
            }
            unsafe {
                let geo = self.factory.CreatePathGeometry().ok()?;
                let sink = geo.Open().ok()?;
                sink.BeginFigure(pt(pts[0].0, pts[0].1), if closed { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW });
                let rest: Vec<D2D_POINT_2F> = pts[1..].iter().map(|p| pt(p.0, p.1)).collect();
                sink.AddLines(&rest);
                sink.EndFigure(if closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
                sink.Close().ok()?;
                Some(geo)
            }
        }

        /// Replay `scene` onto `rt`. Coordinates are logical px scaled by `scale`.
        pub fn draw(&self, rt: &ID2D1RenderTarget, scene: &Scene, scale: f32) {
            unsafe {
                rt.SetTransform(&Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: 0.0, M32: 0.0 });
                rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                let Ok(solid) = rt.CreateSolidColorBrush(&d2d_color(Color::rgb(0, 0, 0)), None) else { return };
                let mut clips = 0;
                for cmd in &scene.cmds {
                    self.cmd(rt, &solid, cmd, scale, &mut clips);
                }
                while clips > 0 {
                    rt.PopAxisAlignedClip();
                    clips -= 1;
                }
            }
        }

        unsafe fn gradient(&self, rt: &ID2D1RenderTarget, fill: &Fill, b: D2D_RECT_F) -> Option<ID2D1Brush> {
            let (a, c, diag) = match *fill {
                Fill::Solid(_) => return None,
                Fill::VGrad(a, c) => (a, c, false),
                Fill::DGrad(a, c) => (a, c, true),
            };
            let (sp, ep) = if diag {
                (pt(b.left, b.top), pt(b.right, b.bottom))
            } else {
                (pt((b.left + b.right) / 2.0, b.top), pt((b.left + b.right) / 2.0, b.bottom))
            };
            let bits = |c: Color| [c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()];
            let key = (Interface::as_raw(rt) as usize, bits(a), bits(c));
            // A cached brush just needs its endpoints moved to this bounds;
            // the stop collection and gamma are identical for these colours.
            if let Some(br) = self.grads.borrow().get(&key).cloned() {
                br.SetStartPoint(sp);
                br.SetEndPoint(ep);
                return br.cast::<ID2D1Brush>().ok();
            }
            let stops = [D2D1_GRADIENT_STOP { position: 0.0, color: d2d_color(a) }, D2D1_GRADIENT_STOP { position: 1.0, color: d2d_color(c) }];
            let coll = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP).ok()?;
            let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: sp, endPoint: ep };
            let br = rt.CreateLinearGradientBrush(&props, None, &coll).ok()?;
            let mut map = self.grads.borrow_mut();
            if map.len() >= 256 {
                map.clear();
            }
            map.insert(key, br.clone());
            br.cast::<ID2D1Brush>().ok()
        }

        unsafe fn fill_with(&self, rt: &ID2D1RenderTarget, solid: &ID2D1SolidColorBrush, fill: &Fill, bounds: D2D_RECT_F) -> ID2D1Brush {
            match fill {
                Fill::Solid(c) => {
                    solid.SetColor(&d2d_color(*c));
                    solid.cast::<ID2D1Brush>().unwrap()
                }
                other => self.gradient(rt, other, bounds).unwrap_or_else(|| solid.cast::<ID2D1Brush>().unwrap()),
            }
        }

        unsafe fn cmd(&self, rt: &ID2D1RenderTarget, solid: &ID2D1SolidColorBrush, cmd: &Cmd, scale: f32, clips: &mut i32) {
            match cmd {
                Cmd::Body { pts, fill, rim, shadow } => {
                    let Some(geo) = self.path(pts, true) else { return };
                    let bounds = bounds_of(pts);
                    // Soft shadow: stacked, widening, low-alpha strokes offset downward.
                    let base = Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: 0.0, M32: 0.0 };
                    rt.SetTransform(&Matrix3x2 { M32: 5.0 * scale, ..base });
                    for i in 1..=5 {
                        let k = i as f32 / 5.0;
                        solid.SetColor(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: shadow * 0.11 * (1.0 - k * 0.8) });
                        rt.DrawGeometry(&geo, solid, 3.6 * i as f32, &self.round);
                    }
                    rt.SetTransform(&base);
                    let br = self.fill_with(rt, solid, fill, bounds);
                    rt.FillGeometry(&geo, &br, None::<&ID2D1Brush>);
                    solid.SetColor(&d2d_color(*rim));
                    rt.DrawGeometry(&geo, solid, 1.0, None::<&ID2D1StrokeStyle>);
                }
                Cmd::Rect { r, radius, fill } => {
                    let b = rc(*r);
                    let br = self.fill_with(rt, solid, fill, b);
                    if *radius <= 0.5 {
                        rt.FillRectangle(&b, &br);
                    } else {
                        let rad = radius.min(r.w / 2.0).min(r.h / 2.0);
                        rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: b, radiusX: rad, radiusY: rad }, &br);
                    }
                }
                Cmd::Stroke { r, radius, width, color, dashed } => {
                    solid.SetColor(&d2d_color(*color));
                    let rad = radius.min(r.w / 2.0).min(r.h / 2.0);
                    let rr = D2D1_ROUNDED_RECT { rect: rc(*r), radiusX: rad, radiusY: rad };
                    rt.DrawRoundedRectangle(&rr, solid, *width, if *dashed { &self.dashed } else { &self.round });
                }
                Cmd::Circle { cx, cy, r, fill } => {
                    let b = D2D_RECT_F { left: cx - r, top: cy - r, right: cx + r, bottom: cy + r };
                    let br = self.fill_with(rt, solid, fill, b);
                    rt.FillEllipse(&D2D1_ELLIPSE { point: pt(*cx, *cy), radiusX: *r, radiusY: *r }, &br);
                }
                Cmd::Ring { cx, cy, r, width, from, to, color } => {
                    solid.SetColor(&d2d_color(*color));
                    if to - from >= 0.999 {
                        rt.DrawEllipse(&D2D1_ELLIPSE { point: pt(*cx, *cy), radiusX: *r, radiusY: *r }, solid, *width, None::<&ID2D1StrokeStyle>);
                    } else {
                        let steps = (((to - from) * 96.0).ceil() as usize).max(2);
                        let pts: Vec<(f32, f32)> = (0..=steps)
                            .map(|i| {
                                let a = (from + (to - from) * i as f32 / steps as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                                (cx + r * a.cos(), cy + r * a.sin())
                            })
                            .collect();
                        if let Some(g) = self.path(&pts, false) {
                            rt.DrawGeometry(&g, solid, *width, &self.round);
                        }
                    }
                }
                Cmd::Line { x1, y1, x2, y2, width, color } => {
                    solid.SetColor(&d2d_color(*color));
                    rt.DrawLine(pt(*x1, *y1), pt(*x2, *y2), solid, *width, &self.round);
                }
                Cmd::Poly { pts, fill } => {
                    let Some(g) = self.path(pts, true) else { return };
                    let br = self.fill_with(rt, solid, fill, bounds_of(pts));
                    rt.FillGeometry(&g, &br, None::<&ID2D1Brush>);
                }
                Cmd::Polyline { pts, width, color } => {
                    let Some(g) = self.path(pts, false) else { return };
                    solid.SetColor(&d2d_color(*color));
                    rt.DrawGeometry(&g, solid, *width, &self.round);
                }
                Cmd::Text { s, r, size, weight, color, align, karaoke, edit } => {
                    if s.is_empty() && edit.is_none() {
                        return;
                    }
                    let Some(fmt) = self.format(*size, *weight, *align) else { return };
                    let wide: Vec<u16> = s.encode_utf16().collect();
                    if let Some(ed) = edit {
                        // Editable text: measured caret + selection, scrolled
                        // horizontally so the caret stays inside `r`.
                        let n_chars = s.chars().count();
                        let utf16_of = |ci: usize| -> usize { s.chars().take(ci.min(n_chars)).map(|c| c.len_utf16()).sum() };
                        let pref_w = |ci: usize| self.text_width(&wide[..utf16_of(ci)], &fmt);
                        let tw = self.text_width(&wide, &fmt);
                        let caret_x = pref_w(ed.caret);
                        let off = if tw > r.w { (caret_x - r.w + 6.0).clamp(0.0, (tw - r.w).max(0.0)) } else { 0.0 };
                        rt.PushAxisAlignedClip(&rc(*r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                        if let Some((a, b)) = ed.sel {
                            let (xa, xb) = (r.x + pref_w(a) - off, r.x + pref_w(b) - off);
                            solid.SetColor(&d2d_color(ed.sel_color));
                            rt.FillRectangle(&D2D_RECT_F { left: xa, top: r.y + 1.0, right: xb.max(xa + 1.0), bottom: r.y + r.h - 1.0 }, solid);
                        }
                        if !wide.is_empty() {
                            let tr = D2D_RECT_F { left: r.x - off, top: r.y, right: r.x - off + (tw + 4.0).max(r.w), bottom: r.y + r.h };
                            solid.SetColor(&d2d_color(*color));
                            rt.DrawText(&wide, &fmt, &tr, solid, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
                        }
                        if ed.show_caret {
                            let h = (size * 1.15).min(r.h - 2.0).max(2.0);
                            let top = r.y + (r.h - h) / 2.0;
                            let x = r.x + caret_x - off;
                            solid.SetColor(&d2d_color(ed.caret_color));
                            rt.FillRectangle(&D2D_RECT_F { left: x + 1.0, top, right: x + 2.6, bottom: top + h }, solid);
                        }
                        rt.PopAxisAlignedClip();
                    } else {
                        solid.SetColor(&d2d_color(*color));
                        let Some(layout) = self.layout(s, &wide, &fmt, Self::fmt_key(*size, *weight, *align, false, false), r.w, r.h) else {
                            return;
                        };
                        rt.DrawTextLayout(pt(r.x, r.y), &layout, solid, D2D1_DRAW_TEXT_OPTIONS_NONE);
                        if let Some((frac, lit)) = karaoke {
                            let tw = Self::layout_width(&layout).min(r.w);
                            let clip = D2D_RECT_F { left: r.x, top: r.y, right: r.x + tw * frac, bottom: r.y + r.h };
                            rt.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                            solid.SetColor(&d2d_color(*lit));
                            rt.DrawTextLayout(pt(r.x, r.y), &layout, solid, D2D1_DRAW_TEXT_OPTIONS_NONE);
                            rt.PopAxisAlignedClip();
                        }
                    }
                }
                Cmd::Runs { r, size, runs } => {
                    rt.PushAxisAlignedClip(&rc(*r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                    let mut x = r.x;
                    for run in runs {
                        if run.text.is_empty() {
                            continue;
                        }
                        let Some(fmt) = self.format_styled(*size, run.weight, Align::Left, run.italic, run.mono) else { continue };
                        let wide: Vec<u16> = run.text.encode_utf16().collect();
                        let tw = self.text_width(&wide, &fmt);
                        let pad = if run.chip.is_some() { crate::ui::scene::CHIP_PAD } else { 0.0 };
                        if let Some(chip) = run.chip {
                            let h = (size * 1.45).min(r.h);
                            let cr = D2D_RECT_F { left: x, top: r.y + (r.h - h) / 2.0, right: x + tw + pad * 2.0, bottom: r.y + (r.h + h) / 2.0 };
                            solid.SetColor(&d2d_color(chip));
                            rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: cr, radiusX: 5.0, radiusY: 5.0 }, solid);
                        }
                        let tr = D2D_RECT_F { left: x + pad, top: r.y, right: x + pad + tw + 4.0, bottom: r.y + r.h };
                        solid.SetColor(&d2d_color(run.color));
                        rt.DrawText(&wide, &fmt, &tr, solid, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
                        if run.underline {
                            let uy = r.y + r.h / 2.0 + size * 0.62;
                            rt.DrawLine(pt(x + pad, uy), pt(x + pad + tw.max(1.0) - 1.0, uy), solid, 1.0, None::<&ID2D1StrokeStyle>);
                        }
                        x += tw + pad * 2.0;
                    }
                    rt.PopAxisAlignedClip();
                }
                Cmd::Image { r, radius, img, opacity } => {
                    let key = (Interface::as_raw(rt) as usize, img.id);
                    let cached = self.bitmaps.borrow().get(&key).cloned();
                    let bmp = match cached {
                        Some(b) => Some(b),
                        None => {
                            let props = D2D1_BITMAP_PROPERTIES {
                                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                                dpiX: 96.0,
                                dpiY: 96.0,
                            };
                            let created = rt
                                .CreateBitmap(D2D_SIZE_U { width: img.w, height: img.h }, Some(img.bgra.as_ptr() as *const _), img.w * 4, &props)
                                .ok();
                            if let Some(b) = &created {
                                let mut map = self.bitmaps.borrow_mut();
                                // Per-song covers would grow this without a cap.
                                if map.len() >= 128 {
                                    map.clear();
                                }
                                map.insert(key, b.clone());
                            }
                            created
                        }
                    };
                    if let Some(b) = bmp {
                        if *radius <= 0.0 {
                            rt.DrawBitmap(&b, Some(&rc(*r)), *opacity, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None);
                        } else {
                            // Bitmap brush scaled+translated so the image covers
                            // r exactly, then the rounded rect masks the corners.
                            let brush_props = D2D1_BRUSH_PROPERTIES {
                                opacity: *opacity,
                                transform: Matrix3x2 { M11: r.w / img.w as f32, M12: 0.0, M21: 0.0, M22: r.h / img.h as f32, M31: r.x, M32: r.y },
                            };
                            let bmp_props = D2D1_BITMAP_BRUSH_PROPERTIES {
                                extendModeX: D2D1_EXTEND_MODE_CLAMP,
                                extendModeY: D2D1_EXTEND_MODE_CLAMP,
                                interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                            };
                            if let Ok(brush) = rt.CreateBitmapBrush(&b, Some(&bmp_props), Some(&brush_props)) {
                                let rad = radius.min(r.w / 2.0).min(r.h / 2.0);
                                rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rc(*r), radiusX: rad, radiusY: rad }, &brush);
                            }
                        }
                    }
                }
                Cmd::Clip(r) => {
                    rt.PushAxisAlignedClip(&rc(*r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                    *clips += 1;
                }
                Cmd::PopClip => {
                    if *clips > 0 {
                        rt.PopAxisAlignedClip();
                        *clips -= 1;
                    }
                }
            }
        }
    }

    fn bounds_of(pts: &[(f32, f32)]) -> D2D_RECT_F {
        let mut b = D2D_RECT_F { left: f32::MAX, top: f32::MAX, right: f32::MIN, bottom: f32::MIN };
        for (x, y) in pts {
            b.left = b.left.min(*x);
            b.top = b.top.min(*y);
            b.right = b.right.max(*x);
            b.bottom = b.bottom.max(*y);
        }
        if b.right <= b.left {
            b.right = b.left + 1.0;
        }
        if b.bottom <= b.top {
            b.bottom = b.top + 1.0;
        }
        b
    }

    // ------------------------------------------------------------ presenter

    /// Last frame's Direct2D draw time in 1/100 ms (diagnostics only).
    pub static PROF_DRAW: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    /// GPU path: Direct2D on a D3D11 texture that GDI can read back.
    struct Gpu {
        _dev: ID3D11Device,
        _tex: ID3D11Texture2D,
        surface: IDXGISurface1,
        rt: ID2D1RenderTarget,
    }

    fn make_gpu(painter: &Painter, w: i32, h: i32) -> Option<Gpu> {
        unsafe {
            let mut dev: Option<ID3D11Device> = None;
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut dev),
                None,
                None,
            )
            .ok()?;
            let dev = dev?;
            let desc = D3D11_TEXTURE2D_DESC {
                Width: w as u32,
                Height: h as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: D3D11_RESOURCE_MISC_GDI_COMPATIBLE.0 as u32,
            };
            let mut tex: Option<ID3D11Texture2D> = None;
            dev.CreateTexture2D(&desc, None, Some(&mut tex)).ok()?;
            let tex = tex?;
            let surface: IDXGISurface = tex.cast().ok()?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let rt = painter.factory.CreateDxgiSurfaceRenderTarget(&surface, &props).ok()?;
            let surface1: IDXGISurface1 = surface.cast().ok()?;
            Some(Gpu { _dev: dev, _tex: tex, surface: surface1, rt })
        }
    }

    /// Draws into a premultiplied surface and pushes it with UpdateLayeredWindow.
    /// Prefers the GPU path; falls back to a CPU DIB when no hardware device exists.
    pub struct Presenter {
        painter: Painter,
        gpu: Option<Gpu>,
        rt: ID2D1DCRenderTarget,
        dc: HDC,
        dib: HBITMAP,
        old: HGDIOBJ,
        cap: (i32, i32),
    }

    impl Presenter {
        pub fn new(max_w: i32, max_h: i32) -> Option<Self> {
            let painter = Painter::new()?;
            let gpu = if std::env::var_os("TEARDROP_CPU").is_some() { None } else { make_gpu(&painter, max_w, max_h) };
            unsafe {
                let props = D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                    pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                    dpiX: 96.0,
                    dpiY: 96.0,
                    usage: D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE,
                    minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                };
                let rt = painter.factory.CreateDCRenderTarget(&props).ok()?;
                let dc = CreateCompatibleDC(None);
                if dc.is_invalid() {
                    return None;
                }
                // The CPU DIB is only needed as a fallback; keep it tiny while the GPU path is live.
                let (dw, dh) = if gpu.is_some() { (16, 16) } else { (max_w, max_h) };
                let (dib, old) = make_dib(dc, dw, dh)?;
                Some(Self { painter, gpu, rt, dc, dib, old, cap: (max_w, max_h) })
            }
        }

        pub fn is_gpu(&self) -> bool {
            self.gpu.is_some()
        }

        fn ensure_capacity(&mut self, w: i32, h: i32) -> bool {
            if w <= self.cap.0 && h <= self.cap.1 {
                return true;
            }
            let (nw, nh) = (w.max(self.cap.0), h.max(self.cap.1));
            if self.gpu.is_some() {
                match make_gpu(&self.painter, nw, nh) {
                    Some(g) => {
                        // Cached bitmaps/brushes belong to the old target (and its address may be reused).
                        self.painter.bitmaps.borrow_mut().clear();
                        self.painter.grads.borrow_mut().clear();
                        self.gpu = Some(g);
                    }
                    None => return false,
                }
            } else {
                unsafe {
                    let Some((dib, old)) = make_dib(self.dc, nw, nh) else { return false };
                    let _ = DeleteObject(self.dib);
                    self.dib = dib;
                    self.old = old;
                }
            }
            self.cap = (nw, nh);
            true
        }

        /// Render and show. `screen` is the window's top-left in physical px.
        pub fn present(&mut self, hwnd: HWND, scene: &Scene, size_logical: (f32, f32), scale: f32, screen: (i32, i32)) -> bool {
            let pw = (size_logical.0 * scale).ceil().max(1.0) as i32;
            let ph = (size_logical.1 * scale).ceil().max(1.0) as i32;
            if !self.ensure_capacity(pw, ph) {
                return false;
            }
            unsafe {
                let t0 = std::time::Instant::now();
                let dst = POINT { x: screen.0, y: screen.1 };
                let src = POINT { x: 0, y: 0 };
                let sz = SIZE { cx: pw, cy: ph };
                let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
                if let Some(g) = &self.gpu {
                    g.rt.BeginDraw();
                    g.rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
                    self.painter.draw(&g.rt, scene, scale);
                    if g.rt.EndDraw(None, None).is_err() {
                        // Device lost / removed: drop to the CPU path for the rest of the session.
                        self.gpu = None;
                        self.painter.bitmaps.borrow_mut().clear();
                        self.painter.grads.borrow_mut().clear();
                        if let Some((dib, old)) = make_dib(self.dc, self.cap.0, self.cap.1) {
                            let _ = DeleteObject(self.dib);
                            self.dib = dib;
                            self.old = old;
                        }
                        return false;
                    }
                    PROF_DRAW.store((t0.elapsed().as_secs_f32() * 1000.0 * 100.0) as u32, std::sync::atomic::Ordering::Relaxed);
                    let Ok(hdc) = g.surface.GetDC(false) else { return false };
                    let ok = UpdateLayeredWindow(hwnd, None, Some(&dst), Some(&sz), hdc, Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA).is_ok();
                    let _ = g.surface.ReleaseDC(None);
                    return ok;
                }
                let area = RECT { left: 0, top: 0, right: pw, bottom: ph };
                if self.rt.BindDC(self.dc, &area).is_err() {
                    return false;
                }
                self.rt.BeginDraw();
                self.rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
                let base: ID2D1RenderTarget = self.rt.cast().unwrap();
                self.painter.draw(&base, scene, scale);
                if self.rt.EndDraw(None, None).is_err() {
                    return false;
                }
                PROF_DRAW.store((t0.elapsed().as_secs_f32() * 1000.0 * 100.0) as u32, std::sync::atomic::Ordering::Relaxed);
                UpdateLayeredWindow(hwnd, None, Some(&dst), Some(&sz), self.dc, Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA).is_ok()
            }
        }
    }

    impl Drop for Presenter {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.dc, self.old);
                let _ = DeleteObject(self.dib);
                let _ = DeleteDC(self.dc);
            }
        }
    }

    unsafe fn make_dib(dc: HDC, w: i32, h: i32) -> Option<(HBITMAP, HGDIOBJ)> {
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let old = SelectObject(dc, bmp);
        Some((bmp, old))
    }

    // ------------------------------------------------------------ offscreen

    /// Premultiplied BGRA pixels of `scene` drawn on a transparent canvas.
    pub fn render_offscreen(scene: &Scene, size_logical: (f32, f32), scale: f32) -> Option<(u32, u32, Vec<u8>)> {
        use windows::Win32::Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapCacheOnDemand, WICBitmapLockRead,
            WICRect,
        };
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
        let painter = Painter::new()?;
        let pw = (size_logical.0 * scale).ceil().max(1.0) as u32;
        let ph = (size_logical.1 * scale).ceil().max(1.0) as u32;
        unsafe {
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            let bmp = wic.CreateBitmap(pw, ph, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnDemand).ok()?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_UNKNOWN, alphaMode: D2D1_ALPHA_MODE_UNKNOWN },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let rt = painter.factory.CreateWicBitmapRenderTarget(&bmp, &props).ok()?;
            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            painter.draw(&rt, scene, scale);
            rt.EndDraw(None, None).ok()?;
            let rect = WICRect { X: 0, Y: 0, Width: pw as i32, Height: ph as i32 };
            let lock = bmp.Lock(&rect, WICBitmapLockRead.0 as u32).ok()?;
            let stride = lock.GetStride().ok()? as usize;
            let mut len = 0u32;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            lock.GetDataPointer(&mut len, &mut ptr).ok()?;
            let mut out = Vec::with_capacity((pw * ph * 4) as usize);
            for y in 0..ph as usize {
                let row = std::slice::from_raw_parts(ptr.add(y * stride), pw as usize * 4);
                out.extend_from_slice(row);
            }
            Some((pw, ph, out))
        }
    }
}
