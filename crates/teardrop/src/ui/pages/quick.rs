use super::*;
// -------------------------------------------------------------------- quick

/// Quick settings: radio & mute tiles, brightness / volume sliders and a
/// battery read-out — the Windows 11 quick-settings flyout, droplet-sized.
pub(super) fn quick(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Quick);
    let q = &c.vm.quick;
    let gap = 8.0;
    let tw = (a.w - gap * 3.0) / 4.0;
    let th = 56.0;

    // Row 1: real toggles. `on == None` greys out the tile (no hardware).
    let mic_sub = match q.mic_muted {
        Some(true) => "Muted",
        Some(false) => "On",
        None => "Not available",
    };
    // The Wi-Fi/Bluetooth chevrons expand a device list inside the island;
    // Sound's opens Windows sound settings.
    let wifi_sec = q.wifi.map(|_| Action::QsExpand(QsSection::Wifi));
    let bt_sec = q.bluetooth.map(|_| Action::QsExpand(QsSection::Bt));
    let tiles: [(Icon, &str, String, Option<bool>, Action, Option<Action>); 4] = [
        (Icon::Wifi, "Wi-Fi", q.wifi_label(), q.wifi, Action::QsWifi, wifi_sec),
        (Icon::Bt, "Bluetooth", q.bt_label(), q.bluetooth, Action::QsBt, bt_sec),
        (Icon::Speaker, "Sound", if q.muted { "Muted" } else { "On" }.to_string(), Some(!q.muted), Action::QsMute, Some(Action::QsLink(QsLink::Sound))),
        (Icon::Mic, "Microphone", mic_sub.to_string(), q.mic_muted.map(|m| !m), Action::QsMicMute, None),
    ];
    for (i, (icon, name, sub, on, act, link)) in tiles.iter().enumerate() {
        qs_tile(c, Rect::new(a.x + i as f32 * (tw + gap), a.y, tw, th), *icon, name, sub, *on, *act, *link, acc);
    }

    if c.ui.qs_section != QsSection::None {
        qs_list(c, a, th + gap, acc);
        return;
    }

    // Row 2: shortcuts into Windows Settings for what can't be toggled.
    let ly = a.y + th + gap;
    let links: [(Icon, &str, QsLink); 4] = [
        (Icon::Moon, "Night light", QsLink::NightLight),
        (Icon::Plane, "Airplane", QsLink::Airplane),
        (Icon::Bolt, "Battery saver", QsLink::BatterySaver),
        (Icon::Gear, "Settings", QsLink::Settings),
    ];
    for (i, (icon, name, l)) in links.iter().enumerate() {
        qs_link_tile(c, Rect::new(a.x + i as f32 * (tw + gap), ly, tw, th), *icon, name, *l, acc);
    }

    // Sliders: brightness hides entirely on PCs that can't drive a panel.
    let mut sy = ly + th + 14.0;
    if let Some(b) = q.brightness {
        qs_slider(c, Rect::new(a.x, sy, a.w, 30.0), Icon::Sun, Action::QsLink(QsLink::Display), Action::QsBright, b as f32 / 100.0, acc, &format!("{b}"));
        sy += 34.0;
    }
    let vol_icon = if q.muted { Icon::Muted } else { Icon::Speaker };
    let vol_acc = if q.muted { acc.fade(0.45) } else { acc };
    qs_slider(c, Rect::new(a.x, sy, a.w, 30.0), vol_icon, Action::QsMute, Action::QsVol, q.volume_pct as f32 / 100.0, vol_acc, &format!("{}", q.volume_pct));

    // Footer: battery level, charging state.
    let sys = &c.vm.system;
    let by = a.bottom() - 10.0;
    match sys.battery_pct {
        Some(pct) => {
            let br = Rect::new(a.x + 1.0, by - 6.0, 23.0, 12.0);
            c.s.stroke(br, 3.0, 1.2, pal().text_3);
            c.s.rect(Rect::new(br.right() + 0.5, by - 2.5, 2.5, 5.0), 1.0, pal().text_3);
            let fw = (br.w - 4.0) * pct as f32 / 100.0;
            let col = if pct <= 20 { accent(Page::Voice) } else { pal().text_2 };
            c.s.rect(Rect::new(br.x + 2.0, br.y + 2.0, fw.max(0.0), br.h - 4.0), 1.5, col);
            let label = format!("{pct}%{}", if sys.charging { " \u{00b7} charging" } else { "" });
            c.text(label, Rect::new(a.x + 34.0, by - 8.0, 160.0, 16.0), 10.5, Weight::Medium, pal().text_3, Align::Left);
        }
        None => {
            icons::draw(&mut c.s, Icon::Bolt, a.x + 9.0, by, 13.0, pal().text_3);
            c.text("On AC power", Rect::new(a.x + 20.0, by - 8.0, 160.0, 16.0), 10.5, Weight::Medium, pal().text_3, Align::Left);
        }
    }
}

