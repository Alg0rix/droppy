//! Calendar events from iCal feeds (Google "secret address in iCal format",
//! Outlook "published calendar" ICS, any other `.ics` link).
//!
//! * The feed URL is a secret: it lives only in `Settings` and is never put in
//!   the view model, a log line or an error message.
//! * Feed text is untrusted: join links are only offered for known meeting
//!   hosts over https.
//! * Recurring events are expanded by our own small RRULE engine (`recur.rs`) into a
//!   window around now; time zones come from Windows itself (`tz.rs`), so there is
//!   no bundled time-zone database. An unknown zone falls back to the PC's own.
//! * The last good result is cached so the agenda shows at startup and offline.

use crate::tz::{self, Zone};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

/// How far around "now" occurrences are expanded.
const PAST_DAYS: i64 = 62;
const AHEAD_DAYS: i64 = 180;
const FEED_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Join {
    pub platform: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub uid: String,
    /// Unix seconds.
    pub start: i64,
    pub end: i64,
    pub title: String,
    pub location: String,
    #[serde(default)]
    pub description: String,
    pub all_day: bool,
    pub join: Option<Join>,
}

impl Event {
    /// Stable key for "already reminded" bookkeeping.
    pub fn key(&self) -> String {
        format!("{}@{}", self.uid, self.start)
    }
}

// --------------------------------------------------------------- ICS parsing

/// Undo RFC 5545 line folding on raw bytes (a fold can split a UTF-8 char).
pub fn unfold(raw: &[u8]) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let rest = &raw[i..];
        let fold = if rest.starts_with(b"\r\n ") || rest.starts_with(b"\r\n\t") {
            3
        } else if rest.starts_with(b"\n ") || rest.starts_with(b"\n\t") {
            2
        } else {
            0
        };
        if fold > 0 {
            i += fold;
        } else {
            out.push(raw[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Prop {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub value: String,
}

impl Prop {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// `NAME;K=V;K2="v:2":value` -> parts; the first colon outside quotes ends the name/params.
pub fn parse_line(line: &str) -> Option<Prop> {
    let mut in_quote = false;
    let mut split = None;
    for (i, ch) in line.char_indices() {
        match ch {
            '"' => in_quote = !in_quote,
            ':' if !in_quote => {
                split = Some(i);
                break;
            }
            _ => {}
        }
    }
    let i = split?;
    let (head, value) = (&line[..i], &line[i + 1..]);
    let mut parts = Vec::new();
    let (mut cur, mut q) = (String::new(), false);
    for ch in head.chars() {
        match ch {
            '"' => q = !q,
            ';' if !q => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    parts.push(cur);
    let name = parts.remove(0).to_uppercase();
    let params = parts.iter().filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_uppercase(), v.to_string())).collect();
    Some(Prop { name, params, value: value.to_string() })
}

/// `\n`, `\,`, `\;`, `\\`.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

/// A parsed DTSTART / DTEND / EXDATE value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dt {
    Date(NaiveDate),
    Utc(DateTime<Utc>),
    /// Wall-clock time in a zone (`None` = this PC's zone).
    Zoned(NaiveDateTime, Option<Zone>),
}

fn parse_naive(v: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()
}

/// Resolve a TZID (IANA, Windows name, quoted or not) to a zone; unknown = None (this PC's zone).
pub fn zone_of(tzid: Option<&str>) -> Option<Zone> {
    tz::resolve(tzid?)
}

pub fn parse_dt(p: &Prop) -> Option<Dt> {
    let v = p.value.trim();
    if p.param("VALUE") == Some("DATE") || (v.len() == 8 && !v.contains('T')) {
        return NaiveDate::parse_from_str(v, "%Y%m%d").ok().map(Dt::Date);
    }
    let naive = parse_naive(v)?;
    if v.ends_with('Z') {
        return Some(Dt::Utc(Utc.from_utc_datetime(&naive)));
    }
    Some(Dt::Zoned(naive, zone_of(p.param("TZID"))))
}

/// `P1DT2H30M`, `PT45M`, `P1W` -> seconds.
pub fn parse_duration(v: &str) -> Option<i64> {
    let v = v.trim();
    let (neg, v) = match v.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, v.strip_prefix('+').unwrap_or(v)),
    };
    let v = v.strip_prefix('P')?;
    let (mut total, mut num, mut in_time) = (0i64, String::new(), false);
    for ch in v.chars() {
        match ch {
            'T' => in_time = true,
            '0'..='9' => num.push(ch),
            'W' | 'D' | 'H' | 'M' | 'S' => {
                let n: i64 = std::mem::take(&mut num).parse().ok()?;
                let unit: i64 = match (ch, in_time) {
                    ('W', _) => 604_800,
                    ('D', _) => 86_400,
                    ('H', _) => 3_600,
                    ('M', true) => 60,
                    ('M', false) => return None,
                    _ => 1,
                };
                total = total.checked_add(n.checked_mul(unit)?)?;
            }
            _ => return None,
        }
    }
    Some(if neg { -total } else { total })
}

#[derive(Debug, Clone, Default)]
pub struct RawEvent {
    pub uid: String,
    pub summary: String,
    pub location: String,
    pub description: String,
    pub url: String,
    pub conference: String,
    pub start: Option<Dt>,
    pub end: Option<Dt>,
    pub duration: Option<i64>,
    pub rrule: Option<String>,
    pub exdates: Vec<Dt>,
    pub recurrence_id: Option<Dt>,
    pub cancelled: bool,
}

/// All VEVENTs in a feed (VTIMEZONE and VALARM blocks are skipped).
pub fn parse_ics(raw: &[u8]) -> Vec<RawEvent> {
    let text = unfold(raw);
    let mut out = Vec::new();
    let mut cur: Option<RawEvent> = None;
    let mut skip_depth = 0;
    for line in text.lines() {
        let Some(p) = parse_line(line) else { continue };
        match (p.name.as_str(), p.value.as_str()) {
            ("BEGIN", "VEVENT") if cur.is_none() => cur = Some(RawEvent::default()),
            ("END", "VEVENT") => {
                if let Some(e) = cur.take() {
                    out.push(e);
                }
            }
            ("BEGIN", "VALARM") if cur.is_some() => skip_depth += 1,
            ("END", "VALARM") if skip_depth > 0 => skip_depth -= 1,
            _ if skip_depth > 0 => {}
            _ => {
                let Some(e) = cur.as_mut() else { continue };
                match p.name.as_str() {
                    "UID" => e.uid = p.value.clone(),
                    "SUMMARY" => e.summary = unescape(&p.value),
                    "LOCATION" => e.location = unescape(&p.value),
                    "DESCRIPTION" | "X-ALT-DESC" => {
                        e.description.push(' ');
                        e.description.push_str(&unescape(&p.value));
                    }
                    "URL" => e.url = p.value.clone(),
                    "X-GOOGLE-CONFERENCE" | "X-MICROSOFT-SKYPETEAMSMEETINGURL" => e.conference = p.value.clone(),
                    "DTSTART" => e.start = parse_dt(&p),
                    "DTEND" => e.end = parse_dt(&p),
                    "DURATION" => e.duration = parse_duration(&p.value),
                    "RRULE" => e.rrule = Some(p.value.clone()),
                    "EXDATE" => e.exdates.extend(p.value.split(',').filter_map(|v| parse_dt(&Prop { name: p.name.clone(), params: p.params.clone(), value: v.to_string() }))),
                    "RECURRENCE-ID" => e.recurrence_id = parse_dt(&p),
                    "STATUS" => e.cancelled = p.value.eq_ignore_ascii_case("CANCELLED"),
                    _ => {}
                }
            }
        }
    }
    out
}

// ----------------------------------------------------------------- join links

/// Platform name for a meeting URL we are willing to open, else None.
pub fn meeting_platform(url: &str) -> Option<&'static str> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?.to_lowercase();
    let host = host.split('@').next_back()?;
    let is = |d: &str| host == d || host.ends_with(&format!(".{d}"));
    Some(if is("meet.google.com") {
        "Google Meet"
    } else if is("zoom.us") || is("zoom.com") || is("zoomgov.com") {
        "Zoom"
    } else if is("teams.microsoft.com") || is("teams.live.com") {
        "Teams"
    } else if is("webex.com") {
        "Webex"
    } else if is("whereby.com") {
        "Whereby"
    } else if is("gotomeeting.com") || is("goto.com") {
        "GoTo"
    } else {
        return None;
    })
}

