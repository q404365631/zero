//! Top-level app state — composed of mode, conversation log,
//! prompt buffer, and a shared handle to the engine-state mirror.
//!
//! Ownership rules:
//!   - `AppState` is !Send because of the crossterm backend it
//!     will eventually render through; keep it single-threaded.
//!   - The engine-state mirror is `Arc<RwLock<EngineState>>` and
//!     is written only by the WS subscriber task; the app reads
//!     it.
//!   - Command execution is async and happens in the event loop,
//!     not in `submit_prompt`. `submit_prompt` only buffers the
//!     line into [`AppState::pending_input`]; the loop drains it.
//!   - Persistence is optional. When a [`SessionSink`] is present,
//!     every log entry is recorded synchronously.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use zero_commands::{
    Command, DispatchOutput, FrictionDecision, ModeTarget, OutputLine, OverlayTarget, ReplayKind,
};
use zero_engine_client::{EngineEvent, EngineState, RateBudget};
use zero_operator_state::friction::FrictionLevel;

use crate::app::event_ring::EventRing;
use crate::app::log::{ConversationLog, EntryKind, LogEntry};
use crate::app::mode::Mode;
use crate::app::picker::SlashPicker;
use crate::app::prompt::PromptBuffer;
use crate::app::session::SessionSink;
use crate::theme::Theme;

// Each boolean on `AppState` tracks an orthogonal operator-
// facing toggle (quit intent, screen-reader mode, live-stream
// pane visibility, verbose rendering). Collapsing them into a
// state machine would obscure the fact that any combination is
// valid — e.g. an operator with screen_reader+verbose both on
// is a real configuration. Allow the lint with the rationale
// inline so a later contributor does not have to rediscover it.
/// First-live-trade ceremony copy (§8.4, exact shape).
///
/// Three short system lines — acknowledge, contextualise,
/// orient. No celebration language. The block renders
/// inline in the conversation pane between the engine
/// event stream and the next prompt, so the operator sees
/// it the same way they see any other system line.
///
/// Kept as a module-level constant so the copy is testable
/// by reference rather than by re-matching a regex, and so
/// a future spec tweak is a one-file PR.
const CEREMONY_LINES: &[&str] = &[
    "first live position observed.",
    "from here on every fill is real. so is every loss.",
    "type /risk to see what the engine is watching for you. /break takes you out of the seat.",
];

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
pub struct AppState {
    pub mode: Mode,
    pub log: ConversationLog,
    pub prompt: PromptBuffer,
    pub theme: Theme,
    pub engine: Arc<RwLock<EngineState>>,
    pub should_quit: bool,
    /// Buffered input line waiting for the event loop to dispatch
    /// it. `None` between submissions.
    pub pending_input: Option<String>,
    /// Optional session persistence. When set, every log entry is
    /// recorded.
    pub sink: Option<SessionSink>,
    /// Currently visible modal overlay. `Some` grabs input: the
    /// next key press dismisses (except Ctrl+C, which still exits).
    /// `None` is the normal prompt-editing state.
    pub overlay: Option<ActiveOverlay>,
    /// Slash-command picker. `Some` only when the prompt's first
    /// row starts with `/` and there is no modal overlay active.
    /// Rebuilt by [`AppState::refresh_picker`] after every input
    /// event; the selection is preserved across rebuilds when the
    /// previous highlighted entry still matches the new filter.
    pub picker: Option<SlashPicker>,
    /// Whether the operator has opted in to screen-reader-
    /// friendly rendering (plain ASCII, role prefixes, no dimming).
    /// Toggled by `Ctrl+R`; persists for the session only.
    pub screen_reader: bool,
    /// Conversation scrollback offset in rows from the bottom.
    /// `0` means "stuck to bottom" — new entries auto-scroll into
    /// view. Any non-zero value means the operator has scrolled up
    /// and should *not* be yanked back by background traffic.
    pub log_scroll: u16,
    /// Bounded ring of engine events sourced from the WS
    /// subscriber's broadcast channel. Rendered by the
    /// live-stream pane (`]` to toggle). The ring is always
    /// populated — even when the pane is hidden — so toggling
    /// it on immediately shows recent activity rather than a
    /// blank surface.
    pub event_ring: EventRing,
    /// Whether the live-stream pane is currently visible. Toggled
    /// by `]`. Starts hidden so the conversation pane owns the
    /// full height on launch — operators opt in when they want
    /// the firehose.
    pub live_stream_visible: bool,
    /// Whether verbose rendering is active. Today this expands
    /// log timestamps from `HH:MM:SS` to `YYYY-MM-DD HH:MM:SS`
    /// (honest across sessions that span midnight) and is
    /// reserved for future "rich event detail" toggles. Toggled
    /// by `/verbose`. Starts `false` so the conversation pane
    /// defaults to compact wording — operators opt in when they
    /// want the fuller trace.
    pub verbose: bool,
    /// Whether the daily-wrap generator is suppressed for this
    /// session. Set by `/wrap-off`; the operator cannot make it
    /// sticky — per ADDENDUM_A §9.1 next session's wrap runs
    /// again. The field is session-scoped, never persisted, and
    /// read by the binary's `run_tui` exit path (via
    /// [`crate::AppExit::wrap_off`]) to decide whether to run
    /// the wrap generator before returning to the shell.
    pub wrap_off: bool,
    /// Outstanding coaching notices waiting for the operator
    /// to acknowledge via `/continue`. Today the coaching
    /// stream does not emit anything into this buffer (no
    /// engine-side coaching channel has landed), so the field
    /// is initialized empty and `/coaching reset` is an honest
    /// no-op on the receiving end. Kept here rather than
    /// inside a future coaching module because the dispatcher
    /// already sends a `coaching_reset` signal, and having the
    /// buffer alongside the other orthogonal toggles keeps the
    /// state contract observable in one place.
    pub coaching_notices: Vec<String>,
    /// Handle on the CLI-side `RateBudget` attached to the
    /// `HttpClient`. Cloned at app construction (the handle is
    /// an `Arc`, O(1) clone). The status-bar widget reads a
    /// `BudgetSnapshot` from this every frame to render the
    /// `rate:N/M` segment. `None` means no bucket was attached
    /// (e.g. `--no-persist` + no API token + offline-only tests)
    /// — the widget falls back to `rate:?` in that case.
    pub rate_budget: Option<RateBudget>,
    /// Latch for the first-live-trade ceremony (§8.4). Starts
    /// `true` in two cases: (a) no session store is attached
    /// (`--no-persist`) — without a store we cannot honor
    /// "once ever", and a ceremony-on-every-run would be a
    /// regression, so we suppress it; (b) the
    /// [`zero_session::milestones::FIRST_LIVE_TRADE_AT`]
    /// milestone is already set in the store — the operator
    /// has traded before and does not need a first-trade
    /// greeting. On ingesting a `Positions` event whose open
    /// set is non-empty, if this field is still `false`, the
    /// ceremony is rendered + persisted + the latch flips.
    /// Session-scoped; never reset within a run.
    pub first_live_trade_recorded: bool,
    /// **M2 §4** — monotonic instant of the most recent risk-
    /// overlay dismissal. The auto-open hook refuses to reopen a
    /// Risk overlay within [`Self::RISK_DISMISS_COOLDOWN`] of
    /// this timestamp *unless* the trigger strictly escalates
    /// (L3 → L4) or a fresh guardrail threshold trips (see
    /// [`Self::risk_overlay_last_seen_alert_pct`]). Operators
    /// pushing through `Esc` on a steady L3 must not be
    /// bombarded — §4 of `M2_PLAN.md`.
    pub risk_overlay_last_dismissed_at: Option<Instant>,
    /// **M2 §4** — the `Risk.last_drawdown_alert_pct` value
    /// observed at the moment the operator last dismissed a
    /// Risk overlay. Compared against the engine's current
    /// value each tick: a change means the engine has tripped a
    /// *new* guardrail threshold, which overrides the
    /// dismiss-cooldown and forces the overlay back open. `None`
    /// means "no dismissal pending" — the first open does not
    /// consult this field.
    pub risk_overlay_last_seen_alert_pct: Option<f64>,
    /// **M2 §4** — the trigger the most-recently open Risk
    /// overlay was built against. Used to detect L3 → L4
    /// escalation inside the dismiss cooldown: if the incoming
    /// trigger is strictly stronger (L4 beats L3; Proximity +
    /// L3 combined is already L3's territory and does not
    /// re-open). `None` when no overlay is currently or
    /// recently open.
    pub risk_overlay_last_trigger: Option<RiskOverlayTrigger>,
}

