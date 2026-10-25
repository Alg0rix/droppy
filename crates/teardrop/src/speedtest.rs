//! Native internet speed test: latency, download and upload against
//! Cloudflare's public speed endpoints, over WinINet (no external tools).
//!
//! `start()` spawns a worker; the UI polls `snapshot()` each frame. Results
//! are best-effort: any network failure ends the run as `Failed`.

use crate::viewmodel::{SpeedPhase, SpeedView};
use std::sync::Mutex;

static STATE: Mutex<SpeedView> = Mutex::new(SpeedView::IDLE);

pub fn snapshot() -> SpeedView {
    STATE.lock().map(|s| *s).unwrap_or(SpeedView::IDLE)
}

fn update(f: impl FnOnce(&mut SpeedView)) {
    if let Ok(mut s) = STATE.lock() {
        f(&mut s);
    }
}

/// Mbit/s from a byte count over a duration.
pub fn mbps(bytes: u64, secs: f32) -> f32 {
    if secs <= 0.0 {
        return 0.0;
    }
    bytes as f32 * 8.0 / 1_000_000.0 / secs
}

/// Median of a set of samples (0 when empty).
pub fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

pub fn start() {
    let go = STATE.lock().map(|mut s| {
        if s.phase.running() {
            false
        } else {
            *s = SpeedView { phase: SpeedPhase::Ping, ..SpeedView::IDLE };
            true
        }
    });
    if go.unwrap_or(false) {
        std::thread::spawn(|| {
            if !run() {
                update(|s| s.phase = SpeedPhase::Failed);
            }
        });
    }
}

#[cfg(not(windows))]
fn run() -> bool {
    false
}

#[cfg(windows)]
fn run() -> bool {
    net::run()
}

#[cfg(windows)]
mod net {
    use super::*;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinInet::*;

    const HOST: &str = "speed.cloudflare.com";
    const THREADS: usize = 4;
    const DOWN_SECS: f32 = 8.0;
    const UP_SECS: f32 = 6.0;
    /// Ramp-up ignored when computing the final figure.
    const WARMUP: f32 = 1.5;
    const FLAGS: u32 = 0x8000_0000 | 0x0080_0000 | 0x0400_0000 | 0x0040_0000; // RELOAD | SECURE | NO_CACHE_WRITE | KEEP_CONNECTION

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// A WinINet handle that closes itself. Stored as usize so it is `Send`.
    struct Handle(usize);