/// First known meeting link in any of the fields (URLs end at whitespace, `<>"')`).
pub fn find_join(fields: &[&str]) -> Option<Join> {
    for f in fields {
        let f = f.replace("&amp;", "&");
        let mut rest = f.as_str();
        while let Some(i) = rest.find("https://") {
            let tail = &rest[i..];
            let end = tail.find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'' | ')' | '\\')).unwrap_or(tail.len());
            let url = tail[..end].trim_end_matches(['.', ',', ';']);
            if let Some(platform) = meeting_platform(url) {
                return Some(Join { platform: platform.to_string(), url: url.to_string() });
            }
            rest = &tail[8.min(tail.len())..];
        }
    }
    None
}

// ------------------------------------------------------------------ expansion

/// A value as Unix seconds. Dates are local midnight; a time without a zone
/// uses `fallback`, else this PC's zone.
fn to_secs(d: Dt, fallback: Option<Zone>) -> Option<i64> {
    match d {
        Dt::Utc(u) => Some(u.timestamp()),
        Dt::Date(date) => tz::to_utc_secs(date.and_hms_opt(0, 0, 0)?, None),
        Dt::Zoned(n, z) => tz::to_utc_secs(n, z.or(fallback)),
    }
}

/// Expand feed events into concrete instances overlapping `[from, to]` (Unix seconds).
/// Strip HTML markup / angle-bracketed URLs and collapse whitespace so
/// `Event::description` is clean for the detail card.
pub fn clean_description(raw: &str) -> String {
    let s = raw
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<BR>", "\n")
        .replace("</p>", "\n")
        .replace("</div>", "\n")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let after = &rest[lt + 1..];
        if let Some(gt) = after.find('>') {
            let inner = &after[..gt];
            if inner.starts_with("https://") || inner.starts_with("http://") {
                out.push_str(inner);
            }
            rest = &after[gt + 1..];
        } else {
            out.push('<');
            rest = after;
        }
    }
    out.push_str(rest);
    let cleaned = out.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
    cleaned.chars().take(600).collect()
}

