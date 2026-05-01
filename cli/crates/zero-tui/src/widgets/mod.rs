//! Widget library.
//!
//! `stat` is the honesty primitive — every number in the TUI
//! passes through it. `statusbar`, `prompt`, `conversation`, and
//! `pane` compose the M1 shell; verdict blocks, position-row
//! widgets, calibration bars, and scar cards land later.

pub mod calibration;
pub mod conversation;
pub mod live_stream;
pub mod overlay;
pub mod pane;
pub mod picker;
pub mod position_row;
pub mod prompt;
pub mod stat;
pub mod statusbar;
pub mod verdict;
