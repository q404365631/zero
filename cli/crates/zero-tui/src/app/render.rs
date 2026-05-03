//! Top-level render — composes status bar, prompt, and the
//! per-mode pane into a single frame.

use chrono::{DateTime, Utc};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use std::time::Instant;

use crate::app::mode::Mode;
use crate::app::state::{ActiveOverlay, AppState};
use crate::widgets::conversation::ConversationPane;
use crate::widgets::live_stream::LiveStreamPane;
use crate::widgets::overlay::{FrictionPauseOverlay, RiskOverlay, StateOverlay, VerdictOverlay};
use crate::widgets::pane::{CockpitPane, DecisionsPane, HeatPane, PositionsPane};
use crate::widgets::picker::{PickerWidget, picker_rows};
use crate::widgets::prompt::PromptWidget;
use crate::widgets::statusbar::StatusBar;

/// Rows reserved for the live-stream pane when visible. Eight
/// rows = one header + seven event rows, comfortable on an 80×24
/// terminal without starving the conversation pane. Kept here
/// rather than on the widget so the layout math is the single
/// source of truth.
pub const LIVE_STREAM_ROWS: u16 = 8;

/// Minimum mode-area height we will retain before collapsing the
/// live-stream pane in a cramped terminal. Below this, we hide
/// the pane for this frame — the operator keeps their conversation
/// view and the `live_stream_visible` flag is untouched (so the
/// pane reappears as soon as the terminal grows).
const MIN_MODE_ROWS_WITH_STREAM: u16 = 6;

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    render_at(frame, state, Utc::now());
}

/// Render with an explicit wall-clock instant, used by snapshot
/// tests to produce a stable `feed:<age>s` string.
/// Maximum visible prompt rows. The buffer can hold more (capped
/// at `prompt::MAX_LINES`) but we never let the prompt eat more
/// than this many rows of screen real estate — the conversation
/// pane is the more important surface. Operators with a 6+ line
/// draft typically want to send it; the prompt scrolls internally
/// at that point (and we draw a "…" continuation indicator that
/// lands with the wrap pass — for M1, deeper drafts simply cap
/// the visible portion at the top).
const MAX_VISIBLE_PROMPT_ROWS: u16 = 6;

pub fn render_at(frame: &mut Frame<'_>, state: &AppState, now: DateTime<Utc>) {
    let size = frame.area();
    let prompt_rows = u16::try_from(state.prompt.height())
        .unwrap_or(u16::MAX)
        .clamp(1, MAX_VISIBLE_PROMPT_ROWS);
    // Picker rows — reserved above the prompt so the picker sits
    // directly between the conversation pane and the prompt. No
    // rows allocated when there is no active picker; the mode
    // pane reclaims the space.
    let picker_rows_needed = state
        .picker
        .as_ref()
        .map_or(0, picker_rows)
        .min(MAX_VISIBLE_PICKER_ROWS);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),                     // mode pane
            Constraint::Length(picker_rows_needed), // picker (0..=N)
            Constraint::Length(prompt_rows),        // prompt (1..=N)
            Constraint::Length(1),                  // status bar
        ])
        .split(size);

    // If the live-stream pane is visible AND the mode area has
    // room for both a meaningful conversation view and the full
    // pane, split vertically. Otherwise the mode area takes the
    // entire height (degrades gracefully on cramped terminals
    // without losing the operator's toggled preference).
    let mode_area = chunks[0];
    let (mode_rect, live_stream_rect) = if state.live_stream_visible
        && mode_area.height.saturating_sub(LIVE_STREAM_ROWS) >= MIN_MODE_ROWS_WITH_STREAM
    {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(MIN_MODE_ROWS_WITH_STREAM),
                Constraint::Length(LIVE_STREAM_ROWS),
            ])
            .split(mode_area);
        (split[0], Some(split[1]))
    } else {
        (mode_area, None)
    };

    render_mode(frame, state, mode_rect);

    if let Some(area) = live_stream_rect {
        frame.render_widget(
            LiveStreamPane {
                ring: &state.event_ring,
                theme: state.theme,
            },
            area,
        );
    }

    // Modal overlays paint on top of the mode pane only — status
    // bar + prompt stay visible so the operator never loses the
    // live engine reading while reading a modal. Overlays use the
    // full mode area (pre-split) so they stay centered and legible
    // even when the live-stream pane is carved out.
    if let Some(ov) = state.overlay.as_ref() {
        render_overlay(frame, state, ov, mode_area, now);
    }

    if picker_rows_needed > 0
        && let Some(picker) = state.picker.as_ref()
    {
        frame.render_widget(
            PickerWidget {
                picker,
                theme: state.theme,
            },
            chunks[1],
        );
    }

    let prompt = PromptWidget {
        prompt: &state.prompt,
        theme: state.theme,
    };
    let (cursor_col, cursor_row) = prompt.cursor_position();
    frame.render_widget(prompt, chunks[2]);
    // Clamp the cursor row to the visible window — if the buffer
    // grew past `MAX_VISIBLE_PROMPT_ROWS`, the cursor lands on the
    // last visible row rather than off-screen.
    let visible_cursor_row = cursor_row.min(prompt_rows.saturating_sub(1));
    frame.set_cursor_position((chunks[2].x + cursor_col, chunks[2].y + visible_cursor_row));

    let engine_snapshot = state.engine.read().clone();
    // Re-read the CLI-side rate bucket every frame. The
    // snapshot is O(µs) (Mutex lock + a clock tick), so paying
    // for it on each render is cheaper than routing the field
    // through the engine mirror — and the bucket is a CLI
    // process handle, not engine state, so `EngineState` is
    // the wrong home for it. The bucket handle on `AppState`
    // is `Clone`'d from `HttpClient::rate_budget()` at app
    // construction (see `zero::run_tui`).
    let rate_budget = state
        .rate_budget
        .as_ref()
        .map(zero_engine_client::RateBudget::snapshot);
    let status = StatusBar {
        mode: state.mode,
        engine: &engine_snapshot,
        theme: state.theme,
        now,
        rate_budget,
    };
    frame.render_widget(status, chunks[3]);
}

