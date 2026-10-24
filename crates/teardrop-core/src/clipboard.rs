//! In-memory clipboard ring. Never persisted to disk.
#[derive(Debug, Clone, Default)]
pub struct ClipboardRing {
    items: Vec<String>,
    max: usize,
}

impl ClipboardRing {
    pub fn new() -> Self {
        Self { items: Vec::new(), max: 20 }
    }

    /// Returns false when ignored (empty, duplicate-of-head, or over size cap).
    pub fn push(&mut self, text: &str) -> bool {
        let t = text.trim();
        if t.is_empty() || t.len() > 32_768 { return false; }
        if self.items.first().map(|s| s == t).unwrap_or(false) { return false; }
        self.items.retain(|s| s != t);
        self.items.insert(0, t.to_string());
        self.items.truncate(self.max);
        true
    }

    /// Drop one entry (e.g. a calendar link that was copied to be pasted into Teardrop).
    pub fn remove(&mut self, text: &str) {
        let t = text.trim();
        self.items.retain(|s| s != t);
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}
