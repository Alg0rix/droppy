use super::*;
// ----------------------------------------------------------------------- ai

/// Word-wrap `text` into visual lines whose estimated pixel width fits `max_w`.
pub fn wrap_lines(text: &str, max_w: f32, size: f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.lines() {
        let para = para.trim_end();
        if para.is_empty() {
            continue;
        }
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if est_w(&candidate, size) * 1.12 <= max_w || cur.is_empty() {
                cur = candidate;
            } else {
                out.push(cur);
                cur = word.to_string();
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

pub const AI_HIST_ROWS: usize = 5;

pub fn ai_hist_max_scroll(n: usize) -> usize {
    n.saturating_sub(AI_HIST_ROWS)
}

pub(super) fn ai(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Ai);
    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    c.text("AI CHAT", Rect::new(a.x + 12.0, a.y, 200.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    let a = Rect::new(a.x, a.y + 20.0, a.w, a.h - 20.0);
    let ai = &c.vm.ai;
    let prov = c.set.ai_provider;
    let model = c.set.active_ai_model();

    // Top bar: Provider/Model badge on the left, New / History / Setup pills on the right.
    let setup_r = Rect::new(a.right() - 52.0, a.y, 52.0, 24.0);
    let hist_r = Rect::new(setup_r.x - 5.0 - 54.0, a.y, 54.0, 24.0);
    let new_r = Rect::new(hist_r.x - 5.0 - 44.0, a.y, 44.0, 24.0);
    c.pill(new_r, Action::AiNewChat, "New", None, 11.0);
    c.pill(hist_r, Action::AiToggleHistory, "History", ai.show_history.then_some(acc), 11.0);
    c.pill(setup_r, Action::AiToggleSetup, "Setup", ai.show_setup.then_some(acc), 11.0);

    let badge_r = Rect::new(a.x, a.y, new_r.x - a.x - 8.0, 24.0);
    let bh = c.hov(Action::AiToggleSetup);
    c.s.rect(badge_r, 12.0, pal().surface.mix(pal().surface_hover, bh));
    let ready = ai.configured || prov == AiProvider::Ollama || prov == AiProvider::OpenAiCompat;
    c.s.circle(badge_r.x + 12.0, badge_r.cy(), 3.8, if ready { pal().green } else { pal().amber });
    let head = format!("{} \u{00b7} {}", prov.label(), model);
    c.text(head, Rect::new(badge_r.x + 22.0, badge_r.y, badge_r.w - 28.0, badge_r.h), 11.5, Weight::Semibold, pal().text, Align::Left);
    c.hit(badge_r, Action::AiToggleSetup);

    if ai.show_setup {
        ai_setup(c, a, acc);
    } else if ai.show_history {
        ai_history(c, a, acc);
    } else {
        ai_chat(c, a, acc);
    }
}

fn ai_setup(c: &mut Ctx, a: Rect, acc: Color) {
    use crate::ai::AiFocusField;
    let ai = &c.vm.ai;
    let prov = c.set.ai_provider;

    // 1. Provider segmented control (6 providers).
    let seg_r = Rect::new(a.x, a.y + 30.0, a.w, 26.0);
    let items = AiProvider::ALL.map(|p| (Action::AiSetProvider(p), p.short_label(), p == prov));
    segmented(c, seg_r, &items, 11.0);

    // 2. Provider auth / endpoint card.
    let card = Rect::new(a.x, a.y + 62.0, a.w, 56.0);
    c.s.rect(card, 12.0, pal().surface);
    match prov {
        AiProvider::ChatGpt => {
            c.text(
                "Sign in with ChatGPT \u{00b7} Codex Subscription OAuth",
                Rect::new(card.x + 12.0, card.y + 6.0, card.w - 24.0, 18.0),
                12.0,
                Weight::Semibold,
                pal().text,
                Align::Left,
            );
            if ai.chatgpt_login_busy {
                if !ai.chatgpt_user_code.is_empty() {
                    c.text(format!("Code: {}", ai.chatgpt_user_code), Rect::new(card.x + 12.0, card.y + 27.0, card.w - 176.0, 22.0), 13.5, Weight::Bold, acc, Align::Left);
                    c.pill(Rect::new(card.right() - 156.0, card.y + 26.0, 92.0, 24.0), Action::AiChatGptCopyCode, "Copy & Open", Some(acc), 11.0);
                    c.pill(Rect::new(card.right() - 58.0, card.y + 26.0, 50.0, 24.0), Action::AiChatGptSignIn, "Cancel", None, 11.0);
                } else {
                    c.text(
                        "Requesting device code from auth.openai.com\u{2026}",
                        Rect::new(card.x + 12.0, card.y + 28.0, card.w - 86.0, 20.0),
                        11.5,
                        Weight::Regular,
                        pal().text_2,
                        Align::Left,
                    );
                    c.pill(Rect::new(card.right() - 66.0, card.y + 26.0, 58.0, 24.0), Action::AiChatGptSignIn, "Cancel", None, 11.0);
                }
            } else if ai.chatgpt_signed_in {
                c.s.circle(card.x + 17.0, card.y + 37.0, 3.8, pal().green);
                c.text("Connected (DPAPI OAuth token)", Rect::new(card.x + 26.0, card.y + 27.0, card.w - 186.0, 20.0), 11.5, Weight::Medium, pal().text_2, Align::Left);
                c.pill(Rect::new(card.right() - 152.0, card.y + 26.0, 80.0, 24.0), Action::AiRefreshModels, "Sync models", None, 10.5);
                c.pill(Rect::new(card.right() - 66.0, card.y + 26.0, 58.0, 24.0), Action::AiChatGptSignOut, "Sign out", None, 10.5);
            } else {
                c.text(
                    "Use your ChatGPT Plus, Pro, or Team subscription",
                    Rect::new(card.x + 12.0, card.y + 28.0, card.w - 166.0, 20.0),
                    11.5,
                    Weight::Regular,
                    pal().text_3,
                    Align::Left,
                );
                c.pill(Rect::new(card.right() - 148.0, card.y + 25.0, 140.0, 25.0), Action::AiChatGptSignIn, "Sign in with ChatGPT", Some(acc), 11.0);
            }
        }
        AiProvider::OpenCodeGo => {
            c.text(
                "OpenCode Go \u{00b7} https://opencode.ai/zen/go/v1",
                Rect::new(card.x + 12.0, card.y + 6.0, card.w - 24.0, 18.0),
                12.0,
                Weight::Semibold,
                pal().text,
                Align::Left,
            );
            let sub = if ai.configured { "Key saved (DPAPI) \u{00b7} Responses / Messages / Chat" } else { "Copy your OpenCode Go API key, then click Paste Key" };
            c.text(sub, Rect::new(card.x + 12.0, card.y + 28.0, card.w - 176.0, 20.0), 11.0, Weight::Regular, if ai.configured { pal().text_2 } else { pal().text_3 }, Align::Left);
            c.pill(Rect::new(card.right() - 158.0, card.y + 25.0, 74.0, 25.0), Action::AiPasteKey, "Paste Key", (!ai.configured).then_some(acc), 11.0);
            c.pill(Rect::new(card.right() - 78.0, card.y + 25.0, 70.0, 25.0), Action::AiRefreshModels, "Sync models", None, 10.5);
        }
        AiProvider::OpenAiCompat => {
            let url_box = Rect::new(card.x + 10.0, card.y + 6.0, card.w - 174.0, 22.0);
            let editing_url = c.ui.input.is_some() && ai.focus_field == AiFocusField::BaseUrl;
            c.s.rect(url_box, 7.0, if editing_url { pal().surface_hover } else { Color::rgba(0, 0, 0, 0.25) });
            if editing_url {
                c.s.stroke(url_box, 7.0, 1.2, acc);
            }
            let url_rect = Rect::new(url_box.x + 8.0, url_box.y, url_box.w - 16.0, url_box.h);
            if editing_url {
                let f = c.ui.input.as_ref().unwrap();
                let ed = field_edit(c, f, acc);
                c.s.text_edit(f.as_str().to_string(), url_rect, 11.0, Weight::Medium, pal().text, ed);
            } else {
                c.text(c.set.active_ai_url().to_string(), url_rect, 11.0, Weight::Medium, pal().text, Align::Left);
            }
            c.hit_text(url_box, Action::AiFocusBaseUrl);
            c.pill(Rect::new(card.right() - 158.0, card.y + 6.0, 72.0, 22.0), Action::AiPasteUrl, "Paste URL", None, 10.5);
            c.pill(Rect::new(card.right() - 80.0, card.y + 6.0, 72.0, 22.0), Action::AiPasteKey, "Paste Key", (!ai.configured).then_some(acc), 10.5);

            let note =
                if ai.configured { "API key saved (DPAPI) \u{00b7} OpenAI / OpenRouter / Groq / vLLM" } else { "Key optional for local servers \u{00b7} Paste Key for cloud APIs" };
            c.text(note, Rect::new(card.x + 12.0, card.y + 32.0, card.w - 86.0, 18.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
            c.pill(Rect::new(card.right() - 68.0, card.y + 31.0, 60.0, 20.0), Action::AiRefreshModels, "Sync", None, 10.0);
        }
        AiProvider::Gemini | AiProvider::Claude => {
            c.text(prov.full_name(), Rect::new(card.x + 12.0, card.y + 6.0, card.w - 110.0, 18.0), 12.0, Weight::Semibold, pal().text, Align::Left);
            let sub = if ai.configured { "API key saved (encrypted with Windows DPAPI)" } else { "Copy your API key to the clipboard, then click Paste Key" };
            c.text(sub, Rect::new(card.x + 12.0, card.y + 28.0, card.w - 110.0, 20.0), 11.0, Weight::Regular, if ai.configured { pal().text_2 } else { pal().text_3 }, Align::Left);
            c.pill(Rect::new(card.right() - 90.0, card.y + 15.0, 82.0, 26.0), Action::AiPasteKey, "Paste Key", (!ai.configured).then_some(acc), 11.0);
        }
        AiProvider::Ollama => {
            c.text("Local Ollama \u{00b7} runs offline on your PC", Rect::new(card.x + 12.0, card.y + 5.0, card.w - 24.0, 18.0), 12.0, Weight::Semibold, pal().text, Align::Left);
            let url_box = Rect::new(card.x + 10.0, card.y + 27.0, card.w - 98.0, 22.0);
            let editing_url = c.ui.input.is_some() && ai.focus_field == AiFocusField::BaseUrl;
            c.s.rect(url_box, 7.0, if editing_url { pal().surface_hover } else { Color::rgba(0, 0, 0, 0.25) });
            if editing_url {
                c.s.stroke(url_box, 7.0, 1.2, acc);
            }
            let url_rect = Rect::new(url_box.x + 8.0, url_box.y, url_box.w - 16.0, url_box.h);
            if editing_url {
                let f = c.ui.input.as_ref().unwrap();
                let ed = field_edit(c, f, acc);
                c.s.text_edit(f.as_str().to_string(), url_rect, 11.0, Weight::Medium, pal().text, ed);
            } else {
                c.text(c.set.active_ai_url().to_string(), url_rect, 11.0, Weight::Medium, pal().text, Align::Left);
            }
            c.hit_text(url_box, Action::AiFocusBaseUrl);
            c.pill(Rect::new(card.right() - 82.0, card.y + 27.0, 74.0, 22.0), Action::AiPasteUrl, "Paste URL", None, 10.5);
        }
    }

    // 3. Model selector row: editable model field + Prev/Next + preset chips.
    let my = a.y + 125.0;
    c.text("Model", Rect::new(a.x, my, 42.0, 24.0), 11.5, Weight::Medium, pal().text_2, Align::Left);
    let mbox = Rect::new(a.x + 44.0, my, 134.0, 24.0);
    let editing_model = c.ui.input.is_some() && ai.focus_field == AiFocusField::Model;
    c.s.rect(mbox, 8.0, if editing_model { pal().surface_hover } else { pal().surface });
    if editing_model {
        c.s.stroke(mbox, 8.0, 1.2, acc);
    }
    let model_rect = Rect::new(mbox.x + 8.0, mbox.y, mbox.w - 16.0, mbox.h);
    if editing_model {
        let f = c.ui.input.as_ref().unwrap();
        let ed = field_edit(c, f, acc);
        c.s.text_edit(f.as_str().to_string(), model_rect, 11.5, Weight::Semibold, pal().text, ed);
    } else {
        c.text(c.set.active_ai_model().to_string(), model_rect, 11.5, Weight::Semibold, pal().text, Align::Left);
    }
    c.hit_text(mbox, Action::AiFocusModel);

    c.icon_btn(Rect::new(mbox.right() + 4.0, my, 24.0, 24.0), Action::AiCycleModel(-1), Icon::Prev, 10.0, pal().text_2);
    c.icon_btn(Rect::new(mbox.right() + 30.0, my, 24.0, 24.0), Action::AiCycleModel(1), Icon::Next, 10.0, pal().text_2);

    let mut cx = mbox.right() + 58.0;
    for (idx, m) in ai.available_models.iter().take(2).enumerate() {
        let rem = a.right() - cx;
        if rem < 60.0 {
            break;
        }
        let w = (est_w(m, 10.5) * 1.12 + 20.0).clamp(60.0, (rem - 2.0).min(118.0));
        let is_cur = m == c.set.active_ai_model();
        c.pill(Rect::new(cx, my + 1.0, w, 22.0), Action::AiSelectModel(idx as u16), m, is_cur.then_some(acc), 10.0);
        cx += w + 4.0;
    }

    // 4. Share my day toggle + learned memories badge.
    let sy = a.y + 156.0;
    c.text("Share my day (calendar, tasks, water, focus, PC stats)", Rect::new(a.x, sy, a.w - 52.0, 22.0), 12.0, Weight::Regular, pal().text, Align::Left);
    c.switch(Rect::new(a.right() - 42.0, sy, 42.0, 22.0), SettingKey::AiShareDay, acc);

    // 5. Status / footer note.
    let fy = a.bottom() - 18.0;
    let (foot, col) = if !ai.error.is_empty() {
        (ai.error.clone(), pal().amber)
    } else if !ai.status_note.is_empty() {
        (ai.status_note.clone(), acc)
    } else {
        (format!("Secrets encrypted with Windows DPAPI \u{00b7} {} learned memor{}", ai.memories_count, if ai.memories_count == 1 { "y" } else { "ies" }), pal().text_3)
    };
    c.text(foot, Rect::new(a.x, fy, a.w, 18.0), 10.5, Weight::Medium, col, Align::Left);
}

fn ai_history(c: &mut Ctx, a: Rect, acc: Color) {
    let ai = &c.vm.ai;
    let body = Rect::new(a.x, a.y + 32.0, a.w, a.h - 32.0);
    if ai.history.is_empty() {
        c.empty_state(body, Icon::Sparkle, acc, "No conversations yet", "Send a message to start chatting");
        return;
    }
    let scroll = c.ui.scroll_of(Page::Ai).min(ai_hist_max_scroll(ai.history.len()));
    for slot in 0..AI_HIST_ROWS {
        let idx = scroll + slot;
        let Some(item) = ai.history.get(idx) else { break };
        let r = Rect::new(a.x, body.y + slot as f32 * 38.0, a.w, 34.0);
        let sel = Action::AiSelectChat(item.id);
        let del = Action::AiDeleteChat(item.id);
        let hv = c.hov(sel).max(c.hov(del));
        let active = item.id == ai.active_id;
        c.s.rect(r, 10.0, if active { acc.fade(0.16) } else { pal().surface.mix(pal().surface_hover, hv) });
        if active {
            c.s.circle(r.x + 14.0, r.cy(), 3.5, acc);
        }
        let tx = if active { r.x + 26.0 } else { r.x + 14.0 };
        c.text(item.title.clone(), Rect::new(tx, r.y, r.w - 110.0, r.h), 12.5, if active { Weight::Semibold } else { Weight::Regular }, pal().text, Align::Left);
        c.text(
            format!("{} msg{}", item.msg_count, if item.msg_count == 1 { "" } else { "s" }),
            Rect::new(r.right() - 88.0, r.y, 52.0, r.h),
            11.0,
            Weight::Regular,
            pal().text_3,
            Align::Right,
        );
        c.hit(Rect::new(r.x, r.y, r.w - 32.0, r.h), sel);
        c.ghost_icon(Rect::new(r.right() - 28.0, r.cy() - 12.0, 24.0, 24.0), del, Icon::Close, 11.5, pal().text_2);
    }
}

fn ai_chat(c: &mut Ctx, a: Rect, acc: Color) {
    use crate::ai::{AiFocusField, Role, STARTER_PROMPTS};
    let ai = &c.vm.ai;
    let prov = c.set.ai_provider;

    // Prompt input bar at the bottom.
    let in_y = a.bottom() - 34.0;
    let field = Rect::new(a.x, in_y, a.w - 40.0, 34.0);
    let typing = c.ui.input.is_some() && ai.focus_field == AiFocusField::Prompt;
    c.s.rect(field, 12.0, if typing { pal().surface_hover } else { c.surface_fill(Action::AiInput) });
    if typing {
        c.s.stroke(field, 12.0, 1.4, acc.fade(0.85));
    }
    icons::draw(&mut c.s, Icon::Sparkle, field.x + 16.0, field.cy(), 14.0, if typing { acc } else { pal().text_3 });
    let tr = Rect::new(field.x + 32.0, field.y, field.w - 42.0, field.h);
    let shown = if typing { c.ui.input.as_ref().map(|f| f.as_str()).unwrap_or("") } else { ai.input.as_str() };
    if !shown.is_empty() {
        if typing {
            let f = c.ui.input.as_ref().unwrap();
            let ed = field_edit(c, f, acc);
            c.s.text_edit(f.as_str().to_string(), tr, 12.5, Weight::Regular, pal().text, ed);
        } else {
            c.text(shown.to_string(), tr, 12.5, Weight::Regular, pal().text, Align::Left);
        }
    } else {
        let placeholder = if ai.busy { "Thinking\u{2026}" } else { "Ask anything, add a task, or plan your day\u{2026}" };
        c.text(placeholder, tr, 12.5, Weight::Regular, pal().text_3, Align::Left);
        if typing && (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
            c.s.rect(Rect::new(tr.x, field.cy() - 8.0, 1.5, 16.0), 0.7, acc);
        }
    }
    c.hit_text(field, Action::AiInput);

    // Send button.
    let send_r = Rect::new(a.right() - 34.0, in_y, 34.0, 34.0);
    let can_send = !shown.trim().is_empty() && !ai.busy;
    c.s.rect(send_r, 17.0, if can_send { acc } else { pal().surface });
    icons::draw(&mut c.s, Icon::Send, send_r.cx(), send_r.cy(), 15.0, if can_send { ON_ACCENT } else { pal().text_3 });
    c.hit(send_r, Action::AiSend);

    let body = Rect::new(a.x, a.y + 30.0, a.w, in_y - (a.y + 30.0) - 6.0);

    // Empty chat state: optional setup banner + 4 starter suggestion cards.
    if ai.messages.is_empty() && !ai.busy {
        let mut grid_y = body.y;
        let mut card_h = 68.0;
        if !ai.configured && prov != AiProvider::Ollama && prov != AiProvider::OpenAiCompat {
            let ban = Rect::new(body.x, body.y, body.w, 32.0);
            c.s.rect(ban, 10.0, acc.fade(0.12));
            let msg = match prov {
                AiProvider::ChatGpt => "Sign in with your ChatGPT subscription to start",
                _ => "Add your API key in Setup to start chatting",
            };
            c.text(msg, Rect::new(ban.x + 12.0, ban.y, ban.w - 134.0, ban.h), 11.5, Weight::Medium, pal().text, Align::Left);
            let (cta_act, cta_lbl) = if prov == AiProvider::ChatGpt { (Action::AiChatGptSignIn, "Sign in") } else { (Action::AiToggleSetup, "Open Setup") };
            c.pill(Rect::new(ban.right() - 112.0, ban.y + 4.0, 104.0, 24.0), cta_act, cta_lbl, Some(acc), 11.0);
            grid_y += 38.0;
            card_h = 56.0;
        }
        let cw = (body.w - 10.0) / 2.0;
        for (i, (title, prompt)) in STARTER_PROMPTS.iter().enumerate() {
            let (col, row) = (i % 2, i / 2);
            let r = Rect::new(body.x + col as f32 * (cw + 10.0), grid_y + row as f32 * (card_h + 8.0), cw, card_h);
            let act = Action::AiStarter(i as u8);
            let hv = c.hov(act);
            c.s.rect(r, 12.0, if c.pressed(act) { pal().surface_press } else { pal().surface.mix(pal().surface_hover, hv) });
            c.text(*title, Rect::new(r.x + 12.0, r.y + 7.0, r.w - 24.0, 18.0), 12.5, Weight::Semibold, pal().text, Align::Left);
            let lines = wrap_lines(prompt, r.w - 24.0, 10.5);
            for (li, l) in lines.iter().take(2).enumerate() {
                c.text(l.clone(), Rect::new(r.x + 12.0, r.y + 26.0 + li as f32 * 13.0, r.w - 24.0, 14.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
            }
            c.hit(r, act);
        }
        return;
    }

    // Pre-layout the conversation so we can bottom-align and scroll cleanly.
    // Assistant replies are rendered as markdown beside a small avatar; the
    // user's messages stay as accent bubbles on the right.
    const GUTTER: f32 = 32.0;
    enum RowItem {
        UserBubble(Vec<String>),
        Assistant { msg_idx: usize, md: std::rc::Rc<super::markdown::Laid> },
        ActionRow { msg_idx: usize, act_idx: usize, label: String, can_undo: bool, undone: bool },
        Thinking,
        ErrorBanner(String),
    }

    let md_w = body.w - GUTTER - 6.0;
    let mut items = Vec::new();
    for (mi, m) in ai.messages.iter().enumerate() {
        match m.role {
            Role::User => {
                items.push(RowItem::UserBubble(wrap_lines(&m.text, body.w * 0.72, 12.0)));
            }
            Role::Assistant => {
                items.push(RowItem::Assistant { msg_idx: mi, md: super::markdown::laid(&m.text, md_w) });
                for (ai_idx, act) in m.actions.iter().enumerate() {
                    items.push(RowItem::ActionRow { msg_idx: mi, act_idx: ai_idx, label: act.label.clone(), can_undo: act.can_undo(), undone: act.undone });
                }
            }
        }
    }
    if ai.busy {
        items.push(RowItem::Thinking);
    }
    if !ai.error.is_empty() {
        items.push(RowItem::ErrorBanner(ai.error.clone()));
    }

    /// Row below an assistant reply that holds its Copy button.
    const COPY_ROW: f32 = 22.0;
    let item_h = |it: &RowItem| -> f32 {
        match it {
            RowItem::UserBubble(lines) => (lines.len() as f32 * 16.0 + 12.0).max(28.0) + 8.0,
            RowItem::Assistant { md, .. } => md.height.max(LH_MIN) + COPY_ROW + 8.0,
            RowItem::ActionRow { .. } => 26.0,
            RowItem::Thinking => 32.0,
            RowItem::ErrorBanner(_) => 28.0,
        }
    };
    const LH_MIN: f32 = 18.0;

    let total_h: f32 = items.iter().map(&item_h).sum();
    let max_scroll = (total_h - body.h).max(0.0);
    let scroll_px = (ai.scroll as f32 * 24.0).min(max_scroll);
    let mut y = if total_h <= body.h { body.y } else { body.bottom() - total_h + scroll_px };

    let avatar = |c: &mut Ctx, x: f32, y: f32, pulse: f32| {
        c.s.circle(x + 11.0, y + 11.0, 11.0, acc.fade(0.16));
        icons::draw(&mut c.s, Icon::Sparkle, x + 11.0, y + 11.0, 12.0, acc.fade(pulse));
    };

    c.s.clip(body);
    for it in &items {
        let h = item_h(it);
        if y + h < body.y - 2.0 {
            y += h;
            continue;
        }
        if y > body.bottom() + 2.0 {
            break;
        }
        match it {
            RowItem::UserBubble(lines) => {
                let bh = h - 8.0;
                let max_line = lines.iter().map(|l| est_w(l, 12.0) * 1.12).fold(40.0_f32, f32::max);
                let bw = (max_line + 24.0).clamp(64.0, body.w * 0.78);
                let br = Rect::new(body.right() - bw, y, bw, bh);
                c.s.rect(br, 14.0, acc.fade(0.2));
                c.s.stroke(br, 14.0, 1.0, acc.fade(0.22));
                for (li, l) in lines.iter().enumerate() {
                    c.text(l.clone(), Rect::new(br.x + 12.0, br.y + 6.0 + li as f32 * 16.0, br.w - 24.0, 16.0), 12.0, Weight::Regular, pal().text, Align::Left);
                }
            }
            RowItem::Assistant { msg_idx, md } => {
                avatar(c, body.x, y + 1.0, 1.0);
                md.draw(&mut c.s, body.x + GUTTER, y + 1.0, md_w, acc);
                let copy_r = Rect::new(body.x + GUTTER - 2.0, y + md.height.max(LH_MIN) + 5.0, 40.0, 18.0);
                if copy_r.y >= body.y && copy_r.bottom() <= body.bottom() {
                    c.pill(copy_r, Action::AiCopyReply(*msg_idx as u16), "Copy", None, 9.5);
                }
            }
            RowItem::ActionRow { msg_idx, act_idx, label, can_undo, undone } => {
                let ar = Rect::new(body.x + GUTTER, y, body.w - GUTTER, 22.0);
                c.s.rect(ar, 8.0, pal().green.fade(0.12));
                icons::draw(&mut c.s, Icon::Check, ar.x + 12.0, ar.cy(), 11.0, pal().green);
                let txt = if *undone { format!("{label} (undone)") } else { label.clone() };
                c.text(txt, Rect::new(ar.x + 24.0, ar.y, ar.w - 84.0, ar.h), 11.0, Weight::Medium, if *undone { pal().text_3 } else { pal().text_2 }, Align::Left);
                if *can_undo {
                    let undo_r = Rect::new(ar.right() - 52.0, ar.y + 2.0, 48.0, 18.0);
                    if undo_r.y >= body.y && undo_r.bottom() <= body.bottom() {
                        c.pill(undo_r, Action::AiUndoAction(*msg_idx as u16, *act_idx as u8), "Undo", None, 9.5);
                    }
                }
            }
            RowItem::Thinking => {
                let t = c.now_ms as f32 / 1000.0;
                let pulse = (t * std::f32::consts::TAU / 1.2).sin() * 0.35 + 0.65;
                avatar(c, body.x, y + 1.0, pulse);
                // Three dots chasing each other.
                for k in 0..3 {
                    let ph = ((t * 1.6 - k as f32 * 0.18) % 1.0).max(0.0);
                    let lift = (ph * std::f32::consts::PI).sin().max(0.0);
                    c.s.circle(body.x + GUTTER + 6.0 + k as f32 * 11.0, y + 12.0 - lift * 3.0, 2.6, pal().text_2.fade(0.45 + 0.55 * lift));
                }
            }
            RowItem::ErrorBanner(err) => {
                let br = Rect::new(body.x, y, body.w, 24.0);
                c.s.rect(br, 8.0, pal().amber.fade(0.14));
                c.text(err.clone(), Rect::new(br.x + 10.0, br.y, br.w - 20.0, br.h), 11.0, Weight::Medium, pal().amber, Align::Left);
            }
        }
        y += h;
    }
    c.s.pop_clip();
}

/// "Check for updates" / "Update to vX" button for the GitHub release check.
pub(super) fn update_pill(c: &mut Ctx, r: Rect) {
    use crate::update::UpdatePhase::*;
    let u = &c.vm.update;
    let acc = accent(Page::Speed);
    match u.phase {
        Idle | Failed => {
            let label = if u.phase == Failed { "Update failed \u{2014} retry" } else { "Check for updates" };
            let act = if u.phase == Failed && !u.version.is_empty() { Action::UpdateInstall } else { Action::UpdateCheck };
            c.pill(r, act, label, None, 11.0);
        }
        UpToDate => c.pill(r, Action::UpdateCheck, &format!("Up to date \u{00b7} v{}", crate::update::CURRENT), None, 11.0),
        Checking => c.pill(r, Action::Island, "Checking\u{2026}", None, 11.0),
        Available => c.pill(r, Action::UpdateInstall, &format!("Update to v{}", u.version), Some(acc), 11.5),
        Downloading => c.pill(r, Action::Island, "Downloading\u{2026}", Some(acc.fade(0.45)), 11.0),
    }
}
