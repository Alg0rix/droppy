//! `teardrop.exe --demo <dir>`: animates the real island through a scripted
//! tour (hover-open, every page, close, HUDs) at 60 fps and pipes the frames
//! to ffmpeg, producing `demo.gif` and `demo.mp4`. Used for the README.

#![cfg(windows)]

use super::render_d2d::d2d::render_offscreen;
use super::state::IslandState;
use super::ui::layout::{build, IslandView};
use super::ui::state::{Action, UiState};
use super::viewmodel::{hud_volume, hud_water_due, ViewModel};
use chrono::{Duration, Utc};
use std::io::Write;
use std::process::{Command, Stdio};
use teardrop_core::{parse_lrc, Page, Settings};

const FPS: f32 = 60.0;

enum Ev {
    Open,
    Close,
    Hover(Option<Action>),
    Show(Page),
    Type(&'static str),
    StartFocus,
    Hud(fn(u64) -> teardrop_core::Hud),
    Click(Action),
}

fn timeline() -> Vec<(f32, Ev)> {
    vec![
        (0.0, Ev::Hover(None)),
        (2.0, Ev::Hover(Some(Action::Island))),
        (2.5, Ev::Open),
        (4.4, Ev::Hover(Some(Action::Tab(Page::Focus)))),
        (4.7, Ev::Show(Page::Focus)),
        (4.7, Ev::StartFocus),
        (6.4, Ev::Hover(Some(Action::Tab(Page::Water)))),
        (6.7, Ev::Show(Page::Water)),
        (7.6, Ev::Hover(Some(Action::WaterAdd(250)))),
        (8.0, Ev::Click(Action::WaterAdd(250))),
        (9.0, Ev::Hover(Some(Action::Tab(Page::Shelf)))),
        (9.3, Ev::Show(Page::Shelf)),
        (10.6, Ev::Hover(Some(Action::ShelfOpen(1)))),
        (11.8, Ev::Hover(Some(Action::Tab(Page::Tasks)))),
        (12.1, Ev::Show(Page::Tasks)),
        (12.6, Ev::Type("Plan the launch")),
        (15.0, Ev::Hover(Some(Action::Tab(Page::Clipboard)))),
        (15.3, Ev::Show(Page::Clipboard)),
        (16.6, Ev::Hover(Some(Action::ClipCopy(1)))),
        (17.8, Ev::Hover(Some(Action::Tab(Page::Today)))),
        (18.1, Ev::Show(Page::Today)),
        (19.8, Ev::Hover(Some(Action::Gear))),
        (20.1, Ev::Show(Page::Settings)),
        (21.8, Ev::Hover(Some(Action::Tab(Page::Music)))),
        (22.1, Ev::Show(Page::Music)),
        (24.0, Ev::Hover(None)),
        (24.2, Ev::Close),
        (26.0, Ev::Hud(|ms| hud_volume(72, false, ms))),
        (28.6, Ev::Hud(hud_water_due)),
        (31.6, Ev::Hover(None)),
    ]
}

fn backdrop(w: u32, h: u32) -> Vec<f32> {
    let mut bg = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        let t = y as f32 / h as f32;
        for x in 0..w {
            let s = x as f32 / w as f32;
            let v = ((s - 0.5) * 2.0).abs();
            bg.push(34.0 + 60.0 * t + 10.0 * (1.0 - v));
            bg.push(40.0 + 34.0 * t + 6.0 * (1.0 - v));
            bg.push(74.0 + 58.0 * t + 14.0 * (1.0 - v));
        }
    }
    bg
}

