//! Teardrop core: pure, platform-agnostic logic.
//!
//! Everything here is unit-tested on any OS. The Windows app in
//! `crates/teardrop` only handles Win32 / Direct2D / WinRT plumbing
//! and delegates all decisions to these types.

pub mod apps;
pub mod clipboard;
pub mod droplet;
pub mod hud;
pub mod lyrics;
pub mod media;
pub mod mesh;
pub mod notes;
pub mod pages;
pub mod pomodoro;
pub mod search;
pub mod settings;
pub mod shelf;
pub mod spring;
pub mod sysmon;
pub mod system;
pub mod tasks;
pub mod water;

pub use apps::{AppCategory, AppTheme, EyeTheme};
pub use clipboard::ClipboardRing;
pub use droplet::{DropletGeom, OutlinePt};
pub use hud::{Hud, HudKind};
pub use lyrics::{line_at, line_progress, parse_lrc, LyricLine};
pub use media::{Playback, Track};
pub use notes::{format_note_date, fuzzy_score, preview_of, title_of, Note, NoteDateFilter, NoteList};
pub use pages::Page;
pub use pomodoro::{Phase, Pomodoro};
pub use settings::{AiProvider, EyeStyle, IdleLook, IslandTheme, Settings};
pub use shelf::{Shelf, ShelfItem};
pub use spring::Spring;
pub use system::SystemSnapshot;
pub use sysmon::{SysTab, SysView};
pub use tasks::{Task, TaskList};
pub use water::{add_label, fmt_glasses, glasses_label, WaterLog, GLASS_ML};
