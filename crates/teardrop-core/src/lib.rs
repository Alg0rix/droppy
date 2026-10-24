//! Teardrop core: pure, platform-agnostic logic.
//!
//! Everything here is unit-tested on any OS. The Windows app in
//! `crates/teardrop` only handles Win32 / Direct2D / WinRT plumbing
//! and delegates all decisions to these types.

pub mod clipboard;
pub mod droplet;
pub mod hud;
pub mod lyrics;
pub mod media;
pub mod pages;
pub mod pomodoro;
pub mod settings;
pub mod shelf;
pub mod spring;
pub mod system;
pub mod tasks;
pub mod water;

pub use clipboard::ClipboardRing;
pub use droplet::{DropletGeom, OutlinePt};
pub use hud::{Hud, HudKind};
pub use lyrics::{line_at, line_progress, parse_lrc, LyricLine};
pub use media::{Playback, Track};
pub use pages::Page;
pub use pomodoro::{Phase, Pomodoro};
pub use settings::{AiProvider, EyeStyle, IdleLook, Settings};
pub use shelf::{Shelf, ShelfItem};
pub use spring::Spring;
pub use system::SystemSnapshot;
pub use tasks::{Task, TaskList};
pub use water::WaterLog;