/// Windows 11-style toggle tile: accent when on, dim when off, ghosted when
/// the hardware isn't there. `link` adds a corner chevron (expands the
/// tile's list or opens its Settings page).
fn qs_tile(c: &mut Ctx, r: Rect, icon: Icon, name: &str, sub: &str, on: Option<bool>, act: Action, link: Option<Action>, acc: Color) {
    let h = c.hov(act);
    let lit = on == Some(true);
    let dead = on.is_none();
    if lit {
        let mut col = acc.mix(WHITE, h * 0.15);
        if c.pressed(act) {
            col = col.mix(Color::rgb(0, 0, 0), 0.12);
        }
        c.s.rect(r, 12.0, col);
    } else {
        c.s.rect(r, 12.0, pal().surface.mix(pal().surface_hover, h * if dead { 0.4 } else { 1.0 }));
    }
    let ink = if lit {
        ON_ACCENT
    } else if dead {
        pal().text_3.fade(0.6)
    } else {
        pal().text
    };
    let sub_ink = if lit { ON_ACCENT.fade(0.65) } else { pal().text_3.fade(if dead { 0.6 } else { 1.0 }) };
    icons::draw(&mut c.s, icon, r.x + 15.0, r.y + 14.0, 15.0, ink);
    c.text(name, Rect::new(r.x + 9.0, r.bottom() - 28.0, r.w - 16.0, 14.0), 11.0, Weight::Semibold, ink, Align::Left);
    c.text(sub.to_string(), Rect::new(r.x + 9.0, r.bottom() - 14.5, r.w - 16.0, 12.0), 9.0, Weight::Regular, sub_ink, Align::Left);
    if !dead {
        c.hit(r, act);
    }
    if let Some(la) = link {
        let lr = Rect::new(r.right() - 22.0, r.y + 5.0, 17.0, 17.0);
        let lh = c.hov(la);
        if lh > 0.01 {
            let fill = if lit { Color::rgba(0, 0, 0, 0.14) } else { pal().surface_hover.fade(lh) };
            c.s.circle(lr.cx(), lr.cy(), 8.5, fill);
        }
        icons::draw(&mut c.s, Icon::External, lr.cx(), lr.cy(), 9.0, ink.fade(0.55 + 0.45 * lh));
        c.hit(lr, la);
    }
}

/// Shortcut tile that opens a `ms-settings:` page.
fn qs_link_tile(c: &mut Ctx, r: Rect, icon: Icon, name: &str, l: QsLink, acc: Color) {
    let act = Action::QsLink(l);
    let h = c.hov(act);
    c.s.rect(r, 12.0, pal().surface.mix(pal().surface_hover, h));
    icons::draw(&mut c.s, icon, r.x + 16.0, r.cy(), 14.0, pal().text_2.mix(acc, h * 0.8));
    c.text(name, Rect::new(r.x + 30.0, r.cy() - 8.0, r.w - 36.0, 16.0), 10.0, Weight::Medium, pal().text_2.mix(pal().text, h), Align::Left);
    c.hit(r, act);
}

/// Slider row: clickable icon at the left, drag/click track, % at the right.
fn qs_slider(c: &mut Ctx, r: Rect, icon: Icon, icon_act: Action, track_act: Action, frac: f32, acc: Color, value: &str) {
    c.icon_btn(Rect::new(r.x, r.y + 2.0, 26.0, 26.0), icon_act, icon, 14.0, pal().text_2);
    let tx = r.x + 36.0;
    let tw = r.w - 36.0 - 42.0;
    let ty = r.cy() - 3.0;
    c.s.rect(Rect::new(tx, ty, tw, 6.0), 3.0, pal().surface.mix(pal().surface_hover, 0.4));
    let frac = frac.clamp(0.0, 1.0);
    c.s.rect(Rect::new(tx, ty, (tw * frac).max(6.0), 6.0), 3.0, acc);
    let grow = c.hov(track_act) * 1.4 + if c.pressed(track_act) { 1.4 } else { 0.0 };
    c.s.circle(tx + tw * frac, ty + 3.0, 7.0 + grow, WHITE);
    c.hit(Rect::new(tx - 5.0, r.y, tw + 10.0, r.h), track_act);
    c.text(value.to_string(), Rect::new(r.right() - 40.0, r.y, 40.0, r.h), 11.5, Weight::Medium, pal().text_2, Align::Right);
}

