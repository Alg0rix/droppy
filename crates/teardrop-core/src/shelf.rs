//! File shelf: dropped files kept handy.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShelfItem {
    pub path: String,
    pub name: String,
    pub pinned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shelf {
    pub items: Vec<ShelfItem>,
    pub max_items: usize,
}

impl Default for Shelf {
    fn default() -> Self {
        Self::new()
    }
}

impl Shelf {
    pub fn new() -> Self {
        Self { items: Vec::new(), max_items: 24 }
    }

    pub fn file_name_of(path: &str) -> String {
        path.replace('/', "\\").rsplit('\\').next().unwrap_or(path).to_string()
    }

    pub fn add(&mut self, path: &str, now: DateTime<Utc>) -> bool {
        if self.items.iter().any(|i| i.path == path) { return false; }
        self.items.push(ShelfItem { path: path.to_string(), name: Self::file_name_of(path), pinned_at: now });
        while self.items.len() > self.max_items {
            self.items.remove(0);
        }
        true
    }

    pub fn remove(&mut self, path: &str) -> bool {
        let n = self.items.len();
        self.items.retain(|i| i.path != path);
        self.items.len() != n
    }
}