/// Why the [`ActiveOverlay::Risk`] overlay opened.
///
/// Populated at open-time by the auto-open hook so the widget can
/// render context-specific copy (L3 "approaching guardrail", L4
/// "engine halted"), and so the rate-limiter can distinguish
/// "same trigger, operator already saw it" from "new escalation,
/// operator must see it again within the cooldown".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskOverlayTrigger {
    /// The operator-state snapshot the engine returned carries
    /// `FrictionLevel::L3` (TILT + guardrail proximity) or `L4`
    /// (TILT + halt). The level is carried so re-open logic can
    /// detect L3 → L4 escalations and bypass the dismiss-cooldown.
    Friction(FrictionLevel),
    /// The engine's `Risk.drawdown_pct` is within
    /// [`AppState::GUARDRAIL_PROXIMITY_PP`] of its
    /// `Risk.last_drawdown_alert_pct` threshold. This fires even
    /// without an L3 classifier verdict — the engine's own
    /// proximity reading is authoritative for "you are about to
    /// trip a guardrail."
    Proximity,
}

/// Runtime representation of a live overlay. This exists as a
/// separate enum (not just `OverlayTarget`) so overlays can carry
/// ephemeral state — scroll offset, filter text, a timer, a
/// typed-confirm buffer — that the input + render layers mutate
/// without going back through dispatch.
#[derive(Debug, Clone)]
pub enum ActiveOverlay {
    /// The operator-state overview. Sourced every render from
    /// `engine.operator_state`; carries no state of its own.
    State,
    /// Per-coin gate verdict. Carries the full
    /// [`zero_engine_client::Evaluation`] returned by the engine
    /// so the overlay renders exactly what the engine said — no
    /// local recomputation, no synthetic fields.
    Verdict(Box<zero_engine_client::Evaluation>),
    /// Friction pause — visible countdown and, at L2+, a typed
    /// confirmation before a risk-increasing command runs. The
    /// overlay owns the pending [`Command`] so the event loop can
    /// re-dispatch via `run_bypass_friction` on completion.
    FrictionPause(FrictionPause),
    /// **M2 §4** risk overlay. Opens automatically when the
    /// operator-state snapshot reports L3/L4 or the engine reports
    /// drawdown within [`AppState::GUARDRAIL_PROXIMITY_PP`] of the
    /// last hard-alert threshold. The overlay never owns a
    /// pending command — unlike [`ActiveOverlay::FrictionPause`],
    /// it is a *context* surface, not a *gate*. The operator
    /// dismisses with any key; the auto-open hook honors a
    /// 60 s cooldown (see [`AppState::RISK_DISMISS_COOLDOWN`])
    /// unless the trigger strictly escalates.
    Risk {
        trigger: RiskOverlayTrigger,
        /// Monotonic `Instant::now()` at the moment the overlay
        /// opened. Not used for timing inside this struct — the
        /// rate-limiter anchors on
        /// `AppState::risk_overlay_last_dismissed_at` — but kept
        /// here so tests, logs, and future "overlay has been up
        /// for Xs" surfaces have a single source of truth.
        opened_at: Instant,
    },
}

/// Ordering among Risk overlay triggers used by the auto-open
/// hook to decide whether a *new* trigger observed on the
/// current tick warrants re-opening an already-dismissed
/// overlay before the 60 s cooldown expires. The rule is
/// strictly "safety-upward": L4 beats L3 beats Proximity;
/// equal-strength triggers never bypass the cooldown.
fn trigger_rank(t: RiskOverlayTrigger) -> u8 {
    match t {
        RiskOverlayTrigger::Proximity => 1,
        RiskOverlayTrigger::Friction(FrictionLevel::L3) => 2,
        RiskOverlayTrigger::Friction(FrictionLevel::L4) => 3,
        // Defensive: L0..L2 should never reach the auto-open
        // hook (poll_risk_overlay filters by L3+/Proximity),
        // but if they do, treat them as the weakest so they
        // cannot accidentally bypass the cooldown.
        RiskOverlayTrigger::Friction(_) => 0,
    }
}

fn trigger_strictly_escalates(prev: RiskOverlayTrigger, next: RiskOverlayTrigger) -> bool {
    trigger_rank(next) > trigger_rank(prev)
}

impl ActiveOverlay {
    /// Construct an overlay from a [`OverlayTarget`] signal
    /// emitted by the dispatcher. Only applies to self-contained
    /// overlays; friction overlays are built separately because
    /// they need the full [`FrictionDecision`] + pending
    /// [`Command`].
    #[must_use]
    pub fn from_target(t: OverlayTarget) -> Self {
        match t {
            OverlayTarget::State => Self::State,
            OverlayTarget::Verdict(eval) => Self::Verdict(eval),
        }
    }
}

/// Live state of a friction-pause overlay.
///
/// The overlay's lifecycle in M1:
/// 1. Dispatcher returns `FrictionDecision::Pause | TypedConfirm`
///    plus `pending_command = Some(cmd)`.
/// 2. [`AppState::apply_dispatch`] opens a [`FrictionPause`] with
///    `started_at = now`.
/// 3. The TUI render path draws a countdown + (L2) an input box.
/// 4. Operator hits `Esc` → [`AppState::dismiss_overlay`]; the
///    pending command is discarded.
/// 5. L1: the event loop's tick handler polls [`is_complete`];
///    when the pause elapses the overlay is closed and the
///    command is re-dispatched with
///    [`zero_commands::run_bypass_friction`].
/// 6. L2: the operator types into `confirm_input`; when the
///    pause has also elapsed and the buffer matches
///    `confirm_word`, the same completion path runs. Typing the
///    word before the pause ends does nothing — the pause is
///    mandatory (§3, Addendum A). The widget dims the input
///    during the pause to make this visible.
#[derive(Debug, Clone)]
pub struct FrictionPause {
    pub command: Command,
    pub level: FrictionLevel,
    pub started_at: Instant,
    pub pause: Duration,
    /// The word the operator must type at L2+. `None` at L1 (the
    /// pause alone is the gate).
    pub confirm_word: Option<String>,
    /// Operator's in-progress typed confirmation. Empty at open.
    /// Only mutated at L2+; input handling ignores it at L1.
    pub confirm_input: String,
}

/// Why a friction-pause overlay ended. Used by the event loop to
/// decide whether to re-dispatch the pending command or drop it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrictionOutcome {
    /// Pause (and, at L2+, confirmation) is complete — re-dispatch
    /// the command via `run_bypass_friction`.
    Confirmed,
    /// Operator cancelled (Esc) or the overlay is still pending.
    /// Not complete; do nothing.
    Pending,
}

impl FrictionPause {
    #[must_use]
    pub fn from_decision(
        command: Command,
        decision: &FrictionDecision,
        now: Instant,
    ) -> Option<Self> {
        match decision {
            // `Proceed` has no pause to render. `HardStop` is
            // the load-bearing refusal: it never opens a
            // typeable widget — the operator sees the advisory
            // (see `dispatch::friction_advisory`) and reaches
            // for `Reduces`. Collapsing them into the same
            // early-return is the honest render.
            FrictionDecision::Proceed | FrictionDecision::HardStop { .. } => None,
            FrictionDecision::Pause { pause, level } => Some(Self {
                command,
                level: *level,
                started_at: now,
                pause: *pause,
                confirm_word: None,
                confirm_input: String::new(),
            }),
            FrictionDecision::TypedConfirm { pause, level } => {
                let word = decision.confirm_word().map_or_else(
                    || zero_commands::TYPED_CONFIRM_WORD.to_string(),
                    std::borrow::Cow::into_owned,
                );
                Some(Self {
                    command,
                    level: *level,
                    started_at: now,
                    pause: *pause,
                    confirm_word: Some(word),
                    confirm_input: String::new(),
                })
            }
            // M2 §3: L3 opens the same typed-confirm style
            // overlay as L2, but the typed target is the
            // `phrase` (a full sentence) rather than a single
            // word. The widget renders the phrase inside the
            // overlay so the operator reads it while they type.
            FrictionDecision::WaitAndReread {
                pause,
                level,
                phrase,
            } => Some(Self {
                command,
                level: *level,
                started_at: now,
                pause: *pause,
                confirm_word: Some(phrase.clone()),
                confirm_input: String::new(),
            }),
        }
    }