pub fn expand(raw: &[RawEvent], from: i64, to: i64) -> Vec<Event> {
    // Overrides (RECURRENCE-ID) replace or cancel a single instance of their series.
    let mut overrides: HashMap<(String, i64), &RawEvent> = HashMap::new();
    for e in raw.iter().filter(|e| e.recurrence_id.is_some()) {
        if let Some(rid) = e.recurrence_id.and_then(|d| to_secs(d, None)) {
            overrides.insert((e.uid.clone(), rid), e);
        }
    }
    let mut out = Vec::new();
    for e in raw.iter().filter(|e| e.recurrence_id.is_none()) {
        let Some(start_dt) = e.start else { continue };
        let all_day = matches!(start_dt, Dt::Date(_));
        // The zone the event's wall-clock times live in (None = this PC's).
        let zone = match start_dt {
            Dt::Zoned(_, z) => z,
            Dt::Utc(_) => Some(Zone::Utc),
            Dt::Date(_) => None,
        };
        let Some(start) = to_secs(start_dt, None) else { continue };
        // Length: DTEND, else DURATION, else a day (dates) / nothing (date-times).
        let len = match (e.end.and_then(|d| to_secs(d, zone)), e.duration) {
            (Some(end), _) => (end - start).max(0),
            (None, Some(d)) => d.max(0),
            (None, None) => {
                if all_day {
                    86_400
                } else {
                    0
                }
            }
        };
        let join = find_join(&[&e.conference, &e.url, &e.location, &e.description]);
        let make = |s: i64, e_: &RawEvent| Event {
            uid: e.uid.clone(),
            start: s,
            end: s + len,
            title: if e_.summary.is_empty() { "(No title)".into() } else { e_.summary.clone() },
            location: e_.location.clone(),
            description: clean_description(&e_.description),
            all_day,
            join: join.clone(),
        };
        let overlaps = |s: i64| s + len.max(1) > from && s <= to;

        let Some(rule) = &e.rrule else {
            if !e.cancelled && overlaps(start) {
                out.push(make(start, e));
            }
            continue;
        };
        // Recurring: walk from DTSTART (COUNT counts from the first instance) in
        // wall-clock time, converting each occurrence so DST is applied per date.
        let exdates: Vec<i64> = e.exdates.iter().filter_map(|d| to_secs(*d, zone)).collect();
        let wall_start = match start_dt {
            Dt::Zoned(n, _) => n,
            Dt::Utc(u) => u.naive_utc(),
            Dt::Date(d) => d.and_hms_opt(0, 0, 0).unwrap_or_default(),
        };
        let horizon = DateTime::from_timestamp(to + 2 * 86_400, 0).unwrap_or_default().naive_utc();
        let convert = |n: NaiveDateTime| tz::to_utc_secs(n, zone);
        let starts: Vec<i64> = match crate::recur::occurrences(wall_start, rule, horizon, &convert) {
            Some(v) => v.into_iter().filter_map(convert).collect(),
            // A rule we can't read still shows its first instance.
            None => vec![start],
        };
        for s in starts {
            if exdates.contains(&s) {
                continue;
            }
            match overrides.get(&(e.uid.clone(), s)) {
                Some(o) if o.cancelled => {}
                Some(o) => {
                    let ns = o.start.and_then(|d| to_secs(d, None)).unwrap_or(s);
                    if overlaps(ns) {
                        let nlen = o.end.and_then(|d| to_secs(d, None)).map(|en| (en - ns).max(0)).unwrap_or(len);
                        let mut ev = make(ns, o);
                        ev.end = ns + nlen;
                        out.push(ev);
                    }
                }
                None if !e.cancelled && overlaps(s) => out.push(make(s, e)),
                None => {}
            }
        }
    }
    out.sort_by_key(|e| (e.start, e.end));
    out.dedup_by(|a, b| a.uid == b.uid && a.start == b.start);
    out
}