/// Rows visible in the expanded Quick list (card_h 340 → content 282;
/// tiles 64 + header 26, minus 38 for a shown password field).
pub fn quick_visible_rows(pass: bool) -> usize {
    ((282.0f32 - 64.0 - 26.0 - if pass { 38.0 } else { 0.0 }) / 30.0).floor() as usize
}

/// Rows the expanded Quick list can scroll past the bottom.
pub fn quick_max_scroll(vm: &ViewModel, sec: QsSection, pass: bool) -> usize {
    let rows = match sec {
        QsSection::Wifi => vm.quick.wifi_nets.len(),
        QsSection::Bt => vm.quick.bt_devs.len(),
        QsSection::None => 0,
    };
    rows.saturating_sub(quick_visible_rows(pass))
}

/// The expanded radio list under the tiles: header + scrollable rows + an
/// optional password field (Wi-Fi joins that need a key).
fn qs_list(c: &mut Ctx, a: Rect, top_off: f32, acc: Color) {
    let q = &c.vm.quick;
    let wifi = c.ui.qs_section == QsSection::Wifi;
    let (title, link) = if wifi { ("Wi-Fi networks", QsLink::Wifi) } else { ("Bluetooth devices", QsLink::Bluetooth) };
    let top = a.y + top_off;

    // Header: title left, rescan / settings / close right.
    c.text(title, Rect::new(a.x + 2.0, top, 220.0, 20.0), 12.5, Weight::Semibold, pal().text, Align::Left);
    c.ghost_icon(Rect::new(a.right() - 22.0, top - 1.0, 20.0, 20.0), Action::QsExpand(QsSection::None), Icon::Close, 9.5, pal().text_3);
    c.ghost_icon(Rect::new(a.right() - 46.0, top - 1.0, 20.0, 20.0), Action::QsLink(link), Icon::External, 10.0, pal().text_3);
    c.ghost_icon(Rect::new(a.right() - 70.0, top - 1.0, 20.0, 20.0), Action::QsRescan, Icon::Reset, 10.0, pal().text_3);

    let radio_on = if wifi { q.wifi == Some(true) } else { q.bluetooth == Some(true) };
    let pass_h = if wifi && c.ui.qs_pass_ssid.is_some() { 38.0 } else { 0.0 };
    let list = Rect::new(a.x, top + 26.0, a.w, a.bottom() - top - 26.0 - pass_h);
    if !radio_on {
        c.text(
            format!("Turn {} on to see {}", if wifi { "Wi-Fi" } else { "Bluetooth" }, if wifi { "networks" } else { "devices" }),
            Rect::new(list.x, list.y + 14.0, list.w, 18.0),
            11.0,
            Weight::Regular,
            pal().text_3,
            Align::Left,
        );
        return;
    }

    let row_h = 30.0;
    let visible = (list.h / row_h).floor() as usize;
    let nrows = if wifi { q.wifi_nets.len() } else { q.bt_devs.len() };
    let scroll = c.ui.scroll_of(Page::Quick).min(nrows.saturating_sub(visible));
    c.s.clip(list);
    if nrows == 0 {
        c.text(if wifi { "No networks found" } else { "No devices found" }, Rect::new(list.x, list.y + 12.0, list.w, 18.0), 11.0, Weight::Regular, pal().text_3, Align::Left);
    }
    for vi in 0..visible.min(nrows.saturating_sub(scroll)) {
        let i = scroll + vi;
        let r = Rect::new(list.x, list.y + vi as f32 * row_h, list.w, row_h - 4.0);
        if wifi {
            qs_wifi_row(c, r, &q.wifi_nets[i], i, acc);
        } else {
            qs_bt_row(c, r, &q.bt_devs[i], i, acc);
        }
    }
    c.s.pop_clip();

    if wifi && pass_h > 0.0 {
        qs_pass_row(c, a, acc);
    }
}