    /// Remaining pause duration at `now`. Zero when the pause has
    /// elapsed. Saturating so callers don't see negatives.
    #[must_use]
    pub fn remaining(&self, now: Instant) -> Duration {
        self.pause
            .saturating_sub(now.saturating_duration_since(self.started_at))
    }

    /// Whether the mandatory pause window has elapsed. True gates
    /// the typed-confirm input at L2+.
    #[must_use]
    pub fn pause_elapsed(&self, now: Instant) -> bool {
        self.remaining(now).is_zero()
    }

    /// Whether the operator's typed input matches the confirm
    /// word. Case-sensitive (matches the constant in
    /// `zero-commands`), trimmed. Always false at L1.
    #[must_use]
    pub fn confirm_word_matches(&self) -> bool {
        match &self.confirm_word {
            None => false,
            Some(word) => self.confirm_input.trim() == word.as_str(),
        }
    }

    /// Evaluate completion state. The return discriminates only
    /// between "ready to re-dispatch" and "still pending" —
    /// cancellation is a separate path (the overlay is simply
    /// dismissed). At L1 the pause alone completes the gate. At
    /// L2+ both the pause and the typed confirmation are required.
    #[must_use]
    pub fn outcome(&self, now: Instant) -> FrictionOutcome {
        if !self.pause_elapsed(now) {
            return FrictionOutcome::Pending;
        }
        match self.confirm_word {
            None => FrictionOutcome::Confirmed,
            Some(_) => {
                if self.confirm_word_matches() {
                    FrictionOutcome::Confirmed
                } else {
                    FrictionOutcome::Pending
                }
            }
        }
    }

    /// Append a character to the confirm buffer. No-op at L1 and
    /// while the pause is still running — the widget dims the
    /// field to make this legible. Max length is a tight bound so
    /// a run-away Repeat cannot blow memory.
    pub fn push_char(&mut self, c: char, now: Instant) {
        if self.confirm_word.is_none() || !self.pause_elapsed(now) {
            return;
        }
        if self.confirm_input.len() < 32 {
            self.confirm_input.push(c);
        }
    }

    /// Delete the last character from the confirm buffer. Same
    /// gating as [`push_char`].
    pub fn pop_char(&mut self, now: Instant) {
        if self.confirm_word.is_none() || !self.pause_elapsed(now) {
            return;
        }
        self.confirm_input.pop();
    }
}

impl AppState {
    /// New state with persistence disabled.
    #[must_use]
    pub fn new(engine: Arc<RwLock<EngineState>>) -> Self {
        Self::new_with_sink(engine, None)
    }

    /// New state, optionally persisted.
    #[must_use]
    pub fn new_with_sink(engine: Arc<RwLock<EngineState>>, sink: Option<SessionSink>) -> Self {
        // Pre-seed the first-live-trade latch based on whether
        // the persistent store has already recorded the
        // milestone. The two reasons for defaulting to `true`
        // (suppressed) are spelled out in the field's docs.
        let first_live_trade_recorded = match &sink {
            None => true,
            Some(s) => matches!(
                s.store()
                    .get_milestone(zero_session::milestones::FIRST_LIVE_TRADE_AT),
                Ok(Some(_))
            ),
        };

        let mut s = Self {
            mode: Mode::default(),
            log: ConversationLog::with_capacity(2048),
            prompt: PromptBuffer::new(),
            theme: Theme::default(),
            engine,
            should_quit: false,
            pending_input: None,
            sink,
            overlay: None,
            picker: None,
            screen_reader: false,
            log_scroll: 0,
            event_ring: EventRing::new(),
            live_stream_visible: false,
            verbose: false,
            wrap_off: false,
            coaching_notices: Vec::new(),
            rate_budget: None,
            first_live_trade_recorded,
            risk_overlay_last_dismissed_at: None,
            risk_overlay_last_seen_alert_pct: None,
            risk_overlay_last_trigger: None,
        };
        s.push(LogEntry::new(
            EntryKind::System,
            "zero — Ctrl+1..4 switch modes, Ctrl+C or /quit exits, /help for commands.",
        ));
        // M2 §4: prime the risk overlay on construction. If the
        // engine mirror already carries an L3+/halted snapshot
        // (e.g. session attach after an incident, or tests that
        // seed the mirror up-front), the overlay must be visible
        // from frame zero — operators reconnecting into a halted
        // engine must land inside the context surface, not on an
        // empty prompt.
        s.poll_risk_overlay(Instant::now());
        s
    }

    /// Append without persisting. Used during replay so rehydrated
    /// rows are not rewritten.
    pub fn append_silent(&mut self, entry: LogEntry) {
        self.log.push(entry);
    }

    /// Append + persist. All runtime-generated entries flow here.
    pub fn push(&mut self, entry: LogEntry) {
        if let Some(s) = &self.sink {
            s.record(&entry);
        }
        self.log.push(entry);
    }

    pub fn push_system(&mut self, text: impl Into<String>) {
        self.push(LogEntry::new(EntryKind::System, text));
    }

    /// Submit the current prompt. Echoes the typed line into the
    /// log and queues it for async dispatch by the event loop.
    /// Short-circuits whitespace-only input with no log noise.
    ///
    /// Submission clears the picker and re-attaches the
    /// conversation pane to the bottom: the operator sees their
    /// command's output without having to hit PageDown first.
    pub fn submit_prompt(&mut self) {
        let Some(line) = self.prompt.take() else {
            return;
        };
        if line.trim().is_empty() {
            return;
        }
        // Echo the submitted line; newlines render as literal
        // `\n` for compactness in the scrollback so a 6-line
        // multi-line prompt does not eat six scrollback rows.
        let echo = if line.contains('\n') {
            line.replace('\n', " ↵ ")
        } else {
            line.clone()
        };
        self.push(LogEntry::new(EntryKind::Prompt, format!("> {echo}")));
        self.pending_input = Some(line);
        self.picker = None;
        self.scroll_log_to_bottom();
    }

    /// Apply a [`DispatchOutput`] — append lines, switch modes,
    /// flip flags. Called by the event loop after `dispatch` returns.
    pub fn apply_dispatch(&mut self, out: DispatchOutput) {
        if out.clear_log {
            self.log = ConversationLog::with_capacity(2048);
        }
        for line in out.lines {
            let (kind, text) = match line {
                OutputLine::System(t) => (EntryKind::System, t),
                OutputLine::Command(t) => (EntryKind::Command, t),
                OutputLine::Warn(t) => (EntryKind::Warn, t),
                OutputLine::Alert(t) => (EntryKind::Alert, t),
            };
            self.push(LogEntry::new(kind, text));
        }
        // Replay lines bypass the sink so `/resume` does not
        // double-persist a prior session into the current one.
        // We preserve the original `at_ms` so rendered "age"
        // readings stay truthful — a freshly stamped clock on
        // replay would silently rewrite the operator's history.
        for rl in out.replay_lines {
            let kind = replay_kind_to_entry(rl.kind);
            let entry = if let Some(ts) =
                chrono::DateTime::<chrono::Utc>::from_timestamp_millis(rl.at_ms)
            {
                LogEntry::new(kind, rl.text).at(ts)
            } else {
                LogEntry::new(kind, rl.text)
            };
            self.append_silent(entry);
        }
        if let Some(target) = out.mode_change {
            self.mode = mode_from_target(target);
        }
        if let Some(ov) = out.show_overlay {
            self.overlay = Some(ActiveOverlay::from_target(ov));
        } else if out.dismiss_overlay {
            // Explicit dismissal signal from the dispatcher.
            // `show_overlay` wins above — opening and closing in
            // the same tick would be contradictory and the
            // open-path data is the caller's real intent.
            // Otherwise this is how `/clear` and empty-evaluate
            // errors tear down a stale modal so the operator is
            // not left staring at an unrelated verdict card.
            self.overlay = None;
        }
        // If the dispatcher returned a non-Proceed friction decision
        // *with* a pending command, open the friction-pause overlay.
        // This is the only path that can produce a FrictionPause —
        // the dispatcher owns the Label → level → decision mapping
        // and the TUI just renders the gate.
        if let (Some(decision), Some(cmd)) = (out.friction, out.pending_command)
            && !matches!(decision, FrictionDecision::Proceed)
            && let Some(fp) = FrictionPause::from_decision(cmd, &decision, Instant::now())
        {
            self.overlay = Some(ActiveOverlay::FrictionPause(fp));
        }
        if out.quit {
            self.should_quit = true;
        }
        // Verbose toggle — dispatcher resolves `/verbose toggle`
        // into an absolute target, so applying here is a plain
        // assignment. We do not emit a duplicate confirmation
        // line; the dispatcher already pushed "verbose on/off"
        // into `lines` above.
        if let Some(v) = out.verbose_toggle {
            self.verbose = v;
        }
        // `/wrap-off` resolves to an absolute target at dispatch
        // time, same contract shape as verbose. We do not run
        // the wrap generator here — the flag is only consumed
        // at session-finalize time, which is not yet wired.
        // The assignment alone carries the operator's intent
        // across the boundary; the generator will pick it up
        // when it lands.
        if let Some(w) = out.wrap_off_toggle {
            self.wrap_off = w;
        }
        // `/coaching reset` — empty the buffer. Today nothing
        // pushes into `coaching_notices`, so this is a well-
        // shaped no-op on the receiving side: the contract is
        // stable for the eventual coaching stream wiring.
        if out.coaching_reset {
            self.coaching_notices.clear();
        }
    }

