//! Installed-app launcher index for the Windows page search field.
//!
//! App hits come from `shellcatalog`: every packaged app in shell:AppsFolder
//! plus every .lnk under both Start Menu roots, with Settings pages and
//! Control Panel tasks merged in. On top of those, `winsearch` layers real
//! Windows Search index hits — the same catalog Start Menu queries. That
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

/// (display name, launch path) for every installed app: packaged apps from
/// shell:AppsFolder plus Start Menu .lnks. Builds inside `shellcatalog`'s
/// OnceLock on the worker thread — never call this before that lands.
pub fn installed_apps() -> &'static [(String, String)] {
    crate::shellcatalog::apps()
}

/// Icon source for Settings-page hits: the Settings app's AppsFolder item.
pub const SETTINGS_ICON: &str = r"shell:AppsFolder\windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel";

/// One launcher hit: display name, a path to open (lnk, file, uri, or shell
/// parsing path), what to draw the icon from, and the right-side tag.
#[derive(Clone)]
pub struct Hit {
    pub name: String,
    pub path: String,
    pub icon: String,
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
    // Warm each shell catalog, waking the UI as each lands so hits widen as
    // more sources come online. Settings/tasks are slow (seconds, cold) and
    // get their own STA thread so index queries never wait behind them.
    std::thread::Builder::new()
        .name("shell-catalog".into())
        .spawn(|| {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            let _ = crate::shellcatalog::tasks();
            notify();
            let _ = crate::shellcatalog::settings();
            notify();
        })
        .ok();
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

/// Catalog-readiness bits folded into the `hits()` cache key: a catalog
/// landing mid-query must recompute, not serve stale hits.
const READY_APPS: u8 = 1;
const READY_SETTINGS: u8 = 2;
const READY_TASKS: u8 = 4;

fn ready() -> u8 {
    (crate::shellcatalog::loaded_apps().is_some() as u8) * READY_APPS
        + (crate::shellcatalog::loaded_settings().is_some() as u8) * READY_SETTINGS
        + (crate::shellcatalog::loaded_tasks().is_some() as u8) * READY_TASKS
}

/// `hits()` runs every layout frame, so results are cached against the
/// inputs that could change them: query, finished-search seq, which shell
/// catalogs have landed, history generation.
struct LastHits {
    q: String,
    seq: u64,
    ready: u8,
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
    let r = ready();
    let gen = HIST_GEN.load(Ordering::SeqCst);
    let last = LAST.get_or_init(|| Mutex::new(LastHits { q: String::new(), seq: 0, ready: 0, gen: 0, hits: Vec::new() }));
    let mut c = last.lock().unwrap();
    if c.q == q && c.seq == seq && c.ready == r && c.gen == gen {
        return c.hits.clone();
    }
    let out = compute(&q, r, rows.as_deref().unwrap_or(&[]));
    *c = LastHits { q, seq, ready: r, gen, hits: out.clone() };
    out
}

fn compute(q: &str, ready: u8, rows: &[crate::winsearch::Row]) -> Vec<Hit> {
    let mut out: Vec<Hit> = Vec::new();
    let now = now_secs();
    let hist = history().lock().unwrap();
    // Until a catalog lands there is nothing to filter (never block the UI
    // thread on shell enumeration or the settings XML walk).
    let apps: &[(String, String)] = if ready & READY_APPS != 0 { crate::shellcatalog::loaded_apps().unwrap_or(&[]) } else { &[] };
    let settings = if ready & READY_SETTINGS != 0 { crate::shellcatalog::loaded_settings().unwrap_or(&[]) } else { &[] };
    let tasks = if ready & READY_TASKS != 0 { crate::shellcatalog::loaded_tasks().unwrap_or(&[]) } else { &[] };
    // CmdPal drops fuzzy-only hits for very short queries — too noisy.
    let short = q.chars().count() <= 2;
    let mut seen = std::collections::HashSet::new();
    let mut scored: Vec<(u32, String, String)> = Vec::new();
    for (n, p) in apps {
        if !seen.insert(n.to_lowercase()) {
            continue;
        }
        let score = search::rank_with_keywords(q, n, &app_subtitle(p), app_keywords(n), hist.weight(p, now));
        if score == 0 || (short && search::tier_of(score) < Tier::WordOrAcronym) {
            continue;
        }
        scored.push((score, n.clone(), p.clone()));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    for (_, n, p) in scored.into_iter().take(6) {
        out.push(Hit { name: n, icon: p.clone(), path: p, tag: "app".to_string() });
    }
    // Settings pages and Control Panel tasks are noisy catalogs — they only
    // show up on word-boundary-or-better matches — then merge by score.
    let dup = |cand: &Hit, list: &[Hit]| list.iter().any(|h| h.path.eq_ignore_ascii_case(&cand.path) || h.name.eq_ignore_ascii_case(&cand.name));
    let mut mid: Vec<(u32, Hit)> = Vec::new();
    for e in settings {
        let score = search::rank_with_keywords(q, &e.name, "", &e.keywords, hist.weight(&e.uri, now));
        if search::tier_of(score) < Tier::WordOrAcronym {
            continue;
        }
        let h = Hit { name: e.name.clone(), path: e.uri.clone(), icon: SETTINGS_ICON.to_string(), tag: "settings".to_string() };
        if !dup(&h, &out) {
            mid.push((score, h));
        }
    }
    for (n, p) in tasks {
        let score = search::rank(q, n, "", hist.weight(p, now));
        if search::tier_of(score) < Tier::WordOrAcronym {
            continue;
        }
        let h = Hit { name: n.clone(), icon: p.clone(), path: p.clone(), tag: "control panel".to_string() };
        if !dup(&h, &out) {
            mid.push((score, h));
        }
    }
    mid.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase())));
    let mut taken: Vec<Hit> = Vec::new();
    for (_, h) in mid {
        if taken.len() == 4 {
            break;
        }
        if !dup(&h, &taken) {
            taken.push(h);
        }
    }
    out.extend(taken);
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
        scored_rows.push((score, Hit { name: r.name.clone(), icon: r.path.clone(), path: r.path.clone(), tag }));
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

