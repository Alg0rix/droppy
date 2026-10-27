use super::*;
// -------------------------------------------------------------------- voice

pub(super) fn voice(c: &mut Ctx, a: Rect) {
    use crate::voice::VoicePhase;
    let acc = accent(Page::Voice);
    let v = &c.vm.voice;
    let rec = v.phase == VoicePhase::Recording;
    let cx = a.cx();

    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, if rec { acc } else { pal().text_3 });
    c.text(if rec { "RECORDING" } else { "VOICE RECORDER" }, Rect::new(a.x + 12.0, a.y, 200.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);

    c.text(crate::voice::clock(v.elapsed_ms), Rect::new(a.x, a.y + 20.0, a.w, 34.0), 28.0, Weight::Semibold, if rec { pal().text } else { pal().text_3 }, Align::Center);

    // Waveform of the last few seconds, mirrored around the middle.
    let n = c.ui.voice_bars.len();
    let (bw, gap) = (5.0, 3.0);
    let x0 = cx - (n as f32 * (bw + gap) - gap) / 2.0;
    let mid = a.y + 78.0;
    for (i, b) in c.ui.voice_bars.iter().enumerate() {
        let h = 3.0 + b * 40.0;
        let fade = 0.35 + 0.65 * (i as f32 / n as f32);
        c.s.rect(Rect::new(x0 + i as f32 * (bw + gap), mid - h / 2.0, bw, h), 2.5, if rec { acc.fade(fade) } else { pal().ink(0.12) });
    }

    // Record / stop button.
    let (by, br) = (a.y + 132.0, 32.0);
    let act = Action::VoiceToggle;
    let hv = c.hov(act);
    if rec {
        let pulse = ((c.now_ms % 1400) as f32 / 1400.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
        c.s.circle(cx, by, br + 6.0 + 4.0 * pulse, acc.fade(0.16));
        c.s.circle(cx, by, br + hv * 2.0, acc);
        c.s.rect(Rect::new(cx - 11.0, by - 11.0, 22.0, 22.0), 5.0, WHITE);
    } else {
        c.s.circle(cx, by, br + 6.0, acc.fade(0.10 + 0.10 * hv));
        c.s.ring(cx, by, br, 3.0, 0.0, 1.0, acc);
        c.s.circle(cx, by, 14.0 + hv * 2.0, acc);
    }
    c.hit(Rect::new(cx - br, by - br, br * 2.0, br * 2.0), act);

    // Status line.
    let (msg, col) = match v.phase {
        VoicePhase::Recording => ("Recording\u{2026} tap to stop".to_string(), pal().text_2),
        VoicePhase::Failed => (v.error.clone(), pal().amber),
        VoicePhase::Idle => match &v.saved {
            Some(name) => (format!("Saved to the shelf \u{00b7} {name}"), pal().text_2),
            None => ("Tap to record from your microphone".to_string(), pal().text_3),
        },
    };
    c.text(msg, Rect::new(a.x, a.y + 176.0, a.w, 18.0), 12.0, Weight::Medium, col, Align::Center);
    c.pill(Rect::new(a.right() - 100.0, a.bottom() - 30.0, 100.0, 22.0), Action::VoiceFolder, "Open folder", None, 10.5);
}
