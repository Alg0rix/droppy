//! AI coding-tool usage: Claude Code, Codex and OpenCode.
//!
//! Rules this module keeps:
//! * Credentials are only read inside the worker thread and never stored in
//!   shared state, logged, or put in an error message.
//! * Nothing is ever refreshed or written. Claude Code's and Codex's refresh
//!   tokens rotate, so a refresh from here could sign the CLI out. The files
//!   are re-read on every poll to pick up the CLI's own refreshes; an expired
//!   token just shows "open Claude Code to sign in again".
//! * A token is only sent to its own provider's host (`api.anthropic.com`).
//! * Codex needs no token at all: it records its rate limits in its session
//!   logs. OpenCode usage comes from its local database.
//!
//! Polling starts the first time the Usage tab is opened.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

/// One rate-limit window (e.g. the 5-hour or the weekly one).
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub used_pct: f32,
    /// Unix seconds when the window resets.
    pub resets_at: Option<i64>,
}

impl Window {
    /// Used percent as of `now`: a window whose reset time has passed is empty.
    pub fn used_now(&self, now_secs: i64) -> f32 {
        match self.resets_at {
            Some(r) if r <= now_secs => 0.0,
            _ => self.used_pct.clamp(0.0, 100.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Limits {
    pub plan: String,
    /// 5-hour window.
    pub primary: Option<Window>,
    /// Weekly window.
    pub secondary: Option<Window>,
    /// Why there is nothing to show ("Not signed in", ...), when empty.
    pub note: String,
    /// When the numbers were recorded (Unix seconds), for local sources.
    pub as_of: Option<i64>,
}

impl Limits {
    pub fn has_data(&self) -> bool {
        self.primary.is_some() || self.secondary.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spend {
    pub label: String,
    pub tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OpenCode {
    pub today_tokens: u64,
    pub today_cost: f64,
    pub week_tokens: u64,
    pub week_cost: f64,
    /// Busiest models this week, most tokens first.
    pub top: Vec<Spend>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UsageView {
    pub claude: Limits,
    pub codex: Limits,
    pub opencode: OpenCode,
    /// A first poll has finished.
    pub loaded: bool,
    pub refreshing: bool,
}

static STATE: Mutex<Option<UsageView>> = Mutex::new(None);
static REVISION: AtomicU32 = AtomicU32::new(0);
static RUNNING: AtomicBool = AtomicBool::new(false);

fn publish(f: impl FnOnce(&mut UsageView)) {
    if let Ok(mut s) = STATE.lock() {
        f(s.get_or_insert_with(UsageView::default));
        REVISION.fetch_add(1, Ordering::Release);
    }
}

pub fn revision() -> u32 {
    REVISION.load(Ordering::Acquire)
}

pub fn snapshot() -> UsageView {
    STATE.lock().ok().and_then(|s| s.clone()).unwrap_or_default()
}

/// Refresh all three sources on a background thread (no-op while one runs).
pub fn refresh() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    publish(|v| v.refreshing = true);
    std::thread::spawn(|| {
        let claude = read_claude();
        publish(|v| v.claude = claude);
        let codex = read_codex();
        publish(|v| v.codex = codex);
        let opencode = read_opencode();
        publish(|v| {
            v.opencode = opencode;
            v.loaded = true;
            v.refreshing = false;
        });
        RUNNING.store(false, Ordering::SeqCst);
    });
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

// ----------------------------------------------------------------- formatting

/// "2h 14m", "3d 4h", "now" for the time left until `resets_at`.
pub fn reset_text(resets_at: i64, now_secs: i64) -> String {
    let left = resets_at - now_secs;
    if left <= 0 {
        return "now".into();
    }
    let (d, h, m) = (left / 86_400, left % 86_400 / 3_600, left % 3_600 / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{}m", m.max(1))
    }
}

/// 1_234_567 -> "1.2M".
pub fn compact_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}K", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

// --------------------------------------------------------------------- Claude

const CLAUDE_HOST: &str = "api.anthropic.com";
const CLAUDE_USAGE: &str = "https://api.anthropic.com/api/oauth/usage";

fn claude_credentials_path() -> Option<PathBuf> {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).or_else(|| home().map(|h| h.join(".claude")))?;
    Some(dir.join(".credentials.json"))
}

/// Parse an ISO-8601 timestamp to Unix seconds.
fn iso_secs(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp())
}

fn claude_window(v: &Value) -> Option<Window> {
    let used = v.get("utilization")?.as_f64()? as f32;
    Some(Window { used_pct: used, resets_at: v.get("resets_at").and_then(|r| r.as_str()).and_then(iso_secs) })
}

/// `/api/oauth/usage` body -> (5-hour, weekly).
pub fn parse_claude_usage(body: &str) -> (Option<Window>, Option<Window>) {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return (None, None) };
    (v.get("five_hour").and_then(claude_window), v.get("seven_day").and_then(claude_window))
}

/// The plan label from the credentials' `subscriptionType` ("max" -> "Max").
fn plan_label(raw: &str) -> String {
    let mut c = raw.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn read_claude() -> Limits {
    let mut out = Limits::default();
    let Some(text) = claude_credentials_path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        out.note = "Claude Code isn't signed in".into();
        return out;
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        out.note = "Couldn't read Claude's sign-in".into();
        return out;
    };
    let oauth = &v["claudeAiOauth"];
    out.plan = oauth["subscriptionType"].as_str().map(plan_label).unwrap_or_default();
    let (Some(token), expires) = (oauth["accessToken"].as_str(), oauth["expiresAt"].as_i64().unwrap_or(0)) else {
        out.note = "Claude Code isn't signed in with a subscription".into();
        return out;
    };
    if expires > 0 && expires <= chrono::Utc::now().timestamp_millis() {
        // Never refresh from here: that rotates the CLI's token. Claude Code
        // renews it itself the next time it runs.
        out.note = "Open Claude Code to sign in again".into();
        return out;
    }
    if !crate::http::host_is(CLAUDE_USAGE, CLAUDE_HOST) {
        return out;
    }
    let auth = format!("Bearer {token}");
    match crate::http::get(CLAUDE_USAGE, &[("Authorization", &auth), ("anthropic-beta", "oauth-2025-04-20"), ("Accept", "application/json")], 256 * 1024) {
        Ok(body) => {
            let (p, s) = parse_claude_usage(&String::from_utf8_lossy(&body));
            if p.is_none() && s.is_none() {
                out.note = "Usage isn't available for this plan".into();
            }
            out.primary = p;
            out.secondary = s;
        }
        Err(401 | 403) => out.note = "Open Claude Code to sign in again".into(),
        Err(429) => out.note = "Rate limited \u{2014} try again in a minute".into(),
        Err(0) => out.note = "Offline".into(),
        Err(_) => out.note = "Usage is unavailable right now".into(),
    }
    out
}

// ---------------------------------------------------------------------- Codex

fn codex_sessions_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".codex")))?;
    Some(dir.join("sessions"))
}

