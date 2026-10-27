use super::*;
// ------------------------------------------------------------ timers

pub(super) fn set_timer(hwnd: HWND, ms: u32) {
    if TIMER_MS.get() != ms {
        unsafe {
            let _ = SetTimer(hwnd, T_ANIM, ms, None);
        }
        TIMER_MS.set(ms);
    }
}

pub(super) fn kill_timer(hwnd: HWND) {
    if TIMER_MS.get() != 0 {
        unsafe {
            let _ = KillTimer(hwnd, T_ANIM);
        }
        TIMER_MS.set(0);
    }
}

/// Something changed: run the vsync-locked frame loop until it settles.
pub(super) fn wake(_hwnd: HWND) {
    if !FAST.get() {
        LAST_TICK.set(None);
        FAST.set(true);
    }
}
