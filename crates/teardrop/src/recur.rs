//! A small iCalendar RRULE engine over wall-clock times.
//!
//! Supported: FREQ=DAILY|WEEKLY|MONTHLY|YEARLY, INTERVAL, COUNT, UNTIL,
//! BYDAY (with ordinals like `2TU` / `-1FR` for monthly and yearly rules),
//! BYMONTHDAY (negative counts from the month's end), BYMONTH, BYSETPOS, WKST.
//! Rules using anything else (BYYEARDAY, BYWEEKNO, BYHOUR ...) return `None`
//! so the caller can still show the first instance.
//!
//! It works on `NaiveDateTime` in the event's own zone; converting each
//! occurrence to an instant (and so handling DST) is the caller's job. That is
//! what lets us avoid a bundled time-zone database and a regex-based crate.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};

/// Upper bound on periods walked, so a hostile rule can't spin forever.
const MAX_PERIODS: i64 = 20_000;
/// Upper bound on occurrences returned.
const MAX_OCCURRENCES: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug)]
struct Rule {
    freq: Freq,
    interval: i64,
    count: Option<usize>,
    until: Option<String>,
    by_day: Vec<(Option<i32>, Weekday)>,
    by_monthday: Vec<i32>,
    by_month: Vec<u32>,
    by_setpos: Vec<i32>,
    wkst: Weekday,
}

fn weekday(code: &str) -> Option<Weekday> {
    Some(match code {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    })
}

fn ints(v: &str) -> Option<Vec<i32>> {
    v.split(',').map(|x| x.trim().parse().ok()).collect()
}

fn parse_rule(text: &str) -> Option<Rule> {
    let mut r = Rule { freq: Freq::Daily, interval: 1, count: None, until: None, by_day: vec![], by_monthday: vec![], by_month: vec![], by_setpos: vec![], wkst: Weekday::Mon };
    let mut have_freq = false;
    for part in text.split(';').filter(|p| !p.is_empty()) {
        let (k, v) = part.split_once('=')?;
        match k.to_ascii_uppercase().as_str() {
            "FREQ" => {
                r.freq = match v.to_ascii_uppercase().as_str() {
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    _ => return None,
                };
                have_freq = true;
            }
            "INTERVAL" => r.interval = v.parse::<i64>().ok().filter(|n| (1..=10_000).contains(n))?,
            "COUNT" => r.count = Some(v.parse::<usize>().ok()?.min(MAX_OCCURRENCES)),
            "UNTIL" => r.until = Some(v.to_string()),
            "WKST" => r.wkst = weekday(&v.to_ascii_uppercase())?,
            "BYDAY" => {
                for tok in v.split(',') {
                    let tok = tok.trim().to_ascii_uppercase();
                    let split = tok.len().checked_sub(2)?;
                    let (ord, wd) = tok.split_at(split);
                    let ord = if ord.is_empty() { None } else { Some(ord.parse::<i32>().ok()?) };
                    r.by_day.push((ord, weekday(wd)?));
                }
            }
            "BYMONTHDAY" => r.by_monthday = ints(v)?,
            "BYMONTH" => r.by_month = ints(v)?.into_iter().map(|m| u32::try_from(m).ok().filter(|m| (1..=12).contains(m))).collect::<Option<_>>()?,
            "BYSETPOS" => r.by_setpos = ints(v)?.into_iter().map(|p| (p != 0).then_some(p)).collect::<Option<_>>()?,
            // Anything we don't implement: refuse rather than expand it wrongly.
            _ => return None,
        }
    }
    have_freq.then_some(r)
}

/// Days in a month, or 0 when the year/month is outside what chrono can represent.
fn days_in_month(y: i32, m: u32) -> u32 {
    let Some(first) = NaiveDate::from_ymd_opt(y, m, 1) else { return 0 };
    let next = if m == 12 { NaiveDate::from_ymd_opt(y.saturating_add(1), 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) };
    next.map(|n| (n - first).num_days() as u32).unwrap_or(0)
}