fn codex_window(v: &Value) -> Option<Window> {
    let used = v.get("used_percent")?.as_f64()? as f32;
    Some(Window { used_pct: used, resets_at: v.get("resets_at").and_then(|r| r.as_i64()) })
}

/// One session-log line -> limits, when it carries `rate_limits`.
pub fn parse_codex_line(line: &str) -> Option<Limits> {
    if !line.contains("rate_limits") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let rl = v.pointer("/payload/rate_limits")?;
    let primary = rl.get("primary").and_then(codex_window);
    let secondary = rl.get("secondary").and_then(codex_window);
    if primary.is_none() && secondary.is_none() {
        return None;
    }
    Some(Limits {
        plan: rl.get("plan_type").and_then(|p| p.as_str()).map(plan_label).unwrap_or_default(),
        primary,
        secondary,
        note: String::new(),
        as_of: v.get("timestamp").and_then(|t| t.as_str()).and_then(iso_secs),
    })
}

/// Newest session logs first (their names embed the start time).
fn newest_rollouts(root: &Path, want: usize) -> Vec<PathBuf> {
    let sorted_desc = |dir: &Path| {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
        v.sort();
        v.reverse();
        v
    };
    let mut out = Vec::new();
    'walk: for year in sorted_desc(root) {
        for month in sorted_desc(&year) {
            for day in sorted_desc(&month) {
                for f in sorted_desc(&day) {
                    if f.extension().is_some_and(|e| e == "jsonl") {
                        out.push(f);
                        if out.len() >= want {
                            break 'walk;
                        }
                    }
                }
            }
        }
    }
    out
}

