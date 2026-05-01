//! App module — event loop, state, input dispatch, render.
//!
//! The [`App`] entry point takes a shared `EngineState` handle
//! (fed by a `WsSubscriber` owned elsewhere) and a
//! [`zero_commands::DispatchContext`], then runs the TUI event
//! loop until the operator exits.

pub mod event_ring;
pub mod input;
pub mod log;
pub mod mode;
pub mod picker;
pub mod prompt;
pub mod render;
pub mod session;
pub mod state;
pub mod terminal;

use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use parking_lot::RwLock;
use thiserror::Error;
use tokio::sync::broadcast;
use zero_commands::{DispatchContext, run_bypass_friction};
use zero_engine_client::{EngineEvent, EngineState};

pub use mode::Mode;
pub use session::SessionSink;
pub use state::{ActiveOverlay, AppState, FrictionOutcome, FrictionPause};

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
}

/// Summary returned by [`App::run`] on a clean shutdown.
///
/// Carries the pieces of session state the caller needs for
/// post-session bookkeeping — today just the `wrap_off` flag
/// the operator may have toggled with `/wrap-off`, so the
/// caller knows whether to run the daily wrap generator.
///
/// Kept as a `#[non_exhaustive]` struct so adding another
/// post-session signal later (e.g. an explicit wrap-format
/// override, or a milestone pending write) is additive — no
/// caller will break by reading `wrap_off` today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct AppExit {
    /// The operator used `/wrap-off` this session — suppress
    /// the daily wrap. Per Addendum A §9.1 the suppression is
    /// session-scoped; next session's wrap runs again.
    pub wrap_off: bool,
}

/// Interactive application entry point.
#[derive(Debug)]
pub struct App {
    state: AppState,
    ctx: DispatchContext,
    /// Optional tap on the `WsSubscriber`'s broadcast channel.
    /// When set, the event loop drains typed `EngineEvent`s into
    /// `state.event_ring` for the live-stream pane. When `None`
    /// (no subscriber, or caller opted out), the pane still
    /// renders — just with its honest empty state.
    events: Option<broadcast::Receiver<EngineEvent>>,
}

impl App {
    #[must_use]
    pub fn new(engine: Arc<RwLock<EngineState>>, ctx: DispatchContext) -> Self {
        let rate_budget = ctx.http.as_ref().and_then(|c| c.rate_budget().cloned());
        let mut state = AppState::new(engine);
        state.rate_budget = rate_budget;
        Self {
            state,
            ctx,
            events: None,
        }
    }

    /// Construct with an active session sink — prompts and
    /// dispatcher output will be persisted.
    #[must_use]
    pub fn new_with_sink(
        engine: Arc<RwLock<EngineState>>,
        ctx: DispatchContext,
        sink: SessionSink,
    ) -> Self {
        let rate_budget = ctx.http.as_ref().and_then(|c| c.rate_budget().cloned());
        let mut state = AppState::new_with_sink(engine, Some(sink));
        state.rate_budget = rate_budget;
        Self {
            state,
            ctx,
            events: None,
        }
    }

    /// Attach a broadcast receiver sourced from
    /// `WsSubscriber::events()`. Received events land in
    /// `AppState::event_ring` and — on broadcast lag — a
    /// synthetic "lagged" marker is recorded so the operator
    /// sees the drop instead of a silent pane. Takes `self` by
    /// value for a fluent `App::new(...).with_events(rx)` pattern.
    #[must_use]
    pub fn with_events(mut self, rx: broadcast::Receiver<EngineEvent>) -> Self {
        self.events = Some(rx);
        self
    }

    /// Mutable access for pre-launch seeding (welcome messages,
    /// retry counters, etc.).
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// Run the event loop until the user quits.
    ///
    /// Returns an [`AppExit`] summary on a clean shutdown so
    /// the caller can run post-session bookkeeping (daily wrap,
    /// milestone writes) without having to poke at `App`'s
    /// internal state. On an error, the error is returned; the
    /// caller must assume the session did not end cleanly and
    /// skip post-session I/O.
    ///
    /// # Errors
    /// Propagates any terminal I/O error.
    pub async fn run(mut self) -> Result<AppExit, AppError> {
        let mut term = terminal::TerminalGuard::init()?;
        let mut events = EventStream::new();
        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Initial draw so the operator sees the shell immediately
        // rather than a blank terminal on the first event wait.
        term.tty.draw(|f| render::render(f, &self.state))?;

        let run_result = self.drive(&mut term, &mut events, &mut ticker).await;

        // Close the session row regardless of how we got here.
        if let Some(sink) = &self.state.sink {
            sink.end();
        }

        run_result.map(|()| AppExit {
            wrap_off: self.state.wrap_off,
        })
    }

