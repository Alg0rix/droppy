//! Installed-app launcher index for the Windows page search field.
//!
//! App hits come from the Start Menu scan (every .lnk under both roots).
//! On top of those, `winsearch` layers real Windows Search index hits —
//! files, folders, shell items — the same catalog Start Menu queries. That
//! query runs on a worker thread after a short typing pause, so the UI
//! thread only ever does cheap in-memory work.
//!
//! Ranking follows PowerToys Command Palette (`teardrop_core::search`):
//! fuzzy score + match tier (exact > prefix > word/acronym > fuzzy), with
//! a frecency boost from launch history ordering hits within a tier.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use teardrop_core::search::{self, Tier};

/// (display name, .lnk path) — Start Menu order, stable across searches.
static APPS: OnceLock<Vec<(String, String)>> = OnceLock::new();

fn scan_dir(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            scan_dir(&p, out);
        } else if p.extension().map(|x| x == "lnk").unwrap_or(false) {
            let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            if !name.is_empty() {
                out.push((name, p.to_string_lossy().into_owned()));
            }
        }
    }
}

pub fn installed_apps() -> &'static [(String, String)] {
    APPS.get_or_init(|| {
        let mut out = Vec::new();
        for dir in [
            std::env::var("APPDATA").map(|a| format!("{a}\\Microsoft\\Windows\\Start Menu\\Programs")),
            std::env::var("PROGRAMDATA").map(|a| format!("{a}\\Microsoft\\Windows\\Start Menu\\Programs")),
        ]
        .into_iter()
        .flatten()
        {
            scan_dir(std::path::Path::new(&dir), &mut out);
        }
        out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        out
    })
}

/// One launcher hit: display name, a path to open (lnk, file, or shell
/// parsing path), and the right-side tag ("app" or the file's type text).
#[derive(Clone)]
pub struct Hit {
    pub name: String,
    pub path: String,
    pub tag: String,
}

/// Quiet time after the last keystroke before the index is queried.
const DEBOUNCE: Duration = Duration::from_millis(140);

/// Wake the UI when background work finishes (set once by the window).
static NOTIFY: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

pub fn set_notify(f: impl Fn() + Send + Sync + 'static) {
    let _ = NOTIFY.set(Box::new(f));
}

/// Ask the UI to redraw (no-op until the window registers a callback).
pub fn notify() {
    if let Some(f) = NOTIFY.get() {
        f();
    }
}

struct Shared {
    /// Latest query the UI asked for, and when it may run.
    want: Option<(String, Instant)>,
    requested: String,
    /// Index rows for `done_q`, from the last finished query.
    done_q: String,
    rows: Vec<crate::winsearch::Row>,
    seq: u64,
}

static SHARED: OnceLock<(Mutex<Shared>, Condvar)> = OnceLock::new();
static WORKER_UP: AtomicBool = AtomicBool::new(false);

/// The search worker's state; first use starts the thread, which also warms
/// the Start Menu scan so the first keystroke doesn't pay for it.
fn shared() -> &'static (Mutex<Shared>, Condvar) {
    let sh = SHARED.get_or_init(|| {
        (Mutex::new(Shared { want: None, requested: String::new(), done_q: String::new(), rows: Vec::new(), seq: 0 }), Condvar::new())
    });
    if !WORKER_UP.swap(true, Ordering::SeqCst) {
        std::thread::Builder::new().name("launcher-search".into()).spawn(worker).ok();
    }
    sh
}

/// Start the worker early (e.g. when the search field gets focus).
pub fn warm() {
    let _ = shared();
}

fn worker() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let _ = installed_apps();
    notify();
    let (lock, cv) = SHARED.get().expect("shared() runs before the worker");
    loop {
        // Sleep until a query is due; a newer keystroke pushes `due` out.
        let q = {
            let mut g = lock.lock().unwrap();
            loop {
                match g.want.clone() {
                    None => g = cv.wait(g).unwrap(),
                    Some((q, due)) => {
                        let now = Instant::now();
                        if now >= due {
                            g.want = None;
                            break q;
                        }
                        g = cv.wait_timeout(g, due - now).unwrap().0;
                    }
                }
            }
        };
        let rows = crate::winsearch::search(&q, 25);
        {
            let mut g = lock.lock().unwrap();
            g.done_q = q;
            g.rows = rows;
            g.seq += 1;
        }
        notify();
    }
}

/// Launch history (frecency) — lazily loaded, saved on each `launch()`.
static HISTORY: OnceLock<Mutex<search::History>> = OnceLock::new();

/// Bumped on every launch so the `hits()` cache re-ranks.
static HIST_GEN: AtomicU64 = AtomicU64::new(0);

fn history() -> &'static Mutex<search::History> {
    HISTORY.get_or_init(|| Mutex::new(crate::store::load(&crate::paths::data_dir(), "launcher-history")))
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// `hits()` runs every layout frame, so results are cached against the
/// inputs that could change them: query, finished-search seq, apps loaded,
/// history generation.
struct LastHits {
    q: String,
    seq: u64,
    apps: bool,
    gen: u64,
    hits: Vec<Hit>,
}

static LAST: OnceLock<Mutex<LastHits>> = OnceLock::new();

