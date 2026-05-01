//! Full-screen-ish modal overlays painted on top of the mode pane.
//!
//! The only overlay in M1 is [`StateOverlay`] — the operator-state
//! overview triggered by `/state`. It is sourced from the engine's
//! `operator_state` mirror (ADR-016); the CLI never computes the
//! label locally. When the mirror is unpopulated the overlay says
//! so, honestly, rather than inventing a default.
//!
//! Design constraints (Addendum A §2.3 / §2.4):
//! - **Descriptive, not judgmental.** No emoji, no "you're doing
//!   great", no "calm down." The label + vector speak for
//!   themselves.
//! - **Shows its work.** Every classifier input (velocity,
//!   deviation, session, loss-reaction, re-entry, sleep-proxy) is
//!   printed, so the operator can see *why* a label is what it is.
//! - **One-key dismiss.** No buttons. Any key press closes the
//!   overlay; Ctrl+C still exits the terminal. See `input.rs`.

use std::time::Instant;

use chrono::{DateTime, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};
use zero_engine_client::{EngineState, Evaluation};
use zero_operator_state::Snapshot as OperatorSnapshot;
use zero_operator_state::friction::FrictionLevel;

use crate::app::state::FrictionPause;
use crate::theme::Theme;
use crate::widgets::verdict::VerdictBlock;

/// The `/state` overlay. See module docs.
#[derive(Debug)]
pub struct StateOverlay<'a> {
    pub engine: &'a EngineState,
    pub theme: Theme,
    /// Wall-clock "now" used for snapshot-age arithmetic. Tests pass
    /// a frozen instant for determinism.
    pub now: DateTime<Utc>,
}

/// Preferred minimum width/height. If the terminal is smaller we
/// clamp to whatever is available and the widget still paints
/// without panicking; it just loses some fields.
const PREFERRED_WIDTH: u16 = 64;
const PREFERRED_HEIGHT: u16 = 18;

impl Widget for StateOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rect = centered(area, PREFERRED_WIDTH, PREFERRED_HEIGHT);
        // Clear the area under the overlay so the mode pane's glyphs
        // don't bleed through at cells we don't rewrite.
        Clear.render(rect, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![Span::styled(
                " operator state ",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            )]))
            .border_style(Style::default().fg(self.theme.metadata));

        let inner = block.inner(rect);
        block.render(rect, buf);

        match &self.engine.operator_state {
            None => render_unseen(inner, buf, self.theme),
            Some(stat) => {
                render_snapshot(inner, buf, self.theme, &stat.value, stat.as_of, self.now);
            }
        }
    }
}

fn render_unseen(area: Rect, buf: &mut Buffer, theme: Theme) {
    let mut y = area.top();
    let put = |buf: &mut Buffer, y: &mut u16, spans: Vec<Span<'_>>| {
        if *y < area.bottom() {
            let line = Line::from(spans);
            let r = Rect {
                x: area.x,
                y: *y,
                width: area.width,
                height: 1,
            };
            line.render(r, buf);
            *y = y.saturating_add(1);
        }
    };
    put(
        buf,
        &mut y,
        vec![Span::styled(
            "engine has not reported operator state yet",
            Style::default().fg(theme.metadata),
        )],
    );
    y = y.saturating_add(1);
    put(
        buf,
        &mut y,
        vec![Span::styled(
            "→ ensure the engine is running with ADR-016 enabled,",
            Style::default().fg(theme.metadata),
        )],
    );
    put(
        buf,
        &mut y,
        vec![Span::styled(
            "  then reopen this overlay with /state",
            Style::default().fg(theme.metadata),
        )],
    );
    put_close_hint(buf, area, theme);
}