    /// Dismiss the active overlay, if any. Idempotent. At a
    /// friction-pause overlay this is the "cancel" path — the
    /// pending command is dropped and never re-dispatched.
    ///
    /// For [`ActiveOverlay::Risk`], this stamps
    /// [`Self::risk_overlay_last_dismissed_at`] with
    /// `Instant::now()` and snapshots the engine's current
    /// `last_drawdown_alert_pct` so the auto-open hook can
    /// enforce the 60 s cooldown unless the trigger strictly
    /// escalates. The snapshot is taken at dismiss time (not at
    /// open time) so a fresh guardrail trip that happens *while*
    /// the overlay is visible still counts — the operator sees
    /// the new threshold in the current overlay and the next
    /// reopen fires only for the *next* fresh trip.
    pub fn dismiss_overlay(&mut self) {
        self.dismiss_overlay_at(Instant::now());
    }

    /// Test-seam variant of [`Self::dismiss_overlay`] that takes
    /// the "now" instant explicitly. Production uses `Instant::now`;
    /// tests pin a fixed monotonic anchor to verify cooldown
    /// arithmetic deterministically.
    pub fn dismiss_overlay_at(&mut self, now: Instant) {
        if matches!(self.overlay, Some(ActiveOverlay::Risk { .. })) {
            self.risk_overlay_last_dismissed_at = Some(now);
            let alert_pct = self
                .engine
                .read()
                .risk
                .as_ref()
                .and_then(|r| r.value.last_drawdown_alert_pct);
            self.risk_overlay_last_seen_alert_pct = alert_pct;
        }
        self.overlay = None;
    }

    /// Monotonic cooldown between a dismissed Risk overlay and
    /// the auto-open hook re-opening one. 60 s per M2 §4. Does
    /// not apply when the new trigger strictly escalates
    /// (L3 → L4) or when the engine reports a fresh guardrail
    /// threshold (`last_drawdown_alert_pct` changed value since
    /// the dismiss).
    pub const RISK_DISMISS_COOLDOWN: Duration = Duration::from_secs(60);

    /// Proximity window, in *percentage points* of drawdown,
    /// that triggers the Risk overlay via
    /// [`RiskOverlayTrigger::Proximity`]. Strictly smaller than
    /// `zero_operator_state::friction::RiskContext::PROXIMITY_PCT`
    /// (1.0 pp) because the overlay is an *earlier* nudge than
    /// the L3 friction escalation — operators should see the
    /// context surface before they hit a typed-reread gate.
    pub const GUARDRAIL_PROXIMITY_PP: f64 = 0.5;

    /// **M2 §4** auto-open hook. Called once per TUI tick with
    /// a monotonic `now`. Inspects the engine mirror's
    /// operator-state snapshot and `Risk` block; opens
    /// [`ActiveOverlay::Risk`] when either trigger fires and
    /// the rate-limiter permits it. Never closes an overlay —
    /// that is the operator's job (via [`Self::dismiss_overlay`]).
    ///
    /// Precedence of triggers:
    /// 1. Friction L4 (halted) — always opens / stays open.
    /// 2. Friction L3 (TILT + proximity) — opens unless cooldown.
    /// 3. Drawdown proximity ≤ `GUARDRAIL_PROXIMITY_PP` — opens
    ///    unless cooldown.
    ///
    /// Rules are layered so a single tick that satisfies both
    /// L3 and Proximity surfaces as `Friction(L3)` (the
    /// higher-signal cause), and an L3 → L4 escalation inside
    /// the cooldown overrides it (safety beats user comfort).
    ///
    /// Invariant: this hook never touches a non-Risk overlay.
    /// If the operator is inside `/state`, `/pool`, or a
    /// friction pause, the Risk overlay defers until that
    /// overlay closes. The guardrail signal does not vanish —
    /// it re-fires on the next tick.
    pub fn poll_risk_overlay(&mut self, now: Instant) {
        // Do not stomp on another overlay. Operator-owned
        // surfaces keep focus; Risk re-evaluates on the next tick.
        if matches!(
            self.overlay,
            Some(
                ActiveOverlay::State | ActiveOverlay::FrictionPause(_) | ActiveOverlay::Verdict(_)
            )
        ) {
            return;
        }

        let (trigger, current_alert_pct) = {
            let eng = self.engine.read();
            let friction = eng.operator_state.as_ref().map(|s| s.value.friction);
            let (drawdown, alert) = eng.risk.as_ref().map_or((None, None), |r| {
                (r.value.drawdown_pct, r.value.last_drawdown_alert_pct)
            });
            let proximity_hit = match (drawdown, alert) {
                (Some(d), Some(a)) => (d - a).abs() <= Self::GUARDRAIL_PROXIMITY_PP,
                _ => false,
            };
            let trigger = match friction {
                Some(FrictionLevel::L4) => Some(RiskOverlayTrigger::Friction(FrictionLevel::L4)),
                Some(FrictionLevel::L3) => Some(RiskOverlayTrigger::Friction(FrictionLevel::L3)),
                _ if proximity_hit => Some(RiskOverlayTrigger::Proximity),
                _ => None,
            };
            (trigger, alert)
        };

        let Some(trigger) = trigger else {
            return;
        };

        // If a Risk overlay is already up, consider upgrading
        // its trigger on escalation (L3 → L4). Never *downgrade*.
        if let Some(ActiveOverlay::Risk {
            trigger: current, ..
        }) = self.overlay
        {
            if trigger_strictly_escalates(current, trigger) {
                self.overlay = Some(ActiveOverlay::Risk {
                    trigger,
                    opened_at: now,
                });
                self.risk_overlay_last_trigger = Some(trigger);
            }
            return;
        }

        // Fresh open — honor the dismiss cooldown unless the
        // engine tripped a new guardrail threshold (distinct
        // `last_drawdown_alert_pct`) or the new trigger strictly
        // escalates the last-seen trigger.
        if let Some(dismissed_at) = self.risk_overlay_last_dismissed_at {
            let within_cooldown = now.duration_since(dismissed_at) < Self::RISK_DISMISS_COOLDOWN;
            let fresh_alert = match (current_alert_pct, self.risk_overlay_last_seen_alert_pct) {
                (Some(cur), Some(prev)) => (cur - prev).abs() > f64::EPSILON,
                (Some(_), None) | (None, Some(_)) => true,
                (None, None) => false,
            };
            let escalates = self
                .risk_overlay_last_trigger
                .is_some_and(|prev| trigger_strictly_escalates(prev, trigger));
            if within_cooldown && !fresh_alert && !escalates {
                return;
            }
        }

        self.overlay = Some(ActiveOverlay::Risk {
            trigger,
            opened_at: now,
        });
        self.risk_overlay_last_trigger = Some(trigger);
    }