fn qs_wifi_row(c: &mut Ctx, r: Rect, n: &teardrop_core::WifiNet, i: usize, acc: Color) {
    let act = Action::QsWifiNet(i as u16);
    let h = c.hov(act);
    if h > 0.01 || c.pressed(act) {
        c.s.rect(r, 8.0, if c.pressed(act) { pal().surface_press } else { pal().surface_hover.fade(h) });
    }
    // Signal bars: four steps rising right, lit by link quality.
    let bars = ((n.quality as u16 * 4 + 99) / 100).min(4) as usize;
    for b in 0..4 {
        let bh = 3.0 + b as f32 * 2.4;
        let col = if b < bars { pal().text_2 } else { pal().text_3.fade(0.3) };
        c.s.rect(Rect::new(r.x + 4.0 + b as f32 * 4.5, r.cy() + 5.0 - bh, 3.0, bh), 1.0, col);
    }
    let ink = if n.connected { pal().text } else { pal().text_2 };
    c.text(n.ssid.clone(), Rect::new(r.x + 28.0, r.y, r.w - 28.0 - 104.0, r.h), 11.5, Weight::Medium, ink, Align::Left);
    let status = if n.connected {
        "Connected"
    } else if n.has_profile {
        "Saved"
    } else if n.secured {
        "Secured"
    } else {
        "Open"
    };
    c.text(status, Rect::new(r.right() - 96.0, r.y, 96.0, r.h), 10.0, Weight::Regular, if n.connected { acc } else { pal().text_3 }, Align::Right);
    c.hit(r, act);
}

fn qs_bt_row(c: &mut Ctx, r: Rect, d: &teardrop_core::BtDev, i: usize, acc: Color) {
    let act = Action::QsBtDev(i as u16);
    let h = c.hov(act);
    if h > 0.01 || c.pressed(act) {
        c.s.rect(r, 8.0, if c.pressed(act) { pal().surface_press } else { pal().surface_hover.fade(h) });
    }
    let ic = if d.connected { acc } else { pal().text_3 };
    icons::draw(&mut c.s, Icon::Bt, r.x + 11.0, r.cy(), 12.0, ic);
    let ink = if d.connected { pal().text } else { pal().text_2 };
    c.text(d.name.clone(), Rect::new(r.x + 28.0, r.y, r.w - 28.0 - 104.0, r.h), 11.5, Weight::Medium, ink, Align::Left);
    let status = if d.connected {
        "Connected"
    } else if d.paired {
        "Paired"
    } else {
        "Pair"
    };
    c.text(status, Rect::new(r.right() - 96.0, r.y, 96.0, r.h), 10.0, Weight::Regular, if d.connected { acc } else { pal().text_3 }, Align::Right);
    c.hit(r, act);
}

/// Password field for joining a secured network with no saved profile.
/// Shows dots, submits on Enter or the Connect pill.
fn qs_pass_row(c: &mut Ctx, a: Rect, acc: Color) {
    let y = a.bottom() - 30.0;
    let ssid = c.ui.qs_pass_ssid.clone().unwrap_or_default();
    let fr = Rect::new(a.x + 22.0, y, a.w - 22.0 - 80.0, 28.0);
    icons::draw(&mut c.s, Icon::Lock, a.x + 10.0, y + 14.0, 11.0, pal().text_3);
    c.s.rect(fr, 9.0, pal().surface);
    c.s.stroke(fr, 9.0, 1.0, acc.fade(0.45));
    match &c.ui.input {
        Some(f) if !f.is_empty() => {
            let masked: String = "\u{2022}".repeat(f.as_str().chars().count());
            let ed = field_edit(c, f, acc);
            c.s.text_edit(masked, Rect::new(fr.x + 10.0, fr.y, fr.w - 14.0, fr.h), 12.0, Weight::Medium, pal().text, ed);
        }
        Some(_) => {
            c.text(format!("Password for {ssid}"), Rect::new(fr.x + 10.0, fr.y, fr.w - 14.0, fr.h), 10.5, Weight::Regular, pal().text_3, Align::Left);
            if (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
                c.s.line(fr.x + 10.0, fr.y + 6.0, fr.x + 10.0, fr.y + fr.h - 6.0, 1.5, acc);
            }
        }
        None => {
            c.text(format!("Password for {ssid}"), Rect::new(fr.x + 10.0, fr.y, fr.w - 14.0, fr.h), 10.5, Weight::Regular, pal().text_3, Align::Left);
        }
    }
    c.hit_text(fr, Action::QsPassField);
    c.pill(Rect::new(a.right() - 78.0, y, 78.0, 28.0), Action::QsWifiConnect, "Connect", Some(acc), 11.0);
}
