//! Presentation layer: a pure scene description built from the view-model.
//!
//! `layout::build` -> `Frame { scene, hits }` -> painter (Direct2D). No
//! Windows types here, so every page is unit-testable on any OS.

pub mod eyes;
pub mod icons;
pub mod layout;
pub mod markdown;
pub mod pages;
pub mod scene;
pub mod state;
pub mod system;
pub mod textedit;
pub mod theme;
pub mod viz;