fn add_months(y: i32, m: u32, by: i64) -> (i32, u32) {
    let idx = (y as i64 * 12 + (m as i64 - 1)).saturating_add(by);
    (idx.div_euclid(12).clamp(i32::MIN as i64, i32::MAX as i64) as i32, (idx.rem_euclid(12)) as u32 + 1)
}

/// The n-th (1-based, or -1 = last) `wd` of a month.
fn nth_weekday(y: i32, m: u32, wd: Weekday, n: i32) -> Option<NaiveDate> {
    if n == 0 {
        return None;
    }
    let dim = days_in_month(y, m);
    let days: Vec<u32> = (1..=dim).filter(|d| NaiveDate::from_ymd_opt(y, m, *d).is_some_and(|x| x.weekday() == wd)).collect();
    let i = if n > 0 { n as usize - 1 } else { days.len().checked_sub(n.unsigned_abs() as usize)? };
    days.get(i).and_then(|d| NaiveDate::from_ymd_opt(y, m, *d))
}

fn week_start(d: NaiveDate, wkst: Weekday) -> NaiveDate {
    let back = (d.weekday().num_days_from_monday() + 7 - wkst.num_days_from_monday()) % 7;
    d.checked_sub_signed(Duration::days(back as i64)).unwrap_or(d)
}

impl Rule {
    fn month_ok(&self, d: NaiveDate) -> bool {
        self.by_month.is_empty() || self.by_month.contains(&d.month())
    }

    fn monthday_ok(&self, d: NaiveDate) -> bool {
        if self.by_monthday.is_empty() {
            return true;
        }
        let dim = days_in_month(d.year(), d.month()) as i32;
        self.by_monthday.iter().any(|md| if *md > 0 { *md == d.day() as i32 } else { dim + 1 + *md == d.day() as i32 })
    }

    fn weekday_ok(&self, d: NaiveDate) -> bool {
        self.by_day.is_empty() || self.by_day.iter().any(|(_, w)| *w == d.weekday())
    }

    /// Candidate dates inside one month for MONTHLY / YEARLY rules.
    fn month_dates(&self, y: i32, m: u32, dtstart_day: u32) -> Vec<NaiveDate> {
        let dim = days_in_month(y, m);
        let mut out: Vec<NaiveDate> = Vec::new();
        let by_md: Vec<NaiveDate> = self
            .by_monthday
            .iter()
            .filter_map(|md| {
                let day = if *md > 0 { *md as i64 } else { dim as i64 + 1 + *md as i64 };
                (1..=dim as i64).contains(&day).then(|| NaiveDate::from_ymd_opt(y, m, day as u32)).flatten()
            })
            .collect();
        let by_wd: Vec<NaiveDate> = self
            .by_day
            .iter()
            .flat_map(|(ord, wd)| match ord {
                Some(n) => nth_weekday(y, m, *wd, *n).into_iter().collect::<Vec<_>>(),
                None => (1..=dim).filter_map(|d| NaiveDate::from_ymd_opt(y, m, d)).filter(|d| d.weekday() == *wd).collect(),
            })
            .collect();
        match (self.by_monthday.is_empty(), self.by_day.is_empty()) {
            (true, true) => out.extend(NaiveDate::from_ymd_opt(y, m, dtstart_day)),
            (false, true) => out = by_md,
            (true, false) => out = by_wd,
            (false, false) => out = by_md.into_iter().filter(|d| by_wd.contains(d)).collect(),
        }
        out.sort();
        out.dedup();
        out
    }
}

