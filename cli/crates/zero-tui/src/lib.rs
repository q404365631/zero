//! ratatui application — shell, status bar, prompt, conversation,
//! live-stream pane, and the widget library.
//!
//! The application owns nothing business-logical; it is a renderer
//! over `EngineState` and a dispatcher to `zero-commands`.

#![allow(clippy::module_name_repetitions)]

pub mod app;
pub mod layout;
pub mod theme;
pub mod widgets;

pub use app::{ActiveOverlay, App, AppError, AppExit, AppState, Mode};
pub use theme::Theme;