    /// Rebuild [`Self::picker`] from the current prompt. Picker
    /// is suppressed whenever an overlay is active (the operator
    /// is inside a modal; no ambient popup on top of it) or the
    /// prompt's first row does not start with `/`.
    ///
    /// Selection is preserved across rebuilds when the previously
    /// highlighted command name still appears in the new match
    /// list. This keeps `Up/Down` + typing interleavable without
    /// the selection jumping back to the top on every keystroke.
    pub fn refresh_picker(&mut self) {
        if self.overlay.is_some() {
            self.picker = None;
            return;
        }
        let first = self
            .prompt
            .line(0)
            .map(|chars| chars.iter().collect::<String>())
            .unwrap_or_default();
        let prev_name = self
            .picker
            .as_ref()
            .and_then(SlashPicker::selected)
            .map(|m| m.info.name);
        let new_picker = SlashPicker::from_prompt_line(&first);
        self.picker = new_picker.map(|mut p| {
            if let Some(name) = prev_name
                && let Some(i) = p.matches().iter().position(|m| m.info.name == name)
            {
                for _ in 0..i {
                    p.select_next();
                }
            }
            p
        });
    }

    /// Scroll the conversation pane up by `rows` (toward older
    /// entries). A non-zero offset detaches the pane from the
    /// bottom so new entries no longer auto-scroll.
    pub fn scroll_log_up(&mut self, rows: u16) {
        self.log_scroll = self.log_scroll.saturating_add(rows);
    }

    /// Scroll the conversation pane down by `rows` (toward the
    /// newest entry). Hitting 0 re-attaches to the bottom.
    pub fn scroll_log_down(&mut self, rows: u16) {
        self.log_scroll = self.log_scroll.saturating_sub(rows);
    }

    /// Re-attach the conversation pane to the newest entry.
    /// Called on prompt submit so the operator sees their command
    /// output without having to scroll back manually.
    pub fn scroll_log_to_bottom(&mut self) {
        self.log_scroll = 0;
    }

    /// Toggle screen-reader mode (`Ctrl+R`). Returns the new
    /// value so the caller can log it.
    pub fn toggle_screen_reader(&mut self) -> bool {
        self.screen_reader = !self.screen_reader;
        self.screen_reader
    }

    /// Toggle the live-stream pane (`]`). Returns the new
    /// visibility so callers can echo a confirmation line when
    /// the pane is hidden and the operator needs the hint.
    pub fn toggle_live_stream(&mut self) -> bool {
        self.live_stream_visible = !self.live_stream_visible;
        self.live_stream_visible
    }

    /// Append a decoded engine event to the live-stream ring.
    /// Called by the event loop's broadcast-receiver arm.
    pub fn record_engine_event(&mut self, evt: EngineEvent) {
        self.maybe_fire_first_live_trade_ceremony(&evt);
        self.event_ring.push_event(evt);
    }

    /// Record an engine event with an explicit wall-clock time.
    /// Exists for snapshot tests that need to pin the rendered
    /// timestamp regardless of when the test runs.
    pub fn record_engine_event_at(&mut self, evt: EngineEvent, ts: chrono::DateTime<chrono::Utc>) {
        self.maybe_fire_first_live_trade_ceremony(&evt);
        self.event_ring.push_event_at(evt, ts);
    }

    /// First-live-trade ceremony (§8.4).
    ///
    /// The engine does not today emit a typed "first fill" or
    /// "decision executed" event; the closest thing the CLI
    /// can observe is a `Positions` push whose `items` is
    /// non-empty. We treat that as the signal: if the
    /// `FIRST_LIVE_TRADE_AT` milestone is unset, this is the
    /// first open position this persistent store has ever
    /// seen, and we render the ceremony + persist the
    /// milestone.
    ///
    /// Honest caveats the implementation respects:
    /// - **Pre-existing positions on first install** will
    ///   fire the ceremony on the first `Positions` push.
    ///   That is the correct behavior from the CLI's point of
    ///   view: "first trade this tool has ever witnessed."
    /// - **No-persist mode** suppresses the ceremony
    ///   unconditionally; without a milestone store a
    ///   ceremony every run is worse than silence.
    /// - **Milestone write failure** still flips the in-
    ///   memory latch so the ceremony does not loop in this
    ///   session; the next session rechecks the store and
    ///   will refire if the milestone never landed — better
    ///   one duplicate ceremony than one infinite loop.
    fn maybe_fire_first_live_trade_ceremony(&mut self, evt: &EngineEvent) {
        if self.first_live_trade_recorded {
            return;
        }
        let EngineEvent::Positions(p) = evt else {
            return;
        };
        if p.items.is_empty() {
            return;
        }
        self.first_live_trade_recorded = true;

        // Persist the milestone first so a crash during the
        // ceremony push still records "this happened." The
        // timestamp is RFC-3339 to match the other milestone
        // values (`WELCOME_SHOWN`, `LAST_DAILY_WRAP_AT`).
        if let Some(sink) = &self.sink {
            let now = chrono::Utc::now().to_rfc3339();
            if let Err(e) = sink
                .store()
                .set_milestone(zero_session::milestones::FIRST_LIVE_TRADE_AT, &now)
            {
                tracing::warn!(err = %e, "first-live-trade milestone write failed");
            }
        }

        // Medically honest ceremony copy — no confetti, no
        // "congratulations", no gamified counter. Three
        // system lines: what happened, what it means, what to
        // do next. Reads the same way the welcome reads.
        for text in CEREMONY_LINES {
            self.push(LogEntry::new(EntryKind::System, *text));
        }
    }

    /// Record that the broadcast receiver lagged `skipped`
    /// frames. A synthetic marker lands in the ring so the
    /// live-stream pane can surface the drop honestly instead
    /// of letting the firehose look calm after a burst.
    pub fn record_events_lagged(&mut self, skipped: u64) {
        self.event_ring.push_lagged(skipped);
    }

    /// Deterministic sibling of [`Self::record_events_lagged`].
    pub fn record_events_lagged_at(&mut self, skipped: u64, ts: chrono::DateTime<chrono::Utc>) {
        self.event_ring.push_lagged_at(skipped, ts);
    }

    /// If a friction-pause overlay is currently open and its
    /// outcome is `Confirmed` at `now`, take the pending command
    /// and close the overlay. Returns `Some(cmd)` — the caller
    /// (event loop) is responsible for dispatching it via
    /// [`zero_commands::run_bypass_friction`] and applying the
    /// resulting output. Returns `None` when there is no friction
    /// overlay, the outcome is still pending, or the overlay is
    /// not a friction variant.
    #[must_use]
    pub fn take_confirmed_friction_command(&mut self, now: Instant) -> Option<Command> {
        let confirmed = match self.overlay.as_ref() {
            Some(ActiveOverlay::FrictionPause(fp)) => {
                matches!(fp.outcome(now), FrictionOutcome::Confirmed)
            }
            _ => false,
        };
        if !confirmed {
            return None;
        }
        match self.overlay.take() {
            Some(ActiveOverlay::FrictionPause(fp)) => Some(fp.command),
            _ => None,
        }
    }
}

fn mode_from_target(t: ModeTarget) -> Mode {
    match t {
        ModeTarget::Conversation => Mode::Conversation,
        ModeTarget::Positions => Mode::Positions,
        ModeTarget::Decisions => Mode::Decisions,
        ModeTarget::Heat => Mode::Heat,
    }
}