pub fn run(dir: &str) -> i32 {
    let out = std::path::PathBuf::from(dir);
    let _ = std::fs::create_dir_all(&out);
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    }
    let scale: f32 = std::env::var("TEARDROP_DEMO_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.5);
    let (cw, ch) = (((560.0 * scale) as u32) & !1, ((372.0 * scale) as u32) & !1);
    let bg = backdrop(cw, ch);

    let gif = out.join("demo.gif");
    let mp4 = out.join("demo.mp4");
    let filter = "[0:v]split[a][b];[a]fps=30,scale=720:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=128:stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle[g]";
    let child = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{cw}x{ch}"), "-r", "60", "-i", "-"])
        .args(["-filter_complex", filter, "-map", "[g]", "-loop", "0"])
        .arg(&gif)
        .args(["-map", "[b]", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-movflags", "+faststart"])
        .arg(&mp4)
        .stdin(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        eprintln!("demo: ffmpeg not found on PATH");
        return 2;
    };
    let mut pipe = child.stdin.take().unwrap();

    let base = Utc::now();
    let mut vm = ViewModel::demo(base);
    vm.lyric_lines = parse_lrc("[00:00.00]Ripples keep the shape of you\n[01:02.00]Surface tension holding true\n[01:09.00]Fall into the blue\n[01:20.00]Hold the line\n");
    vm.track.position_ms = 61_000;
    vm.shelf.items.truncate(4);
    let set = Settings::default();
    let mut isl = IslandState::new();
    let mut ui = UiState::new(Page::Music);
    let mut events = timeline().into_iter().peekable();
    let end = 33.0f32;
    let total = (end * FPS) as usize;
    let dt = 1.0 / FPS;
    let mut typing: Option<(f32, &'static str)> = None;
    let mut ui_clock_ms = 0u64;

    for f in 0..total {
        let t = f as f32 * dt;
        let now = base + Duration::milliseconds((t * 1000.0) as i64);
        let now_ms = now.timestamp_millis() as u64;
        ui_clock_ms = now_ms;
        vm.now = now;
        vm.track.position_ms = 61_000 + (t * 1000.0) as u64;
        vm.lyric_idx = teardrop_core::line_at(&vm.lyric_lines, vm.track.position_ms);
        while events.peek().map(|(et, _)| *et <= t).unwrap_or(false) {
            let (_, ev) = events.next().unwrap();
            match ev {
                Ev::Open => isl.toggle(),
                Ev::Close => isl.toggle(),
                Ev::Hover(h) => {
                    ui.set_hover(h);
                    isl.set_hover(matches!(h, Some(Action::Island)));
                }
                Ev::Show(p) => {
                    vm.page = p;
                    ui.set_hover(None);
                }
                Ev::Type(s) => typing = Some((t, s)),
                Ev::StartFocus => vm.pomo.start_focus(now - Duration::minutes(7)),
                Ev::Hud(mk) => vm.hud = Some(mk(now_ms)),
                Ev::Click(Action::WaterAdd(ml)) => {
                    let day = vm.day();
                    vm.water.add(day, ml);
                    ui.toast(&format!("+{ml} ml"), now_ms);
                    ui.set_hover(None);
                }
                Ev::Click(_) => {}
            }
        }
        if let Some((t0, s)) = typing {
            let n = (((t - t0) * 11.0) as usize).min(s.chars().count());
            ui.input = Some(s.chars().take(n).collect());
            if n == s.chars().count() && t - t0 > 1.9 {
                vm.tasks.add(s, now);
                ui.input = Some(String::new());
                typing = None;
            }
        }
        if t > 24.0 {
            ui.input = None;
        }
        // 240 Hz physics, same as the live app.
        for _ in 0..4 {
            isl.tick(1.0 / 240.0);
        }
        ui.tick(dt, &vm, &set, now_ms);
        isl.geom.pill_w = ui.compact_w;
        isl.geom.pill_h = ui.compact_h;
        let view = IslandView { geom: isl.geom, open: isl.open.value, wobble_px: isl.wobble_px };
        let frame = build(&vm, &set, &ui, &view, now_ms);
        let Some((pw, ph, bgra)) = render_offscreen(&frame.scene, (frame.w, frame.h), scale) else {
            eprintln!("demo: render failed at frame {f}");
            return 2;
        };
        let ox = ((cw as i32 - pw as i32) / 2).max(0) as u32;
        let mut canvas: Vec<u8> = bg.iter().map(|v| *v as u8).collect();
        for y in 0..ph.min(ch) {
            for x in 0..pw {
                let i = ((y * pw + x) * 4) as usize;
                let a = bgra[i + 3] as f32 / 255.0;
                if a <= 0.0 || ox + x >= cw {
                    continue;
                }
                let o = (((y * cw) + ox + x) * 3) as usize;
                let src = [bgra[i + 2] as f32, bgra[i + 1] as f32, bgra[i] as f32];
                for c in 0..3 {
                    canvas[o + c] = (src[c] + bg[o + c] * (1.0 - a)).clamp(0.0, 255.0) as u8;
                }
            }
        }
        if pipe.write_all(&canvas).is_err() {
            eprintln!("demo: ffmpeg closed the pipe");
            return 2;
        }
    }
    let _ = ui_clock_ms;
    drop(pipe);
    match child.wait() {
        Ok(s) if s.success() => {
            println!("demo: {} and {}", gif.display(), mp4.display());
            0
        }
        _ => {
            eprintln!("demo: ffmpeg failed");
            2
        }
    }
}