#[allow(clippy::too_many_lines)]
fn render_snapshot(
    area: Rect,
    buf: &mut Buffer,
    theme: Theme,
    snap: &OperatorSnapshot,
    as_of: DateTime<Utc>,
    now: DateTime<Utc>,
) {
    let mut y = area.top();
    let width = area.width;

    // ── Big label + friction ───────────────────────────────────────
    let label_color = theme.resolve_hint(snap.label.color_hint());
    let age_secs = (now - as_of).num_seconds().max(0);
    let age_str = format_age(age_secs);

    draw_line(
        buf,
        area,
        &mut y,
        width,
        vec![
            Span::styled("label  ", Style::default().fg(theme.metadata)),
            Span::styled(
                snap.label.short().to_string(),
                Style::default()
                    .fg(label_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("    ", Style::default()),
            Span::styled("friction ", Style::default().fg(theme.metadata)),
            Span::styled(
                format!("{:?}", snap.friction),
                Style::default().fg(theme.primary),
            ),
            Span::styled("    ", Style::default()),
            Span::styled("as-of ", Style::default().fg(theme.metadata)),
            Span::styled(age_str, Style::default().fg(theme.metadata)),
        ],
    );

    y = y.saturating_add(1);

    // ── State vector components ────────────────────────────────────
    draw_header(buf, area, &mut y, width, theme, "state vector");

    let v = &snap.vector;

    // Velocity row
    let baseline = v
        .velocity
        .baseline_1h
        .map_or("—".into(), |b| format!("{b:.1}/h"));
    draw_kv(
        buf,
        area,
        &mut y,
        width,
        theme,
        "velocity",
        &format!(
            "1h={}  4h={}  24h={}   baseline={}",
            v.velocity.last_1h, v.velocity.last_4h, v.velocity.last_24h, baseline
        ),
    );

    // Deviation
    let dev_10 = if v.deviation.verdicts_last_10 == 0 {
        "—".into()
    } else {
        format!(
            "{}/{} ({:.0}%)",
            v.deviation.overrides_last_10,
            v.deviation.verdicts_last_10,
            100.0 * v.deviation.rate_last_10(),
        )
    };
    draw_kv(
        buf,
        area,
        &mut y,
        width,
        theme,
        "deviation",
        &format!(
            "last-10={}   last-50={}/{}",
            dev_10, v.deviation.overrides_last_50, v.deviation.verdicts_last_50,
        ),
    );

    // Session
    let session_ms = v.session.active_duration_ms;
    let focus_ms = v.session.longest_focus_ms;
    let since_break_ms = v.session.since_last_break_ms;
    draw_kv(
        buf,
        area,
        &mut y,
        width,
        theme,
        "session",
        &format!(
            "active={}  longest-focus={}  since-break={}",
            format_ms(session_ms),
            format_ms(focus_ms),
            format_ms(since_break_ms),
        ),
    );

    // Loss reaction
    let lr_baseline = v.loss_reaction.baseline_ms.map_or("—".into(), format_ms);
    draw_kv(
        buf,
        area,
        &mut y,
        width,
        theme,
        "loss-reac",
        &format!(
            "median-10={}  fastest-session={}  baseline={}",
            format_ms(v.loss_reaction.median_last_10_ms),
            format_ms(v.loss_reaction.fastest_session_ms),
            lr_baseline,
        ),
    );

    // Re-entry
    draw_kv(
        buf,
        area,
        &mut y,
        width,
        theme,
        "re-entry",
        &format!(
            "15m={}  30m={}  2h={}",
            v.re_entry.within_15m, v.re_entry.within_30m, v.re_entry.within_2h,
        ),
    );

    // Sleep proxy + break flag
    let sleep = v
        .sleep_proxy
        .hours_since_rest_ended
        .map_or("—".into(), |h| format!("{h}h"));
    let on_break = if v.on_break { "yes" } else { "no" };
    draw_kv(
        buf,
        area,
        &mut y,
        width,
        theme,
        "sleep",
        &format!("hours-since-rest={sleep}   on-break={on_break}"),
    );

    put_close_hint(buf, area, theme);
}

fn draw_line(buf: &mut Buffer, area: Rect, y: &mut u16, width: u16, spans: Vec<Span<'_>>) {
    if *y >= area.bottom() {
        return;
    }
    let r = Rect {
        x: area.x,
        y: *y,
        width,
        height: 1,
    };
    Line::from(spans).render(r, buf);
    *y = y.saturating_add(1);
}

fn draw_header(buf: &mut Buffer, area: Rect, y: &mut u16, width: u16, theme: Theme, text: &str) {
    draw_line(
        buf,
        area,
        y,
        width,
        vec![Span::styled(
            text.to_string(),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        )],
    );
}

fn draw_kv(
    buf: &mut Buffer,
    area: Rect,
    y: &mut u16,
    width: u16,
    theme: Theme,
    key: &str,
    value: &str,
) {
    draw_line(
        buf,
        area,
        y,
        width,
        vec![
            Span::styled(format!("  {key:<10} "), Style::default().fg(theme.metadata)),
            Span::styled(value.to_string(), Style::default().fg(theme.primary)),
        ],
    );
}

fn put_close_hint(buf: &mut Buffer, area: Rect, theme: Theme) {
    if area.height == 0 {
        return;
    }
    let r = Rect {
        x: area.x,
        // Pin to the last row of the inner rect so the hint is
        // always visible regardless of how tall the vector block
        // grew.
        y: area.bottom().saturating_sub(1),
        width: area.width,
        height: 1,
    };
    Line::from(vec![Span::styled(
        "press any key to close",
        Style::default()
            .fg(theme.metadata)
            .add_modifier(Modifier::DIM),
    )])
    .render(r, buf);
}

/// Center a rect of preferred size inside `area`, clamping to the
/// available space. A terminal smaller than the preferred size just
/// uses the whole area.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

fn format_age(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m{}s ago", secs / 60, secs % 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

/// Friction-pause overlay — renders a visible countdown (L1) or a
/// countdown-plus-typed-confirm field (L2+). The overlay is read
/// from [`FrictionPause`]; completion is the event loop's job,
/// not the widget's.
///
/// Why the widget takes an `Instant` by argument instead of
/// calling `Instant::now()` itself: tests. We want deterministic
/// rendering at fractional countdown states.
#[derive(Debug)]
pub struct FrictionPauseOverlay<'a> {
    pub pause: &'a FrictionPause,
    pub theme: Theme,
    pub now: Instant,
}

/// Preferred size for the friction overlay. Narrower than the
/// state overlay — this is a gate, not a data dump.
const FRICTION_PREFERRED_WIDTH: u16 = 56;
const FRICTION_PREFERRED_HEIGHT: u16 = 11;

impl Widget for FrictionPauseOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rect = centered(area, FRICTION_PREFERRED_WIDTH, FRICTION_PREFERRED_HEIGHT);
        Clear.render(rect, buf);

        // Border color tracks severity — amber at L1, alert at L2+.
        let border_color = match self.pause.level {
            FrictionLevel::L0 => self.theme.metadata,
            FrictionLevel::L1 => self.theme.caution,
            FrictionLevel::L2 | FrictionLevel::L3 | FrictionLevel::L4 => self.theme.alert,
        };

        let title = format!(" friction {level:?} — pause ", level = self.pause.level);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![Span::styled(
                title,
                Style::default()
                    .fg(border_color)
                    .add_modifier(Modifier::BOLD),
            )]))
            .border_style(Style::default().fg(border_color));

        let inner = block.inner(rect);
        block.render(rect, buf);

        render_friction_body(inner, buf, self.theme, self.pause, self.now, border_color);
    }
}

