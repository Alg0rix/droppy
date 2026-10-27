//! Scratch notes: a newest-first notebook of free-text memos.
//!
//! A note has no separate title field. The first non-empty line doubles as the
//! row title and the rest is the preview, so typing is one continuous block
//! (Enter inserts a newline, not a submit).
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Hash)]
pub enum NoteDateFilter {
    #[default]
    All,
    Today,
    Week,
    Month,
}

impl NoteDateFilter {
    pub const ALL: [NoteDateFilter; 4] = [NoteDateFilter::All, NoteDateFilter::Today, NoteDateFilter::Week, NoteDateFilter::Month];

    pub fn label(self) -> &'static str {
        match self {
            NoteDateFilter::All => "All",
            NoteDateFilter::Today => "Today",
            NoteDateFilter::Week => "7 Days",
            NoteDateFilter::Month => "30 Days",
        }
    }

    pub fn matches(self, note_time: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        match self {
            NoteDateFilter::All => true,
            NoteDateFilter::Today => {
                let local_note = note_time.with_timezone(&chrono::Local);
                let local_now = now.with_timezone(&chrono::Local);
                local_note.date_naive() == local_now.date_naive()
            }
            NoteDateFilter::Week => (now - note_time).num_seconds() <= 7 * 86_400,
            NoteDateFilter::Month => (now - note_time).num_seconds() <= 30 * 86_400,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub text: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Note {
    /// Score this note against a search query.
    /// Title matches are weighted heavier than body matches.
    pub fn match_query(&self, query: &str) -> Option<i32> {
        let q = query.trim();
        if q.is_empty() {
            return Some(0);
        }
        let title = title_of(&self.text);
        let title_score = fuzzy_score(q, &title).map(|s| s * 2 + 100);
        let body_score = fuzzy_score(q, &self.text);

        match (title_score, body_score) {
            (Some(ts), Some(bs)) => Some(ts.max(bs)),
            (Some(ts), None) => Some(ts),
            (None, Some(bs)) => Some(bs),
            (None, None) => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoteList {
    pub notes: Vec<Note>,
}

impl NoteList {
    /// Insert a new note at the top (newest first) and return it.
    pub fn add(&mut self, text: &str, now: DateTime<Utc>) -> &Note {
        let id = format!("{}-{}", now.timestamp_millis(), self.notes.len());
        self.notes.insert(0, Note { id, text: text.to_string(), created_at: now, updated_at: now });
        &self.notes[0]
    }

    /// Replace a note's text; `updated_at` only moves when the text changes.
    pub fn update(&mut self, id: &str, text: &str, now: DateTime<Utc>) -> bool {
        if let Some(n) = self.notes.iter_mut().find(|n| n.id == id) {
            if n.text != text {
                n.text = text.to_string();
                n.updated_at = now;
            }
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let n = self.notes.len();
        self.notes.retain(|n| n.id != id);
        self.notes.len() != n
    }

    pub fn get(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}

/// First non-empty line, trimmed — the row title.
pub fn title_of(text: &str) -> String {
    text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string()
}

/// Everything after the title line, whitespace-collapsed — the row preview.
pub fn preview_of(text: &str) -> String {
    let mut lines = text.lines().map(str::trim);
    // Skip up to and including the title line.
    for line in lines.by_ref() {
        if !line.is_empty() {
            break;
        }
    }
    lines.filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Accurate fuzzy matching with scoring:
/// Returns `Some(score)` if `query` matches `target`, or `None` if not.
///
/// Features:
/// - Case-insensitive
/// - Exact substring match gets a major bonus (+1000)
/// - Word prefix match gets +500
/// - Subsequence fuzzy match with word-boundary and consecutive char bonuses
/// - Penalty for excessive length
pub fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let t = target.to_lowercase();

    // Direct exact substring check
    if let Some(pos) = t.find(&q) {
        let mut score = 1000;
        if pos == 0 {
            score += 500; // starts with query
        } else if t.as_bytes().get(pos - 1).is_some_and(|b| b.is_ascii_whitespace() || *b == b'-' || *b == b'_' || *b == b'/') {
            score += 300; // starts at word boundary
        }
        score -= (pos as i32).min(100);
        score -= (t.len() as i32).min(200);
        return Some(score);
    }

    // Subsequence fuzzy match
    let q_chars: Vec<char> = q.chars().collect();
    let t_chars: Vec<char> = t.chars().collect();
    let mut q_idx = 0;
    let mut score = 100;
    let mut prev_matched_idx: Option<usize> = None;

    for (t_idx, &tc) in t_chars.iter().enumerate() {
        if q_idx < q_chars.len() && tc == q_chars[q_idx] {
            // Matched character
            score += 10;
            // Consecutive match bonus
            if let Some(prev) = prev_matched_idx {
                if prev + 1 == t_idx {
                    score += 15;
                }
            }
            // Word boundary bonus
            if t_idx == 0 || t_chars[t_idx - 1].is_whitespace() || matches!(t_chars[t_idx - 1], '-' | '_' | '/' | '.') {
                score += 25;
            }
            prev_matched_idx = Some(t_idx);
            q_idx += 1;
        }
    }

    if q_idx == q_chars.len() {
        score -= (t.len() as i32).min(150);
        Some(score.max(1))
    } else {
        None
    }
}

/// Formats a note's timestamp for clean display on the note card.
/// E.g. "Today, 14:30", "Yesterday, 09:15", "Oct 24, 14:30", or "Oct 24, 2025".
pub fn format_note_date(dt: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let local_dt = dt.with_timezone(&chrono::Local);
    let local_now = now.with_timezone(&chrono::Local);
    let days_diff = (local_now.date_naive() - local_dt.date_naive()).num_days();

    if days_diff == 0 {
        format!("Today, {}", local_dt.format("%H:%M"))
    } else if days_diff == 1 {
        format!("Yesterday, {}", local_dt.format("%H:%M"))
    } else if local_dt.date_naive().year() == local_now.date_naive().year() {
        local_dt.format("%b %d, %H:%M").to_string()
    } else {
        local_dt.format("%b %d, %Y").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    #[test]
    fn notes_are_newest_first_and_addressable() {
        let mut list = NoteList::default();
        let first = list.add("milk", t(10)).id.clone();
        let second = list.add("eggs", t(20)).id.clone();
        assert_eq!(list.notes[0].id, second);
        assert_eq!(list.notes[1].id, first);
        assert_eq!(list.get(&first).unwrap().text, "milk");
        assert!(list.remove(&first));
        assert!(!list.remove(&first));
        assert_eq!(list.notes.len(), 1);
    }

    #[test]
    fn update_touches_updated_at_only_on_change() {
        let mut list = NoteList::default();
        let id = list.add("draft", t(10)).id.clone();
        assert!(list.update(&id, "draft", t(99)));
        assert_eq!(list.get(&id).unwrap().updated_at, t(10));
        assert!(list.update(&id, "draft 2", t(99)));
        assert_eq!(list.get(&id).unwrap().text, "draft 2");
        assert_eq!(list.get(&id).unwrap().updated_at, t(99));
        assert!(!list.update("missing", "x", t(99)));
    }

    #[test]
    fn first_line_is_the_title_and_the_rest_previews() {
        assert_eq!(title_of("  Ship the notes page\nbody here"), "Ship the notes page");
        assert_eq!(preview_of("  Ship the notes page\nbody here"), "body here");
        assert_eq!(title_of("\n\n  later\nmore"), "later");
        assert_eq!(preview_of("\n\n  later\nmore"), "more");
        assert_eq!(title_of("   "), "");
        assert_eq!(preview_of(""), "");
        assert_eq!(preview_of("only a title"), "");
        assert_eq!(title_of("one line only"), "one line only");
    }

    #[test]
    fn fuzzy_search_accurate_and_ranked() {
        assert!(fuzzy_score("rust", "Rust Programming").is_some());
        assert!(fuzzy_score("prog", "Rust Programming").is_some());
        assert!(fuzzy_score("rp", "Rust Programming").is_some());
        assert!(fuzzy_score("xyz", "Rust Programming").is_none());

        // Exact match scores higher than subsequence
        let exact = fuzzy_score("rust", "rust").unwrap();
        let subseq = fuzzy_score("rt", "rust").unwrap();
        assert!(exact > subseq);

        let note = Note { id: "1".into(), text: "Grocery List\nMilk, eggs, and bread".into(), created_at: t(100), updated_at: t(100) };
        assert!(note.match_query("groc").is_some());
        assert!(note.match_query("milk").is_some());
        assert!(note.match_query("bread").is_some());
        assert!(note.match_query("coffee").is_none());
    }

    #[test]
    fn note_date_filter_works() {
        let now = t(1_000_000); // ~11.5 days
        let note_recent = t(990_000); // 10,000s ago (~2.7 hours)
        let note_old = t(10); // 999,990s ago (> 11 days)

        assert!(NoteDateFilter::All.matches(note_old, now));
        assert!(NoteDateFilter::Week.matches(note_recent, now));
        assert!(!NoteDateFilter::Week.matches(note_old, now));
    }
}