/// Hard ceiling on picker rows. `PICKER_MAX_VISIBLE` in
/// [`crate::app::picker`] is the semantic cap; this constant is a
/// layout-level guard that the picker never steals more than 6
/// conversation rows even if the catalog grows.
const MAX_VISIBLE_PICKER_ROWS: u16 = 6;

fn render_overlay(
    frame: &mut Frame<'_>,
    state: &AppState,
    ov: &ActiveOverlay,
    area: Rect,
    now: DateTime<Utc>,
) {
    match ov {
        ActiveOverlay::State => {
            let snap = state.engine.read().clone();
            frame.render_widget(
                StateOverlay {
                    engine: &snap,
                    theme: state.theme,
                    now,
                },
                area,
            );
        }
        ActiveOverlay::FrictionPause(fp) => {
            frame.render_widget(
                FrictionPauseOverlay {
                    pause: fp,
                    theme: state.theme,
                    // Render path uses monotonic Instant for pause
                    // arithmetic — the wall-clock `now` is only used
                    // for the state overlay's as-of age.
                    now: Instant::now(),
                },
                area,
            );
        }
        ActiveOverlay::Verdict(eval) => {
            frame.render_widget(
                VerdictOverlay {
                    evaluation: eval.as_ref(),
                    theme: state.theme,
                },
                area,
            );
        }
        ActiveOverlay::Risk { trigger, .. } => {
            let snap = state.engine.read().clone();
            frame.render_widget(
                RiskOverlay {
                    engine: &snap,
                    trigger: *trigger,
                    theme: state.theme,
                    now,
                },
                area,
            );
        }
    }
}

fn render_mode(frame: &mut Frame<'_>, state: &AppState, area: Rect) {
    match state.mode {
        Mode::Conversation => {
            let pane = ConversationPane {
                log: &state.log,
                theme: state.theme,
                scroll: state.log_scroll,
                screen_reader: state.screen_reader,
                verbose: state.verbose,
            };
            frame.render_widget(pane, area);
        }
        Mode::Positions => {
            let snap = state.engine.read().clone();
            frame.render_widget(
                PositionsPane {
                    engine: &snap,
                    theme: state.theme,
                },
                area,
            );
        }
        Mode::Decisions => {
            frame.render_widget(DecisionsPane { theme: state.theme }, area);
        }
        Mode::Heat => {
            let snap = state.engine.read().clone();
            frame.render_widget(
                HeatPane {
                    engine: &snap,
                    theme: state.theme,
                },
                area,
            );
        }
        Mode::Cockpit => {
            let snap = state.engine.read().clone();
            frame.render_widget(
                CockpitPane {
                    engine: &snap,
                    theme: state.theme,
                },
                area,
            );
        }
    }
}
