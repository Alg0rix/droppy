use super::*;
// -------------------------------------------------------------------- chat

/// Max message-scroll in message units (0 = pinned to the newest).
pub fn chat_max_scroll(n: usize) -> usize {
    n.saturating_sub(1)
}

/// BLE mesh chat: peer sidebar + conversation + input (bitchat whitepaper).
pub(super) fn chat(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Chat);
    let vm = c.vm;
    let chat = &vm.chat;

    if !c.set.mesh_chat {
        let r = Rect::new(a.x, a.y, a.w, a.h - 40.0);
        c.empty_state(r, Icon::Chat, pal().text_3, "Mesh chat is disabled", "Enable it to advertise and scan over Bluetooth LE");
        let bw = 150.0;
        c.pill(Rect::new(a.cx() - bw / 2.0, a.cy() + 34.0, bw, 28.0), Action::Toggle(SettingKey::MeshChat), "Enable mesh chat", Some(acc), 11.5);
        return;
    }

    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, if chat.radio_on { acc } else { pal().text_3 });
    c.text("MESH CHAT", Rect::new(a.x + 12.0, a.y, 110.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    c.text(chat.status.clone(), Rect::new(a.x + 120.0, a.y, a.w - 120.0, 15.0), 10.5, Weight::Medium, pal().text_3, Align::Right);

    let body = Rect::new(a.x, a.y + 20.0, a.w, a.h - 20.0);
    let side_w = 142.0;
    let side = Rect::new(body.x, body.y, side_w, body.h);
    let main = Rect::new(side.right() + 10.0, body.y, body.w - side_w - 10.0, body.h);

    // -- sidebar ---------------------------------------------------------
    // Our nickname (click to edit; Enter commits via ChatNick).
    let nick_r = Rect::new(side.x, side.y, side.w, 26.0);
    let editing = chat.editing_nick;
    c.s.rect(nick_r, 9.0, if editing { pal().surface_hover } else { c.surface_fill(Action::ChatNick) });
    if editing {
        c.s.stroke(nick_r, 9.0, 1.4, acc.fade(0.85));
    }
    if let (true, Some(f)) = (editing, c.ui.input.as_ref()) {
        let ed = field_edit(c, f, acc);
        c.s.text_edit(f.as_str().to_string(), Rect::new(nick_r.x + 10.0, nick_r.y, nick_r.w - 16.0, nick_r.h), 12.0, Weight::Medium, pal().text, ed);
    } else {
        c.text(format!("@{}", chat.nick), Rect::new(nick_r.x + 10.0, nick_r.y, nick_r.w - 56.0, nick_r.h), 12.0, Weight::Semibold, pal().text, Align::Left);
        // Short peer id — compare in person to pin an identity.
        c.text(chat.me.chars().take(4).collect::<String>(), Rect::new(nick_r.right() - 48.0, nick_r.y, 44.0, nick_r.h), 10.0, Weight::Medium, pal().text_3, Align::Right);
    }
    c.hit(nick_r, Action::ChatNick);

    // The public room.
    let room_r = Rect::new(side.x, nick_r.bottom() + 6.0, side.w, 28.0);
    let sel_room = chat.sel.is_none();
    let room_a = Action::ChatSelect(0);
    c.s.rect(room_r, 9.0, if sel_room { acc.fade(0.2) } else { c.surface_fill(room_a) });
    c.s.circle(room_r.x + 13.0, room_r.cy(), 4.0, if sel_room { acc } else { pal().text_3 });
    c.text("room", Rect::new(room_r.x + 24.0, room_r.y, room_r.w - 54.0, room_r.h), 12.0, Weight::Semibold, pal().text, Align::Left);
    if chat.unread_room > 0 {
        let b = Rect::new(room_r.right() - 24.0, room_r.cy() - 8.0, 16.0, 16.0);
        c.s.rect(b, 8.0, acc);
        c.text(format!("{}", chat.unread_room.min(99)), b, 9.5, Weight::Bold, ON_ACCENT, Align::Center);
    }
    c.hit(room_r, room_a);

    // Peers: presence dot + nick (+ lock when a Noise session is up).
    let mut y = room_r.bottom() + 4.0;
    let max_y = side.bottom() - 30.0;
    for (i, p) in chat.peers.iter().enumerate() {
        if y > max_y {
            break;
        }
        let r = Rect::new(side.x, y, side.w, 28.0);
        let sel = chat.sel.as_deref() == Some(p.id.as_str());
        let act = Action::ChatSelect((i + 1) as u16);
        if sel {
            c.s.rect(r, 9.0, acc.fade(0.2));
        } else {
            c.s.rect(r, 9.0, c.surface_fill(act));
        }
        let dot = if p.fresh { acc } else { pal().text_3 };
        c.s.circle(r.x + 13.0, r.cy(), 3.5, dot);
        let name_w = r.w - 24.0 - if p.secure { 20.0 } else { 6.0 } - if p.unread > 0 { 24.0 } else { 4.0 };
        c.text(p.nick.clone(), Rect::new(r.x + 24.0, r.y, name_w, r.h), 12.0, if sel { Weight::Semibold } else { Weight::Regular }, pal().text, Align::Left);
        if p.secure {
            // tiny lock
            let lx = r.right() - 18.0 - if p.unread > 0 { 24.0 } else { 0.0 };
            let ly = r.cy();
            c.s.stroke(Rect::new(lx - 3.5, ly - 2.0, 7.0, 6.5), 2.0, 1.3, acc);
            c.s.ring(lx, ly - 2.5, 2.6, 1.2, 0.5, 0.5, acc);
        }
        if p.unread > 0 {
            let b = Rect::new(r.right() - 24.0, r.cy() - 8.0, 16.0, 16.0);
            c.s.rect(b, 8.0, acc);
            c.text(format!("{}", p.unread.min(99)), b, 9.5, Weight::Bold, ON_ACCENT, Align::Center);
        }
        c.hit(r, act);
        y += 30.0;
    }
    if chat.peers.is_empty() {
        c.text("no peers yet", Rect::new(side.x + 4.0, y + 4.0, side.w - 8.0, 30.0), 11.0, Weight::Regular, pal().text_3, Align::Left);
    }

    // -- conversation -----------------------------------------------------
    let in_y = main.bottom() - 34.0;
    let msg_area = Rect::new(main.x, main.y, main.w, in_y - main.y - 8.0);

    // Title line.
    let secure = chat.sel.as_deref().and_then(|id| chat.peers.iter().find(|p| p.id == id)).map(|p| p.secure).unwrap_or(false);
    let title = if chat.sel.is_none() {
        "mesh room \u{00b7} everyone in range".to_string()
    } else if secure {
        format!("{} \u{00b7} encrypted", chat.sel_nick)
    } else {
        format!("{} \u{00b7} encrypting\u{2026}", chat.sel_nick)
    };
    c.text(title, Rect::new(msg_area.x + 2.0, msg_area.y, msg_area.w - 4.0, 16.0), 11.0, Weight::Semibold, pal().text_3, Align::Left);
    let list = Rect::new(msg_area.x, msg_area.y + 18.0, msg_area.w, msg_area.h - 18.0);

    if chat.msgs.is_empty() {
        let hint = if chat.sel.is_none() { "say hi to the mesh" } else { "no messages yet" };
        c.empty_state(list, Icon::Chat, acc, hint, if chat.peers.is_empty() { "waiting for a droppy in range" } else { "" });
    } else {
        // Bottom-up flow; `scroll` = messages hidden from the end.
        let scroll = c.ui.scroll_of(Page::Chat).min(chat_max_scroll(chat.msgs.len()));
        let mut cy = list.bottom() - 2.0;
        c.s.clip(list);
        for m in chat.msgs.iter().rev().skip(scroll) {
            let lines = wrap_lines(&m.text, list.w - 24.0, 12.5);
            let h = 15.0 + lines.len() as f32 * 15.0 + 7.0;
            cy -= h;
            if cy + h < list.y {
                break;
            }
            let who = if m.mine { "you".to_string() } else { m.who.clone() };
            let when = chrono::DateTime::from_timestamp_millis(m.ts).map(|t| local_naive(t).format("%H:%M").to_string()).unwrap_or_default();
            let mut meta = who.clone();
            if m.mine {
                if m.failed {
                    meta.push_str(" \u{00b7} failed");
                } else if m.read {
                    meta.push_str(" \u{00b7} read");
                } else if m.delivered.unwrap_or(false) {
                    meta.push_str(" \u{00b7} sent");
                } else if chat.sel.is_some() {
                    meta.push_str(" \u{00b7} \u{2026}");
                }
            }
            let meta_col = if m.failed {
                pal().red
            } else if m.mine {
                acc
            } else {
                pal().text_3
            };
            c.text(meta, Rect::new(list.x + 2.0, cy, list.w - 60.0, 14.0), 10.0, Weight::Semibold, meta_col, Align::Left);
            c.text(when, Rect::new(list.right() - 58.0, cy, 56.0, 14.0), 10.0, Weight::Regular, pal().text_3, Align::Right);
            let mut ly = cy + 15.0;
            for l in &lines {
                c.text(l.clone(), Rect::new(list.x + 10.0, ly, list.w - 24.0, 15.0), 12.5, Weight::Regular, pal().text, Align::Left);
                ly += 15.0;
            }
        }
        c.s.pop_clip();
        if scroll > 0 {
            c.text(format!("{} newer", scroll), Rect::new(list.x, list.bottom() - 14.0, list.w, 14.0), 9.5, Weight::Medium, acc, Align::Center);
        }
    }

    // -- input ------------------------------------------------------------
    let field = Rect::new(main.x, in_y, main.w - 40.0, 34.0);
    let typing = c.ui.input.is_some() && !chat.editing_nick;
    c.s.rect(field, 12.0, if typing { pal().surface_hover } else { c.surface_fill(Action::ChatInput) });
    if typing {
        c.s.stroke(field, 12.0, 1.4, acc.fade(0.85));
    }
    icons::draw(&mut c.s, Icon::Chat, field.x + 16.0, field.cy(), 13.0, if typing { acc } else { pal().text_3 });
    let tr = Rect::new(field.x + 34.0, field.y, field.w - 44.0, field.h);
    match &c.ui.input {
        Some(f) if typing && !f.is_empty() => {
            let ed = field_edit(c, f, acc);
            c.s.text_edit(f.as_str().to_string(), tr, 12.5, Weight::Regular, pal().text, ed);
        }
        _ => {
            let ph = if editing {
                "nickname\u{2026}"
            } else if chat.sel.is_some() {
                "dm over noise\u{2026}"
            } else {
                "message the room\u{2026}"
            };
            c.text(ph, tr, 12.5, Weight::Regular, pal().text_3, Align::Left);
            if typing && (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
                c.s.rect(Rect::new(tr.x, field.cy() - 8.0, 1.5, 16.0), 0.7, acc);
            }
        }
    }
    c.hit_text(field, Action::ChatInput);

    let send_r = Rect::new(main.right() - 34.0, in_y, 34.0, 34.0);
    let can_send = c.ui.input.as_ref().map(|f| !f.as_str().trim().is_empty()).unwrap_or(false) && !editing;
    c.s.rect(send_r, 17.0, if can_send { acc } else { pal().surface });
    icons::draw(&mut c.s, Icon::Send, send_r.cx(), send_r.cy(), 15.0, if can_send { ON_ACCENT } else { pal().text_3 });
    c.hit(send_r, Action::ChatSend);
}