    async fn drive(
        &mut self,
        term: &mut terminal::TerminalGuard,
        events: &mut EventStream,
        ticker: &mut tokio::time::Interval,
    ) -> Result<(), AppError> {
        while !self.state.should_quit {
            tokio::select! {
                _ = ticker.tick() => {
                    self.tick_friction().await;
                    term.tty.draw(|f| render::render(f, &self.state))?;
                }
                maybe_event = events.next() => {
                    match maybe_event {
                        Some(Ok(Event::Key(key))) => {
                            // Only react to key *presses*. On
                            // platforms that emit release events
                            // (KittyKeyboard), we drop them so a
                            // single key press does not double-fire.
                            if matches!(
                                key.kind,
                                crossterm::event::KeyEventKind::Press
                                    | crossterm::event::KeyEventKind::Repeat,
                            ) {
                                input::handle_key(&mut self.state, key);
                            }
                        }
                        // Resize triggers a redraw at the end of
                        // the match. All other non-key events are
                        // dropped silently.
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            tracing::warn!(err = %e, "event stream error");
                        }
                        None => break,
                    }
                    // Drain any input the user submitted this tick.
                    if let Some(line) = self.state.pending_input.take() {
                        // Snapshot the TUI's current verbose state
                        // onto the context so `/verbose toggle`
                        // resolves into an absolute target at
                        // dispatch time. Cheap — `DispatchContext`
                        // is `Clone` and the with_verbose builder
                        // is a two-field copy.
                        let ctx = self
                            .ctx
                            .clone()
                            .with_verbose(self.state.verbose)
                            .with_wrap_off(self.state.wrap_off);
                        match zero_commands::dispatch(&ctx, &line).await {
                            Ok(Some(out)) => self.state.apply_dispatch(out),
                            Ok(None) => {}
                            Err(e) => tracing::warn!(err = ?e, "dispatch error"),
                        }
                    }
                    // A key might have completed a friction gate
                    // (L2: typed the confirm word); check every
                    // turn, not just on tick.
                    self.tick_friction().await;
                    term.tty.draw(|f| render::render(f, &self.state))?;
                }
                // Tap on the WS subscriber's broadcast channel.
                // Runs in the same select! so events update the
                // ring + trigger a redraw without waiting for the
                // 100 ms ticker — the live-stream pane should feel
                // near-instant. The arm is always present even when
                // no receiver is attached (falls through to a
                // pending future) so we do not have to rewrite the
                // macro based on construction config.
                ev = Self::next_engine_event(&mut self.events) => {
                    match ev {
                        Ok(event) => {
                            self.state.record_engine_event(event);
                            term.tty.draw(|f| render::render(f, &self.state))?;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            // Honest: mark the drop in the ring so
                            // the pane cannot look calm after we
                            // threw frames away. The subscriber's
                            // own broadcast buffer is 128 slots —
                            // a Lagged here means a genuine burst,
                            // not a pathological slow consumer.
                            tracing::warn!(skipped, "ws broadcast lagged");
                            self.state.record_events_lagged(skipped);
                            term.tty.draw(|f| render::render(f, &self.state))?;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            // Subscriber shut down. Disable the arm
                            // for the rest of the session so we do
                            // not busy-loop on a closed channel;
                            // the pane retains whatever history
                            // was already captured.
                            tracing::info!("ws broadcast channel closed");
                            self.events = None;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Helper future for the broadcast-channel branch of the
    /// event loop's `select!`. When no receiver is attached it
    /// stays pending forever — the other branches still drive
    /// the app normally. Keeps the select! macro readable
    /// without conditional compilation tricks.
    async fn next_engine_event(
        rx: &mut Option<broadcast::Receiver<EngineEvent>>,
    ) -> Result<EngineEvent, broadcast::error::RecvError> {
        match rx.as_mut() {
            Some(r) => r.recv().await,
            None => std::future::pending().await,
        }
    }

    /// If a friction-pause overlay is in the `Confirmed` state,
    /// consume it and re-dispatch the pending command via the
    /// bypass path. This is the only call site of
    /// [`run_bypass_friction`] inside the TUI — keeping it here
    /// preserves the rule that the dispatcher alone decides when
    /// to skip the friction ladder.
    async fn tick_friction(&mut self) {
        let now = Instant::now();
        if let Some(cmd) = self.state.take_confirmed_friction_command(now) {
            let out = run_bypass_friction(&self.ctx, cmd).await;
            self.state.apply_dispatch(out);
        }
        // M2 §4: after any friction-gate completion (which may
        // have reopened the prompt), re-evaluate the engine
        // mirror for L3+/guardrail-proximity and surface the
        // Risk overlay. This runs every 100 ms tick *and* every
        // input event so the overlay is visibly responsive
        // without flooding — the rate-limiter inside
        // `poll_risk_overlay` is what prevents re-open spam.
        self.state.poll_risk_overlay(now);
    }
}