// ---------------------------------------------------------------------- view

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FeedState {
    #[default]
    Idle,
    Loading,
    Ok,
    /// Short, URL-free reason (see `Failure`).
    Failed,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CalendarView {
    pub events: Vec<Event>,
    pub state: FeedState,
    pub error: String,
    pub updated: Option<i64>,
}

impl CalendarView {
    /// Next meeting that hasn't ended: ongoing first, timed events only.
    pub fn next(&self, now: i64) -> Option<&Event> {
        self.events.iter().find(|e| !e.all_day && e.end.max(e.start + 60) > now && e.start <= now + 24 * 3600)
    }

    /// Upcoming timed events, then today's all-day ones, soonest first.
    /// All events occurring on the local calendar date `(year, month, day)`.
    pub fn events_on_day(&self, year: i32, month: u32, day: u32) -> Vec<&Event> {
        let Some(date) = NaiveDate::from_ymd_opt(year, month, day) else {
            return Vec::new();
        };
        let (lo, hi) = local_span_bounds(date, date);
        self.events.iter().filter(|e| may_overlap(e, lo, hi) && event_on_date(e, date)).collect()
    }

    pub fn agenda(&self, now: i64, limit: usize) -> Vec<&Event> {
        self.events.iter().filter(|e| e.end.max(e.start + 60) > now).take(limit).collect()
    }
}

static STATE: Mutex<Option<CalendarView>> = Mutex::new(None);
static REVISION: AtomicU32 = AtomicU32::new(0);
static RUNNING: AtomicBool = AtomicBool::new(false);

fn publish(f: impl FnOnce(&mut CalendarView)) {
    if let Ok(mut s) = STATE.lock() {
        f(s.get_or_insert_with(CalendarView::default));
        REVISION.fetch_add(1, Ordering::Release);
    }
}

pub fn revision() -> u32 {
    REVISION.load(Ordering::Acquire)
}

pub fn snapshot() -> CalendarView {
    STATE.lock().ok().and_then(|s| s.clone()).unwrap_or_default()
}

/// Accept `https://` links (and `webcal://`, which is https in disguise).
pub fn normalize_feed_url(s: &str) -> Option<String> {
    let s = s.trim();
    let url = if let Some(r) = s.strip_prefix("webcal://") {
        format!("https://{r}")
    } else if s.starts_with("https://") {
        s.to_string()
    } else {
        return None;
    };
    (url.len() < 2048 && !url.contains(char::is_whitespace) && url.len() > 12).then_some(url)
}

#[derive(Debug, PartialEq)]
enum Failure {
    Offline,
    TooLarge,
    NotCalendar,
    Rejected,
}

impl Failure {
    fn text(&self) -> &'static str {
        match self {
            Failure::Offline => "Couldn't reach the calendar",
            Failure::TooLarge => "Calendar feed is too large",
            Failure::NotCalendar => "That link isn't an iCal feed",
            Failure::Rejected => "The calendar link was rejected",
        }
    }
}

fn fetch_feed(url: &str, from: i64, to: i64) -> Result<Vec<Event>, Failure> {
    match crate::http::get(url, &[("Accept", "text/calendar, */*")], FEED_LIMIT) {
        Ok(body) => {
            if !body.windows(15).any(|w| w.eq_ignore_ascii_case(b"BEGIN:VCALENDAR")) {
                return Err(Failure::NotCalendar);
            }
            Ok(expand(&parse_ics(&body), from, to))
        }
        // `http::get` reports an over-limit body as a 200 with nothing in it.
        Err(200) => Err(Failure::TooLarge),
        Err(401 | 403 | 404 | 410) => Err(Failure::Rejected),
        Err(_) => Err(Failure::Offline),
    }
}

fn cache_file() -> std::path::PathBuf {
    crate::paths::cache_dir().join("calendar-events.json")
}

/// Show the last good events right away (startup / offline).
pub fn load_cache() {
    let events: Option<Vec<Event>> = std::fs::read_to_string(cache_file()).ok().and_then(|t| serde_json::from_str(&t).ok());
    if let Some(events) = events {
        publish(|v| {
            v.events = events;
            v.state = FeedState::Ok;
        });
    }
}

/// Fetch every feed on a background thread (no-op while one is running).
/// The URLs are moved into the thread and never stored in shared state.
pub fn refresh(urls: Vec<String>) {
    if urls.is_empty() {
        publish(|v| *v = CalendarView::default());
        let _ = std::fs::remove_file(cache_file());
        return;
    }
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    publish(|v| v.state = FeedState::Loading);
    std::thread::spawn(move || {
        let now = Utc::now().timestamp();
        let (from, to) = (now - PAST_DAYS * 86_400, now + AHEAD_DAYS * 86_400);
        let mut events = Vec::new();
        let mut failure = None;
        for url in &urls {
            match fetch_feed(url, from, to) {
                Ok(mut e) => events.append(&mut e),
                Err(f) => failure = Some(f),
            }
        }
        events.sort_by_key(|e| (e.start, e.end));
        if failure.is_none() || !events.is_empty() {
            let _ = std::fs::write(cache_file(), serde_json::to_string(&events).unwrap_or_default());
        }
        publish(|v| match &failure {
            // Keep the cached events when a refresh fails; just say so.
            Some(f) if events.is_empty() => {
                v.state = FeedState::Failed;
                v.error = f.text().to_string();
            }
            _ => {
                v.events = events;
                v.state = FeedState::Ok;
                v.error = failure.as_ref().map(|f| f.text().to_string()).unwrap_or_default();
                v.updated = Some(Utc::now().timestamp());
            }
        });
        RUNNING.store(false, Ordering::SeqCst);
    });
}