/// Hits for `query`. Start Menu apps come back immediately; real Windows
/// Search rows (files, folders, shell items — ranked by the index) are
/// appended once the debounced background query for exactly this text has
/// finished, at which point the UI is woken to redraw.
pub fn hits(query: &str) -> Vec<Hit> {
    let q = query.trim().to_lowercase();
    let (lock, cv) = shared();
    if q.is_empty() {
        let mut g = lock.lock().unwrap();
        g.want = None;
        g.requested.clear();
        return Vec::new();
    }
    let (seq, rows) = {
        let mut g = lock.lock().unwrap();
        if g.requested != q {
            g.requested = q.clone();
            g.want = Some((q.clone(), Instant::now() + DEBOUNCE));
            cv.notify_all();
        }
        (g.seq, if g.done_q == q { Some(g.rows.clone()) } else { None })
    };
    let apps_ready = APPS.get().is_some();
    let gen = HIST_GEN.load(Ordering::SeqCst);
    let last = LAST.get_or_init(|| Mutex::new(LastHits { q: String::new(), seq: 0, apps: false, gen: 0, hits: Vec::new() }));
    let mut c = last.lock().unwrap();
    if c.q == q && c.seq == seq && c.apps == apps_ready && c.gen == gen {
        return c.hits.clone();
    }
    let out = compute(&q, apps_ready, rows.as_deref().unwrap_or(&[]));
    *c = LastHits { q, seq, apps: apps_ready, gen, hits: out.clone() };
    out
}

fn compute(q: &str, apps_ready: bool, rows: &[crate::winsearch::Row]) -> Vec<Hit> {
    let mut out: Vec<Hit> = Vec::new();
    let now = now_secs();
    let hist = history().lock().unwrap();
    // Until the worker's warm-up scan lands there is nothing to filter (never
    // block the UI thread on the Start Menu walk).
    let apps: &[(String, String)] = if apps_ready { installed_apps() } else { &[] };
    // CmdPal drops fuzzy-only hits for very short queries — too noisy.
    let short = q.chars().count() <= 2;
    let mut seen = std::collections::HashSet::new();
    let mut scored: Vec<(u32, String, String)> = Vec::new();
    for (n, p) in apps {
        if !seen.insert(n.to_lowercase()) {
            continue;
        }
        let score = search::rank(q, n, &app_subtitle(p), hist.weight(p, now));
        if score == 0 || (short && search::tier_of(score) < Tier::WordOrAcronym) {
            continue;
        }
        scored.push((score, n.clone(), p.clone()));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    for (_, n, p) in scored.into_iter().take(6) {
        out.push(Hit { name: n, path: p, tag: "app".to_string() });
    }
    // Index rows keep their own skip/dedupe/tag rules. A zero score stays —
    // the index matched it with its own tokenizer — but sorts last; the
    // stable sort keeps index order as the tiebreak.
    let mut scored_rows: Vec<(u32, Hit)> = Vec::new();
    for r in rows {
        if r.name.is_empty() || r.path.is_empty() {
            continue;
        }
        let dup = |h: &Hit| h.path.eq_ignore_ascii_case(&r.path) || h.name.eq_ignore_ascii_case(&r.name);
        if out.iter().any(&dup) || scored_rows.iter().any(|(_, h)| dup(h)) {
            continue;
        }
        let tag = if r.path.to_lowercase().ends_with(".lnk") {
            "app".to_string()
        } else if r.kind.is_empty() {
            "file".to_string()
        } else {
            r.kind.to_lowercase()
        };
        let score = search::rank(q, &r.name, "", hist.weight(&r.path, now));
        scored_rows.push((score, Hit { name: r.name.clone(), path: r.path.clone(), tag }));
    }
    scored_rows.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
    for (_, h) in scored_rows {
        out.push(h);
        if out.len() == 12 {
            break;
        }
    }
    out
}

/// Subtitle used for ranking a Start Menu app: its folder under Programs
/// ("Steam", "Tools"), empty when the .lnk sits directly in Programs.
fn app_subtitle(path: &str) -> String {
    std::path::Path::new(path)
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|s| s.to_str())
        .filter(|s| !s.eq_ignore_ascii_case("Programs"))
        .unwrap_or_default()
        .to_string()
}

/// Open a .lnk shortcut via the shell (no window focus needed on us) and
/// record the launch for frecency ranking.
pub fn launch(path: &str) {
    unsafe {
        let _ = windows::Win32::UI::Shell::ShellExecuteW(windows::Win32::Foundation::HWND(std::ptr::null_mut()), windows::core::w!("open"), &windows::core::HSTRING::from(path), windows::core::PCWSTR::null(), windows::core::PCWSTR::null(), windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL);
    }
    let mut h = history().lock().unwrap();
    h.record(path, now_secs());
    HIST_GEN.fetch_add(1, Ordering::SeqCst);
    let dir = crate::paths::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    crate::store::save(&dir, "launcher-history", &*h);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Typing fast must never block the caller, and only the final text may
    /// reach the index (earlier prefixes are superseded inside the debounce).
    #[test]
    fn hits_never_block_and_debounce_to_the_final_query() {
        let started = Instant::now();
        for q in ["w", "wi", "win", "wind", "windo", "window"] {
            let t = Instant::now();
            let _ = hits(q);
            assert!(t.elapsed() < Duration::from_millis(100), "hits({q}) blocked for {:?}", t.elapsed());
        }
        // All six calls landed well inside one debounce window.
        assert!(started.elapsed() < DEBOUNCE, "typing simulation outran the debounce");
        let (lock, _) = shared();
        let seq_before = lock.lock().unwrap().seq;
        let t0 = Instant::now();
        loop {
            let g = lock.lock().unwrap();
            if g.done_q == "window" {
                // At most one finished query for the whole burst (a prior
                // test-run query may have bumped seq once already).
                assert!(g.seq <= seq_before + 1, "intermediate prefixes were queried: seq {} -> {}", seq_before, g.seq);
                break;
            }
            drop(g);
            assert!(t0.elapsed() < Duration::from_secs(10), "debounced query never finished");
            std::thread::sleep(Duration::from_millis(20));
        }
        // Finished rows now merge into the hits for that exact query.
        assert!(hits("window").len() <= 12);
    }
}