fn render_friction_body(
    area: Rect,
    buf: &mut Buffer,
    theme: Theme,
    fp: &FrictionPause,
    now: Instant,
    severity: ratatui::style::Color,
) {
    let mut y = area.top();
    let width = area.width;

    // Line 1: the command being gated, for unambiguous context.
    // If the operator has three overlays across sessions they
    // should never have to guess which /execute this is.
    draw_line(
        buf,
        area,
        &mut y,
        width,
        vec![
            Span::styled("command  ", Style::default().fg(theme.metadata)),
            Span::styled(
                fp.command.name().to_string(),
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ],
    );

    // Line 2: countdown in tenths so the timer doesn't look
    // frozen between integer ticks. Severity-colored so at TILT
    // the red number is part of the friction, not decoration.
    draw_line(
        buf,
        area,
        &mut y,
        width,
        vec![
            Span::styled("pause    ", Style::default().fg(theme.metadata)),
            Span::styled(
                format_remaining(fp.remaining(now)),
                Style::default().fg(severity).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" / {}s", fp.pause.as_secs()),
                Style::default().fg(theme.metadata),
            ),
        ],
    );

    // Blank separator before the typed-confirm surface (L2+) or
    // the close hint (L1).
    y = y.saturating_add(1);

    if fp.confirm_word.is_some() {
        render_confirm_input(area, buf, theme, fp, now, severity, &mut y);
    }

    render_close_hint(area, buf, theme);
}

