//! Scratch notes: a newest-first notebook of free-text memos.
//!
//! A note has no separate title field. The first non-empty line doubles as the
//! row title and the rest is the preview, so typing is one continuous block
//! (Enter inserts a newline, not a submit).
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub text: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoteList {
    pub notes: Vec<Note>,
}

impl NoteList {
    /// Insert a new note at the top (newest first) and return it.
    pub fn add(&mut self, text: &str, now: DateTime<Utc>) -> &Note {
        let id = format!("{}-{}", now.timestamp_millis(), self.notes.len());
        self.notes.insert(
            0,
            Note { id, text: text.to_string(), created_at: now, updated_at: now },
        );
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
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
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
}
