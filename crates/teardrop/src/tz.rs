//! Time zones without a bundled database.
//!
//! Calendar feeds name zones two ways: Windows names (`SE Asia Standard Time`,
//! Outlook) and IANA names (`Asia/Jakarta`, Google). Windows already knows its
//! own zones and their daylight-saving rules, so Windows names are looked up in
//! the OS list and IANA names go through a table of common zones to the
//! matching Windows name. Conversions use the OS rules. An unknown zone falls
//! back to this PC's own zone at the call site.

use chrono::{Local, NaiveDateTime, TimeZone};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Utc,
    /// Index into the OS zone list on Windows, `chrono_tz::TZ_VARIANTS` elsewhere.
    Win(u16),
}

/// IANA names -> Windows zone name, for the zones calendars commonly use.
const TABLE: &[(&[&str], &str)] = &[
    (&["Pacific/Honolulu"], "Hawaiian Standard Time"),
    (&["America/Anchorage"], "Alaskan Standard Time"),
    (&["America/Los_Angeles", "America/Vancouver", "America/Tijuana"], "Pacific Standard Time"),
    (&["America/Denver", "America/Edmonton", "America/Boise"], "Mountain Standard Time"),
    (&["America/Phoenix"], "US Mountain Standard Time"),
    (&["America/Chicago", "America/Winnipeg"], "Central Standard Time"),
    (&["America/Regina"], "Canada Central Standard Time"),
    (&["America/Guatemala", "America/Costa_Rica", "America/El_Salvador"], "Central America Standard Time"),
    (&["America/Mexico_City", "America/Monterrey"], "Central Standard Time (Mexico)"),
    (&["America/New_York", "America/Toronto", "America/Detroit", "America/Indiana/Indianapolis"], "Eastern Standard Time"),
    (&["America/Bogota", "America/Lima", "America/Guayaquil"], "SA Pacific Standard Time"),
    (&["America/Halifax"], "Atlantic Standard Time"),
    (&["America/Caracas"], "Venezuela Standard Time"),
    (&["America/Santiago"], "Pacific SA Standard Time"),
    (&["America/St_Johns"], "Newfoundland Standard Time"),
    (&["America/Sao_Paulo"], "E. South America Standard Time"),
    (&["America/Argentina/Buenos_Aires", "America/Buenos_Aires"], "Argentina Standard Time"),
    (&["Atlantic/Reykjavik"], "Greenwich Standard Time"),
    (&["Europe/London", "Europe/Dublin", "Europe/Lisbon"], "GMT Standard Time"),
    (&["Africa/Casablanca"], "Morocco Standard Time"),
    (&["Europe/Berlin", "Europe/Amsterdam", "Europe/Rome", "Europe/Vienna", "Europe/Stockholm", "Europe/Zurich", "Europe/Oslo"], "W. Europe Standard Time"),
    (&["Europe/Budapest", "Europe/Prague", "Europe/Belgrade"], "Central Europe Standard Time"),
    (&["Europe/Paris", "Europe/Madrid", "Europe/Brussels", "Europe/Copenhagen"], "Romance Standard Time"),
    (&["Europe/Warsaw", "Europe/Zagreb", "Europe/Sarajevo"], "Central European Standard Time"),
    (&["Africa/Lagos", "Africa/Kinshasa"], "W. Central Africa Standard Time"),
    (&["Europe/Bucharest", "Europe/Athens"], "GTB Standard Time"),
    (&["Europe/Chisinau"], "E. Europe Standard Time"),
    (&["Europe/Kiev", "Europe/Kyiv", "Europe/Helsinki", "Europe/Riga", "Europe/Tallinn", "Europe/Vilnius"], "FLE Standard Time"),
    (&["Europe/Istanbul"], "Turkey Standard Time"),
    (&["Africa/Cairo"], "Egypt Standard Time"),
    (&["Africa/Johannesburg"], "South Africa Standard Time"),
    (&["Asia/Jerusalem"], "Israel Standard Time"),
    (&["Europe/Moscow", "Europe/Minsk"], "Russian Standard Time"),
    (&["Africa/Nairobi", "Africa/Addis_Ababa"], "E. Africa Standard Time"),
    (&["Asia/Riyadh", "Asia/Baghdad", "Asia/Kuwait"], "Arab Standard Time"),
    (&["Asia/Dubai", "Asia/Muscat"], "Arabian Standard Time"),
    (&["Asia/Tehran"], "Iran Standard Time"),
    (&["Asia/Karachi"], "Pakistan Standard Time"),
    (&["Asia/Tashkent"], "West Asia Standard Time"),
    (&["Asia/Kolkata", "Asia/Calcutta"], "India Standard Time"),
    (&["Asia/Colombo"], "Sri Lanka Standard Time"),
    (&["Asia/Kathmandu"], "Nepal Standard Time"),
    (&["Asia/Almaty"], "Central Asia Standard Time"),
    (&["Asia/Dhaka"], "Bangladesh Standard Time"),
    (&["Asia/Yangon", "Asia/Rangoon"], "Myanmar Standard Time"),
    (&["Asia/Bangkok", "Asia/Jakarta", "Asia/Pontianak", "Asia/Ho_Chi_Minh", "Asia/Saigon", "Asia/Phnom_Penh"], "SE Asia Standard Time"),
    (&["Asia/Shanghai", "Asia/Hong_Kong", "Asia/Macau"], "China Standard Time"),
    (&["Asia/Singapore", "Asia/Kuala_Lumpur", "Asia/Manila", "Asia/Makassar", "Asia/Brunei"], "Singapore Standard Time"),
    (&["Australia/Perth"], "W. Australia Standard Time"),
    (&["Asia/Taipei"], "Taipei Standard Time"),
    (&["Asia/Tokyo", "Asia/Jayapura"], "Tokyo Standard Time"),
    (&["Asia/Seoul"], "Korea Standard Time"),
    (&["Australia/Adelaide"], "Cen. Australia Standard Time"),
    (&["Australia/Darwin"], "AUS Central Standard Time"),
    (&["Australia/Sydney", "Australia/Melbourne", "Australia/Canberra"], "AUS Eastern Standard Time"),
    (&["Australia/Brisbane"], "E. Australia Standard Time"),
    (&["Pacific/Auckland"], "New Zealand Standard Time"),
];