fn render_confirm_input(
    area: Rect,
    buf: &mut Buffer,
    theme: Theme,
    fp: &FrictionPause,
    now: Instant,
    severity: ratatui::style::Color,
    y: &mut u16,
) {
    let word = fp
        .confirm_word
        .as_deref()
        .expect("caller gates on confirm_word presence");
    let width = area.width;
    let pause_elapsed = fp.pause_elapsed(now);
    // Dim during the mandatory pause so the operator can *see*
    // that typing is being rejected; switch to severity color
    // once the field goes live.
    let input_color = if pause_elapsed {
        severity
    } else {
        theme.metadata
    };
    let prompt = if pause_elapsed {
        format!("type '{word}' then Enter")
    } else {
        format!("type '{word}' after pause")
    };
    draw_line(
        buf,
        area,
        y,
        width,
        vec![Span::styled(prompt, Style::default().fg(theme.metadata))],
    );

    // Input field with a trailing cursor glyph — solid once the
    // pause is over, faint while paused so the pane does not
    // look dead.
    let cursor = if pause_elapsed { "▊" } else { "▌" };
    draw_line(
        buf,
        area,
        y,
        width,
        vec![
            Span::styled("  > ", Style::default().fg(theme.metadata)),
            Span::styled(
                fp.confirm_input.clone(),
                Style::default()
                    .fg(input_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, Style::default().fg(input_color)),
        ],
    );

    // Match / mismatch hint — only after the pause, only when the
    // operator started typing, so "mismatch" doesn't flash the
    // moment the field opens.
    if pause_elapsed && !fp.confirm_input.is_empty() {
        let (text, color) = if fp.confirm_word_matches() {
            ("match — command will run on the next tick", theme.primary)
        } else if word.starts_with(fp.confirm_input.trim()) {
            ("keep typing…", theme.metadata)
        } else {
            ("mismatch — backspace to correct", theme.alert)
        };
        draw_line(
            buf,
            area,
            y,
            width,
            vec![Span::styled(text, Style::default().fg(color))],
        );
    }
}

fn render_close_hint(area: Rect, buf: &mut Buffer, theme: Theme) {
    if area.height == 0 {
        return;
    }
    let r = Rect {
        x: area.x,
        y: area.bottom().saturating_sub(1),
        width: area.width,
        height: 1,
    };
    Line::from(vec![Span::styled(
        "Esc to cancel · Ctrl+C exits zero",
        Style::default()
            .fg(theme.metadata)
            .add_modifier(Modifier::DIM),
    )])
    .render(r, buf);
}

/// Render the remaining pause as `Xs` at whole seconds and
/// `X.Ys` otherwise, with a floor of `0.0s` so the field never
/// blinks back to `Xs` after crossing zero.
fn format_remaining(d: std::time::Duration) -> String {
    if d.is_zero() {
        return "0.0s".into();
    }
    let total = d.as_millis();
    let seconds = total / 1000;
    let tenths = (total % 1000) / 100;
    if tenths == 0 {
        format!("{seconds}.0s")
    } else {
        format!("{seconds}.{tenths}s")
    }
}

/// Verdict overlay — centered modal that wraps [`VerdictBlock`]
/// in a bordered, titled frame and paints a close hint.
///
/// The overlay owns none of the rendering logic for the card
/// itself — that lives in [`VerdictBlock`] and is already unit-
/// tested against the widget module. The overlay is a presentation
/// wrapper: frame, clear, title, close hint. Any change to the
/// verdict card's shape happens in one place.
///
/// # Dismissal
///
/// Same as the state overlay — any key closes the overlay; the
/// input layer routes the dismissal, see `app::input`.
#[derive(Debug)]
pub struct VerdictOverlay<'a> {
    pub evaluation: &'a Evaluation,
    pub theme: Theme,
}

const VERDICT_PREFERRED_WIDTH: u16 = 72;
const VERDICT_PREFERRED_HEIGHT: u16 = 14;

impl Widget for VerdictOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rect = centered(area, VERDICT_PREFERRED_WIDTH, VERDICT_PREFERRED_HEIGHT);
        Clear.render(rect, buf);

        // Border color lifts `PASS`/`HOLD`/`REJECT` up to the title
        // so the outcome is visible even in the peripheral-vision
        // glance that a modal is designed for. Falls back to
        // metadata for unknown / missing verdicts so the border
        // never asserts an outcome the engine did not produce.
        // The verdict string is derived from real fields (see
        // `Evaluation::verdict`) rather than a wire field — an
        // empty `layers` list means there is nothing to derive
        // from, so fall through to the metadata color.
        let border_color =
            if self.evaluation.layers.is_empty() && self.evaluation.direction.is_none() {
                self.theme.metadata
            } else {
                match crate::widgets::verdict::VerdictSeverity::parse(self.evaluation.verdict()) {
                    crate::widgets::verdict::VerdictSeverity::Pass => self.theme.primary,
                    crate::widgets::verdict::VerdictSeverity::Hold => self.theme.caution,
                    crate::widgets::verdict::VerdictSeverity::Reject => self.theme.alert,
                    crate::widgets::verdict::VerdictSeverity::Unknown => self.theme.metadata,
                }
            };

        let title_text = match self.evaluation.coin.as_deref() {
            Some(c) if !c.is_empty() => format!(" verdict · {c} "),
            _ => " verdict ".to_string(),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![Span::styled(
                title_text,
                Style::default()
                    .fg(border_color)
                    .add_modifier(Modifier::BOLD),
            )]))
            .border_style(Style::default().fg(border_color));

        let inner = block.inner(rect);
        block.render(rect, buf);

        // Leave the last inner row for the close hint so the
        // hint is always visible no matter how many gates the
        // verdict has; `VerdictBlock` renders top-aligned and
        // truncates gracefully when it runs out of rows.
        let card_rows = inner.height.saturating_sub(1);
        let card_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: card_rows,
        };
        VerdictBlock {
            evaluation: self.evaluation,
            theme: self.theme,
        }
        .render(card_area, buf);

        put_close_hint(buf, inner, self.theme);
    }
}

fn format_ms(ms: u64) -> String {
    let s = ms / 1000;
    if s == 0 && ms > 0 {
        // Sub-second, but non-zero — print ms so tests that pass
        // small numbers can still see the shape.
        return format!("{ms}ms");
    }
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
    }
}