// ------------------------------------------------------------------- display

/// "in 4m", "in 1h 5m", "now", "ended".
pub fn countdown(ev: &Event, now: i64) -> String {
    if now >= ev.start {
        return if now < ev.end.max(ev.start + 60) { "now".into() } else { "ended".into() };
    }
    let left = ev.start - now;
    let (h, m) = (left / 3600, (left % 3600 + 59) / 60);
    if h >= 24 {
        let d = h / 24;
        format!("in {d}d")
    } else if h > 0 {
        format!("in {h}h {}m", m % 60)
    } else {
        format!("in {}m", m.max(1))
    }
}

/// "Today", "Tomorrow" or a weekday for the event's local date.
pub fn day_label(ev: &Event, now: i64) -> String {
    let local = |t: i64| Local.timestamp_opt(t, 0).single().map(|d| d.date_naive());
    let (Some(d), Some(today)) = (local(ev.start), local(now)) else { return String::new() };
    match (d - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        n if (2..7).contains(&n) => d.format("%A").to_string(),
        _ => d.format("%a %-d %b").to_string(),
    }
}

pub fn clock_label(ev: &Event) -> String {
    if ev.all_day {
        return "All day".into();
    }
    Local.timestamp_opt(ev.start, 0).single().map(|d| d.format("%H:%M").to_string()).unwrap_or_default()
}

/// Days of `year`/`month` (1-based) that have an event, for dots on the grid.
/// Formatted time range (`"09:00 – 09:15"` or `"All day"`) in local time.
pub fn time_range_label(ev: &Event) -> String {
    if ev.all_day {
        return "All day".into();
    }
    let fmt = |t: i64| Local.timestamp_opt(t, 0).single().map(|d| d.format("%H:%M").to_string());
    match (fmt(ev.start), (ev.end > ev.start).then(|| fmt(ev.end)).flatten()) {
        (Some(s), Some(e)) if s != e => format!("{s} \u{2013} {e}"),
        (Some(s), _) => s,
        _ => String::new(),
    }
}

/// Full local date label for an event (`"Wed, 28 Oct 2026"`).
pub fn full_date_label(ev: &Event) -> String {
    Local.timestamp_opt(ev.start, 0).single().map(|d| d.format("%a, %-d %b %Y").to_string()).unwrap_or_default()
}

/// Whether `e` occurs on the local calendar date `date` (including multi-day spans).
pub fn event_on_date(e: &Event, date: NaiveDate) -> bool {
    let Some(s) = Local.timestamp_opt(e.start, 0).single().map(|d| d.date_naive()) else {
        return false;
    };
    let end_ts = (e.end - 1).max(e.start);
    let e_date = Local.timestamp_opt(end_ts, 0).single().map(|d| d.date_naive()).unwrap_or(s);
    date >= s && date <= e_date
}

/// Days of `year`/`month` (1-based) that have an event, for dots on the grid.
pub fn busy_days(events: &[Event], year: i32, month: u32) -> Vec<u32> {
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return Vec::new();
    };
    let last = first.checked_add_months(chrono::Months::new(1)).and_then(|d| d.pred_opt()).unwrap_or(first);
    let (lo, hi) = local_span_bounds(first, last);
    let mut busy = [false; 32];
    for e in events.iter().filter(|e| may_overlap(e, lo, hi)) {
        let Some(s) = Local.timestamp_opt(e.start, 0).single().map(|d| d.date_naive()) else {
            continue;
        };
        let end = Local.timestamp_opt((e.end - 1).max(e.start), 0).single().map(|d| d.date_naive()).unwrap_or(s);
        let (from, to) = (s.max(first), end.min(last));
        if from <= to {
            for d in from.day()..=to.day() {
                busy[d as usize] = true;
            }
        }
    }
    (1..=last.day()).filter(|&d| busy[d as usize]).collect()
}

/// Conservative Unix-second bounds around the local dates `first..=last`
/// (padded past any UTC offset) so most events skip time-zone lookups.
fn local_span_bounds(first: NaiveDate, last: NaiveDate) -> (i64, i64) {
    const PAD: i64 = 2 * 86_400;
    let ts = |d: NaiveDate| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
    (ts(first) - PAD, ts(last) + 86_400 + PAD)
}

fn may_overlap(e: &Event, lo: i64, hi: i64) -> bool {
    e.start < hi && e.end.max(e.start) >= lo
}

#[allow(dead_code)]
fn busy_days_legacy_unused(events: &[Event], year: i32, month: u32) -> Vec<u32> {
    let mut days: Vec<u32> = events.iter().filter_map(|e| Local.timestamp_opt(e.start, 0).single()).filter(|d| d.year() == year && d.month() == month).map(|d| d.day()).collect();
    days.sort_unstable();
    days.dedup();
    days
}