/// Last `max` bytes of a file as text (a partial first line is dropped).
fn read_tail(path: &Path, max: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(max);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    Some(if start > 0 { text.split_once('\n').map(|x| x.1.to_string()).unwrap_or_default() } else { text })
}

fn read_codex() -> Limits {
    let Some(root) = codex_sessions_dir() else {
        return Limits { note: "Codex isn't installed".into(), ..Default::default() };
    };
    for f in newest_rollouts(&root, 8) {
        if let Some(tail) = read_tail(&f, 768 * 1024) {
            if let Some(l) = tail.lines().rev().find_map(parse_codex_line) {
                return l;
            }
        }
    }
    Limits { note: "Run Codex once to see its limits".into(), ..Default::default() }
}

// ------------------------------------------------------------------- OpenCode

fn opencode_db_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".local").join("share")))?;
    Some(base.join("opencode").join("opencode.db"))
}

const SPEND_SQL: &str = "SELECT COALESCE(json_extract(data,'$.providerID'),'?'), COALESCE(json_extract(data,'$.modelID'),'?'), \
     COALESCE(SUM(json_extract(data,'$.tokens.total')),0), COALESCE(SUM(json_extract(data,'$.cost')),0.0) \
     FROM message WHERE time_created >= ?1 AND json_extract(data,'$.role') = 'assistant' GROUP BY 1, 2";

fn spend_since(db: &crate::sqlite::Conn, since_ms: i64) -> Result<Vec<Spend>, crate::sqlite::Error> {
    let rows = db.query_i64(SPEND_SQL, since_ms)?;
    Ok(rows
        .into_iter()
        .filter(|r| r.len() == 4)
        .map(|r| Spend { label: format!("{}/{}", r[0], r[1]), tokens: r[2].parse::<f64>().unwrap_or(0.0).max(0.0) as u64, cost: r[3].parse::<f64>().unwrap_or(0.0).max(0.0) })
        .collect())
}

/// Fold per-model rows into totals and the busiest few.
pub fn summarize(today: &[Spend], week: &[Spend]) -> OpenCode {
    let mut top = week.to_vec();
    top.sort_by_key(|e| std::cmp::Reverse(e.tokens));
    top.truncate(3);
    OpenCode {
        today_tokens: today.iter().map(|s| s.tokens).sum(),
        today_cost: today.iter().map(|s| s.cost).sum(),
        week_tokens: week.iter().map(|s| s.tokens).sum(),
        week_cost: week.iter().map(|s| s.cost).sum(),
        top,
        note: String::new(),
    }
}