/// **M2 §4** risk overlay. Surfaces the engine's current `Risk`
/// block alongside the operator-state snapshot's vector components
/// so a TILT + guardrail-proximity situation is unambiguous: the
/// operator sees exactly *how close* to the hard alert they are,
/// and *why* the classifier flagged the operator as TILT.
///
/// Design constraints (same spirit as [`StateOverlay`]):
/// - Context surface, not a gate — does not own a pending
///   command, does not gate dispatch. Operator dismissal is any
///   keypress; the auto-open hook will re-fire on the next tick
///   if the guardrail signal is still live, subject to the 60 s
///   cooldown.
/// - Descriptive, not judgmental. Numbers and distances only.
/// - L4 HardStop opens this overlay with a banner that says the
///   engine is halted — the ceremony is *context*, not a bypass.
///   Risk-reducing commands (`/kill`, `/flatten`, `/cancel`)
///   still proceed through dispatch without the overlay
///   blocking them (see `two_am_scenarios.rs`).
#[derive(Debug)]
pub struct RiskOverlay<'a> {
    pub engine: &'a EngineState,
    pub trigger: crate::app::state::RiskOverlayTrigger,
    pub theme: Theme,
    pub now: DateTime<Utc>,
}

/// Preferred minimum size for the Risk overlay. Narrower than the
/// state overlay because the content is denser (fewer lines) and
/// on an 80×24 terminal we want both a visible conversation
/// margin and a centered card that cannot clip the "press any key"
/// hint.
const RISK_OVERLAY_WIDTH: u16 = 60;
const RISK_OVERLAY_HEIGHT: u16 = 16;

impl Widget for RiskOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rect = centered(area, RISK_OVERLAY_WIDTH, RISK_OVERLAY_HEIGHT);
        Clear.render(rect, buf);

        let (title_text, title_fg) = match self.trigger {
            crate::app::state::RiskOverlayTrigger::Friction(FrictionLevel::L4) => {
                (" engine halted — risk context ", self.theme.alert)
            }
            crate::app::state::RiskOverlayTrigger::Friction(FrictionLevel::L3) => {
                (" approaching guardrail — risk context ", self.theme.caution)
            }
            crate::app::state::RiskOverlayTrigger::Friction(_) => {
                (" risk context ", self.theme.primary)
            }
            crate::app::state::RiskOverlayTrigger::Proximity => {
                (" drawdown near alert — risk context ", self.theme.caution)
            }
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![Span::styled(
                title_text,
                Style::default().fg(title_fg).add_modifier(Modifier::BOLD),
            )]))
            .border_style(Style::default().fg(self.theme.metadata));
        let inner = block.inner(rect);
        block.render(rect, buf);

        render_risk_body(inner, buf, self.theme, self.engine, self.trigger, self.now);
    }
}