/// Which events should raise a "starting soon" alert right now: timed, due
/// within `lead` seconds (or just started, within the last minute) and not
/// already announced. `seen` is updated so each instance fires once.
pub fn due_alerts<'a>(events: &'a [Event], now: i64, lead: i64, seen: &mut Vec<String>) -> Vec<&'a Event> {
    let mut out = Vec::new();
    for e in events.iter().filter(|e| !e.all_day) {
        let until = e.start - now;
        if until <= lead && until > -60 && !seen.contains(&e.key()) {
            seen.push(e.key());
            out.push(e);
        }
    }
    // Forget instances that are long over so the list stays small.
    seen.retain(|k| k.rsplit('@').next().and_then(|s| s.parse::<i64>().ok()).is_some_and(|s| s > now - 3600));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Saturday 2026-10-24 00:00 UTC.
    const NOW: i64 = 1_792_800_000;
    fn window() -> (i64, i64) {
        (NOW - 86_400, NOW + 14 * 86_400)
    }

    const GOOGLE: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:standup@x\r\nDTSTART;TZID=Asia/Jakarta:20260105T090000\r\nDTEND;TZID=Asia/Jakarta:20260105T091500\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE\r\nEXDATE;TZID=Asia/Jakarta:20261026T090000\r\nSUMMARY:Standup\r\nX-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:standup@x\r\nRECURRENCE-ID;TZID=Asia/Jakarta:20261028T090000\r\nDTSTART;TZID=Asia/Jakarta:20261028T100000\r\nDTEND;TZID=Asia/Jakarta:20261028T101500\r\nSUMMARY:Standup (moved)\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:holiday@x\r\nDTSTART;VALUE=DATE:20261030\r\nDTEND;VALUE=DATE:20261031\r\nSUMMARY:Holiday\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn google_feed_expands_with_exdate_override_and_all_day() {
        let (from, to) = window();
        let evs = expand(&parse_ics(GOOGLE.as_bytes()), from, to);
        let timed: Vec<_> = evs.iter().filter(|e| !e.all_day).collect();
        // Mon 26 Oct is excluded, Wed 28 Oct moved to 10:00, then Mon 2 and Wed 4 Nov.
        assert_eq!(timed.len(), 3, "{timed:#?}");
        let jakarta = |y, m, d, h| chrono::FixedOffset::east_opt(7 * 3600).unwrap().with_ymd_and_hms(y, m, d, h, 0, 0).unwrap().timestamp();
        assert_eq!(timed[0].start, jakarta(2026, 10, 28, 10));
        assert_eq!(timed[0].title, "Standup (moved)");
        assert_eq!(timed[1].start, jakarta(2026, 11, 2, 9));
        assert_eq!(timed[2].start, jakarta(2026, 11, 4, 9));
        assert_eq!(timed[1].end - timed[1].start, 900);
        let j = timed[1].join.as_ref().unwrap();
        assert_eq!((j.platform.as_str(), j.url.as_str()), ("Google Meet", "https://meet.google.com/abc-defg-hij"));
        let day = evs.iter().find(|e| e.all_day).unwrap();
        assert_eq!(day.title, "Holiday");
        assert_eq!(day.end - day.start, 86_400);
    }

    const OUTLOOK: &str = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:SE Asia Standard Time\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:monthly@o\r\nDTSTART;TZID=\"SE Asia Standard Time\":20260930T140000\r\nDTEND;TZID=\"SE Asia Standard Time\":20260930T150000\r\nRRULE:FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1\r\nSUMMARY:Monthly review\r\nDESCRIPTION:Join: <https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc%40thread.v2/0?context=%7b%22Tid%22%7d>\r\nBEGIN:VALARM\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn outlook_feed_uses_windows_zone_names_and_setpos() {
        let (from, to) = window();
        let evs = expand(&parse_ics(OUTLOOK.as_bytes()), from, to);
        assert_eq!(evs.len(), 1, "{evs:#?}");
        // Last weekday of October 2026 is Friday the 30th, 14:00 in Bangkok (UTC+7).
        let want = chrono::FixedOffset::east_opt(7 * 3600).unwrap().with_ymd_and_hms(2026, 10, 30, 14, 0, 0).unwrap().timestamp();
        assert_eq!(evs[0].start, want);
        let j = evs[0].join.as_ref().unwrap();
        assert_eq!(j.platform, "Teams");
        assert!(j.url.starts_with("https://teams.microsoft.com/l/meetup-join/") && !j.url.contains('>'));
    }

    #[test]
    fn folded_multibyte_text_survives() {
        let raw = b"BEGIN:VEVENT\r\nUID:u\r\nDTSTART:20261025T100000Z\r\nSUMMARY:Caf\xC3\r\n \xA9 sync\\, team\r\nEND:VEVENT\r\n";
        let ev = &parse_ics(raw)[0];
        assert_eq!(ev.summary, "Caf\u{e9} sync, team");
    }

    #[test]
    fn quoted_colons_stay_in_params() {
        let p = parse_line("DTSTART;TZID=\"W. Europe: Standard\":20261025T100000").unwrap();
        assert_eq!(p.param("TZID"), Some("W. Europe: Standard"));
        assert_eq!(p.value, "20261025T100000");
    }

    #[test]
    fn duration_and_missing_end() {
        assert_eq!(parse_duration("PT1H30M"), Some(5400));
        assert_eq!(parse_duration("P1DT2H"), Some(93_600));
        assert_eq!(parse_duration("P1W"), Some(604_800));
        assert_eq!(parse_duration("nonsense"), None);
        assert_eq!(parse_duration("P99999999999999999999D"), None);
        assert_eq!(parse_duration("P9999999999999999D"), None, "overflow is refused, not wrapped");
        let raw = b"BEGIN:VEVENT\r\nUID:d\r\nDTSTART:20261025T100000Z\r\nDURATION:PT45M\r\nSUMMARY:x\r\nEND:VEVENT\r\n";
        let (from, to) = window();
        let ev = &expand(&parse_ics(raw), from, to)[0];
        assert_eq!(ev.end - ev.start, 2700);
    }

    #[test]
    fn garbage_feeds_never_panic() {
        // Deterministic pseudo-random mixes of valid and broken calendar lines.
        let lines = [
            "BEGIN:VCALENDAR",
            "BEGIN:VEVENT",
            "END:VEVENT",
            "END:VCALENDAR",
            "BEGIN:VALARM",
            "END:VALARM",
            "UID:a",
            "UID:",
            "SUMMARY:x\\,y\\nz",
            "SUMMARY:\\",
            "DTSTART:20261025T100000Z",
            "DTSTART;TZID=\"SE Asia Standard Time\":20261025T100000",
            "DTSTART;TZID=Nowhere/Land:20261025T100000",
            "DTSTART;VALUE=DATE:20261025",
            "DTSTART:99999999T999999Z",
            "DTSTART:00010101",
            "DTSTART:",
            "DTEND:20250101T000000Z",
            "DTEND;VALUE=DATE:19000101",
            "DURATION:P99999999999999999999W",
            "DURATION:-PT5M",
            "RRULE:FREQ=DAILY;INTERVAL=2",
            "RRULE:FREQ=MONTHLY;BYDAY=-1FR;BYSETPOS=-1",
            "RRULE:FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=29",
            "RRULE:FREQ=WEEKLY;COUNT=99999999999",
            "RRULE:\u{0}",
            "EXDATE:20261026T100000Z,garbage,",
            "EXDATE;TZID=x:",
            "RECURRENCE-ID:20261025T100000Z",
            "RECURRENCE-ID;VALUE=DATE:20261025",
            "STATUS:CANCELLED",
            ":::",
            ";;;:",
            "=",
            "",
            "DESCRIPTION:see https://meet.google.com/abc and https://",
            "LOCATION:https://zoom.us/j/1",
            "X-GOOGLE-CONFERENCE:https://\u{e9}",
            " folded",
            "\tfolded",
        ];
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let (from, to) = window();
        for _ in 0..400 {
            let mut feed = String::new();
            for _ in 0..(next() % 40) {
                feed.push_str(lines[(next() % lines.len() as u64) as usize]);
                feed.push_str(if next() % 5 == 0 { "\n" } else { "\r\n" });
            }
            let _ = expand(&parse_ics(feed.as_bytes()), from, to);
        }
        // Raw bytes, including invalid UTF-8.
        for _ in 0..200 {
            let bytes: Vec<u8> = (0..(next() % 300)).map(|_| next() as u8).collect();
            let _ = expand(&parse_ics(&bytes), from, to);
        }
    }

    #[test]
    fn broken_rrule_still_shows_first_instance() {
        let raw = b"BEGIN:VEVENT\r\nUID:b\r\nDTSTART:20261025T100000Z\r\nRRULE:FREQ=NONSENSE\r\nSUMMARY:x\r\nEND:VEVENT\r\n";
        let (from, to) = window();
        assert_eq!(expand(&parse_ics(raw), from, to).len(), 1);
    }

    #[test]
    fn only_known_https_meeting_hosts_get_a_join_button() {
        assert_eq!(meeting_platform("https://us02web.zoom.us/j/123?pwd=x"), Some("Zoom"));
        assert_eq!(meeting_platform("http://meet.google.com/x"), None);
        assert_eq!(meeting_platform("file:///C:/evil.exe"), None);
        assert_eq!(meeting_platform("https://meet.google.com.evil.example/x"), None);
        assert_eq!(meeting_platform("https://zoom.us@evil.example/x"), None);
        assert_eq!(meeting_platform("ms-settings:privacy"), None);
        let j = find_join(&["room 4", "see <https://zoom.us/j/99>, thanks"]).unwrap();
        assert_eq!(j.url, "https://zoom.us/j/99");
        assert!(find_join(&["https://evil.example/zoom.us/j/1"]).is_none());
    }

    #[test]
    fn feed_urls_are_normalised() {
        assert_eq!(normalize_feed_url(" webcal://calendar.example/a.ics "), Some("https://calendar.example/a.ics".into()));
        assert!(normalize_feed_url("https://calendar.google.com/calendar/ical/x/private-abc/basic.ics").is_some());
        assert_eq!(normalize_feed_url("http://example.com/a.ics"), None);
        assert_eq!(normalize_feed_url("file:///C:/a.ics"), None);
        assert_eq!(normalize_feed_url("https://x"), None);
    }

    fn ev(uid: &str, s: i64, e: i64) -> Event {
        Event { uid: uid.into(), start: s, end: e, title: uid.into(), location: String::new(), description: String::new(), all_day: false, join: None }
    }

    #[test]
    fn events_on_day_and_description_work() {
        let (from, to) = window();
        let evs = expand(&parse_ics(OUTLOOK.as_bytes()), from, to);
        assert!(evs[0].description.contains("https://teams.microsoft.com/l/meetup-join/"));
        let view = CalendarView { events: evs, state: FeedState::Ok, ..Default::default() };
        let start_local = Local.timestamp_opt(view.events[0].start, 0).single().unwrap().date_naive();
        let on_day = view.events_on_day(start_local.year(), start_local.month(), start_local.day());
        assert_eq!(on_day.len(), 1);
        assert_eq!(on_day[0].title, "Monthly review");
        let empty_day = view.events_on_day(start_local.year(), start_local.month(), 1);
        assert!(empty_day.is_empty());
    }

    #[test]
    fn busy_days_matches_per_day_scan() {
        let evs = vec![
            ev("a", NOW, NOW + 3600),
            ev("multi", NOW + 2 * 86_400, NOW + 5 * 86_400),
            ev("span", NOW - 40 * 86_400, NOW - 20 * 86_400),
            ev("zero", NOW + 9 * 86_400, NOW + 9 * 86_400),
            ev("far", NOW + 120 * 86_400, NOW + 121 * 86_400),
        ];
        let base = Local.timestamp_opt(NOW, 0).single().unwrap().date_naive();
        for off in -2..=2 {
            let d = if off < 0 { base - chrono::Months::new((-off) as u32) } else { base + chrono::Months::new(off as u32) };
            let (y, m) = (d.year(), d.month());
            let slow: Vec<u32> =
                (1..=31).filter_map(|day| NaiveDate::from_ymd_opt(y, m, day)).filter(|date| evs.iter().any(|e| event_on_date(e, *date))).map(|date| date.day()).collect();
            assert_eq!(busy_days(&evs, y, m), slow, "{y}-{m}");
        }
    }

    #[test]
    fn countdown_text() {
        assert_eq!(countdown(&ev("t", NOW + 240, NOW + 3600), NOW), "in 4m");
        assert_eq!(countdown(&ev("t", NOW + 3900, NOW + 7200), NOW), "in 1h 5m");
        assert_eq!(countdown(&ev("t", NOW - 10, NOW + 600), NOW), "now");
        assert_eq!(countdown(&ev("t", NOW - 900, NOW - 300), NOW), "ended");
        assert_eq!(countdown(&ev("t", NOW + 3 * 86_400 + 60, NOW + 4 * 86_400), NOW), "in 3d");
    }

    #[test]
    fn alerts_fire_once_per_instance() {
        let evs = vec![ev("soon", NOW + 240, NOW + 2000), ev("later", NOW + 900, NOW + 2700), ev("started", NOW - 30, NOW + 1700), ev("over", NOW - 600, NOW + 1200)];
        let mut seen = Vec::new();
        let first: Vec<_> = due_alerts(&evs, NOW, 300, &mut seen).iter().map(|e| e.uid.clone()).collect();
        assert_eq!(first, ["soon", "started"]);
        assert!(due_alerts(&evs, NOW + 5, 300, &mut seen).is_empty(), "no repeat after a refresh");
        let later: Vec<_> = due_alerts(&evs, NOW + 700, 300, &mut seen).iter().map(|e| e.uid.clone()).collect();
        assert_eq!(later, ["later"]);
    }

    #[test]
    fn agenda_skips_finished_events() {
        let v = CalendarView { events: vec![ev("a", NOW - 7200, NOW - 3600), ev("b", NOW - 600, NOW + 600), ev("c", NOW + 3600, NOW + 7200)], ..Default::default() };
        assert_eq!(v.agenda(NOW, 5).len(), 2);
        assert_eq!(v.next(NOW).unwrap().start, NOW - 600, "an ongoing meeting is still next");
    }
}

#[cfg(all(test, windows))]
mod live {
    /// Needs internet: `cargo test -p teardrop live_public_feed -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_public_feed() {
        let url = "https://calendar.google.com/calendar/ical/en.indonesian%23holiday%40group.v.calendar.google.com/public/basic.ics";
        let now = chrono::Utc::now().timestamp();
        let evs = super::fetch_feed(url, now - 86_400, now + 365 * 86_400).expect("feed");
        println!("{} events in the next year", evs.len());
        for e in evs.iter().take(4) {
            println!("  {} | {} | all_day={}", super::day_label(e, now), e.title, e.all_day);
        }
        assert!(!evs.is_empty());
        assert!(evs.iter().all(|e| e.all_day));
    }
}