pub fn iana_to_windows(iana: &str) -> Option<&'static str> {
    TABLE.iter().find(|(names, _)| names.contains(&iana)).map(|(_, win)| *win)
}

fn is_utc_name(name: &str) -> bool {
    matches!(name, "UTC" | "Etc/UTC" | "GMT" | "Etc/GMT" | "Z" | "Zulu" | "Etc/Zulu" | "Coordinated Universal Time")
}

/// Resolve a TZID (quoted or not; IANA or Windows name) to a zone. Unknown = None.
pub fn resolve(tzid: &str) -> Option<Zone> {
    let name = tzid.trim().trim_matches('"');
    if is_utc_name(name) {
        return Some(Zone::Utc);
    }
    os::find(name).or_else(|| iana_to_windows(name).and_then(os::find))
}

/// Wall-clock time in `zone` (None = this PC's zone) -> Unix seconds.
pub fn to_utc_secs(n: NaiveDateTime, zone: Option<Zone>) -> Option<i64> {
    match zone {
        Some(Zone::Utc) => Some(n.and_utc().timestamp()),
        Some(Zone::Win(i)) => os::to_utc_secs(i, n),
        None => Local.from_local_datetime(&n).earliest().map(|d| d.timestamp()),
    }
}

#[cfg(windows)]
mod os {
    use super::Zone;
    use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
    use std::sync::OnceLock;
    use windows::Win32::Foundation::SYSTEMTIME;
    use windows::Win32::System::Time::{EnumDynamicTimeZoneInformation, TzSpecificLocalTimeToSystemTimeEx, DYNAMIC_TIME_ZONE_INFORMATION};