#[allow(clippy::too_many_lines)]
fn render_risk_body(
    area: Rect,
    buf: &mut Buffer,
    theme: Theme,
    engine: &EngineState,
    trigger: crate::app::state::RiskOverlayTrigger,
    now: DateTime<Utc>,
) {
    let mut y = area.top();
    let width = area.width;

    // ── Banner row — trigger reason ────────────────────────────
    let banner = match trigger {
        crate::app::state::RiskOverlayTrigger::Friction(FrictionLevel::L4) => (
            "HARD STOP",
            "engine halted; risk-reducing commands still go through",
            theme.alert,
        ),
        crate::app::state::RiskOverlayTrigger::Friction(FrictionLevel::L3) => (
            "L3 FRICTION",
            "tilt + drawdown close to guardrail",
            theme.caution,
        ),
        crate::app::state::RiskOverlayTrigger::Friction(_) => {
            ("CAUTION", "friction escalated", theme.caution)
        }
        crate::app::state::RiskOverlayTrigger::Proximity => (
            "PROXIMITY",
            "drawdown within 0.5 pp of last alert",
            theme.caution,
        ),
    };
    draw_line(
        buf,
        area,
        &mut y,
        width,
        vec![
            Span::styled(
                banner.0.to_string(),
                Style::default().fg(banner.2).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(banner.1.to_string(), Style::default().fg(theme.metadata)),
        ],
    );
    y = y.saturating_add(1);

    // ── /risk line ────────────────────────────────────────────
    draw_header(buf, area, &mut y, width, theme, "risk");
    match engine.risk.as_ref() {
        None => {
            draw_line(
                buf,
                area,
                &mut y,
                width,
                vec![Span::styled(
                    "  engine has not reported risk yet",
                    Style::default().fg(theme.metadata),
                )],
            );
        }
        Some(r) => {
            let risk = &r.value;
            let dd = risk.drawdown_pct.map_or("—".into(), |v| format!("{v:.2}%"));
            let alert = risk
                .last_drawdown_alert_pct
                .map_or("—".into(), |v| format!("{v:.2}%"));
            let distance = match (risk.drawdown_pct, risk.last_drawdown_alert_pct) {
                (Some(d), Some(a)) => format!("{:+.2}pp", a - d),
                _ => "—".into(),
            };
            draw_kv(
                buf,
                area,
                &mut y,
                width,
                theme,
                "drawdown",
                &format!("{dd}   alert-at {alert}   Δ {distance}"),
            );
            let equity = risk
                .account_value
                .map_or("—".into(), |v| format!("${v:.0}"));
            let peak = risk.peak_equity.map_or("—".into(), |v| format!("${v:.0}"));
            draw_kv(
                buf,
                area,
                &mut y,
                width,
                theme,
                "equity",
                &format!("{equity}   peak {peak}"),
            );
            let halt_state = if risk.is_halted() {
                let reason = risk.halt_reason.as_deref().unwrap_or("halted");
                format!("HALTED — {reason}")
            } else {
                "ok".to_string()
            };
            draw_kv(buf, area, &mut y, width, theme, "halt", &halt_state);
        }
    }
    y = y.saturating_add(1);

    // ── operator-state vector proximity components ────────────
    draw_header(buf, area, &mut y, width, theme, "state");
    match engine.operator_state.as_ref() {
        None => {
            draw_line(
                buf,
                area,
                &mut y,
                width,
                vec![Span::styled(
                    "  engine has not reported operator state yet",
                    Style::default().fg(theme.metadata),
                )],
            );
        }
        Some(s) => {
            let snap = &s.value;
            let label_color = theme.resolve_hint(snap.label.color_hint());
            let age = (now - s.as_of).num_seconds().max(0);
            draw_line(
                buf,
                area,
                &mut y,
                width,
                vec![
                    Span::styled("  label    ", Style::default().fg(theme.metadata)),
                    Span::styled(
                        snap.label.short().to_string(),
                        Style::default()
                            .fg(label_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("   friction ", Style::default().fg(theme.metadata)),
                    Span::styled(
                        format!("{:?}", snap.friction),
                        Style::default().fg(theme.primary),
                    ),
                    Span::styled("   as-of ", Style::default().fg(theme.metadata)),
                    Span::styled(format_age(age), Style::default().fg(theme.metadata)),
                ],
            );
            let v = &snap.vector;
            draw_kv(
                buf,
                area,
                &mut y,
                width,
                theme,
                "velocity",
                &format!(
                    "1h={} 4h={} 24h={}",
                    v.velocity.last_1h, v.velocity.last_4h, v.velocity.last_24h
                ),
            );
            draw_kv(
                buf,
                area,
                &mut y,
                width,
                theme,
                "re-entry",
                &format!(
                    "15m={} 30m={} 2h={}",
                    v.re_entry.within_15m, v.re_entry.within_30m, v.re_entry.within_2h
                ),
            );
            let sleep = v
                .sleep_proxy
                .hours_since_rest_ended
                .map_or("—".into(), |h| format!("{h}h"));
            draw_kv(
                buf,
                area,
                &mut y,
                width,
                theme,
                "sleep",
                &format!("hours-since-rest={sleep}"),
            );
        }
    }

    put_close_hint(buf, area, theme);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use zero_engine_client::{Source, Stat};
    use zero_operator_state::{Label, StateVector};

    fn render_overlay(engine: &EngineState, now: DateTime<Utc>) -> Vec<String> {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).expect("terminal");
        term.draw(|f| {
            let ov = StateOverlay {
                engine,
                theme: Theme::default(),
                now,
            };
            f.render_widget(ov, f.area());
        })
        .expect("draw");
        let buf = term.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    fn snapshot_at(label: Label, as_of: DateTime<Utc>) -> Stat<OperatorSnapshot> {
        let snap = OperatorSnapshot::new(label, StateVector::default(), as_of, 1);
        Stat::new(snap, Source::Http).with_as_of(as_of)
    }

    #[test]
    fn unseen_snapshot_shows_explanation_and_close_hint() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let engine = EngineState::new();
        let lines = render_overlay(&engine, now);
        let joined = lines.join("\n");
        assert!(joined.contains("not reported"), "{joined}");
        assert!(joined.contains("/state"), "{joined}");
        assert!(joined.contains("press any key to close"), "{joined}");
    }

    #[test]
    fn populated_snapshot_shows_label_friction_and_vector_keys() {
        let as_of = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let now = as_of + chrono::Duration::seconds(12);
        let mut engine = EngineState::new();
        engine.operator_state = Some(snapshot_at(Label::Elevated, as_of));
        let lines = render_overlay(&engine, now);
        let joined = lines.join("\n");
        assert!(joined.contains("ELEVATED"), "{joined}");
        assert!(joined.contains("friction"), "{joined}");
        assert!(joined.contains("L1"), "{joined}");
        assert!(joined.contains("state vector"), "{joined}");
        for key in [
            "velocity",
            "deviation",
            "session",
            "loss-reac",
            "re-entry",
            "sleep",
        ] {
            assert!(joined.contains(key), "missing {key} in: {joined}");
        }
        assert!(joined.contains("12s ago"), "{joined}");
    }

    #[test]
    fn tiny_terminal_does_not_panic() {
        // 20×4 is smaller than the preferred size; the widget must
        // clamp and continue to paint.
        let as_of = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        engine.operator_state = Some(snapshot_at(Label::Tilt, as_of));
        let backend = TestBackend::new(20, 4);
        let mut term = Terminal::new(backend).expect("terminal");
        term.draw(|f| {
            let ov = StateOverlay {
                engine: &engine,
                theme: Theme::default(),
                now: as_of,
            };
            f.render_widget(ov, f.area());
        })
        .expect("tiny draw must not panic");
    }

    #[test]
    fn format_age_boundaries() {
        assert_eq!(format_age(0), "0s ago");
        assert_eq!(format_age(59), "59s ago");
        assert_eq!(format_age(60), "1m0s ago");
        assert_eq!(format_age(3599), "59m59s ago");
        assert_eq!(format_age(3600), "1h ago");
    }

    #[test]
    fn format_ms_boundaries() {
        assert_eq!(format_ms(0), "0s");
        assert_eq!(format_ms(500), "500ms");
        assert_eq!(format_ms(1_000), "1s");
        assert_eq!(format_ms(59_000), "59s");
        assert_eq!(format_ms(60_000), "1m00s");
        assert_eq!(format_ms(3_600_000), "1h00m");
    }

    // ── FrictionPauseOverlay tests ────────────────────────────────

    use std::time::Duration;
    use zero_commands::Command;
    use zero_operator_state::friction::FrictionLevel;

    fn render_friction_at(fp: &FrictionPause, now: Instant) -> Vec<String> {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).expect("terminal");
        term.draw(|f| {
            let w = FrictionPauseOverlay {
                pause: fp,
                theme: Theme::default(),
                now,
            };
            f.render_widget(w, f.area());
        })
        .expect("draw");
        let buf = term.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn l1_pause_shows_command_countdown_and_close_hint() {
        let started = Instant::now();
        let fp = FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L1,
            started_at: started,
            pause: Duration::from_secs(3),
            confirm_word: None,
            confirm_input: String::new(),
        };
        let lines = render_friction_at(&fp, started + Duration::from_millis(1_500));
        let joined = lines.join("\n");
        assert!(joined.contains("friction L1"), "{joined}");
        assert!(joined.contains("/execute"), "{joined}");
        assert!(joined.contains("1.5s"), "countdown tenths: {joined}");
        assert!(joined.contains("/ 3s"), "total pause shown: {joined}");
        assert!(joined.contains("Esc to cancel"), "{joined}");
        // L1 should never render a confirm-word prompt.
        assert!(
            !joined.contains("type '"),
            "L1 overlay must not show a confirm word"
        );
    }

    #[test]
    fn l2_overlay_shows_confirm_word_and_dim_field_during_pause() {
        let started = Instant::now();
        let fp = FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L2,
            started_at: started,
            pause: Duration::from_secs(10),
            confirm_word: Some("execute".into()),
            confirm_input: String::new(),
        };
        let lines = render_friction_at(&fp, started + Duration::from_secs(3));
        let joined = lines.join("\n");
        assert!(joined.contains("friction L2"), "{joined}");
        assert!(joined.contains("type 'execute' after pause"), "{joined}");
        assert!(joined.contains("7.0s"), "remaining shown: {joined}");
    }

    #[test]
    fn l2_overlay_shows_accept_prompt_once_pause_elapses() {
        let started = Instant::now()
            .checked_sub(Duration::from_secs(11))
            .expect("monotonic Instant supports 11s subtraction");
        let fp = FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L2,
            started_at: started,
            pause: Duration::from_secs(10),
            confirm_word: Some("execute".into()),
            confirm_input: "exec".into(),
        };
        let lines = render_friction_at(&fp, Instant::now());
        let joined = lines.join("\n");
        assert!(joined.contains("type 'execute' then Enter"), "{joined}");
        assert!(joined.contains("exec"), "confirm buffer shown: {joined}");
        assert!(
            joined.contains("keep typing"),
            "prefix-match hint: {joined}"
        );
    }

    #[test]
    fn l2_overlay_surfaces_mismatch_when_wrong_word_typed() {
        let started = Instant::now()
            .checked_sub(Duration::from_secs(11))
            .expect("monotonic Instant supports 11s subtraction");
        let fp = FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L2,
            started_at: started,
            pause: Duration::from_secs(10),
            confirm_word: Some("execute".into()),
            confirm_input: "zzz".into(),
        };
        let lines = render_friction_at(&fp, Instant::now());
        let joined = lines.join("\n");
        assert!(joined.contains("mismatch"), "{joined}");
    }

    #[test]
    fn l2_overlay_reports_match_when_word_complete() {
        let started = Instant::now()
            .checked_sub(Duration::from_secs(11))
            .expect("monotonic Instant supports 11s subtraction");
        let fp = FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L2,
            started_at: started,
            pause: Duration::from_secs(10),
            confirm_word: Some("execute".into()),
            confirm_input: "execute".into(),
        };
        let lines = render_friction_at(&fp, Instant::now());
        let joined = lines.join("\n");
        assert!(joined.contains("match"), "{joined}");
    }

    #[test]
    fn format_remaining_boundaries() {
        assert_eq!(format_remaining(Duration::ZERO), "0.0s");
        assert_eq!(format_remaining(Duration::from_millis(100)), "0.1s");
        assert_eq!(format_remaining(Duration::from_millis(1_000)), "1.0s");
        assert_eq!(format_remaining(Duration::from_millis(2_900)), "2.9s");
        assert_eq!(format_remaining(Duration::from_secs(10)), "10.0s");
    }

    // ── VerdictOverlay tests ──────────────────────────────────────

    use zero_engine_client::Evaluation;
    use zero_engine_client::models::EvaluationLayer;

    fn render_verdict(eval: &Evaluation, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).expect("terminal");
        term.draw(|f| {
            let w = VerdictOverlay {
                evaluation: eval,
                theme: Theme::default(),
            };
            f.render_widget(w, f.area());
        })
        .expect("draw");
        let buf = term.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    fn pass_eval() -> Evaluation {
        Evaluation {
            coin: Some("BTC".into()),
            direction: Some("LONG".into()),
            conviction: Some(0.72),
            regime: Some("trending".into()),
            consensus: Some(8),
            layers: vec![
                EvaluationLayer {
                    layer: "layer_0".into(),
                    passed: true,
                    value: serde_json::Value::Null,
                    detail: String::new(),
                },
                EvaluationLayer {
                    layer: "layer_1".into(),
                    passed: true,
                    value: serde_json::Value::Null,
                    detail: String::new(),
                },
                EvaluationLayer {
                    layer: "layer_2".into(),
                    passed: true,
                    value: serde_json::Value::Null,
                    detail: String::new(),
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn verdict_overlay_title_carries_coin_and_card_renders_chip() {
        let lines = render_verdict(&pass_eval(), 80, 16);
        let joined = lines.join("\n");
        assert!(
            joined.contains("verdict · BTC"),
            "title missing coin: {joined}"
        );
        assert!(joined.contains("PASS"), "chip missing: {joined}");
        assert!(joined.contains("conf 72%"), "confidence missing: {joined}");
        assert!(
            joined.contains("press any key to close"),
            "close hint missing: {joined}"
        );
    }

    #[test]
    fn verdict_overlay_empty_eval_shows_honest_card() {
        let eval = Evaluation {
            coin: Some("BTC".into()),
            ..Default::default()
        };
        let lines = render_verdict(&eval, 80, 10);
        let joined = lines.join("\n");
        // The card's own empty-state line survives into the overlay.
        assert!(
            joined.contains("no verdict"),
            "empty card leaked through overlay: {joined}"
        );
        // Must NOT leak a fake chip through the overlay frame.
        for needle in [" PASS ", " HOLD ", " REJECT "] {
            assert!(!joined.contains(needle), "fake {needle} leaked: {joined}");
        }
    }

    #[test]
    fn verdict_overlay_missing_coin_keeps_plain_title() {
        let eval = Evaluation {
            direction: Some("NONE".into()),
            layers: vec![EvaluationLayer {
                layer: "layer_0".into(),
                passed: true,
                value: serde_json::Value::Null,
                detail: String::new(),
            }],
            ..Default::default()
        };
        let lines = render_verdict(&eval, 80, 10);
        let joined = lines.join("\n");
        assert!(joined.contains("verdict"), "title missing: {joined}");
        // No `·` divider when there is no coin name to follow it.
        assert!(
            !joined.contains("· "),
            "title must not have dangling separator: {joined}"
        );
    }

    #[test]
    fn verdict_overlay_tiny_terminal_does_not_panic() {
        let eval = pass_eval();
        let backend = TestBackend::new(20, 4);
        let mut term = Terminal::new(backend).expect("terminal");
        term.draw(|f| {
            let w = VerdictOverlay {
                evaluation: &eval,
                theme: Theme::default(),
            };
            f.render_widget(w, f.area());
        })
        .expect("tiny draw must not panic");
    }
}