/// Translate the dispatcher's replay-kind into the TUI's log
/// entry kind. Kept as a one-liner next to [`mode_from_target`]
/// so all dispatch-shape translations live in one place.
const fn replay_kind_to_entry(k: ReplayKind) -> EntryKind {
    match k {
        ReplayKind::Prompt => EntryKind::Prompt,
        ReplayKind::System => EntryKind::System,
        ReplayKind::Command => EntryKind::Command,
        ReplayKind::Warn => EntryKind::Warn,
        ReplayKind::Alert => EntryKind::Alert,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_commands::OverlayTarget;

    fn mk() -> AppState {
        AppState::new(EngineState::shared())
    }

    fn is_state(ov: Option<&ActiveOverlay>) -> bool {
        matches!(ov, Some(ActiveOverlay::State))
    }

    fn is_friction(ov: Option<&ActiveOverlay>) -> bool {
        matches!(ov, Some(ActiveOverlay::FrictionPause(_)))
    }

    #[test]
    fn apply_dispatch_honors_verbose_toggle_absolute() {
        // Dispatcher has already resolved `toggle` to an
        // absolute target before apply_dispatch sees it, so
        // the assignment is trivial — but this test pins the
        // contract in case a future contributor "simplifies"
        // it into a flip that would diverge from what the
        // dispatcher confirmed in the log line.
        let mut s = mk();
        assert!(!s.verbose);
        s.apply_dispatch(DispatchOutput {
            verbose_toggle: Some(true),
            ..Default::default()
        });
        assert!(s.verbose);
        // Setting to the same value must be a no-op without
        // side effects.
        s.apply_dispatch(DispatchOutput {
            verbose_toggle: Some(true),
            ..Default::default()
        });
        assert!(s.verbose);
        s.apply_dispatch(DispatchOutput {
            verbose_toggle: Some(false),
            ..Default::default()
        });
        assert!(!s.verbose);
        // None means "leave alone" — the flag must survive
        // unrelated dispatches.
        s.verbose = true;
        s.apply_dispatch(DispatchOutput::default());
        assert!(s.verbose);
    }

    #[test]
    fn apply_dispatch_honors_wrap_off_absolute_and_leaves_unrelated_alone() {
        // /wrap-off must flip the flag to the dispatcher-
        // resolved target exactly. Unrelated dispatches must
        // not clobber it — otherwise the operator's opt-out
        // would silently re-arm mid-session.
        let mut s = mk();
        assert!(!s.wrap_off);
        s.apply_dispatch(DispatchOutput {
            wrap_off_toggle: Some(true),
            ..Default::default()
        });
        assert!(s.wrap_off);
        s.apply_dispatch(DispatchOutput::default());
        assert!(s.wrap_off, "unrelated dispatch must not clear the opt-out");
    }

    #[test]
    fn apply_dispatch_honors_coaching_reset_signal() {
        // Even when the buffer is empty today, the contract is
        // the thing we are testing: signal => clear. A future
        // coaching-stream push must land in the same buffer
        // this clear empties. Seeding the buffer directly
        // simulates that world.
        let mut s = mk();
        s.coaching_notices.push("loss-reaction < 2m".into());
        s.coaching_notices.push("velocity 2x baseline".into());
        s.apply_dispatch(DispatchOutput {
            coaching_reset: true,
            ..Default::default()
        });
        assert!(s.coaching_notices.is_empty());
        // Negative control — an unrelated dispatch with
        // coaching_reset=false must leave the buffer alone.
        s.coaching_notices.push("fresh notice".into());
        s.apply_dispatch(DispatchOutput::default());
        assert_eq!(s.coaching_notices.len(), 1);
    }

    #[test]
    fn apply_dispatch_sets_overlay_from_show_overlay() {
        let mut s = mk();
        assert!(s.overlay.is_none());
        let out = DispatchOutput {
            show_overlay: Some(OverlayTarget::State),
            ..Default::default()
        };
        s.apply_dispatch(out);
        assert!(is_state(s.overlay.as_ref()));
    }

    #[test]
    fn dismiss_overlay_is_idempotent() {
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::State);
        s.dismiss_overlay();
        assert!(s.overlay.is_none());
        // second call does nothing bad
        s.dismiss_overlay();
        assert!(s.overlay.is_none());
    }

    #[test]
    fn apply_dispatch_preserves_overlay_when_not_signaled() {
        // A dispatch with no show_overlay must not clobber an
        // existing overlay — that would make any follow-up command
        // typed behind the modal accidentally close it, which
        // breaks the "any key dismisses" contract.
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::State);
        let out = DispatchOutput {
            mode_change: Some(ModeTarget::Heat),
            ..Default::default()
        };
        s.apply_dispatch(out);
        assert!(
            is_state(s.overlay.as_ref()),
            "unrelated dispatches must not close the overlay"
        );
    }

    #[test]
    fn apply_dispatch_honors_explicit_dismiss_overlay() {
        // `/clear` and `/evaluate`'s failure paths set
        // `dismiss_overlay = true` to tear down a stale modal so
        // new output is visible. The TUI must honor that signal.
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::State);
        let out = DispatchOutput {
            dismiss_overlay: true,
            ..Default::default()
        };
        s.apply_dispatch(out);
        assert!(
            s.overlay.is_none(),
            "explicit dismiss_overlay must clear the overlay"
        );
    }

    #[test]
    fn apply_dispatch_show_overlay_wins_over_dismiss() {
        // If a single dispatch set both flags we prefer the
        // open path — the dispatcher's *data* is the reason the
        // command ran, and opening-then-immediately-closing in the
        // same tick would be indistinguishable from a bug.
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::State);
        let out = DispatchOutput {
            show_overlay: Some(OverlayTarget::State),
            dismiss_overlay: true,
            ..Default::default()
        };
        s.apply_dispatch(out);
        assert!(
            is_state(s.overlay.as_ref()),
            "show_overlay must win when both are set"
        );
    }

    #[test]
    fn apply_dispatch_opens_friction_overlay_on_l1_pause() {
        let mut s = mk();
        let out = DispatchOutput {
            friction: Some(FrictionDecision::Pause {
                pause: Duration::from_secs(3),
                level: FrictionLevel::L1,
            }),
            pending_command: Some(Command::Execute),
            ..Default::default()
        };
        s.apply_dispatch(out);
        assert!(
            is_friction(s.overlay.as_ref()),
            "L1 should open friction overlay"
        );
        if let Some(ActiveOverlay::FrictionPause(fp)) = &s.overlay {
            assert_eq!(fp.level, FrictionLevel::L1);
            assert!(fp.confirm_word.is_none());
        }
    }

    #[test]
    fn apply_dispatch_opens_friction_overlay_on_l2_typed_confirm() {
        let mut s = mk();
        let out = DispatchOutput {
            friction: Some(FrictionDecision::TypedConfirm {
                pause: Duration::from_secs(10),
                level: FrictionLevel::L2,
            }),
            pending_command: Some(Command::Execute),
            ..Default::default()
        };
        s.apply_dispatch(out);
        if let Some(ActiveOverlay::FrictionPause(fp)) = &s.overlay {
            assert_eq!(fp.level, FrictionLevel::L2);
            assert_eq!(fp.confirm_word.as_deref(), Some("execute"));
        } else {
            panic!("expected FrictionPause, got {:?}", s.overlay);
        }
    }

    #[test]
    fn apply_dispatch_without_pending_command_does_not_open_friction() {
        // Defensive: if a dispatcher change drops pending_command
        // by mistake, we must not crash or open an empty overlay.
        let mut s = mk();
        let out = DispatchOutput {
            friction: Some(FrictionDecision::Pause {
                pause: Duration::from_secs(3),
                level: FrictionLevel::L1,
            }),
            pending_command: None,
            ..Default::default()
        };
        s.apply_dispatch(out);
        assert!(s.overlay.is_none());
    }

    #[test]
    fn l1_pause_completes_after_duration_elapses() {
        let now = Instant::now();
        let fp = FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L1,
            started_at: now,
            pause: Duration::from_secs(3),
            confirm_word: None,
            confirm_input: String::new(),
        };
        assert_eq!(fp.outcome(now), FrictionOutcome::Pending);
        assert_eq!(
            fp.outcome(now + Duration::from_millis(2_999)),
            FrictionOutcome::Pending,
        );
        assert_eq!(
            fp.outcome(now + Duration::from_secs(3)),
            FrictionOutcome::Confirmed,
        );
    }

    #[test]
    fn l2_requires_both_pause_and_word() {
        let now = Instant::now();
        let mut fp = FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L2,
            started_at: now,
            pause: Duration::from_secs(10),
            confirm_word: Some("execute".into()),
            confirm_input: String::new(),
        };
        let past_pause = now + Duration::from_secs(10);

        // Still within pause — typing is rejected.
        for c in "execute".chars() {
            fp.push_char(c, now + Duration::from_secs(1));
        }
        assert!(
            fp.confirm_input.is_empty(),
            "input during mandatory pause must be ignored"
        );
        assert_eq!(fp.outcome(past_pause), FrictionOutcome::Pending);

        // After pause — typing is accepted.
        for c in "exec".chars() {
            fp.push_char(c, past_pause);
        }
        assert_eq!(fp.confirm_input, "exec");
        assert_eq!(fp.outcome(past_pause), FrictionOutcome::Pending);

        // Wrong word never completes.
        assert!(!fp.confirm_word_matches());

        // Complete the word — now confirmed.
        for c in "ute".chars() {
            fp.push_char(c, past_pause);
        }
        assert_eq!(fp.confirm_input, "execute");
        assert!(fp.confirm_word_matches());
        assert_eq!(fp.outcome(past_pause), FrictionOutcome::Confirmed);

        // Backspace — no longer confirmed.
        fp.pop_char(past_pause);
        assert_eq!(fp.outcome(past_pause), FrictionOutcome::Pending);
    }

    #[test]
    fn take_confirmed_command_consumes_overlay() {
        let now = Instant::now();
        let fp = FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L1,
            started_at: now,
            pause: Duration::from_secs(0),
            confirm_word: None,
            confirm_input: String::new(),
        };
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::FrictionPause(fp));
        let taken = s.take_confirmed_friction_command(now);
        assert_eq!(taken, Some(Command::Execute));
        assert!(s.overlay.is_none());
        // Second call is a no-op — the overlay is already gone.
        assert!(s.take_confirmed_friction_command(now).is_none());
    }

    #[test]
    fn take_confirmed_leaves_pending_overlay_in_place() {
        let now = Instant::now();
        let fp = FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L1,
            started_at: now,
            pause: Duration::from_secs(3),
            confirm_word: None,
            confirm_input: String::new(),
        };
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::FrictionPause(fp));
        let taken = s.take_confirmed_friction_command(now + Duration::from_secs(1));
        assert_eq!(taken, None, "still within pause window");
        assert!(matches!(s.overlay, Some(ActiveOverlay::FrictionPause(_))));
    }

    #[test]
    fn take_confirmed_ignores_non_friction_overlays() {
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::State);
        let taken = s.take_confirmed_friction_command(Instant::now());
        assert!(taken.is_none());
        assert!(is_state(s.overlay.as_ref()));
    }

    #[test]
    fn live_stream_starts_hidden_and_toggles_round_trip() {
        let mut s = mk();
        assert!(
            !s.live_stream_visible,
            "new state must start with the pane hidden"
        );
        let on = s.toggle_live_stream();
        assert!(on && s.live_stream_visible);
        let off = s.toggle_live_stream();
        assert!(!off && !s.live_stream_visible);
    }

    #[test]
    fn record_engine_event_appends_to_ring() {
        let mut s = mk();
        assert_eq!(s.event_ring.len(), 0);
        s.record_engine_event(EngineEvent::Heartbeat(chrono::Utc::now()));
        s.record_engine_event(EngineEvent::Heartbeat(chrono::Utc::now()));
        assert_eq!(s.event_ring.len(), 2);
    }

    #[test]
    fn record_events_lagged_appends_marker_without_losing_prior_events() {
        let mut s = mk();
        s.record_engine_event(EngineEvent::Heartbeat(chrono::Utc::now()));
        s.record_events_lagged(4);
        // 1 event + 1 lag marker = 2 items; lag must not
        // replace or overwrite the preceding real event.
        assert_eq!(s.event_ring.len(), 2);
    }

    // ------------------------------------------------------------
    // First-live-trade ceremony (§8.4).
    // ------------------------------------------------------------

    fn mk_with_fresh_store() -> (AppState, std::sync::Arc<zero_session::Store>) {
        use zero_session::Store;
        let store = std::sync::Arc::new(Store::open_in_memory().unwrap());
        let id = store
            .start_session("01HCRM", None, "0.3.0-test", None)
            .unwrap();
        let sink = crate::app::session::SessionSink::new(
            std::sync::Arc::clone(&store),
            id,
            "01HCRM".into(),
        );
        (
            AppState::new_with_sink(EngineState::shared(), Some(sink)),
            store,
        )
    }

    fn positions_with_items(n: usize) -> EngineEvent {
        use zero_engine_client::models::{Position, Positions};
        let items = (0..n)
            .map(|i| Position {
                symbol: format!("COIN{i}"),
                ..Position::default()
            })
            .collect();
        EngineEvent::Positions(Box::new(Positions {
            items,
            account_value: None,
            total_unrealized_pnl: None,
        }))
    }

    #[test]
    fn ceremony_suppressed_without_sink() {
        let mut s = mk();
        // No sink → latch defaults to `true` → ceremony
        // unconditionally suppressed. Confirm by counting
        // log-lines-added: none beyond the preexisting
        // startup system line.
        let before = s.log.len();
        s.record_engine_event(positions_with_items(3));
        assert_eq!(
            s.log.len(),
            before,
            "no-persist run must not render the ceremony"
        );
        assert!(s.first_live_trade_recorded);
    }

    #[test]
    fn ceremony_fires_on_first_nonempty_positions_and_persists_milestone() {
        use zero_session::milestones::FIRST_LIVE_TRADE_AT;
        let (mut s, store) = mk_with_fresh_store();
        assert!(!s.first_live_trade_recorded);
        assert_eq!(store.get_milestone(FIRST_LIVE_TRADE_AT).unwrap(), None);

        let before = s.log.len();
        s.record_engine_event(positions_with_items(1));

        // Every ceremony line landed in the log.
        assert_eq!(
            s.log.len() - before,
            CEREMONY_LINES.len(),
            "exactly one ceremony line per CEREMONY_LINES entry"
        );

        // Latch flipped.
        assert!(s.first_live_trade_recorded);

        // Milestone persisted as RFC-3339.
        let stored = store
            .get_milestone(FIRST_LIVE_TRADE_AT)
            .unwrap()
            .expect("milestone was set");
        assert!(
            chrono::DateTime::parse_from_rfc3339(&stored).is_ok(),
            "milestone value must be RFC-3339 (got {stored:?})"
        );
    }

    #[test]
    fn ceremony_never_fires_on_empty_positions() {
        let (mut s, _store) = mk_with_fresh_store();
        let before = s.log.len();
        s.record_engine_event(positions_with_items(0));
        assert_eq!(
            s.log.len(),
            before,
            "empty positions must not fire ceremony"
        );
        assert!(!s.first_live_trade_recorded);
    }

    #[test]
    fn ceremony_fires_at_most_once_per_session() {
        let (mut s, _store) = mk_with_fresh_store();
        s.record_engine_event(positions_with_items(1));
        let after_first = s.log.len();
        s.record_engine_event(positions_with_items(1));
        s.record_engine_event(positions_with_items(2));
        assert_eq!(
            s.log.len(),
            after_first,
            "subsequent Positions events must not re-fire the ceremony"
        );
    }

    #[test]
    fn ceremony_suppressed_when_milestone_already_set() {
        use zero_session::Store;
        use zero_session::milestones::FIRST_LIVE_TRADE_AT;
        let store = std::sync::Arc::new(Store::open_in_memory().unwrap());
        // Pre-seed the milestone — the operator has traded
        // before this binary launch.
        store
            .set_milestone(FIRST_LIVE_TRADE_AT, "2026-04-20T12:00:00Z")
            .unwrap();
        let id = store
            .start_session("01HCRM2", None, "0.3.0-test", None)
            .unwrap();
        let sink = crate::app::session::SessionSink::new(
            std::sync::Arc::clone(&store),
            id,
            "01HCRM2".into(),
        );
        let mut s = AppState::new_with_sink(EngineState::shared(), Some(sink));

        assert!(
            s.first_live_trade_recorded,
            "latch must pre-close on hydrate"
        );
        let before = s.log.len();
        s.record_engine_event(positions_with_items(5));
        assert_eq!(
            s.log.len(),
            before,
            "a seasoned operator must never see the first-trade ceremony"
        );
    }

    // ── M2 §4: risk-overlay auto-open contract ─────────────────
    //
    // These tests cover the four edges of `poll_risk_overlay`:
    // fresh open on L3/L4/proximity, rate-limited re-open after
    // dismiss, escalation-overrides-cooldown (L3→L4), and
    // fresh-alert-overrides-cooldown. Each seeds the engine
    // mirror directly rather than routing through a dispatch
    // cycle — the hook is a pure function of the mirror + the
    // rate-limiter state, and isolating it makes failures
    // actionable.

    mod risk_overlay {
        use super::*;
        use chrono::TimeZone;
        use zero_engine_client::{Risk, Source, Stat};
        use zero_operator_state::{Label, Snapshot, StateVector, friction::FrictionLevel};

        fn frozen() -> chrono::DateTime<chrono::Utc> {
            chrono::Utc.with_ymd_and_hms(2026, 4, 21, 18, 0, 0).unwrap()
        }

        fn snap_at(label: Label, friction: FrictionLevel) -> Stat<Snapshot> {
            let snap = Snapshot {
                label,
                friction,
                vector: StateVector::default(),
                as_of: frozen(),
                version: 1,
            };
            Stat::new(snap, Source::Ws).with_as_of(frozen())
        }

        fn risk_stat(
            drawdown_pct: Option<f64>,
            alert_pct: Option<f64>,
            halted: bool,
        ) -> Stat<Risk> {
            let risk = Risk {
                drawdown_pct,
                last_drawdown_alert_pct: alert_pct,
                halted,
                ..Risk::default()
            };
            Stat::new(risk, Source::Ws).with_as_of(frozen())
        }

        /// Build a fresh AppState whose engine carries no
        /// operator-state + no risk; `poll_risk_overlay` on a
        /// no-signal mirror must leave the overlay closed. The
        /// constructor already called the hook once, so
        /// starting from `overlay == None` is the contract.
        fn empty_state() -> AppState {
            let s = AppState::new_with_sink(EngineState::shared(), None);
            assert!(s.overlay.is_none(), "empty engine => no overlay");
            s
        }

        fn seed_l3(state: &AppState) {
            let mut eng = state.engine.write();
            eng.operator_state = Some(snap_at(Label::Tilt, FrictionLevel::L3));
        }

        fn seed_l4(state: &AppState) {
            let mut eng = state.engine.write();
            eng.operator_state = Some(snap_at(Label::Tilt, FrictionLevel::L4));
        }

        fn seed_proximity(state: &AppState, drawdown: f64, alert: f64) {
            let mut eng = state.engine.write();
            eng.risk = Some(risk_stat(Some(drawdown), Some(alert), false));
        }

        #[test]
        fn poll_opens_overlay_on_l3_snapshot() {
            let mut s = empty_state();
            seed_l3(&s);
            s.poll_risk_overlay(Instant::now());
            match &s.overlay {
                Some(ActiveOverlay::Risk { trigger, .. }) => {
                    assert_eq!(*trigger, RiskOverlayTrigger::Friction(FrictionLevel::L3));
                }
                other => panic!("expected Risk overlay, got {other:?}"),
            }
        }

        #[test]
        fn poll_opens_overlay_on_l4_snapshot() {
            let mut s = empty_state();
            seed_l4(&s);
            s.poll_risk_overlay(Instant::now());
            match &s.overlay {
                Some(ActiveOverlay::Risk { trigger, .. }) => {
                    assert_eq!(*trigger, RiskOverlayTrigger::Friction(FrictionLevel::L4));
                }
                other => panic!("expected Risk overlay, got {other:?}"),
            }
        }

        #[test]
        fn poll_opens_overlay_on_drawdown_proximity_below_threshold() {
            let mut s = empty_state();
            // 4.6% vs 5.0% = 0.4pp distance, inside the 0.5pp window.
            seed_proximity(&s, 4.6, 5.0);
            s.poll_risk_overlay(Instant::now());
            match &s.overlay {
                Some(ActiveOverlay::Risk { trigger, .. }) => {
                    assert_eq!(*trigger, RiskOverlayTrigger::Proximity);
                }
                other => panic!("expected Risk overlay, got {other:?}"),
            }
        }

        #[test]
        fn poll_does_not_open_overlay_when_drawdown_far_from_alert() {
            let mut s = empty_state();
            // 3.0% vs 5.0% = 2.0pp distance, outside the 0.5pp window.
            seed_proximity(&s, 3.0, 5.0);
            s.poll_risk_overlay(Instant::now());
            assert!(s.overlay.is_none(), "2.0pp distance must not open");
        }

        #[test]
        fn dismiss_then_poll_within_cooldown_does_not_reopen() {
            let mut s = empty_state();
            seed_l3(&s);
            let t0 = Instant::now();
            s.poll_risk_overlay(t0);
            assert!(matches!(s.overlay, Some(ActiveOverlay::Risk { .. })));
            s.dismiss_overlay_at(t0 + Duration::from_secs(5));
            assert!(s.overlay.is_none());
            // Still L3 on mirror; 10 s after dismissal (< 60 s
            // cooldown, same trigger).
            s.poll_risk_overlay(t0 + Duration::from_secs(15));
            assert!(
                s.overlay.is_none(),
                "same-trigger reopen inside cooldown must be suppressed"
            );
        }

        #[test]
        fn dismiss_then_poll_after_cooldown_reopens() {
            let mut s = empty_state();
            seed_l3(&s);
            let t0 = Instant::now();
            s.poll_risk_overlay(t0);
            s.dismiss_overlay_at(t0 + Duration::from_secs(5));
            // Just after the cooldown expires.
            let t_reopen = t0 + Duration::from_secs(5) + AppState::RISK_DISMISS_COOLDOWN;
            s.poll_risk_overlay(t_reopen);
            assert!(
                matches!(s.overlay, Some(ActiveOverlay::Risk { .. })),
                "past cooldown, signal still live => must reopen, got {:?}",
                s.overlay,
            );
        }

        #[test]
        fn escalation_l3_to_l4_overrides_cooldown() {
            let mut s = empty_state();
            seed_l3(&s);
            let t0 = Instant::now();
            s.poll_risk_overlay(t0);
            s.dismiss_overlay_at(t0 + Duration::from_secs(5));
            // Mirror escalates to L4 while we are still in the
            // 60 s cooldown.
            seed_l4(&s);
            s.poll_risk_overlay(t0 + Duration::from_secs(15));
            match &s.overlay {
                Some(ActiveOverlay::Risk { trigger, .. }) => {
                    assert_eq!(
                        *trigger,
                        RiskOverlayTrigger::Friction(FrictionLevel::L4),
                        "L4 escalation must override dismiss-cooldown",
                    );
                }
                other => panic!("expected L4 Risk overlay, got {other:?}"),
            }
        }

        #[test]
        fn fresh_alert_threshold_overrides_cooldown() {
            let mut s = empty_state();
            // Start with a proximity trigger against alert=5.0%.
            seed_proximity(&s, 4.6, 5.0);
            let t0 = Instant::now();
            s.poll_risk_overlay(t0);
            assert!(matches!(s.overlay, Some(ActiveOverlay::Risk { .. })));
            s.dismiss_overlay_at(t0 + Duration::from_secs(5));
            // Engine trips a new alert threshold (5.0% -> 6.0%).
            // Drawdown nudges into its 0.5pp window (5.6%).
            {
                let mut eng = s.engine.write();
                eng.risk = Some(risk_stat(Some(5.6), Some(6.0), false));
            }
            s.poll_risk_overlay(t0 + Duration::from_secs(15));
            assert!(
                matches!(s.overlay, Some(ActiveOverlay::Risk { .. })),
                "fresh guardrail threshold must override cooldown",
            );
        }

        #[test]
        fn poll_does_not_stomp_friction_pause_overlay() {
            let mut s = empty_state();
            // Seed an L3 snapshot but pretend a friction pause
            // is already on screen (e.g. operator ran an
            // /execute at TILT a tick earlier).
            seed_l3(&s);
            let fp = FrictionPause {
                command: Command::Help,
                level: FrictionLevel::L1,
                pause: Duration::from_secs(3),
                started_at: Instant::now(),
                confirm_word: None,
                confirm_input: String::new(),
            };
            s.overlay = Some(ActiveOverlay::FrictionPause(fp));
            s.poll_risk_overlay(Instant::now());
            assert!(
                matches!(s.overlay, Some(ActiveOverlay::FrictionPause(_))),
                "poll must defer to an active friction pause",
            );
        }

        #[test]
        fn l4_mirror_on_construction_opens_overlay_with_hardstop_trigger() {
            // Session-attach scenario: engine was halted before
            // the TUI started. The constructor must surface the
            // overlay so the operator lands inside the context
            // card from frame zero.
            let engine = EngineState::shared();
            {
                let mut eng = engine.write();
                eng.operator_state = Some(snap_at(Label::Tilt, FrictionLevel::L4));
            }
            let s = AppState::new_with_sink(engine, None);
            match &s.overlay {
                Some(ActiveOverlay::Risk { trigger, .. }) => {
                    assert_eq!(*trigger, RiskOverlayTrigger::Friction(FrictionLevel::L4));
                }
                other => panic!("expected L4 Risk overlay at construction, got {other:?}"),
            }
        }
    }
}