fn read_opencode() -> OpenCode {
    use crate::sqlite::{Conn, Error};
    let Some(path) = opencode_db_path().filter(|p| p.exists()) else {
        return OpenCode { note: "OpenCode isn't installed".into(), ..Default::default() };
    };
    // Read-only: opencode owns this database and is usually running on it.
    let db = match Conn::open_readonly(&path) {
        Ok(db) => db,
        Err(Error::Unavailable) => return OpenCode { note: "Needs Windows 10 1809 or newer".into(), ..Default::default() },
        Err(_) => return OpenCode { note: "OpenCode's database is busy".into(), ..Default::default() },
    };
    let now = chrono::Local::now();
    let midnight = now.date_naive().and_hms_opt(0, 0, 0).and_then(|d| d.and_local_timezone(chrono::Local).single()).map(|d| d.timestamp_millis()).unwrap_or(0);
    let week = now.timestamp_millis() - 7 * 86_400_000;
    match (spend_since(&db, midnight), spend_since(&db, week)) {
        (Ok(t), Ok(w)) => summarize(&t, &w),
        _ => OpenCode { note: "Couldn't read OpenCode's usage".into(), ..Default::default() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_usage_body() {
        let body = r#"{"five_hour":{"utilization":23.5,"resets_at":"2026-10-24T15:00:00+00:00"},
                       "seven_day":{"utilization":61.0,"resets_at":"2026-10-28T09:00:00Z"},"seven_day_opus":null}"#;
        let (p, s) = parse_claude_usage(body);
        assert_eq!(p.unwrap().used_pct, 23.5);
        let s = s.unwrap();
        assert_eq!(s.used_pct, 61.0);
        assert!(s.resets_at.unwrap() > 1_700_000_000);
        assert_eq!(parse_claude_usage("nope"), (None, None));
        assert_eq!(parse_claude_usage(r#"{"five_hour":null}"#), (None, None));
    }

    #[test]
    fn codex_line_with_limits() {
        let line = r#"{"timestamp":"2026-10-22T14:16:33.641Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":17.0,"window_minutes":300,"resets_at":1792694468},"secondary":{"used_percent":27.0,"window_minutes":10080,"resets_at":1793100839},"plan_type":"plus"}}}"#;
        let l = parse_codex_line(line).unwrap();
        assert_eq!(l.plan, "Plus");
        assert_eq!(l.primary.unwrap().used_pct, 17.0);
        assert_eq!(l.secondary.unwrap().resets_at, Some(1793100839));
        assert!(l.as_of.is_some());
        assert!(parse_codex_line(r#"{"payload":{"rate_limits":{"primary":null,"secondary":null}}}"#).is_none());
        assert!(parse_codex_line(r#"{"payload":{"type":"agent_message"}}"#).is_none());
    }

    #[test]
    fn expired_windows_read_as_empty() {
        let w = Window { used_pct: 80.0, resets_at: Some(1000) };
        assert_eq!(w.used_now(999), 80.0);
        assert_eq!(w.used_now(1000), 0.0);
        assert_eq!(Window { used_pct: 130.0, resets_at: None }.used_now(0), 100.0);
    }

    #[test]
    fn friendly_numbers() {
        assert_eq!(reset_text(1000 + 3 * 86_400 + 4 * 3600, 1000), "3d 4h");
        assert_eq!(reset_text(1000 + 2 * 3600 + 14 * 60, 1000), "2h 14m");
        assert_eq!(reset_text(1030, 1000), "1m");
        assert_eq!(reset_text(900, 1000), "now");
        assert_eq!(compact_count(950), "950");
        assert_eq!(compact_count(12_345), "12.3K");
        assert_eq!(compact_count(2_500_000), "2.5M");
    }

    #[test]
    fn opencode_summary_ranks_models() {
        let s = |l: &str, t, c| Spend { label: l.into(), tokens: t, cost: c };
        let week = vec![s("a/x", 10, 0.1), s("b/y", 500, 0.5), s("c/z", 50, 0.2), s("d/w", 20, 0.0)];
        let o = summarize(&[s("b/y", 100, 0.2)], &week);
        assert_eq!((o.today_tokens, o.week_tokens), (100, 580));
        assert_eq!(o.top.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), ["b/y", "c/z", "d/w"]);
    }

    #[test]
    fn claude_requests_go_only_to_anthropic() {
        assert!(crate::http::host_is(CLAUDE_USAGE, CLAUDE_HOST));
    }
}

#[cfg(all(test, windows))]
mod live {
    /// Reads real local data: `cargo test -p teardrop live_usage -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_usage() {
        let now = chrono::Utc::now().timestamp();
        let show = |n: &str, l: &super::Limits| {
            let w = |w: &Option<super::Window>| {
                w.as_ref().map(|w| format!("{:.0}% (resets {})", w.used_now(now), super::reset_text(w.resets_at.unwrap_or(0), now))).unwrap_or("-".into())
            };
            println!("{n}: plan={} 5h={} week={} note={:?}", l.plan, w(&l.primary), w(&l.secondary), l.note);
        };
        show("claude", &super::read_claude());
        show("codex", &super::read_codex());
        // All-time check that the system SQLite really reads the live database.
        if let Some(path) = super::opencode_db_path().filter(|p| p.exists()) {
            let db = crate::sqlite::Conn::open_readonly(&path).expect("open opencode db");
            let all = super::spend_since(&db, 0).expect("query");
            println!("opencode all-time: {} models, {} tokens", all.len(), all.iter().map(|s| s.tokens).sum::<u64>());
            assert!(!all.is_empty());
        }
        let o = super::read_opencode();
        println!(
            "opencode: today={} tok ${:.2}, week={} tok ${:.2}, top={:?}, note={:?}",
            o.today_tokens,
            o.today_cost,
            o.week_tokens,
            o.week_cost,
            o.top.iter().map(|s| (&s.label, s.tokens)).collect::<Vec<_>>(),
            o.note
        );
    }
}