    impl Handle {
        fn new(p: *mut c_void) -> Option<Handle> {
            (!p.is_null()).then_some(Handle(p as usize))
        }
        fn raw(&self) -> *mut c_void {
            self.0 as *mut c_void
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = InternetCloseHandle(self.raw() as *const _);
            }
        }
    }

    fn session() -> Option<Handle> {
        unsafe {
            let agent = wide("Droppy/0.1");
            let h = Handle::new(InternetOpenW(PCWSTR::from_raw(agent.as_ptr()), 0, None, None, 0))?;
            let timeout: u32 = 8000;
            for opt in [2u32, 6, 5] {
                let _ = InternetSetOptionW(Some(h.raw() as *const _), opt, Some(&timeout as *const u32 as *const _), 4);
            }
            Some(h)
        }
    }

    fn connect(sess: &Handle) -> Option<Handle> {
        unsafe {
            let host = wide(HOST);
            Handle::new(InternetConnectW(sess.raw() as *const _, PCWSTR::from_raw(host.as_ptr()), 443, None, None, 3, 0, 0))
        }
    }

    /// Open + send one request. Returns the request handle ready for reading.
    fn send(conn: &Handle, verb: &str, path: &str, body: Option<&[u8]>) -> Option<Handle> {
        unsafe {
            let (v, p) = (wide(verb), wide(path));
            let req = Handle::new(HttpOpenRequestW(conn.raw() as *const _, PCWSTR::from_raw(v.as_ptr()), PCWSTR::from_raw(p.as_ptr()), None, None, None, FLAGS, 0))?;
            let hdr: Vec<u16> = "Content-Type: application/octet-stream".encode_utf16().collect();
            let (ptr, len) = body.map(|b| (b.as_ptr() as *const c_void, b.len() as u32)).unwrap_or((std::ptr::null(), 0));
            HttpSendRequestW(req.raw() as *const _, Some(&hdr), Some(ptr), len).ok()?;
            Some(req)
        }
    }

    fn read(req: &Handle, buf: &mut [u8]) -> usize {
        let mut n: u32 = 0;
        unsafe {
            if InternetReadFile(req.raw() as *const _, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut n).is_err() {
                return 0;
            }
        }
        n as usize
    }

    fn latency(conn: &Handle) -> Option<f32> {
        let mut buf = [0u8; 512];
        // The first request pays for TCP + TLS; only the warm ones count.
        let warm = send(conn, "GET", "/__down?bytes=0", None)?;
        while read(&warm, &mut buf) > 0 {}
        let mut ms = Vec::new();
        for _ in 0..6 {
            let t = Instant::now();
            let r = send(conn, "GET", "/__down?bytes=0", None)?;
            while read(&r, &mut buf) > 0 {}
            ms.push(t.elapsed().as_secs_f32() * 1000.0);
        }
        Some(median(&mut ms))
    }

    /// Run `THREADS` workers that add to a shared byte counter until the
    /// deadline, sampling it to publish live speed. Returns the final Mbit/s.
    fn measure(sess: &Arc<Handle>, secs: f32, upload: bool, publish: fn(&mut SpeedView, f32)) -> Option<f32> {
        let bytes = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let workers: Vec<_> = (0..THREADS)
            .map(|_| {
                let (sess, bytes, stop) = (sess.clone(), bytes.clone(), stop.clone());
                std::thread::spawn(move || {
                    let Some(conn) = connect(&sess) else { return };
                    let chunk = vec![0u8; 512 * 1024];
                    let mut buf = vec![0u8; 64 * 1024];
                    while !stop.load(Ordering::Relaxed) {
                        if upload {
                            let Some(r) = send(&conn, "POST", "/__up", Some(&chunk)) else { return };
                            let mut sink = [0u8; 256];
                            while read(&r, &mut sink) > 0 {}
                            bytes.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        } else {
                            let Some(r) = send(&conn, "GET", "/__down?bytes=50000000", None) else { return };
                            while !stop.load(Ordering::Relaxed) {
                                let n = read(&r, &mut buf);
                                if n == 0 {
                                    break;
                                }
                                bytes.fetch_add(n as u64, Ordering::Relaxed);
                            }
                        }
                    }
                })
            })
            .collect();

        let t0 = Instant::now();
        let (mut mark_t, mut mark_b) = (0.0f32, 0u64);
        let (mut warm_t, mut warm_b) = (None, 0u64);
        let mut last: (f32, u64);
        loop {
            std::thread::sleep(Duration::from_millis(150));
            let (t, b) = (t0.elapsed().as_secs_f32(), bytes.load(Ordering::Relaxed));
            last = (t, b);
            if warm_t.is_none() && t >= WARMUP {
                warm_t = Some(t);
                warm_b = b;
            }
            if t - mark_t >= 0.5 {
                let live = mbps(b - mark_b, t - mark_t);
                update(|s| publish(s, live));
                (mark_t, mark_b) = (t, b);
            }
            if t >= secs || workers.iter().all(|w| w.is_finished()) {
                break;
            }
        }
        stop.store(true, Ordering::Relaxed);
        for w in workers {
            let _ = w.join();
        }
        let (t, b) = last;
        let result = match warm_t {
            Some(wt) if t > wt => mbps(b - warm_b, t - wt),
            _ => mbps(b, t),
        };
        (b > 0).then_some(result)
    }

    pub fn run() -> bool {
        let Some(sess) = session().map(Arc::new) else { return false };
        let Some(conn) = connect(&sess) else { return false };
        let Some(ping) = latency(&conn) else { return false };
        update(|s| {
            s.ping_ms = Some(ping);
            s.phase = SpeedPhase::Down;
        });
        let Some(down) = measure(&sess, DOWN_SECS, false, |s, v| s.down = v) else { return false };
        update(|s| {
            s.down = down;
            s.phase = SpeedPhase::Up;
        });
        let up = measure(&sess, UP_SECS, true, |s, v| s.up = v).unwrap_or(0.0);
        update(|s| {
            s.up = up;
            s.phase = SpeedPhase::Done;
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_math() {
        assert_eq!(mbps(12_500_000, 1.0), 100.0);
        assert_eq!(mbps(1, 0.0), 0.0);
    }

    #[test]
    fn median_picks_the_middle() {
        assert_eq!(median(&mut [30.0, 10.0, 20.0]), 20.0);
        assert_eq!(median(&mut []), 0.0);
    }
}

#[cfg(all(test, windows))]
mod live {
    /// Needs internet: `cargo test -p teardrop live_speedtest -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_speedtest() {
        super::start();
        while super::snapshot().phase.running() {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let s = super::snapshot();
        println!("{s:?}");
        assert_eq!(s.phase, crate::viewmodel::SpeedPhase::Done);
        assert!(s.down > 0.0);
    }
}
