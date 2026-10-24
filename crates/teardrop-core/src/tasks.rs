//! Tiny to-do list.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub done: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskList {
    pub tasks: Vec<Task>,
}

impl TaskList {
    pub fn add(&mut self, title: &str, now: DateTime<Utc>) -> &Task {
        let title = title.trim().to_string();
        let id = format!("{}-{}", now.timestamp_millis(), self.tasks.len());
        self.tasks.push(Task { id, title, done: false, created_at: now });
        self.tasks.last().unwrap()
    }

    pub fn toggle(&mut self, id: &str) -> bool {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.done = !t.done;
            true
        } else { false }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let n = self.tasks.len();
        self.tasks.retain(|t| t.id != id);
        self.tasks.len() != n
    }

    pub fn open_count(&self) -> usize {
        self.tasks.iter().filter(|t| !t.done).count()
    }

    pub fn clear_done(&mut self) {
        self.tasks.retain(|t| !t.done);
    }
}