/// Extra match phrases for well-known apps — what Start Menu finds them by
/// that their display name doesn't say ("cmd" -> Command Prompt).
fn app_keywords(name: &str) -> &'static str {
    for (n, kw) in [
        ("Command Prompt", "cmd;terminal;console;shell;command line"),
        ("Windows PowerShell", "terminal;console;shell;ps;pwsh"),
        ("PowerShell", "terminal;console;shell;ps;pwsh"),
        ("Terminal", "cmd;console;shell;powershell;wt;command line"),
        ("File Explorer", "files;folders;explorer"),
        ("Task Manager", "processes;performance;taskmgr"),
        ("Settings", "preferences;options;control panel"),
        ("Control Panel", "settings;control"),
        ("Calculator", "calc;math"),
        ("Notepad", "text editor;txt"),
        ("Snipping Tool", "screenshot;screen capture;snip;print screen"),
        ("Paint", "draw;mspaint"),
        ("Registry Editor", "regedit"),
        ("Microsoft Edge", "browser;web;internet"),
        ("Photos", "images;pictures;viewer"),
        ("Media Player", "music;video;player"),
        ("Clock", "alarm;timer;stopwatch"),
        ("Camera", "webcam"),
        ("Remote Desktop Connection", "rdp;mstsc;remote"),
    ] {
        if n.eq_ignore_ascii_case(name) {
            return kw;
        }
    }
    ""
}

/// Open a hit via the shell (no window focus needed on us) and record the
/// launch for frecency ranking. `::`/`shell:::` parsing paths (Control Panel
/// tasks) go through a PIDL — ShellExecuteW can't resolve them — everything
/// else (shell:AppsFolder\…, ms-settings:, windowsdefender:, files, .lnk)
/// stays on ShellExecuteW.
pub fn launch(path: &str) {
    unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx};
        use windows::Win32::UI::Shell::{Common::ITEMIDLIST, ShellExecuteExW, ShellExecuteW, SHParseDisplayName, SEE_MASK_IDLIST, SHELLEXECUTEINFOW};
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::{w, HSTRING, PCWSTR};
        if path.starts_with("::") || path.starts_with("shell:::") {
            // Control Panel task paths can't be re-parsed — the catalog kept
            // their absolute PIDLs; those stay table-owned, a parsed one is freed.
            let mut parsed: *mut ITEMIDLIST = std::ptr::null_mut();
            let pidl = {
                let p = crate::shellcatalog::task_pidl(path);
                if !p.is_null() {
                    p
                } else if SHParseDisplayName(&HSTRING::from(path), None::<&IBindCtx>, &mut parsed, 0, None).is_ok() {
                    parsed
                } else {
                    std::ptr::null_mut()
                }
            };
            if !pidl.is_null() {
                let verb = w!("open");
                let mut info = SHELLEXECUTEINFOW {
                    cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                    fMask: SEE_MASK_IDLIST,
                    lpVerb: verb,
                    lpIDList: pidl as *mut _,
                    nShow: SW_SHOWNORMAL.0,
                    ..Default::default()
                };
                let _ = ShellExecuteExW(&mut info);
                if !parsed.is_null() {
                    CoTaskMemFree(Some(parsed as *const _));
                }
            }
        } else {
            let _ = ShellExecuteW(HWND(std::ptr::null_mut()), w!("open"), &HSTRING::from(path), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        }
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

    /// The worker holds ONE debounce slot — concurrent tests would overwrite
    /// each other's pending query, so launcher tests run serialized.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Typing fast must never block the caller, and only the final text may
    /// reach the index (earlier prefixes are superseded inside the debounce).
    #[test]
    fn hits_never_block_and_debounce_to_the_final_query() {
        let _guard = TEST_LOCK.lock().unwrap();
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

    /// Live end-to-end: once the worker's shell catalogs and the debounced
    /// index query have landed, `hits("terminal")` should look like Start —
    /// the Terminal app, Settings pages, Command Prompt via keyword alias.
    #[test]
    fn hits_terminal_end_to_end() {
        let _guard = TEST_LOCK.lock().unwrap();
        // Worker warms apps -> settings -> tasks on first use.
        let _ = hits("warm");
        let t0 = Instant::now();
        loop {
            if crate::shellcatalog::loaded_apps().is_some()
                && crate::shellcatalog::loaded_settings().is_some()
                && crate::shellcatalog::loaded_tasks().is_some()
            {
                break;
            }
            assert!(t0.elapsed() < Duration::from_secs(60), "shell catalogs never landed");
            std::thread::sleep(Duration::from_millis(50));
        }
        let (lock, _) = shared();
        let t1 = Instant::now();
        loop {
            let _ = hits("terminal");
            if lock.lock().unwrap().done_q == "terminal" {
                break;
            }
            assert!(t1.elapsed() < Duration::from_secs(15), "index query never finished");
            std::thread::sleep(Duration::from_millis(50));
        }
        let list = hits("terminal");
        eprintln!("hits(\"terminal\"): {} hits", list.len());
        for h in &list {
            eprintln!("  [{}] {} -> {}", h.tag, h.name, h.path);
        }
        assert!(list.iter().any(|h| h.tag == "app"), "no app hits for \"terminal\"");
    }
}