    /// Every zone the OS knows, by registry key name ("SE Asia Standard Time").
    fn zones() -> &'static Vec<(String, DYNAMIC_TIME_ZONE_INFORMATION)> {
        static ZONES: OnceLock<Vec<(String, DYNAMIC_TIME_ZONE_INFORMATION)>> = OnceLock::new();
        ZONES.get_or_init(|| {
            let mut out = Vec::new();
            for i in 0..512u32 {
                let mut d = DYNAMIC_TIME_ZONE_INFORMATION::default();
                if unsafe { EnumDynamicTimeZoneInformation(i, &mut d) } != 0 {
                    break;
                }
                let len = d.TimeZoneKeyName.iter().position(|c| *c == 0).unwrap_or(d.TimeZoneKeyName.len());
                out.push((String::from_utf16_lossy(&d.TimeZoneKeyName[..len]), d));
            }
            out
        })
    }

    pub fn find(name: &str) -> Option<Zone> {
        zones().iter().position(|(n, _)| n.eq_ignore_ascii_case(name)).map(|i| Zone::Win(i as u16))
    }

    pub fn to_utc_secs(i: u16, n: NaiveDateTime) -> Option<i64> {
        let (_, dtzi) = zones().get(i as usize)?;
        let local = SYSTEMTIME {
            wYear: u16::try_from(n.year()).ok()?,
            wMonth: n.month() as u16,
            wDayOfWeek: 0,
            wDay: n.day() as u16,
            wHour: n.hour() as u16,
            wMinute: n.minute() as u16,
            wSecond: n.second() as u16,
            wMilliseconds: 0,
        };
        let mut utc = SYSTEMTIME::default();
        unsafe { TzSpecificLocalTimeToSystemTimeEx(Some(dtzi), &local, &mut utc) }.ok()?;
        NaiveDate::from_ymd_opt(utc.wYear as i32, utc.wMonth as u32, utc.wDay as u32)?
            .and_hms_opt(utc.wHour as u32, utc.wMinute as u32, utc.wSecond as u32)
            .map(|d| d.and_utc().timestamp())
    }
}

#[cfg(not(windows))]
mod os {
    use super::Zone;
    use chrono::{NaiveDateTime, TimeZone};
    use chrono_tz::Tz;

    /// IANA name, or a Windows zone name mapped through the table's first alias.
    fn as_tz(name: &str) -> Option<Tz> {
        name.parse::<Tz>().ok().or_else(|| super::TABLE.iter().find(|(_, w)| w.eq_ignore_ascii_case(name)).and_then(|(aliases, _)| aliases.first()?.parse().ok()))
    }

    /// `Zone::Win` here indexes `TZ_VARIANTS`, which is stable for the process.
    pub fn find(name: &str) -> Option<Zone> {
        let tz = as_tz(name)?;
        chrono_tz::TZ_VARIANTS.iter().position(|t| *t == tz).map(|i| Zone::Win(i as u16))
    }

    pub fn to_utc_secs(i: u16, n: NaiveDateTime) -> Option<i64> {
        chrono_tz::TZ_VARIANTS.get(i as usize)?.from_local_datetime(&n).earliest().map(|d| d.timestamp())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(y: i32, m: u32, d: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
    }

    fn secs(zone: &str, n: NaiveDateTime) -> i64 {
        to_utc_secs(n, Some(resolve(zone).unwrap_or_else(|| panic!("{zone} not resolved")))).unwrap()
    }

    #[test]
    fn iana_and_windows_names_agree() {
        let n = at(2026, 10, 28, 9);
        assert_eq!(secs("Asia/Jakarta", n), secs("SE Asia Standard Time", n));
        assert_eq!(secs("\"SE Asia Standard Time\"", n), n.and_utc().timestamp() - 7 * 3600);
        assert_eq!(resolve("UTC"), Some(Zone::Utc));
        assert_eq!(resolve("Mars/Olympus_Mons"), None);
    }

    #[test]
    fn daylight_saving_comes_from_the_os_rules() {
        // New York is UTC-5 in winter and UTC-4 in summer.
        assert_eq!(secs("America/New_York", at(2026, 1, 15, 9)), at(2026, 1, 15, 14).and_utc().timestamp());
        assert_eq!(secs("America/New_York", at(2026, 7, 15, 9)), at(2026, 7, 15, 13).and_utc().timestamp());
        // Berlin: UTC+1 / UTC+2.
        assert_eq!(secs("W. Europe Standard Time", at(2026, 1, 15, 9)), at(2026, 1, 15, 8).and_utc().timestamp());
        assert_eq!(secs("Europe/Berlin", at(2026, 7, 15, 9)), at(2026, 7, 15, 7).and_utc().timestamp());
        // Southern hemisphere: Sydney is UTC+11 in January, UTC+10 in July.
        assert_eq!(secs("Australia/Sydney", at(2026, 1, 15, 9)), at(2026, 1, 14, 22).and_utc().timestamp());
        assert_eq!(secs("AUS Eastern Standard Time", at(2026, 7, 15, 9)), at(2026, 7, 14, 23).and_utc().timestamp());
    }

    #[test]
    fn every_table_entry_names_a_real_windows_zone() {
        // Guards against typos: each IANA name in the table must resolve to a zone the OS has.
        for (names, win) in TABLE {
            assert!(os::find(win).is_some(), "Windows has no zone called {win}");
            for n in *names {
                assert!(resolve(n).is_some(), "{n} did not resolve");
            }
        }
    }
}