/// Pick the BYSETPOS entries (1-based; negative counts from the end).
fn apply_setpos(dates: Vec<NaiveDate>, setpos: &[i32]) -> Vec<NaiveDate> {
    if setpos.is_empty() {
        return dates;
    }
    let mut out: Vec<NaiveDate> = setpos
        .iter()
        .filter_map(|p| {
            let i = if *p > 0 { (*p as usize).checked_sub(1)? } else { dates.len().checked_sub(p.unsigned_abs() as usize)? };
            dates.get(i).copied()
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Expand `rule` from `start` (the event's wall-clock start) up to `horizon`.
///
/// `to_secs` turns a wall-clock time into Unix seconds in the event's zone;
/// it is only used to compare against UNTIL. `None` means the rule uses
/// something unsupported.
pub fn occurrences(start: NaiveDateTime, rule: &str, horizon: NaiveDateTime, to_secs: &dyn Fn(NaiveDateTime) -> Option<i64>) -> Option<Vec<NaiveDateTime>> {
    let r = parse_rule(rule)?;
    let until_secs: Option<i64> = r.until.as_deref().and_then(|u| {
        if let Some(utc) = u.strip_suffix('Z') {
            NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok().map(|n| n.and_utc().timestamp())
        } else if u.len() == 8 {
            NaiveDate::parse_from_str(u, "%Y%m%d").ok().and_then(|d| d.and_hms_opt(23, 59, 59)).and_then(to_secs)
        } else {
            NaiveDateTime::parse_from_str(u, "%Y%m%dT%H%M%S").ok().and_then(to_secs)
        }
    });
    let (d0, time) = (start.date(), start.time());
    let mut out: Vec<NaiveDateTime> = Vec::new();

    'periods: for k in 0..MAX_PERIODS {
        // k <= 20_000 and interval <= 10_000, so this cannot overflow an i64.
        let step = k * r.interval;
        // Anything past the end of chrono's calendar ends the walk instead of panicking.
        let (mut cands, period_start): (Vec<NaiveDate>, NaiveDate) = match r.freq {
            Freq::Daily => {
                let Some(d) = Duration::try_days(step).and_then(|dur| d0.checked_add_signed(dur)) else { break };
                (if r.month_ok(d) && r.weekday_ok(d) && r.monthday_ok(d) { vec![d] } else { vec![] }, d)
            }
            Freq::Weekly => {
                let Some(ws) = Duration::try_weeks(step).and_then(|dur| week_start(d0, r.wkst).checked_add_signed(dur)) else { break };
                let days = (0..7)
                    .filter_map(|i| ws.checked_add_signed(Duration::days(i)))
                    .filter(|d| r.month_ok(*d) && if r.by_day.is_empty() { d.weekday() == d0.weekday() } else { r.weekday_ok(*d) })
                    .collect();
                (days, ws)
            }
            Freq::Monthly => {
                let (y, m) = add_months(d0.year(), d0.month(), step);
                let Some(first) = NaiveDate::from_ymd_opt(y, m, 1) else { break };
                let days = if r.by_month.is_empty() || r.by_month.contains(&m) { r.month_dates(y, m, d0.day()) } else { vec![] };
                (days, first)
            }
            Freq::Yearly => {
                let Ok(y) = i32::try_from(d0.year() as i64 + step) else { break };
                let Some(jan1) = NaiveDate::from_ymd_opt(y, 1, 1) else { break };
                let months: Vec<u32> = if r.by_month.is_empty() { vec![d0.month()] } else { r.by_month.clone() };
                let mut days: Vec<NaiveDate> = months.iter().flat_map(|m| r.month_dates(y, *m, d0.day())).collect();
                days.sort();
                (days, jan1)
            }
        };
        if period_start.and_time(time) > horizon {
            break;
        }
        cands = apply_setpos(cands, &r.by_setpos);
        for d in cands {
            let dt = d.and_time(time);
            if dt < start {
                continue;
            }
            if dt > horizon {
                break 'periods;
            }
            if let (Some(limit), Some(s)) = (until_secs, to_secs(dt)) {
                if s > limit {
                    break 'periods;
                }
            }
            out.push(dt);
            if r.count.is_some_and(|c| out.len() >= c) || out.len() >= MAX_OCCURRENCES {
                break 'periods;
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, m: u32, d: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
    }

    fn run(start: NaiveDateTime, rule: &str, horizon: NaiveDateTime) -> Vec<String> {
        occurrences(start, rule, horizon, &|n| Some(n.and_utc().timestamp())).unwrap().iter().map(|d| d.format("%Y-%m-%d %H").to_string()).collect()
    }

    #[test]
    fn daily_with_interval_and_count() {
        assert_eq!(run(dt(2026, 1, 1, 9), "FREQ=DAILY;INTERVAL=2;COUNT=3", dt(2026, 12, 31, 0)), ["2026-01-01 09", "2026-01-03 09", "2026-01-05 09"]);
    }

    #[test]
    fn weekly_by_day_across_weeks() {
        // Mon+Wed every other week from Mon 5 Jan 2026.
        let got = run(dt(2026, 1, 5, 9), "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE", dt(2026, 2, 3, 0));
        assert_eq!(got, ["2026-01-05 09", "2026-01-07 09", "2026-01-19 09", "2026-01-21 09", "2026-02-02 09"]);
    }

    #[test]
    fn weekly_default_is_the_start_weekday() {
        assert_eq!(run(dt(2026, 1, 7, 9), "FREQ=WEEKLY;COUNT=3", dt(2027, 1, 1, 0)), ["2026-01-07 09", "2026-01-14 09", "2026-01-21 09"]);
    }

    #[test]
    fn monthly_by_day_of_month_skips_short_months() {
        let got = run(dt(2026, 1, 31, 9), "FREQ=MONTHLY;COUNT=4", dt(2027, 1, 1, 0));
        assert_eq!(got, ["2026-01-31 09", "2026-03-31 09", "2026-05-31 09", "2026-07-31 09"]);
    }

    #[test]
    fn monthly_ordinal_weekday_and_last_weekday_setpos() {
        // 2nd Tuesday of each month.
        let got = run(dt(2026, 1, 13, 9), "FREQ=MONTHLY;BYDAY=2TU;COUNT=3", dt(2027, 1, 1, 0));
        assert_eq!(got, ["2026-01-13 09", "2026-02-10 09", "2026-03-10 09"]);
        // Last weekday (Outlook style).
        let got = run(dt(2026, 9, 30, 14), "FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1;COUNT=3", dt(2027, 1, 1, 0));
        assert_eq!(got, ["2026-09-30 14", "2026-10-30 14", "2026-11-30 14"]);
        // Last Friday.
        let got = run(dt(2026, 1, 30, 9), "FREQ=MONTHLY;BYDAY=-1FR;COUNT=2", dt(2027, 1, 1, 0));
        assert_eq!(got, ["2026-01-30 09", "2026-02-27 09"]);
    }

    #[test]
    fn monthly_negative_monthday() {
        let got = run(dt(2026, 1, 31, 9), "FREQ=MONTHLY;BYMONTHDAY=-1;COUNT=3", dt(2027, 1, 1, 0));
        assert_eq!(got, ["2026-01-31 09", "2026-02-28 09", "2026-03-31 09"]);
    }

    #[test]
    fn yearly_defaults_and_bymonth() {
        assert_eq!(run(dt(2026, 3, 8, 9), "FREQ=YEARLY;COUNT=3", dt(2040, 1, 1, 0)), ["2026-03-08 09", "2027-03-08 09", "2028-03-08 09"]);
        // Leap day only exists in leap years.
        assert_eq!(run(dt(2024, 2, 29, 9), "FREQ=YEARLY;COUNT=2", dt(2040, 1, 1, 0)), ["2024-02-29 09", "2028-02-29 09"]);
        let got = run(dt(2026, 1, 15, 9), "FREQ=YEARLY;BYMONTH=1,7;COUNT=3", dt(2040, 1, 1, 0));
        assert_eq!(got, ["2026-01-15 09", "2026-07-15 09", "2027-01-15 09"]);
    }

    #[test]
    fn until_is_inclusive_and_respects_the_horizon() {
        assert_eq!(run(dt(2026, 1, 1, 9), "FREQ=DAILY;UNTIL=20260103T090000Z", dt(2026, 12, 31, 0)).len(), 3);
        assert_eq!(run(dt(2026, 1, 1, 9), "FREQ=DAILY;UNTIL=20260103", dt(2026, 12, 31, 0)).len(), 3);
        assert_eq!(run(dt(2026, 1, 1, 9), "FREQ=DAILY", dt(2026, 1, 10, 0)).len(), 9);
    }

    #[test]
    fn unsupported_or_broken_rules_are_refused() {
        let never = |_: NaiveDateTime| None;
        assert!(occurrences(dt(2026, 1, 1, 9), "FREQ=YEARLY;BYYEARDAY=100", dt(2027, 1, 1, 0), &never).is_none());
        assert!(occurrences(dt(2026, 1, 1, 9), "FREQ=NONSENSE", dt(2027, 1, 1, 0), &never).is_none());
        assert!(occurrences(dt(2026, 1, 1, 9), "INTERVAL=2", dt(2027, 1, 1, 0), &never).is_none());
        assert!(occurrences(dt(2026, 1, 1, 9), "FREQ=DAILY;INTERVAL=0", dt(2027, 1, 1, 0), &never).is_none());
    }

    #[test]
    fn hostile_rules_and_dates_never_panic() {
        let far = NaiveDate::from_ymd_opt(262_142, 12, 31).unwrap().and_hms_opt(0, 0, 0).unwrap();
        let near = dt(2026, 12, 31, 0);
        // (start, horizon) pairs that reach both ends of chrono's calendar.
        let cases = [
            (dt(1, 1, 1, 0), near),
            (dt(2026, 10, 24, 9), near),
            (dt(2026, 10, 24, 9), far),
            (dt(9999, 12, 31, 23), far),
            (NaiveDate::from_ymd_opt(262_142, 12, 31).unwrap().and_hms_opt(0, 0, 0).unwrap(), far),
            (NaiveDate::from_ymd_opt(-262_143, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap(), near),
        ];
        let freqs = ["DAILY", "WEEKLY", "MONTHLY", "YEARLY"];
        let extras = [
            "", ";INTERVAL=1", ";INTERVAL=10000", ";INTERVAL=99999999999999999999", ";INTERVAL=-3", ";COUNT=0", ";COUNT=99999999999",
            ";BYSETPOS=0", ";BYSETPOS=-2147483648", ";BYSETPOS=2147483647", ";BYDAY=-2147483648MO", ";BYDAY=99FR", ";BYDAY=0TU", ";BYDAY=XX",
            ";BYMONTH=0", ";BYMONTH=13", ";BYMONTH=-1", ";BYMONTH=1,2,3,4,5,6,7,8,9,10,11,12", ";BYMONTHDAY=0", ";BYMONTHDAY=-2147483648",
            ";BYMONTHDAY=31", ";BYMONTHDAY=-31", ";UNTIL=garbage", ";UNTIL=00010101", ";UNTIL=99991231T235959Z", ";WKST=SU", ";WKST=ZZ",
            ";BYDAY=MO,TU,WE,TH,FR,SA,SU;BYSETPOS=-1", ";INTERVAL=7;BYDAY=-1SU;BYMONTH=2", ";BYYEARDAY=1", ";FREQ=DAILY",
        ];
        let to_secs = |n: NaiveDateTime| Some(n.and_utc().timestamp());
        let mut produced = 0usize;
        for (start, horizon) in cases {
            for f in freqs {
                for x in extras {
                    if let Some(v) = occurrences(start, &format!("FREQ={f}{x}"), horizon, &to_secs) {
                        assert!(v.len() <= MAX_OCCURRENCES);
                        produced += v.len();
                    }
                }
            }
        }
        assert!(produced > 0);
        // Rules that are not even key=value pairs.
        for junk in ["", ";;;", "FREQ", "FREQ=", "=DAILY", "FREQ=DAILY;COUNT=abc", "freq=daily;interval=2;count=2", "\u{0}\u{1}", "FREQ=DAILY;INTERVAL"] {
            let _ = occurrences(dt(2026, 1, 1, 9), junk, dt(2027, 1, 1, 0), &to_secs);
        }
    }

    #[test]
    fn a_long_running_daily_event_is_cheap() {
        let got = run(dt(2015, 1, 1, 9), "FREQ=DAILY", dt(2026, 12, 1, 0));
        assert!(got.len() > 4_000 && got.len() < MAX_OCCURRENCES);
    }
}
