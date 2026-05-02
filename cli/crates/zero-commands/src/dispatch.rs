//! Dispatcher — resolves a [`Command`] into a [`DispatchOutput`].
//!
//! The dispatcher is the boundary between "the operator asked for
//! X" and "the engine did Y." It owns the HTTP client, reads the
//! shared `EngineState`, and returns structured output that the
//! caller (TUI or non-interactive entrypoint) renders.

use std::sync::Arc;

use parking_lot::RwLock;
use zero_engine_client::{EngineState, HttpClient, LiveControlResponse};
use zero_operator_state::label::Label;

use crate::command::{
    AutoAction, Command, ConfigAction, DISCLOSURE_OVERRIDE_CONFIRM, HeadlessAction, ModeTarget,
    OverlayTarget, StateOverrideLabel, VerboseAction,
};
use crate::config::{ConfigSource, DoctorSeverity};
use crate::friction::FrictionDecision;
use crate::parse::parse_line;
use crate::risk::RiskDirection;
use crate::session::{ReplayKind, SessionSource};
use crate::supervisor::{
    AutoRequest, AutoSource, SupervisorAction, SupervisorError, SupervisorReply, SupervisorSource,
};

/// One atomic output action the caller must handle. A single
/// dispatch can emit multiple — e.g. `/help` emits several `Line`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputLine {
    /// Informational, rendered as system text.
    System(String),
    /// Multi-field engine output, rendered as command text.
    Command(String),
    /// Rendered amber — an advisory, not a block.
    Warn(String),
    /// Rendered red + bold — operator must not miss.
    Alert(String),
}

impl OutputLine {
    pub fn system(s: impl Into<String>) -> Self {
        Self::System(s.into())
    }
    pub fn command(s: impl Into<String>) -> Self {
        Self::Command(s.into())
    }
    pub fn warn(s: impl Into<String>) -> Self {
        Self::Warn(s.into())
    }
    pub fn alert(s: impl Into<String>) -> Self {
        Self::Alert(s.into())
    }
}

/// One replayed log entry bound for the conversation pane.
///
/// Separate from [`OutputLine`] because replay lines must be
/// appended **silently** — re-persisting every row during a
/// `/resume` would double-count the prior session's events in
/// the new session's `events` table. The TUI's
/// `AppState::apply_dispatch` routes [`OutputLine`] through
/// `push` (which records) and [`ReplayLine`] through
/// `append_silent` (which does not).
///
/// `at_ms` preserves the original wall-clock timestamp so
/// rendered "age" readings stay truthful on replay — a
/// freshly-stamped row would lie about when the event actually
/// happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayLine {
    pub kind: ReplayKind,
    pub at_ms: i64,
    pub text: String,
}

/// What the dispatcher produced. Mode changes and quits are
/// separate side-channel effects so the caller can apply them
/// without string-parsing lines back.
//
// Four `bool` fields (`quit`, `clear_log`, `coaching_reset`,
// `dismiss_overlay`) trip `clippy::struct_excessive_bools`. They
// resist the suggested collapse-into-enum refactor for a concrete
// reason: they are orthogonal side-channel effects, not mutually
// exclusive states. A single command can legitimately emit
// `{clear_log: true, dismiss_overlay: true}` (e.g. `/clear`
// clears both the log and any floating overlay in one tick), or
// `{quit: true, dismiss_overlay: true}` on a quit from inside an
// open overlay. An enum would force the dispatcher to encode
// combinations as variants and every consumer (TUI `apply_dispatch`)
// to re-split them — a lossy projection followed by a re-derivation,
// which is exactly the shape the honesty bar rejects. Each field
// is documented with its exact meaning inline; adding a fifth
// bool should be a deliberate decision, not a silent growth.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DispatchOutput {
    pub lines: Vec<OutputLine>,
    /// Replayed log rows the TUI should append **without**
    /// persisting. Empty for every command except `/resume`, where
    /// the dispatcher emits one entry per event from the prior
    /// session's event log. Kept as a separate field (rather than
    /// an `OutputLine::Replayed` variant) so the routing rule
    /// "replay = silent, lines = recorded" is enforced at the
    /// type level — a future contributor cannot accidentally flip
    /// a normal line into silent mode or vice versa.
    pub replay_lines: Vec<ReplayLine>,
    pub mode_change: Option<ModeTarget>,
    /// Open a modal overlay on top of the current mode. The
    /// dispatcher only signals intent; the TUI resolves
    /// presentation + dismissal keybinds.
    pub show_overlay: Option<OverlayTarget>,
    pub quit: bool,
    pub clear_log: bool,
    pub risk: Option<RiskDirection>,
    /// Friction decision applied to the command. Always present
    /// when `risk` is. `Proceed` means "the command was run
    /// immediately"; `Pause` / `TypedConfirm` means "the command
    /// was *not* run — the caller must honor the friction before
    /// re-dispatching." That split is why `Decision` is emitted
    /// as data rather than baked into the line output.
    pub friction: Option<FrictionDecision>,
    /// Carries the resolved [`Command`] when [`DispatchOutput::friction`]
    /// is `Pause` or `TypedConfirm`, so the caller (TUI) can open
    /// a friction overlay and, after the pause elapses and any
    /// typed confirmation lands, re-run the command with
    /// [`run_bypass_friction`]. `None` for `Proceed` (the command
    /// already ran) and for commands without a risk direction.
    pub pending_command: Option<Command>,
    /// Verbose-mode intent emitted by `/verbose`. `None` means
    /// "leave it alone." A `Some(new_state)` carries the target
    /// boolean the TUI should swing to — the dispatcher resolves
    /// `Toggle` against [`DispatchContext::verbose_snapshot`] so
    /// the TUI is free of the toggle semantics and every
    /// downstream caller sees an absolute state, not an
    /// instruction.
    pub verbose_toggle: Option<bool>,
    /// Wrap-off intent emitted by `/wrap-off`. `Some(true)`
    /// means "skip wrap on the next /quit / session end for
    /// this session only" — the flag never persists across
    /// sessions (per ADDENDUM_A §9.1). `None` means the
    /// command did not touch the flag.
    pub wrap_off_toggle: Option<bool>,
    /// Coaching-reset intent emitted by `/coaching reset`.
    /// `true` means the TUI should empty its coaching buffer.
    /// Kept as a boolean (not an `Option<()>`) so the default
    /// value is immediately legible.
    pub coaching_reset: bool,
    /// Dismiss any active modal overlay. Used by commands whose
    /// "purpose" is to clear operator context (`/clear`) or by
    /// failure paths in commands that might otherwise leave a
    /// stale overlay floating (e.g. `/evaluate <coin>` when the
    /// engine returns an empty body — we emit an alert line and
    /// signal dismissal so the operator is not left staring at
    /// an older, unrelated verdict card). Ignored when
    /// [`DispatchOutput::show_overlay`] is `Some` — opening and
    /// closing in the same tick would be contradictory, and
    /// `show_overlay` wins because the data path is the reason
    /// the command ran.
    pub dismiss_overlay: bool,
}

impl DispatchOutput {
    #[must_use]
    pub fn with_line(mut self, l: OutputLine) -> Self {
        self.lines.push(l);
        self
    }
}

/// A thin read-only handle to the operator's current behavioural
/// label. The dispatcher consults this on every risk-increasing
/// command to compute the [`FrictionDecision`].
///
/// Implementations:
/// - [`StaticLabel`] — a fixed label; used in tests and when the
///   engine has not yet reported one.
/// - (future) `EngineLabel` — polled from `GET /operator/state`.
///
/// The trait is intentionally tiny so a future scheduler or CI
/// runner can plug in its own source without depending on the
/// `engine-client` crate.
pub trait StateSource: Send + Sync + 'static {
    fn label(&self) -> Label;
}

/// Trivial `StateSource` that returns the same label on every
/// call. The default value is [`Label::Steady`] — the "no friction,
/// nothing abnormal" label — so a fresh `DispatchContext` does
/// not accidentally gate commands when the engine has not yet
/// reported a state.
#[derive(Debug, Clone, Copy)]
pub struct StaticLabel(pub Label);

impl StaticLabel {
    #[must_use]
    pub const fn steady() -> Self {
        Self(Label::Steady)
    }
    #[must_use]
    pub const fn tilt() -> Self {
        Self(Label::Tilt)
    }
}

impl Default for StaticLabel {
    fn default() -> Self {
        Self::steady()
    }
}

impl StateSource for StaticLabel {
    fn label(&self) -> Label {
        self.0
    }
}

/// Shared context for dispatch. The HTTP client is optional — the
/// TUI launches even when the engine is unreachable, and commands
/// that need it degrade to a clear error line.
#[derive(Clone)]
pub struct DispatchContext {
    pub http: Option<HttpClient>,
    pub engine: Arc<RwLock<EngineState>>,
    /// Operator-state source. Defaults to `StaticLabel::steady()`
    /// so freshly-constructed contexts never accidentally gate.
    pub state: Arc<dyn StateSource>,
    /// Session store, if persistence is enabled. `None` when
    /// `--no-persist` is set or the DB failed to open — the
    /// session-cohort commands (`/sessions`, `/resume`, `/fork`,
    /// `/save`) then surface a single "persistence disabled"
    /// alert rather than pretending.
    pub sessions: Option<Arc<dyn SessionSource>>,
    /// Config introspection source. `None` in tests + headless
    /// paths; `/config show|doctor` then emit a single
    /// "unavailable" alert rather than panicking. Wired in
    /// production by `zero/src/main.rs` over `zero_config`.
    pub config: Option<Arc<dyn ConfigSource>>,
    /// Current verbose-rendering state, snapshotted at dispatch
    /// time by the caller. Lets `/verbose toggle` resolve into
    /// an absolute target without the dispatcher needing a
    /// trait-level callback. Defaults to `false` so commands
    /// that never touch verbosity do not need to set it.
    pub verbose: bool,
    /// Current wrap-off state, snapshotted at dispatch time.
    /// Lets `/wrap-off` become a no-op (with honest wording)
    /// when already disabled; mirrors `verbose` for the same
    /// reason — dispatcher stays pure.
    pub wrap_off: bool,
    /// Engine Auto-mode source. `None` when the engine is
    /// unreachable or no adapter has been installed — the
    /// dispatcher then surfaces `/auto` as "unavailable"
    /// rather than pretending. Wired in production by
    /// `crates/zero/src/main.rs` atop the engine client.
    pub auto: Option<Arc<dyn AutoSource>>,
    /// Operator-local supervisor source. `None` when no
    /// daemon adapter is installed — the dispatcher then
    /// surfaces `/headless` as "unavailable" and `/kill`
    /// falls through to the non-compound path. Wired by the
    /// future `zero-headless` adapter (ADR-006).
    pub supervisor: Option<Arc<dyn SupervisorSource>>,
}

impl std::fmt::Debug for DispatchContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DispatchContext")
            .field("http_connected", &self.http.is_some())
            .field("label", &self.state.label())
            .field("sessions_enabled", &self.sessions.is_some())
            .field("config_enabled", &self.config.is_some())
            .field("auto_enabled", &self.auto.is_some())
            .field("supervisor_enabled", &self.supervisor.is_some())
            .finish_non_exhaustive()
    }
}

impl DispatchContext {
    #[must_use]
    pub fn new(http: Option<HttpClient>, engine: Arc<RwLock<EngineState>>) -> Self {
        Self {
            http,
            engine,
            state: Arc::new(StaticLabel::steady()),
            sessions: None,
            config: None,
            verbose: false,
            wrap_off: false,
            auto: None,
            supervisor: None,
        }
    }

    /// Override the operator-state source.
    #[must_use]
    pub fn with_state(mut self, src: Arc<dyn StateSource>) -> Self {
        self.state = src;
        self
    }

    /// Attach a session store. Enables the session-cohort
    /// commands; without this they surface an "unavailable" alert.
    #[must_use]
    pub fn with_sessions(mut self, src: Arc<dyn SessionSource>) -> Self {
        self.sessions = Some(src);
        self
    }

    /// Attach a config introspection source. Enables
    /// `/config show` + `/config doctor`; without this they
    /// surface an "unavailable" alert.
    #[must_use]
    pub fn with_config(mut self, src: Arc<dyn ConfigSource>) -> Self {
        self.config = Some(src);
        self
    }

    /// Snapshot the caller's current verbose state so
    /// `/verbose toggle` can resolve into an absolute target
    /// without a round-trip to the TUI.
    #[must_use]
    pub const fn with_verbose(mut self, on: bool) -> Self {
        self.verbose = on;
        self
    }

    /// Snapshot the caller's wrap-off state so `/wrap-off`
    /// can surface an honest "already off" line without the
    /// dispatcher calling back into the TUI.
    #[must_use]
    pub const fn with_wrap_off(mut self, on: bool) -> Self {
        self.wrap_off = on;
        self
    }

    /// Attach an Auto-mode source. Enables `/auto on|off|status`;
    /// without this they surface an "unavailable" alert.
    #[must_use]
    pub fn with_auto(mut self, src: Arc<dyn AutoSource>) -> Self {
        self.auto = Some(src);
        self
    }

    /// Attach a supervisor source. Enables
    /// `/headless start|stop|status` and the `/kill` compound
    /// tear-down. Without this `/headless` surfaces an
    /// "unavailable" alert and `/kill` falls through to the
    /// non-compound path.
    #[must_use]
    pub fn with_supervisor(mut self, src: Arc<dyn SupervisorSource>) -> Self {
        self.supervisor = Some(src);
        self
    }
}

/// Parse, resolve, execute. Returns `Ok(None)` when the line is
/// empty — callers should skip rendering in that case.
///
/// # Errors
///
/// Never returns an error at this layer; engine failures become
/// structured `OutputLine::Alert` entries so the operator sees
/// them in context rather than as an unhandled result. The
/// `Result` signature is kept because future commands (`/rate`,
/// `/plan`, `/execute`) will need it.
pub async fn dispatch(ctx: &DispatchContext, input: &str) -> Result<Option<DispatchOutput>, Never> {
    let parsed = parse_line(input);
    let Some(cmd) = crate::command::resolve(&parsed) else {
        return Ok(None);
    };
    let risk = cmd.risk();
    let label = ctx.state.label();

    // M2 §3: derive an engine-side risk context so `decide_with_risk`
    // can reach L3 (guardrail proximity) / L4 (halted). A fresh
    // `DispatchContext` without a populated engine mirror
    // produces `RiskContext::default()`, which `decide_with_risk`
    // treats identically to `decide` — the L2 cap is preserved
    // exactly where the M1 code had it. The scope on the `read`
    // guard is deliberately tight: only the fields needed to
    // build the context are copied out, so the dispatcher does
    // not hold the `RwLock` across the `run` await point.
    let (risk_ctx, halt_reason, reread_phrase) = {
        let eng = ctx.engine.read();
        eng.risk
            .as_ref()
            .map(|stat| {
                let r = &stat.value;
                let rc = zero_operator_state::RiskContext::from_engine(
                    r.drawdown_pct,
                    r.last_drawdown_alert_pct,
                    r.is_halted(),
                );
                let halt_reason = halt_reason_label(r);
                let reread = reread_phrase_from_risk(r.drawdown_pct, r.last_drawdown_alert_pct);
                (rc, halt_reason, reread)
            })
            .unwrap_or_default()
    };

    let decision = crate::friction::decide_with_risk(
        risk,
        label,
        risk_ctx,
        halt_reason.as_deref(),
        reread_phrase,
    );

    // Only `Proceed` runs the command immediately. `Pause` /
    // `TypedConfirm` / `WaitAndReread` surface as friction
    // metadata + an explanatory line; the caller (TUI /
    // automation) is responsible for honoring the pause/re-read
    // and re-dispatching via `run_bypass_friction`. `HardStop`
    // surfaces the refusal line and the command is dropped — no
    // re-dispatch path, no `pending_command` to carry.
    //
    // CRITICAL: this branch must not be short-circuited for
    // `Reduces` commands. `decide_with_risk` already guarantees
    // `Reduces → Proceed` (tested in
    // `friction::tests::decide_with_risk_reduces_always_proceeds_even_when_halted`),
    // but the asymmetry is load-bearing — a regression that
    // added a pause branch here is the 2 AM failure mode the
    // whole architecture exists to prevent.
    let mut out = if matches!(decision, FrictionDecision::Proceed) {
        run(ctx, &cmd).await
    } else {
        friction_advisory(&cmd, label, &decision)
    };
    // Decide whether to carry the command through to the caller
    // *before* moving `decision` into `out`. When the command was
    // not run because of friction and is *not* a refusal, carry
    // the resolved `Command` so the TUI can re-invoke via
    // `run_bypass_friction` after honoring the pause / re-read.
    // L4 `HardStop` explicitly excludes the pending path — the
    // whole point of a refusal is that no re-dispatch can undo
    // it. Proceed path leaves `pending_command = None` — the
    // command already ran.
    let carry_pending = !matches!(decision, FrictionDecision::Proceed) && !decision.is_refusal();
    out.risk = Some(risk);
    out.friction = Some(decision);
    if carry_pending {
        out.pending_command = Some(cmd);
    }
    Ok(Some(out))
}

/// Pick a human-legible halt label from the engine's halt
/// booleans. Priority: `stop_failure_halt` > `global_halt` >
/// `halted`. `None` when the engine is not halted.
///
/// Kept near the dispatcher (not on `Risk`) because this is a
/// friction-layer concern: the engine-client crate intentionally
/// mirrors the wire shape without re-deriving labels. See
/// `Risk::halt_reason` for the engine's own free-form field
/// (surfaces in UI) — that one may be absent even when a halt
/// boolean is set, so this picker cannot defer to it alone.
fn halt_reason_label(risk: &zero_engine_client::models::Risk) -> Option<String> {
    if risk.stop_failure_halt {
        Some("stop_failure_halt".to_string())
    } else if risk.global_halt {
        Some("global_halt".to_string())
    } else if risk.halted {
        // Prefer the engine's own reason string when present —
        // it is the richest description available on the wire.
        // Fall back to the bare flag name so operators always see
        // something concrete.
        Some(
            risk.halt_reason
                .clone()
                .unwrap_or_else(|| "halted".to_string()),
        )
    } else {
        None
    }
}

/// Format the L3 re-read phrase from engine-reported drawdown
/// numbers. Returns `None` when either field is missing — the
/// decision layer falls back to `FALLBACK_REREAD_PHRASE` so the
/// operator still gets something concrete to type.
fn reread_phrase_from_risk(
    drawdown_pct: Option<f64>,
    last_drawdown_alert_pct: Option<f64>,
) -> Option<String> {
    let dd = drawdown_pct?;
    let alert = last_drawdown_alert_pct?;
    let delta = (alert - dd).abs();
    Some(format!(
        "i acknowledge drawdown {dd:.2}% is within {delta:.2}pp of the {alert:.2}% hard alert"
    ))
}

/// Run a [`Command`] **without** consulting the friction ladder.
///
/// This is the post-friction invocation path. The caller (TUI) has
/// honored the friction — either waited the required pause or
/// accepted a typed confirmation — and now asks the dispatcher to
/// execute. The returned output carries `risk` + `friction =
/// Proceed` so downstream logging + tests see a uniform shape.
///
/// # Safety rails
///
/// This function does **not** lower the risk-asymmetry invariant.
/// `Reduces` and `Neutral` commands reach the same `run` handler
/// as they do via the regular path; there is nothing here that a
/// caller could exploit to execute a never-gated command
/// differently. Calling `run_bypass_friction` on a `Reduces`
/// command is harmless — the command proceeds, same as always.
pub async fn run_bypass_friction(ctx: &DispatchContext, cmd: Command) -> DispatchOutput {
    let risk = cmd.risk();
    let mut out = run(ctx, &cmd).await;
    out.risk = Some(risk);
    out.friction = Some(FrictionDecision::Proceed);
    out
}

fn friction_advisory(cmd: &Command, label: Label, d: &FrictionDecision) -> DispatchOutput {
    // A single log breadcrumb. The pause countdown + typed-confirm
    // live in the TUI's friction-pause overlay (driven by
    // `friction` + `pending_command`); duplicating their wording
    // here would be log noise. Non-TUI callers (scripted runs,
    // tests) still see enough to know the command was gated and
    // by what level.
    let mut out = DispatchOutput::default();
    match d {
        FrictionDecision::Proceed => {}
        FrictionDecision::Pause { pause, level } => {
            out.lines.push(OutputLine::warn(format!(
                "{name}: friction {level:?} — state={label}, pause {pause}s",
                name = cmd.name(),
                pause = pause.as_secs(),
            )));
        }
        FrictionDecision::TypedConfirm { pause, level } => {
            let word = d.confirm_word().map_or_else(
                || crate::friction::TYPED_CONFIRM_WORD.to_string(),
                std::borrow::Cow::into_owned,
            );
            out.lines.push(OutputLine::alert(format!(
                "{name}: friction {level:?} — state={label}, {pause}s pause + type '{word}'",
                name = cmd.name(),
                pause = pause.as_secs(),
            )));
        }
        FrictionDecision::WaitAndReread {
            pause,
            level,
            phrase,
        } => {
            out.lines.push(OutputLine::alert(format!(
                "{name}: friction {level:?} — state={label}, {pause}s pause + re-read: '{phrase}'",
                name = cmd.name(),
                pause = pause.as_secs(),
            )));
        }
        FrictionDecision::HardStop { level, reason } => {
            // L4 is a refusal — surface it as `alert` so the TUI
            // lands the operator on the strongest log colour. No
            // "pause + type" hint: nothing the operator types
            // will un-refuse this. The Reduces path remains open
            // (`/kill`, `/break`, …).
            out.lines.push(OutputLine::alert(format!(
                "{name}: friction {level:?} REFUSED — state={label}, reason={reason}. \
                 Only risk-reducing commands are accepted while the engine is halted.",
                name = cmd.name(),
            )));
        }
    }
    out
}

async fn run(ctx: &DispatchContext, cmd: &Command) -> DispatchOutput {
    match cmd {
        Command::Help => help(),
        Command::Quit => DispatchOutput {
            quit: true,
            lines: vec![OutputLine::system("exiting")],
            ..Default::default()
        },
        Command::Clear => DispatchOutput {
            clear_log: true,
            // Clear is the operator's "clean slate" affordance —
            // dismissing any lingering modal overlay is part of
            // that contract. Without this, a stale verdict/state
            // card can survive a `/clear` and keep obscuring new
            // output until dismissed manually.
            dismiss_overlay: true,
            ..Default::default()
        },
        Command::SwitchMode(m) => DispatchOutput {
            mode_change: Some(*m),
            ..Default::default()
        },
        Command::Status => status(ctx).await,
        Command::Brief => brief(ctx).await,
        Command::Risk => risk_cmd(ctx).await,
        Command::HyperliquidStatus { symbol } => hl_status_cmd(ctx, symbol.as_deref()).await,
        Command::HyperliquidAccount => hl_account_cmd(ctx).await,
        Command::HyperliquidReconcile => hl_reconcile_cmd(ctx).await,
        Command::LiveCertify => live_certify_cmd(ctx).await,
        Command::LiveCockpit => live_cockpit_cmd(ctx).await,
        Command::Immune => immune_cmd(ctx).await,
        Command::Quote { symbol } => quote_cmd(ctx, symbol.as_deref()).await,
        Command::Regime { coin } => regime_cmd(ctx, coin.as_deref()).await,
        Command::Evaluate { coin, extras } => evaluate_cmd(ctx, coin.as_deref(), extras).await,
        Command::Positions => positions_cmd(ctx).await,
        Command::Pulse { limit } => pulse_cmd(ctx, *limit).await,
        Command::Approaching => approaching_cmd(ctx).await,
        Command::Rejections { coin, limit } => rejections_cmd(ctx, coin.as_deref(), *limit).await,
        Command::Kill => kill_cmd(ctx).await,
        Command::FlattenAll => flatten_cmd(ctx).await,
        Command::PauseEntries => pause_cmd(ctx).await,
        Command::ResumeEntries => resume_entries_cmd(ctx).await,
        Command::Break { minutes } => break_stub(ctx, *minutes).await,
        Command::Execute => execute_stub(),
        Command::State => DispatchOutput {
            show_overlay: Some(OverlayTarget::State),
            ..Default::default()
        },
        Command::Sessions { limit } => sessions_cmd(ctx, *limit),
        Command::Resume { needle } => resume_cmd(ctx, needle.as_deref()),
        Command::Fork => fork_cmd(ctx),
        Command::Save { label } => save_cmd(ctx, label.as_deref()),
        Command::Replay { needle } => replay_cmd(ctx, needle.as_deref()),
        Command::Share { needle } => share_cmd(ctx, needle.as_deref()),
        Command::Heat => heat_cmd(ctx).await,
        Command::Config { action } => config_cmd(ctx, action),
        Command::Verbose { action } => verbose_cmd(ctx, action),
        Command::StateOverride { label } => state_override_cmd(*label),
        Command::Continue => continue_cmd(),
        Command::Close { coin } => close_cmd(coin.as_deref()),
        Command::WrapOff => wrap_off_cmd(),
        Command::CoachingReset => coaching_reset_cmd(),
        Command::DisclosureOverride { confirmed } => disclosure_override_cmd(*confirmed),
        Command::Rate { trade_id, rating } => rate_cmd(ctx, trade_id.as_deref(), *rating).await,
        Command::ZeroPrefix { rest } => zero_prefix_hint(rest),
        Command::Auto { action } => auto_cmd(ctx, action),
        Command::Headless { action } => headless_cmd(ctx, action),
        Command::Unknown(head) => DispatchOutput {
            lines: vec![OutputLine::warn(format!(
                "unknown command: /{head}  (try /help)"
            ))],
            ..Default::default()
        },
    }
}

fn help() -> DispatchOutput {
    let mut out = DispatchOutput::default();
    out.lines.push(OutputLine::system("commands:"));
    out.lines
        .push(OutputLine::system("  /help                — this list"));
    out.lines
        .push(OutputLine::system("  /quit                — exit"));
    out.lines
        .push(OutputLine::system("  /clear               — clear the log"));
    out.lines.push(OutputLine::system(
        "  /conv /decisions /heat-mode /pos-mode — switch modes",
    ));
    out.lines
        .push(OutputLine::system("  /status              — engine status"));
    out.lines.push(OutputLine::system(
        "  /brief               — morning briefing",
    ));
    out.lines.push(OutputLine::system(
        "  /risk                — guardrail summary",
    ));
    out.lines.push(OutputLine::system(
        "  /hl-status [coin]    — read-only Hyperliquid info status",
    ));
    out.lines.push(OutputLine::system(
        "  /hl-account          — read-only Hyperliquid account truth",
    ));
    out.lines.push(OutputLine::system(
        "  /hl-reconcile        — Hyperliquid account reconciliation",
    ));
    out.lines.push(OutputLine::system(
        "  /live-certify        — dry-run live execution certification",
    ));
    out.lines.push(OutputLine::system(
        "  /live-cockpit        — live readiness cockpit",
    ));
    out.lines.push(OutputLine::system(
        "  /immune              — immune breaker state",
    ));
    out.lines.push(OutputLine::system(
        "  /quote <coin>        — active paper quote source",
    ));
    out.lines.push(OutputLine::system(
        "  /heat                — composite heat (risk + circuit state)",
    ));
    out.lines
        .push(OutputLine::system("  /regime [coin]       — market regime"));
    out.lines.push(OutputLine::system(
        "  /evaluate <coin>     — gate verdict (overlay)",
    ));
    out.lines.push(OutputLine::system(
        "  /pos                 — open positions",
    ));
    out.lines.push(OutputLine::system(
        "  /pulse [N]           — recent engine events",
    ));
    out.lines.push(OutputLine::system(
        "  /approaching         — coins near a gate",
    ));
    out.lines.push(OutputLine::system(
        "  /rejections [coin] [N] — recent gate rejections",
    ));
    out.lines.push(OutputLine::system(
        "  /kill /flatten-all /pause-entries /break /close  — risk-reducers (instant)",
    ));
    out.lines.push(OutputLine::system(
        "  /resume-entries      — resume new entries (friction-gated)",
    ));
    out.lines.push(OutputLine::system(
        "  /close <coin>        — close a single position",
    ));
    out.lines.push(OutputLine::system(
        "  /execute             — composition change (gated by operator state)",
    ));
    out.lines.push(OutputLine::system(
        "  /state               — full operator-state overview (any key closes)",
    ));
    out.lines.push(OutputLine::system(
        "  /state-override <L>  — declare operator-state label (gated)",
    ));
    out.lines.push(OutputLine::system(
        "  /continue            — acknowledge coaching notice",
    ));
    out.lines.push(OutputLine::system(
        "  /coaching reset      — clear coaching notice buffer",
    ));
    out.lines.push(OutputLine::system(
        "  /wrap-off            — skip daily wrap (this session only)",
    ));
    out.lines.push(OutputLine::system(
        "  /disclosure-override --i-know-what-i-am-doing  — bypass progressive disclosure",
    ));
    // Diagnostic commands grouped at the bottom so an operator
    // scanning for "what do I type when things are broken" finds
    // them without hunting. `/doctor` is the top-level alias for
    // `/config doctor`; both routes exist so operators who think
    // in either shape land somewhere.
    out.lines.push(OutputLine::system(
        "  /doctor              — diagnose config + secrets (alias for /config doctor)",
    ));
    out.lines.push(OutputLine::system(
        "  /config show         — show resolved config values",
    ));
    out.lines.push(OutputLine::system(
        "mode switches are also on Ctrl+1..4. Ctrl+0 returns to Conversation.",
    ));
    out
}

/// Render the "you're already inside zero" hint when an operator
/// types a shell-style `zero …` invocation at the TUI prompt.
///
/// Resolution rules (honesty bar: every suggestion must actually
/// exist, or the hint makes the tool look broken):
///
/// - `zero doctor` → suggest `/doctor` (lands today via the
///   alias we just added).
/// - `zero --version` / `zero version` → suggest `/quit` to
///   return to the shell; the version banner is only exposed
///   out-of-TUI because in-TUI it would show at startup anyway.
///   Reference to a `/version` command is deliberately avoided
///   — that command does not exist inside the TUI.
/// - `zero init` / `zero run` / anything else → suggest
///   `/quit` + re-invoke; these are shell-only entry points and
///   no in-TUI equivalent exists. Telling an operator to
///   `/init` would reproduce the ghost-command mistake from
///   `zero pair`.
/// - `zero` alone → suggest `/help`; the operator may just be
///   exploring.
///
/// The hint echoes the literal tail the operator typed so they
/// see their own intent reflected back (trust-builds-through-
/// precision), then offers the one or two correct next steps.
fn zero_prefix_hint(rest: &str) -> DispatchOutput {
    let tail = rest.trim();
    let hint = match tail {
        "" => "you're already inside zero — try `/help` to list commands".to_owned(),
        "doctor" | "doctor --fix" | "doctor --format json" => {
            "you're already inside zero — try `/doctor` (or `/config doctor`)".to_owned()
        }
        "version" | "--version" | "-V" => {
            "you're already inside zero — the version banner printed at startup; `/quit` returns to the shell".to_owned()
        }
        other => format!(
            "you're already inside zero — `{other}` is a shell subcommand. `/quit` returns to the shell, or try `/help`"
        ),
    };
    DispatchOutput {
        lines: vec![OutputLine::warn(hint)],
        ..Default::default()
    }
}

fn require_http<'a>(ctx: &'a DispatchContext, out: &mut DispatchOutput) -> Option<&'a HttpClient> {
    if let Some(c) = &ctx.http {
        Some(c)
    } else {
        out.lines.push(OutputLine::alert(
            "no engine client configured — run `zero init` or set ZERO_API_URL",
        ));
        None
    }
}

async fn status(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.v2_status().await {
        Ok(s) => {
            let regime = s.regime().unwrap_or("—");
            let conf = match (s.engine_confidence(), s.confidence_level()) {
                (Some(score), Some(level)) => format!("{score:.0} ({level})"),
                (Some(score), None) => format!("{score:.0}"),
                (None, Some(level)) => level.to_string(),
                (None, None) => "—".into(),
            };
            let eq = s.equity().map_or("—".into(), |v| format!("${v:.2}"));
            let open = s.open().map_or("—".into(), |n| n.to_string());
            let upnl = s
                .unrealized_pnl()
                .map_or("—".into(), |v| format!("{v:+.2}"));
            out.lines.push(OutputLine::command(format!(
                "engine: regime={regime}  confidence={conf}  equity={eq}  open={open}  upnl={upnl}"
            )));
            let today = &s.today;
            if today.trades.is_some() || today.pnl.is_some() {
                let trades = today.trades.map_or("—".into(), |n| n.to_string());
                let wins = today.wins.map_or("—".into(), |n| n.to_string());
                let pnl = today.pnl.map_or("—".into(), |v| format!("{v:+.2}"));
                let streak = today.streak.map_or("—".into(), |n| format!("{n:+}"));
                let sizing = today.sizing_mult.map_or("—".into(), |v| format!("{v:.2}x"));
                out.lines.push(OutputLine::system(format!(
                    "  today: trades={trades}  wins={wins}  pnl={pnl}  streak={streak}  sizing={sizing}"
                )));
            }
            let market = &s.market;
            if market.fear_greed.is_some() || market.health.is_some() {
                let fg = market.fear_greed.map_or("—".into(), |n| n.to_string());
                let health = market
                    .health
                    .map_or("—".into(), |v| format!("{:.0}%", v * 100.0));
                let coins = market.coins_tradeable.map_or("—".into(), |n| n.to_string());
                out.lines.push(OutputLine::system(format!(
                    "  market: fear_greed={fg}  health={health}  coins_tradeable={coins}"
                )));
            }
            if let Some(recovery) = &s.recovery {
                let status = recovery.status.as_deref().unwrap_or("unknown");
                let source = recovery.source.as_deref().unwrap_or("unknown");
                let durable = if recovery.durable {
                    "durable"
                } else {
                    "ephemeral"
                };
                let decisions = recovery
                    .current_decisions
                    .or(recovery.decisions_recovered)
                    .map_or("—".into(), |n| n.to_string());
                let fills = recovery
                    .current_fills
                    .or(recovery.fills_recovered)
                    .map_or("—".into(), |n| n.to_string());
                let positions = recovery
                    .current_positions
                    .or(recovery.positions_recovered)
                    .map_or("—".into(), |n| n.to_string());
                out.lines.push(OutputLine::system(format!(
                    "  recovery: {status}  source={source}  journal={durable}  decisions={decisions}  fills={fills}  positions={positions}"
                )));
            }
        }
        Err(e) => out.lines.push(OutputLine::alert(format!("status: {e}"))),
    }
    out
}

async fn brief(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.brief().await {
        Ok(b) => {
            if !b.has_content() {
                out.lines
                    .push(OutputLine::system("(engine has no briefing right now)"));
                return out;
            }
            let open = b.open_positions.unwrap_or(0);
            let fg = b
                .fear_greed
                .map_or("—".into(), |v| format!("{v} ({})", fg_sentiment(v)));
            out.lines.push(OutputLine::command(format!(
                "brief: open={open}  fear_greed={fg}  signals={}  approaching={}",
                b.recent_signals.len(),
                b.approaching.len(),
            )));
            for pos in b.positions.iter().take(8) {
                let pnl = pos
                    .unrealized_pnl
                    .map_or_else(|| "—".into(), |v| format!("{v:+.2}"));
                out.lines.push(OutputLine::system(format!(
                    "  position {}  {}  size={:.4}  entry={:.2}  pnl={}",
                    pos.symbol, pos.side, pos.size, pos.entry, pnl
                )));
            }
            for sig in b.recent_signals.iter().take(5) {
                if let Some(summary) = brief_line_summary(sig) {
                    out.lines
                        .push(OutputLine::system(format!("  signal  {summary}")));
                }
            }
            for app in b.approaching.iter().take(5) {
                if let Some(summary) = brief_line_summary(app) {
                    out.lines
                        .push(OutputLine::system(format!("  approaching  {summary}")));
                }
            }
            if let Some(cycle) = b.last_cycle.as_object()
                && !cycle.is_empty()
            {
                let parts: Vec<String> = cycle
                    .iter()
                    .take(5)
                    .map(|(k, v)| format!("{k}={}", compact_json_value(v)))
                    .collect();
                out.lines.push(OutputLine::system(format!(
                    "  last_cycle  {}",
                    parts.join("  ")
                )));
            }
        }
        Err(e) => out.lines.push(OutputLine::alert(format!("brief: {e}"))),
    }
    out
}

async fn hl_status_cmd(ctx: &DispatchContext, symbol: Option<&str>) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.hyperliquid_status(symbol).await {
        Ok(s) if !s.enabled => {
            let reason = s
                .reason
                .as_deref()
                .unwrap_or("Hyperliquid read-only adapter disabled");
            out.lines
                .push(OutputLine::warn(format!("hl: disabled — {reason}")));
        }
        Ok(s) => {
            let coins = s.coins.map_or("—".into(), |n| n.to_string());
            let secrets = s
                .secrets_required
                .map_or("—".into(), |required| required.to_string());
            out.lines.push(OutputLine::command(format!(
                "hl: enabled  coins={coins}  secrets_required={secrets}"
            )));
            for (symbol, mid) in s.mids.iter().take(8) {
                out.lines
                    .push(OutputLine::system(format!("  {symbol}: mid={mid:.4}")));
            }
        }
        Err(e) => out.lines.push(OutputLine::alert(format!("hl-status: {e}"))),
    }
    out
}

async fn hl_account_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.hyperliquid_account().await {
        Ok(account) => {
            let equity = account
                .account_value
                .map_or("—".into(), |value| format!("${value:.2}"));
            let margin = account
                .margin_used
                .map_or("—".into(), |value| format!("${value:.2}"));
            out.lines.push(OutputLine::command(format!(
                "hl-account: user={}  equity={equity}  margin={margin}  positions={}  open_orders={}",
                account.user,
                account.positions.len(),
                account.open_orders.len()
            )));
            for position in account.positions.iter().take(8) {
                out.lines.push(OutputLine::system(format!(
                    "  {} {} qty={:.6} entry={:.4} value=${:.2} upnl=${:.2}",
                    position.symbol,
                    position.side,
                    position.quantity.abs(),
                    position.entry_price,
                    position.position_value,
                    position.unrealized_pnl
                )));
            }
        }
        Err(e) => out
            .lines
            .push(OutputLine::alert(format!("hl-account: {e}"))),
    }
    out
}

async fn hl_reconcile_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.hyperliquid_reconciliation().await {
        Ok(report) => {
            out.lines.push(OutputLine::command(format!(
                "hl-reconcile: status={}  risk_increasing_allowed={}  reason={}",
                report.status, report.risk_increasing_allowed, report.reason
            )));
            for drift in report.drifts.iter().take(8) {
                let symbol = drift.symbol.as_deref().unwrap_or("account");
                out.lines.push(OutputLine::system(format!(
                    "  {symbol}: {} {} — {}",
                    drift.severity, drift.code, drift.reason
                )));
            }
        }
        Err(e) => out
            .lines
            .push(OutputLine::alert(format!("hl-reconcile: {e}"))),
    }
    out
}

async fn live_certify_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.live_certification().await {
        Ok(report) => {
            let passed = report
                .summary
                .get("passed")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let total = report
                .summary
                .get("total")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(report.drills.len() as u64);
            out.lines.push(OutputLine::command(format!(
                "live-certify: passed={}  live_start_certified={}  drills={passed}/{total}",
                report.passed, report.live_start_certified
            )));
            for drill in report
                .drills
                .iter()
                .filter(|drill| drill.status != "pass")
                .take(8)
            {
                out.lines.push(OutputLine::system(format!(
                    "  {}: {} — {}",
                    drill.name, drill.status, drill.note
                )));
            }
        }
        Err(e) => out
            .lines
            .push(OutputLine::alert(format!("live-certify: {e}"))),
    }
    out
}

async fn live_cockpit_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.live_cockpit().await {
        Ok(cockpit) => {
            let preflight_total = json_u64(&cockpit.preflight.summary, "total");
            let preflight_passed = json_u64(&cockpit.preflight.summary, "passed");
            let preflight_failed = json_u64(&cockpit.preflight.summary, "failed");
            let immune_open = json_u64(&cockpit.immune.summary, "open");
            let immune_blocking = json_u64(&cockpit.immune.summary, "risk_blocking");
            let cert_total = json_u64(&cockpit.certification.summary, "total");
            let cert_passed = json_u64(&cockpit.certification.summary, "passed");
            let timeout = cockpit
                .heartbeat
                .timeout_s
                .map_or_else(|| "n/a".to_string(), |s| s.to_string());

            out.lines.push(OutputLine::command(format!(
                "live-cockpit: live_mode={}  ready={}  risk_allowed={}  controls_ready={}",
                cockpit.live_mode,
                cockpit.ready,
                cockpit.risk_increasing_allowed,
                cockpit.controls_ready
            )));
            out.lines.push(OutputLine::system(format!(
                "  next: {}",
                cockpit.next_action
            )));
            out.lines.push(OutputLine::system(format!(
                "  operator: handle={} id={} role={} scope={}",
                cockpit.operator_context.handle,
                cockpit.operator_context.operator_id,
                cockpit.operator_context.role,
                cockpit.operator_context.scope
            )));
            out.lines.push(OutputLine::system(format!(
                "  preflight: passed={preflight_passed}/{preflight_total} failed={preflight_failed}"
            )));
            out.lines.push(OutputLine::system(format!(
                "  immune: open={immune_open} risk_blocking={immune_blocking}"
            )));
            out.lines.push(OutputLine::system(format!(
                "  reconcile: status={} risk_allowed={} drifts={} - {}",
                cockpit.reconciliation.status,
                cockpit.reconciliation.risk_increasing_allowed,
                cockpit.reconciliation.drifts,
                cockpit.reconciliation.reason
            )));
            out.lines.push(OutputLine::system(format!(
                "  certification: passed={} live_start_certified={} drills={cert_passed}/{cert_total}",
                cockpit.certification.passed, cockpit.certification.live_start_certified
            )));
            out.lines.push(OutputLine::system(format!(
                "  heartbeat: configured={} expired={} timeout_s={timeout}",
                cockpit.heartbeat.configured, cockpit.heartbeat.expired
            )));
            out.lines.push(OutputLine::system(format!(
                "  live-records: total={} accepted={} refused={} exchange_error={}",
                cockpit.live_records.total,
                cockpit.live_records.accepted,
                cockpit.live_records.refused,
                cockpit.live_records.exchange_error
            )));
            for check in cockpit.preflight.failed_checks.iter().take(4) {
                out.lines.push(OutputLine::system(format!(
                    "  preflight:{} {} - {}",
                    check.name, check.status, check.note
                )));
            }
            for breaker in cockpit.immune.open_breakers.iter().take(4) {
                out.lines.push(OutputLine::system(format!(
                    "  breaker:{} {} - {}",
                    breaker.name, breaker.status, breaker.reason
                )));
            }
            out.lines.push(OutputLine::system(
                "  actions: reduce=/pause-entries /kill /flatten-all  resume=/resume-entries",
            ));
        }
        Err(e) => out
            .lines
            .push(OutputLine::alert(format!("live-cockpit: {e}"))),
    }
    out
}

async fn immune_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.immune().await {
        Ok(report) => {
            let open = report
                .summary
                .get("open")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_else(|| report.breakers.iter().filter(|b| b.blocks_risk).count() as u64);
            out.lines.push(OutputLine::command(format!(
                "immune: risk_increasing_allowed={}  open={}  mode={}",
                report.risk_increasing_allowed, open, report.mode
            )));
            for breaker in report
                .breakers
                .iter()
                .filter(|breaker| breaker.blocks_risk)
                .take(8)
            {
                out.lines.push(OutputLine::system(format!(
                    "  {}: {} - {}",
                    breaker.name, breaker.status, breaker.reason
                )));
            }
        }
        Err(e) => out.lines.push(OutputLine::alert(format!("immune: {e}"))),
    }
    out
}

fn json_u64(map: &std::collections::BTreeMap<String, serde_json::Value>, key: &str) -> u64 {
    map.get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

async fn quote_cmd(ctx: &DispatchContext, symbol: Option<&str>) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(symbol) = symbol else {
        out.lines.push(OutputLine::warn(
            "/quote <coin> — name the coin to inspect (e.g. /quote BTC)",
        ));
        return out;
    };
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.market_quote(symbol).await {
        Ok(q) => {
            let live = if q.live { "live" } else { "fixture" };
            out.lines.push(OutputLine::command(format!(
                "quote {}: {:.4}  source={}  mode={live}",
                q.symbol, q.price, q.source
            )));
            if let Some(as_of) = q.as_of {
                out.lines
                    .push(OutputLine::system(format!("  as_of={as_of}")));
            }
        }
        Err(e) => out.lines.push(OutputLine::alert(format!("quote: {e}"))),
    }
    out
}

/// Map a fear-greed score (0..=100) to the conventional label
/// the engine uses on `/brief` and `/v2/status`. Centralized so
/// `/brief` and any future status readout agree on thresholds.
fn fg_sentiment(v: i64) -> &'static str {
    match v {
        i64::MIN..=24 => "extreme fear",
        25..=44 => "fear",
        45..=55 => "neutral",
        56..=74 => "greed",
        _ => "extreme greed",
    }
}

/// Collapse a nested JSON value to a one-line "k=v  k=v" summary for
/// briefing lists. Strings and scalars pass through; objects render
/// their first few keys; arrays render their length. Prevents the
/// brief output from dumping multi-line JSON into the pane.
fn brief_line_summary(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(map) if !map.is_empty() => {
            let parts: Vec<String> = map
                .iter()
                .take(4)
                .map(|(k, v)| format!("{k}={}", compact_json_value(v)))
                .collect();
            Some(parts.join("  "))
        }
        serde_json::Value::Array(items) if !items.is_empty() => {
            Some(format!("[{} items]", items.len()))
        }
        serde_json::Value::Null => None,
        other => Some(compact_json_value(other)),
    }
}

/// Render a JSON scalar the way an operator expects at a glance:
/// numbers un-quoted, strings un-quoted, objects/arrays as their
/// compact count/shape marker.
fn compact_json_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "—".into(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => format!("[{}]", a.len()),
        serde_json::Value::Object(m) => format!("{{{}}}", m.len()),
    }
}

async fn risk_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.risk().await {
        Ok(r) => {
            let halted = r.is_halted();
            // Cross-field consistency probe: a peak-equity tracker
            // that advertises `equity > peak` is violating its own
            // definition — the number the engine is telling the
            // operator to plan against is older than the equity it
            // is simultaneously reporting. Render the mismatch
            // explicitly instead of passing through a confident
            // `dd=0.22%` that was computed against a stale peak.
            // Tolerance: 1% of peak covers float rounding + one
            // missed bus-flush tick without crying wolf.
            // Peak the engine actually derives `drawdown_pct` against
            // is the rolling 30d, not lifetime. Rendering lifetime
            // `peak_equity` next to `dd` produced a second kind of
            // apparent inconsistency — "peak=$613 dd=10%" only
            // reconciles against the 30d peak of $640. Prefer the
            // 30d anchor so the line's arithmetic is checkable by
            // eye; fall back to lifetime if 30d is missing.
            let peak_ref = r.peak_equity_30d.or(r.peak_equity);
            let equity_above_peak = match (r.account_value, peak_ref) {
                (Some(eq), Some(peak)) if peak > 0.0 => eq > peak * 1.01,
                _ => false,
            };
            let dd = if equity_above_peak {
                // Do not display a percent we know is wrong. The
                // warning line below tells the operator why.
                "—".to_string()
            } else {
                r.drawdown_pct.map_or("—".into(), |v| format!("{v:.2}%"))
            };
            let daily_loss = r
                .daily_loss_pct()
                .map(|v| format!("{v:.2}%"))
                .or_else(|| r.daily_loss_usd.map(|v| format!("${v:.2}")))
                .unwrap_or_else(|| "—".into());
            let daily_pnl = r.daily_pnl_usd.map_or("—".into(), |v| format!("{v:+.2}"));
            let eq = r.account_value.map_or("—".into(), |v| format!("${v:.2}"));
            let peak = peak_ref.map_or("—".into(), |v| format!("${v:.2}"));
            let open = r.open_count.map_or("—".into(), |n| n.to_string());
            let state = if halted { "HALTED" } else { "OK" };
            let line = format!(
                "risk: {state}  equity={eq}  peak={peak}  dd={dd}  daily-pnl={daily_pnl}  \
                 daily-loss={daily_loss}  open={open}"
            );
            if halted {
                out.lines.push(OutputLine::alert(line));
                if let Some(reason) = &r.halt_reason {
                    out.lines
                        .push(OutputLine::alert(format!("  halt reason: {reason}")));
                }
                if let Some(until) = &r.halt_until {
                    out.lines
                        .push(OutputLine::alert(format!("  halt until: {until}")));
                }
            } else {
                out.lines.push(OutputLine::command(line));
            }
            if equity_above_peak {
                // Warn (not alert) because this is a data-integrity
                // oddity, not an active risk event. The operator
                // just needs to know the numbers in this row do not
                // agree with themselves and the engine is the
                // source to audit, not the CLI.
                out.lines.push(OutputLine::warn(
                    "  inconsistent: equity > peak — engine peak-equity tracker is stale \
                     (see bus/risk.json vs bus/portfolio.json); dd suppressed",
                ));
            }
            if r.capital_floor_hit {
                out.lines
                    .push(OutputLine::alert("  capital floor hit".to_string()));
            }
        }
        Err(e) => out.lines.push(OutputLine::alert(format!("risk: {e}"))),
    }
    out
}

/// Composite heat: a single-line "how hot am I?" readout that
/// folds every risk-proximity signal the engine exposes into one
/// number.
///
/// Formula:
/// - `heat` (0..=100) is the maximum of the three percent-scaled
///   risk metrics (drawdown / daily-loss / exposure). A percent
///   scale is honest because the engine already returns percent
///   values; we do not invent a denominator we cannot justify.
/// - If the kill switch is on or the circuit breaker is active,
///   heat is pinned to 100 — both conditions mean "no new risk
///   will clear regardless of what any single meter says."
/// - Positions (`n/max`) is appended verbatim rather than folded
///   into the score so operators can read "pos=2/3" without
///   having to invert it into a percent.
///
/// Styling:
/// - Alert (red/bold) when heat >= 80, kill, or breaker. An
///   operator glancing at the log should see those hits without
///   reading the whole line.
/// - Command (neutral) otherwise.
async fn heat_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.risk().await {
        Ok(r) => {
            let dd = r.drawdown_pct.unwrap_or(0.0);
            let daily = r.daily_loss_pct().unwrap_or(0.0);
            let score_pct = dd.max(daily).clamp(0.0, 100.0);
            let pinned = r.is_halted() || r.capital_floor_hit;
            let heat_pct = if pinned { 100.0 } else { score_pct };
            let halted = if r.is_halted() { "on" } else { "off" };
            let floor = if r.capital_floor_hit { "on" } else { "off" };
            let open = r.open_count.map_or("—".into(), |n| n.to_string());
            let level = if pinned {
                "CRITICAL"
            } else if heat_pct >= 80.0 {
                "HIGH"
            } else if heat_pct >= 50.0 {
                "WARM"
            } else {
                "COOL"
            };
            let line = format!(
                "heat: {level} {heat_pct:.0}%  dd={dd:.1}%  daily-loss={daily:.1}%  \
                 open={open}  halted={halted}  floor={floor}"
            );
            if pinned || heat_pct >= 80.0 {
                out.lines.push(OutputLine::alert(line));
            } else {
                out.lines.push(OutputLine::command(line));
            }
        }
        Err(e) => out.lines.push(OutputLine::alert(format!("heat: {e}"))),
    }
    out
}

async fn regime_cmd(ctx: &DispatchContext, coin: Option<&str>) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    let label = coin.unwrap_or("market");
    match http.regime(coin).await {
        Ok(r) => {
            // Some engine builds return a 200 with `{"error": "<msg>"}`
            // when a coin lookup misses — a valid envelope but not a
            // useful regime. Surface that as a real alert rather than
            // rendering a row of em-dashes that looks like data.
            if let Some(err) = r.extra.get("error").and_then(|v| v.as_str()) {
                out.lines
                    .push(OutputLine::alert(format!("regime[{label}]: {err}")));
                return out;
            }
            // Bare-`{}` path: the engine decoded cleanly but had
            // nothing to say (older builds expose `/regime` but
            // never populate it). An em-dash row here is worse
            // than useless — it masquerades as data. Tell the
            // operator plainly that the engine has no regime
            // reading right now.
            if r.regime.is_none() && r.confidence.is_none() {
                out.lines.push(OutputLine::alert(format!(
                    "regime[{label}]: engine has no regime reading (empty response)"
                )));
                return out;
            }
            let name = r.regime.as_deref().unwrap_or("—");
            let conf = r.confidence.map_or("—".into(), |v| format!("{v:.2}"));
            out.lines.push(OutputLine::command(format!(
                "regime[{label}]: {name}  confidence={conf}"
            )));
        }
        Err(e) => out.lines.push(OutputLine::alert(format!("regime: {e}"))),
    }
    out
}

async fn evaluate_cmd(
    ctx: &DispatchContext,
    coin: Option<&str>,
    extras: &[String],
) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    // Missing-argument path: resolve to a usage hint rather than
    // a silent warn so operators never wonder whether the command
    // was accepted. Keeps the picker entry ("/evaluate") and the
    // command consistent.
    let Some(raw) = coin else {
        out.lines.push(OutputLine::warn(
            "/evaluate <coin> — name the coin to evaluate (e.g. /evaluate BTC)",
        ));
        return out;
    };
    let coin = raw.trim();
    if coin.is_empty() {
        out.lines.push(OutputLine::warn(
            "/evaluate <coin> — name the coin to evaluate (e.g. /evaluate BTC)",
        ));
        return out;
    }

    // Surface trailing tokens as a warning before the HTTP call so
    // operators who type `/evaluate sol short` (assuming they can
    // bias direction) are told plainly that the extras do nothing.
    // We still run the evaluate — the coin is unambiguous and
    // aborting would be punitive for a harmless typo.
    if !extras.is_empty() {
        out.lines.push(OutputLine::warn(format!(
            "/evaluate takes only a coin — ignoring extra args: {}",
            extras.join(" ")
        )));
    }

    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.evaluate(coin).await {
        Ok(mut eval) => {
            // Engines sometimes omit the coin on the response even
            // when we passed it in. Backfill so the overlay header
            // is never `?` when we literally just named the coin.
            if eval.coin.is_none() {
                eval.coin = Some(coin.to_string());
            }
            // Guard against a degenerate-but-HTTP-200 response:
            // no layers AND no direction means the verdict card
            // would render its "no verdict — `/evaluate <coin>`
            // to request one" placeholder, which is worse than
            // useless here — the operator *did* request one, and
            // showing the placeholder makes it look like the
            // request silently failed. Emit a real alert instead
            // and dismiss any prior (stale) overlay so the error
            // is visible, not hidden behind an older card.
            if eval.layers.is_empty() && eval.direction.is_none() {
                out.lines.push(OutputLine::alert(format!(
                    "evaluate {coin}: engine returned an empty verdict (no layers, no direction)"
                )));
                out.dismiss_overlay = true;
                return out;
            }
            out.show_overlay = Some(OverlayTarget::Verdict(Box::new(eval)));
            // No lines: the overlay is the output surface. A
            // parallel system line would duplicate everything the
            // card already renders, and once we ship session
            // logging the overlay's payload is what gets recorded.
        }
        Err(e) => {
            out.lines
                .push(OutputLine::alert(format!("evaluate {coin}: {e}")));
            // Dismiss any stale overlay from a prior `/evaluate`
            // so the alert is the visible result, not hidden
            // behind an unrelated card that still reads "verdict
            // · OTHER_COIN".
            out.dismiss_overlay = true;
        }
    }
    out
}

async fn positions_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.positions().await {
        Ok(p) => {
            if p.items.is_empty() {
                out.lines
                    .push(OutputLine::system("flat — no open positions"));
                return out;
            }
            for pos in &p.items {
                let pnl = pos
                    .unrealized_pnl
                    .map_or_else(|| "—".into(), |v| format!("{v:+.2}"));
                out.lines.push(OutputLine::command(format!(
                    "{}  {}  size={:.4}  entry={:.2}  pnl={}",
                    pos.symbol, pos.side, pos.size, pos.entry, pnl
                )));
            }
        }
        Err(e) => out.lines.push(OutputLine::alert(format!("positions: {e}"))),
    }
    out
}

async fn pulse_cmd(ctx: &DispatchContext, limit: Option<u32>) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    let n = limit.unwrap_or_else(Command::default_pulse_limit);
    match http.pulse(n).await {
        Ok(p) => {
            if p.items.is_empty() {
                out.lines.push(OutputLine::system(
                    "(pulse idle — engine has no recent events)",
                ));
                return out;
            }
            for ev in &p.items {
                let ts = trim_ts(ev.ts.as_deref());
                let kind = ev.kind.as_deref().unwrap_or("event");
                let coin = ev.coin.as_deref().unwrap_or("—");
                let msg = ev.message.as_deref().unwrap_or("(no message)");
                let line = format!("{ts}  {kind:<10}  {coin:<6}  {msg}");
                // Route severity=warn/alert to the alert lane so the
                // TUI palette paints it red; everything else is
                // neutral command output.
                match ev.severity.as_deref() {
                    Some("warn" | "warning") => out.lines.push(OutputLine::warn(line)),
                    Some("alert" | "error" | "critical") => {
                        out.lines.push(OutputLine::alert(line));
                    }
                    _ => out.lines.push(OutputLine::command(line)),
                }
            }
        }
        Err(e) => out.lines.push(OutputLine::alert(format!("pulse: {e}"))),
    }
    out
}

async fn approaching_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    match http.approaching().await {
        Ok(feed) => {
            if feed.items.is_empty() {
                out.lines.push(OutputLine::system("(nothing approaching)"));
                return out;
            }
            // Sort by ascending distance so the first row is the
            // candidate the operator actually has to watch. `None`
            // distances sort last — we cannot rank them.
            let mut items = feed.items.clone();
            items.sort_by(|a, b| match (a.distance_to_gate, b.distance_to_gate) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });
            for a in &items {
                let dir = a.direction.as_deref().unwrap_or("—");
                let gate = a.gate.as_deref().unwrap_or("—");
                let dist = a
                    .distance_to_gate
                    .map_or_else(|| "—".into(), |d| format!("{d:+.3}"));
                out.lines.push(OutputLine::command(format!(
                    "{coin:<6}  {dir:<5}  gate={gate:<10}  Δ={dist}",
                    coin = a.coin,
                )));
            }
        }
        Err(zero_engine_client::HttpError::NotFound { .. }) => {
            // Older engine builds don't expose `/approaching`.
            // The bare "not found: /approaching" we used to print
            // looked like a CLI bug; say plainly that this engine
            // just doesn't serve the endpoint so the operator
            // knows it's not something they can fix.
            out.lines.push(OutputLine::alert(
                "approaching: this engine build does not expose /approaching (endpoint missing)",
            ));
        }
        Err(e) => out
            .lines
            .push(OutputLine::alert(format!("approaching: {e}"))),
    }
    out
}

async fn rejections_cmd(
    ctx: &DispatchContext,
    coin: Option<&str>,
    limit: Option<u32>,
) -> DispatchOutput {
    let mut out = DispatchOutput::default();
    let Some(http) = require_http(ctx, &mut out) else {
        return out;
    };
    let n = limit.unwrap_or_else(Command::default_rejections_limit);
    match http.rejections(n, coin).await {
        Ok(feed) => {
            if feed.items.is_empty() {
                let scope = coin.map_or_else(
                    || "(no rejections)".to_string(),
                    |c| format!("(no rejections for {c})"),
                );
                out.lines.push(OutputLine::system(scope));
                return out;
            }
            for r in &feed.items {
                let ts = trim_ts(r.ts.as_deref());
                let coin = r.coin.as_deref().unwrap_or("—");
                let dir = r.direction.as_deref().unwrap_or("—");
                let stage = r.stage.as_deref().unwrap_or("—");
                let reason = r.reason.as_deref().unwrap_or("(no reason)");
                out.lines.push(OutputLine::command(format!(
                    "{ts}  {coin:<6}  {dir:<5}  {stage:<8}  {reason}"
                )));
            }
        }
        Err(e) => out
            .lines
            .push(OutputLine::alert(format!("rejections: {e}"))),
    }
    out
}

/// Truncate an ISO-8601 timestamp down to `HH:MM:SS` for compact
/// line rendering. Falls back to an em-dash when the engine did
/// not supply a timestamp or emitted something we cannot slice
/// (e.g. a short relative marker) — an honest placeholder beats
/// a silent misread.
fn trim_ts(raw: Option<&str>) -> String {
    let Some(s) = raw else {
        return "—       ".into();
    };
    // Extract HH:MM:SS if the string looks like `YYYY-MM-DDTHH:MM:SS…`.
    if let Some(rest) = s.split_once('T').map(|(_, r)| r) {
        let hms: String = rest.chars().take(8).collect();
        if hms.len() == 8 {
            return hms;
        }
    }
    // Short strings (`now`, `5m ago`, unknown formats) flow
    // through as-is — trimming them would mislead.
    if s.len() <= 8 {
        return format!("{s:<8}");
    }
    s.to_string()
}

/// `/kill` — hard stop. Risk-reducer, friction-exempt (see the
/// 2 AM suite in `tests/two_am_scenarios.rs`). Posts to the live
/// executor when an engine client is attached and preserves the
/// compound local-supervisor tear-down behavior.
///
/// Compound contract: when [`DispatchContext::supervisor`] is
/// `Some` *and* the daemon is running, `/kill` tears down the
/// listener socket as part of the same call and tags the
/// confirmation line so the operator sees both effects in one
/// breadcrumb. When no engine client is attached, the command still
/// reports that the live kill was not posted instead of pretending
/// exchange state changed.
async fn kill_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let mut lines = match &ctx.http {
        Some(http) => match http.post_live_kill().await {
            Ok(reply) => render_live_control("/kill", "live kill", &reply),
            Err(e) => vec![OutputLine::alert(format!("/kill — engine refused: {e}"))],
        },
        None => vec![OutputLine::alert(
            "/kill — engine client unavailable; live kill not posted.",
        )],
    };
    let Some(sup) = ctx.supervisor.as_ref() else {
        return DispatchOutput {
            lines,
            ..Default::default()
        };
    };
    match sup.tear_down_socket() {
        // `tear_down_socket` returns `true` only when the daemon
        // was running *and* this call shut it down. `false`
        // means the daemon was already stopped — `/kill` does not
        // need to tag a non-event.
        Ok(true) => lines.push(OutputLine::alert(
            "/kill — headless supervisor stopped and ~/.zero/sock torn down.",
        )),
        Ok(false) => {}
        // A tear-down failure is an honesty bug if we silently
        // dropped it — the operator pressed `/kill` in part to
        // stop the daemon. Surface the error on its own line so
        // the primary `/kill` confirmation is not hidden behind a
        // multi-sentence alert.
        Err(e) => lines.push(OutputLine::alert(format!(
            "/kill — headless tear-down failed: {e}. Manual cleanup may be required."
        ))),
    }
    DispatchOutput {
        lines,
        ..Default::default()
    }
}

async fn flatten_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let Some(http) = &ctx.http else {
        return single_alert("/flatten-all — engine client unavailable; live flatten not posted.");
    };
    match http.post_live_flatten().await {
        Ok(reply) => DispatchOutput {
            lines: render_live_control("/flatten-all", "live flatten", &reply),
            ..Default::default()
        },
        Err(e) => single_alert(format!("/flatten-all — engine refused: {e}")),
    }
}

async fn pause_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let Some(http) = &ctx.http else {
        return single_alert("/pause-entries — engine client unavailable; live pause not posted.");
    };
    match http.post_live_pause().await {
        Ok(reply) => DispatchOutput {
            lines: render_live_control("/pause-entries", "live entries pause", &reply),
            ..Default::default()
        },
        Err(e) => single_alert(format!("/pause-entries — engine refused: {e}")),
    }
}

async fn resume_entries_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let Some(http) = &ctx.http else {
        return single_alert(
            "/resume-entries — engine client unavailable; live resume not posted.",
        );
    };
    match http.post_live_resume().await {
        Ok(reply) => DispatchOutput {
            lines: render_live_control("/resume-entries", "live entries resume", &reply),
            ..Default::default()
        },
        Err(e) => single_alert(format!("/resume-entries — engine refused: {e}")),
    }
}

fn render_live_control(
    command: &str,
    action: &str,
    reply: &LiveControlResponse,
) -> Vec<OutputLine> {
    let reason = reply.reason.as_deref().unwrap_or("no reason supplied");
    if !reply.ok {
        return vec![OutputLine::alert(format!("{command} — refused: {reason}"))];
    }
    let mut parts = vec![format!("{command} — {action} accepted")];
    if let Some(state) = reply.state.as_deref() {
        parts.push(format!("state={state}"));
    }
    if !reply.orders.is_empty() {
        parts.push(format!("orders={}", reply.orders.len()));
    }
    if let Some(operator) = &reply.operator_context
        && !operator.handle.is_empty()
    {
        parts.push(format!("operator={}", operator.handle));
    }
    vec![OutputLine::alert(parts.join(" "))]
}

fn execute_stub() -> DispatchOutput {
    // Reached only when `decide` returned `Proceed` — i.e.
    // operator state is FRESH / STEADY / RECOVERY. The friction
    // ladder has already had its say.
    DispatchOutput {
        lines: vec![OutputLine::command(
            "/execute — composition change accepted (stub). Wire-up to POST /execute lands with the live-trade pack.",
        )],
        ..Default::default()
    }
}

/// `/auto on|off|status|<unknown>|<missing>` — toggle the
/// engine's Auto-mode switch via [`AutoSource`].
///
/// Reached only after the friction ladder has had its say — the
/// `on` action is risk-increasing and therefore gated exactly
/// like `/execute`, while `off` / `status` are Neutral and arrive
/// here unconditionally (see `Command::risk`).
///
/// When no [`AutoSource`] is attached the dispatcher surfaces a
/// single "unavailable" alert — identical honesty policy to
/// `/config`.
fn auto_cmd(ctx: &DispatchContext, action: &AutoAction) -> DispatchOutput {
    let request = match action {
        AutoAction::On => AutoRequest::On,
        AutoAction::Off => AutoRequest::Off,
        AutoAction::Status => AutoRequest::Status,
        AutoAction::Missing => {
            return single_system(
                "/auto — usage: /auto on | off | status. `on` is risk-increasing and friction-gated.",
            );
        }
        AutoAction::Unknown(tok) => {
            return single_warn(format!(
                "/auto — unknown action '{tok}'. usage: /auto on | off | status."
            ));
        }
    };
    let Some(source) = ctx.auto.as_ref() else {
        return single_alert(
            "/auto — unavailable (no engine auto-mode adapter on this invocation).",
        );
    };
    match source.act(request) {
        Ok(reply) => {
            let mode = reply.mode.as_str();
            let line = match (action, reply.changed) {
                // Status is read-only; keep the copy neutral.
                (AutoAction::Status, _) => format!("/auto status — mode={mode}"),
                (AutoAction::On | AutoAction::Off, true) => {
                    format!("/auto — mode={mode} (changed)")
                }
                (AutoAction::On | AutoAction::Off, false) => {
                    // Idempotent flip — same mode as before. No
                    // "changed" tag; silent acceptance here would be
                    // wrong (operator asked for a change) but a
                    // warn line beats an alert since nothing is
                    // broken.
                    return single_warn(format!("/auto — mode already {mode}; no change."));
                }
                (AutoAction::Missing | AutoAction::Unknown(_), _) => {
                    unreachable!("/auto missing/unknown resolve before reaching the source adapter",)
                }
            };
            DispatchOutput {
                lines: vec![OutputLine::command(line)],
                ..Default::default()
            }
        }
        Err(e) => single_alert(format!("/auto — {e}")),
    }
}

/// `/headless start|stop|status|<unknown>|<missing>` — daemon
/// lifecycle surface.
///
/// Dispatches through [`SupervisorSource`]; when no adapter is
/// attached, surfaces a single "unavailable" alert. All three
/// verbs are Neutral (see `Command::risk`) — this handler is
/// reached without friction.
fn headless_cmd(ctx: &DispatchContext, action: &HeadlessAction) -> DispatchOutput {
    let request = match action {
        HeadlessAction::Start => SupervisorAction::Start,
        HeadlessAction::Stop => SupervisorAction::Stop,
        HeadlessAction::Status => SupervisorAction::Status,
        HeadlessAction::Missing => {
            return single_system(
                "/headless — usage: /headless start | stop | status. The daemon is the operator-local supervisor (ADR-006).",
            );
        }
        HeadlessAction::Unknown(tok) => {
            return single_warn(format!(
                "/headless — unknown action '{tok}'. usage: /headless start | stop | status."
            ));
        }
    };
    let Some(source) = ctx.supervisor.as_ref() else {
        return single_alert(
            "/headless — supervisor unavailable (no headless adapter on this invocation).",
        );
    };
    match source.act(request) {
        Ok(reply) => {
            let line = format_headless_reply(action, &reply);
            DispatchOutput {
                lines: vec![OutputLine::command(line)],
                ..Default::default()
            }
        }
        // A `Refused` reply is a warn, not an alert — the call
        // was understood, just not honored (e.g. stop while
        // already stopping). Everything else is an alert.
        Err(SupervisorError::Refused(msg)) => single_warn(format!("/headless — refused: {msg}")),
        Err(e) => single_alert(format!("/headless — {e}")),
    }
}

fn format_headless_reply(action: &HeadlessAction, reply: &SupervisorReply) -> String {
    use crate::supervisor::SupervisorState;
    let state = match &reply.state {
        SupervisorState::Running => "running",
        SupervisorState::Stopped => "stopped",
        SupervisorState::Failed(reason) => {
            return format!("/headless {} — failed: {reason}", headless_verb(action),);
        }
    };
    let changed = if reply.changed { " (changed)" } else { "" };
    let socket = reply
        .socket
        .as_deref()
        .map(|s| format!(" socket={s}"))
        .unwrap_or_default();
    let pid = reply.pid.map(|p| format!(" pid={p}")).unwrap_or_default();
    let uptime = reply
        .uptime
        .map(|d| format!(" uptime={}s", d.as_secs()))
        .unwrap_or_default();
    format!(
        "/headless {} — state={state}{changed}{socket}{pid}{uptime}",
        headless_verb(action),
    )
}

const fn headless_verb(action: &HeadlessAction) -> &'static str {
    match action {
        HeadlessAction::Start => "start",
        HeadlessAction::Stop => "stop",
        HeadlessAction::Status => "status",
        HeadlessAction::Missing | HeadlessAction::Unknown(_) => "(usage)",
    }
}

/// `/sessions [limit]` — paint a newest-first list of sessions.
///
/// We clamp the caller's limit into `[1, max_sessions_limit]` so a
/// stray `/sessions 100000` cannot push the prompt off-screen on a
/// terminal with thousands of historical rows. Persistence-disabled
/// and IO-failure paths emit single-line alerts rather than leaving
/// the operator guessing — the same policy the engine-fetch
/// commands follow.
fn sessions_cmd(ctx: &DispatchContext, limit: Option<u32>) -> DispatchOutput {
    let Some(sessions) = ctx.sessions.as_ref() else {
        return single_alert("/sessions — persistence disabled (no session store).");
    };
    let effective = limit
        .unwrap_or_else(Command::default_sessions_limit)
        .clamp(1, Command::max_sessions_limit());
    let rows = match sessions.list(effective) {
        Ok(rows) => rows,
        Err(e) => return single_alert(format!("/sessions — {e}")),
    };
    if rows.is_empty() {
        return DispatchOutput {
            lines: vec![OutputLine::system(
                "/sessions — no prior sessions on record.",
            )],
            ..Default::default()
        };
    }
    let current = sessions.current_ulid();
    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push(OutputLine::command(format!(
        "/sessions — {n} recent session(s)",
        n = rows.len()
    )));
    for row in rows {
        let marker = if Some(&row.ulid) == current.as_ref() {
            "*"
        } else {
            " "
        };
        let started = format_ms_short(row.started_at_ms);
        let state = if row.ended_at_ms.is_some() {
            "ended"
        } else {
            "live/interrupted"
        };
        let parent = row
            .parent_ulid
            .as_deref()
            .map(|p| format!(" parent:{p}"))
            .unwrap_or_default();
        let events = if row.n_events >= 0 {
            format!(" {n} evt", n = row.n_events)
        } else {
            String::new()
        };
        lines.push(OutputLine::system(format!(
            "{marker} {ulid} · {started} · {state}{events}{parent}",
            ulid = row.ulid,
        )));
    }
    DispatchOutput {
        lines,
        ..Default::default()
    }
}

/// `/resume <ulid|label>` — replay a prior session into the log
/// **silently** (without re-persisting). The split between
/// `lines` and `replay_lines` is what keeps that invariant
/// honest: the "resuming …" banner is a new recorded line; every
/// rehydrated row goes onto `replay_lines` and `AppState` appends
/// those without writing to the current session's events table.
///
/// Missing argument is surfaced as a usage hint rather than
/// silently doing nothing — matches the `/evaluate` convention so
/// picker + help paths stay uniform.
fn resume_cmd(ctx: &DispatchContext, needle: Option<&str>) -> DispatchOutput {
    fetch_and_paint_session(ctx, needle, SessionVerb::Resume)
}

/// `/replay <ulid|label>` — identical to `/resume` from the
/// dispatcher's perspective: look up the session, paint a banner,
/// emit `replay_lines`. The semantic difference — that `/replay`
/// does **not** switch the active session — lives in the caller
/// (the TUI): today the dispatcher never rotates the `sessions`
/// adapter's active ulid, so both commands are non-destructive
/// at this layer. The split is meaningful because the operator
/// model calls for distinct language ("resume this session" vs.
/// "replay this session"), and keeping the commands separate
/// now means we don't rename later if a `/resume` variant grows
/// a session-switch hook (it will, once we expose
/// `SessionSource::switch_to` for cross-session hopping).
fn replay_cmd(ctx: &DispatchContext, needle: Option<&str>) -> DispatchOutput {
    fetch_and_paint_session(ctx, needle, SessionVerb::Replay)
}

/// Which verb is driving this call. Keeps every user-visible
/// string keyed to the command name so `/resume` never leaks
/// "replay" wording and vice versa — operator-facing copy that
/// drifts from the command invoked is a small honesty debt that
/// compounds quickly.
#[derive(Debug, Clone, Copy)]
enum SessionVerb {
    Resume,
    Replay,
}

impl SessionVerb {
    const fn name(self) -> &'static str {
        match self {
            Self::Resume => "/resume",
            Self::Replay => "/replay",
        }
    }

    const fn banner_prefix(self) -> &'static str {
        match self {
            Self::Resume => "resuming",
            Self::Replay => "replaying",
        }
    }
}

fn fetch_and_paint_session(
    ctx: &DispatchContext,
    needle: Option<&str>,
    verb: SessionVerb,
) -> DispatchOutput {
    let name = verb.name();
    let Some(sessions) = ctx.sessions.as_ref() else {
        return single_alert(format!("{name} — persistence disabled (no session store)."));
    };
    let Some(needle) = needle else {
        return DispatchOutput {
            lines: vec![OutputLine::system(format!(
                "{name} <ulid|label> — try /sessions for a list of ids."
            ))],
            ..Default::default()
        };
    };
    let summary = match sessions.find(needle) {
        Ok(s) => s,
        Err(crate::session::SessionError::NotFound) => {
            return single_alert(format!(
                "{name} — no session matches '{needle}'. Try /sessions."
            ));
        }
        Err(e) => return single_alert(format!("{name} — {e}")),
    };
    // Cap at 200 to match the launch-time replay policy in
    // `main.rs::open_session_store`; a dramatically larger cap
    // would change the shape of the conversation pane on replay
    // and operators should see a consistent on-load experience
    // whether resumption is implicit (startup) or explicit here.
    let events = match sessions.list_events(&summary.ulid, 200) {
        Ok(e) => e,
        Err(e) => return single_alert(format!("{name} — {e}")),
    };
    let banner = format!(
        "{prefix} {ulid} · {started} · {n} event(s)",
        prefix = verb.banner_prefix(),
        ulid = summary.ulid,
        started = format_ms_short(summary.started_at_ms),
        n = events.len(),
    );
    let replay_lines: Vec<ReplayLine> = events
        .into_iter()
        .map(|e| ReplayLine {
            kind: e.kind,
            at_ms: e.at_ms,
            text: e.text,
        })
        .collect();
    DispatchOutput {
        lines: vec![OutputLine::command(banner)],
        replay_lines,
        ..Default::default()
    }
}

/// `/fork` — start a new session with `parent_ulid = current`.
///
/// The session store is the authority for the new ulid; the
/// dispatcher only echoes what it got back. If the store reports
/// no current session (persistence disabled, or an impossibly
/// early invocation), we surface that honestly — forking off
/// nothing is a no-op and operators deserve to know.
fn fork_cmd(ctx: &DispatchContext) -> DispatchOutput {
    let Some(sessions) = ctx.sessions.as_ref() else {
        return single_alert("/fork — persistence disabled (no session store).");
    };
    match sessions.fork_from_current() {
        Ok(Some(child)) => DispatchOutput {
            lines: vec![OutputLine::command(format!(
                "/fork — new session {child}; parent carries over."
            ))],
            ..Default::default()
        },
        Ok(None) => single_alert("/fork — no current session to fork from."),
        Err(e) => single_alert(format!("/fork — {e}")),
    }
}

/// `/save <label>` — attach a human label to the current session.
///
/// We resolve the current ulid up front (rather than letting the
/// store do it) so the saved line can echo "saved X → <ulid>",
/// giving the operator a confirming readout instead of a silent
/// success. A missing label prints a usage hint.
fn save_cmd(ctx: &DispatchContext, label: Option<&str>) -> DispatchOutput {
    let Some(sessions) = ctx.sessions.as_ref() else {
        return single_alert("/save — persistence disabled (no session store).");
    };
    let Some(label) = label else {
        return DispatchOutput {
            lines: vec![OutputLine::system(
                "/save <label> — pick a short name you'll recognise later.",
            )],
            ..Default::default()
        };
    };
    let Some(ulid) = sessions.current_ulid() else {
        return single_alert("/save — no active session to label.");
    };
    match sessions.save_label(&ulid, label) {
        Ok(()) => DispatchOutput {
            lines: vec![OutputLine::command(format!("/save — '{label}' → {ulid}"))],
            ..Default::default()
        },
        Err(e) => single_alert(format!("/save — {e}")),
    }
}

/// `/share [ulid|label]` — render a session snapshot as a JSON
/// block inside the conversation log.
///
/// This is the minimal viable share primitive: the snapshot lives
/// in the pane the operator is already looking at, so they can
/// select-and-copy without a clipboard-API dep or a filesystem
/// policy. Writing to a file (and its twin concerns — default
/// paths, overwrite rules, mode-640 vs. 644) is a follow-up.
///
/// When `needle` is omitted we share the current session. That
/// makes the common case ("capture what just happened") a single
/// keystroke. Missing session store or missing needle both
/// surface honest alerts — no empty JSON, no partial success.
fn share_cmd(ctx: &DispatchContext, needle: Option<&str>) -> DispatchOutput {
    let Some(sessions) = ctx.sessions.as_ref() else {
        return single_alert("/share — persistence disabled (no session store).");
    };
    // Resolve the target: explicit needle wins, else current.
    let target = needle
        .map(ToOwned::to_owned)
        .or_else(|| sessions.current_ulid());
    let Some(needle) = target else {
        return single_alert("/share — no active session and no ulid/label given. Try /sessions.");
    };
    let summary = match sessions.find(&needle) {
        Ok(s) => s,
        Err(crate::session::SessionError::NotFound) => {
            return single_alert(format!(
                "/share — no session matches '{needle}'. Try /sessions."
            ));
        }
        Err(e) => return single_alert(format!("/share — {e}")),
    };
    let events = match sessions.list_events(&summary.ulid, 1000) {
        Ok(e) => e,
        Err(e) => return single_alert(format!("/share — {e}")),
    };
    let n = events.len();
    let json = render_share_json(&summary, &events);
    // Two lines: a one-line header so the operator can see
    // at-a-glance what's being shared, and the fenced JSON
    // block. Keeping them separate means `OutputLine::Command`
    // formatting (fixed-width) applies to the header and the
    // body renders with embedded newlines the TUI preserves.
    DispatchOutput {
        lines: vec![
            OutputLine::command(format!(
                "/share — {ulid} · {n} event(s) · copy the block below",
                ulid = summary.ulid,
            )),
            OutputLine::system(json),
        ],
        ..Default::default()
    }
}

/// `/config` dispatcher. Pure routing over [`ConfigSource`];
/// the adapter (`zero/src/main.rs` in production) owns the
/// actual TOML + keychain reads, so the command crate can stay
/// filesystem-free and deterministic under tests.
///
/// Every outcome resolves to at least one line: missing
/// source, unknown action, missing action, and successful
/// readouts all emit a `DispatchOutput` with concrete lines so
/// the operator never sees a silent success.
fn config_cmd(ctx: &DispatchContext, action: &ConfigAction) -> DispatchOutput {
    match action {
        ConfigAction::Missing => single_warn(
            "/config <show|doctor> — show resolved values or diagnose config + secrets.",
        ),
        ConfigAction::Unknown(other) => single_warn(format!(
            "/config: unknown action '{other}'. Try /config show or /config doctor."
        )),
        ConfigAction::Show => {
            let Some(source) = ctx.config.as_ref() else {
                return single_alert("/config — config introspection unavailable.");
            };
            let rows = source.show();
            if rows.is_empty() {
                // Empty-state stays honest: the adapter said
                // "nothing to show" (fresh install, no config
                // on disk) so we mirror that rather than
                // pretending we printed something.
                return DispatchOutput {
                    lines: vec![OutputLine::system(
                        "/config show — no config loaded. Run `zero init`.",
                    )],
                    ..Default::default()
                };
            }
            let mut out = DispatchOutput::default();
            out.lines.push(OutputLine::command(format!(
                "/config show — {n} field(s)",
                n = rows.len()
            )));
            let label_width = rows.iter().map(|r| r.label.len()).max().unwrap_or(0);
            for row in rows {
                // Right-pad the label so columns line up —
                // cheaper than a table widget and survives
                // word-wrap in narrow terminals.
                out.lines.push(OutputLine::system(format!(
                    "  {label:<width$}  {value}",
                    label = row.label,
                    width = label_width,
                    value = row.value,
                )));
            }
            out
        }
        ConfigAction::Doctor => {
            let Some(source) = ctx.config.as_ref() else {
                return single_alert("/config — config introspection unavailable.");
            };
            let findings = source.doctor();
            if findings.is_empty() {
                // No findings at all is suspect — typically a
                // misconfigured adapter. Surface as System so
                // operators can tell the command ran but had
                // nothing to report.
                return DispatchOutput {
                    lines: vec![OutputLine::system(
                        "/config doctor — no findings (adapter returned empty list).",
                    )],
                    ..Default::default()
                };
            }
            let mut out = DispatchOutput::default();
            let n_err = findings
                .iter()
                .filter(|f| matches!(f.severity, DoctorSeverity::Error))
                .count();
            let n_warn = findings
                .iter()
                .filter(|f| matches!(f.severity, DoctorSeverity::Warn))
                .count();
            let header = format!(
                "/config doctor — {total} check(s)  errors={n_err}  warnings={n_warn}",
                total = findings.len(),
            );
            // Header promotes to Alert when any error is
            // present so operators glancing at the log see
            // failure immediately; Warn otherwise downgrades
            // to Command so a clean run is not visually
            // indistinguishable from an alerting one.
            if n_err > 0 {
                out.lines.push(OutputLine::alert(header));
            } else {
                out.lines.push(OutputLine::command(header));
            }
            for f in findings {
                let prefix = match f.severity {
                    DoctorSeverity::Ok => "  ok    ",
                    DoctorSeverity::Warn => "  warn  ",
                    DoctorSeverity::Error => "  ERROR ",
                };
                for emitted in wrap_doctor_row(prefix, &f.message, f.severity) {
                    out.lines.push(emitted);
                }
            }
            out
        }
    }
}

/// Width budget used when wrapping doctor findings. Chosen as
/// the narrow-end standard terminal (80 cols) minus the 8-col
/// prefix slot (`"  ok    "` / `"  warn  "` / `"  ERROR "`).
/// Rows that fit under this budget render as a single line,
/// identical to the pre-wrap behavior.
///
/// Rationale for the specific number: the original failure mode
/// (screenshotted 2026-04-22) was the ERROR row `engine token
/// unset — pass --token, set ZERO_API_TOKEN, or run \`zero
/// init --force\`` — about 82 characters with prefix — getting
/// clipped at the backtick by ratatui's single-row
/// `Line::render`. A body budget of 70 cols wraps that string
/// onto a second line and preserves the remediation hint in
/// full.
const DOCTOR_ROW_WRAP_BODY_COLS: usize = 70;

/// The fixed-width prefix column for doctor rows. All three
/// severities produce 8-character prefixes by construction:
/// `"  ok    "`, `"  warn  "`, `"  ERROR "`. Continuation lines
/// of a wrapped row indent by this many spaces so wrapped body
/// text aligns vertically with the first-line body. The
/// `debug_assert_eq!` in `wrap_doctor_row` pins the invariant
/// so a future prefix-copy change cannot silently break
/// alignment.
const DOCTOR_ROW_PREFIX_COLS: usize = 8;

/// Wrap a doctor finding's message into one or more output lines
/// so the rightmost characters don't get eaten by terminal
/// clipping. Continuation lines use the same [`OutputLine`]
/// kind as the first so a wrapped ERROR reads as one semantic
/// unit (all-alert-styled) rather than fragmenting into
/// mixed-color chunks.
///
/// Wrapping is word-based via `str::split_whitespace`; a single
/// token longer than the body budget (e.g. a URL) is emitted on
/// its own line with the continuation indent, un-broken —
/// breaking a URL mid-character would lose information that the
/// operator may need to paste into a browser.
fn wrap_doctor_row(prefix: &str, message: &str, severity: DoctorSeverity) -> Vec<OutputLine> {
    debug_assert_eq!(
        prefix.len(),
        DOCTOR_ROW_PREFIX_COLS,
        "doctor row prefix must be exactly {DOCTOR_ROW_PREFIX_COLS} cols for continuation alignment"
    );

    let make_line = |text: String| match severity {
        DoctorSeverity::Ok => OutputLine::system(text),
        DoctorSeverity::Warn => OutputLine::warn(text),
        DoctorSeverity::Error => OutputLine::alert(text),
    };
    let continuation_indent = " ".repeat(DOCTOR_ROW_PREFIX_COLS);

    // Fast path: the full row fits in the budget. Emit as-is so
    // the common case (short `ok` rows) doesn't pay for the
    // wrapping dance.
    if message.chars().count() <= DOCTOR_ROW_WRAP_BODY_COLS {
        return vec![make_line(format!("{prefix}{message}"))];
    }

    let mut lines = Vec::new();
    let mut current = String::with_capacity(DOCTOR_ROW_WRAP_BODY_COLS);
    let mut is_first = true;

    for word in message.split_whitespace() {
        let word_len = word.chars().count();
        let current_len = current.chars().count();
        let needs_space = !current.is_empty();
        let prospective = current_len + usize::from(needs_space) + word_len;

        if prospective > DOCTOR_ROW_WRAP_BODY_COLS && !current.is_empty() {
            let pfx = if is_first {
                prefix.to_owned()
            } else {
                continuation_indent.clone()
            };
            lines.push(make_line(format!("{pfx}{current}")));
            is_first = false;
            current.clear();
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    if !current.is_empty() {
        let pfx = if is_first {
            prefix.to_owned()
        } else {
            continuation_indent
        };
        lines.push(make_line(format!("{pfx}{current}")));
    }

    lines
}

/// `/verbose` dispatcher.
///
/// Resolves `Toggle` against `ctx.verbose` so the
/// `DispatchOutput.verbose_toggle` channel always carries an
/// absolute target — the TUI never has to re-implement toggle
/// semantics on top of whatever the operator typed.
///
/// A no-op transition (e.g. `/verbose on` when already on) is
/// deliberately kept: we still emit the confirmation line so
/// the operator sees that the command landed.
fn verbose_cmd(ctx: &DispatchContext, action: &VerboseAction) -> DispatchOutput {
    let target = match action {
        VerboseAction::On => true,
        VerboseAction::Off => false,
        VerboseAction::Toggle => !ctx.verbose,
        VerboseAction::Unknown(other) => {
            return single_warn(format!(
                "/verbose — unknown '{other}'. Use on|off|toggle (or no argument to toggle)."
            ));
        }
    };
    let word = if target { "on" } else { "off" };
    DispatchOutput {
        lines: vec![OutputLine::system(format!("verbose {word}"))],
        verbose_toggle: Some(target),
        ..Default::default()
    }
}

// ── Addendum A cohort ───────────────────────────────────────────
//
// Every handler below is a stub in the honest sense: the local
// acknowledgement / journaling is real, and the slot for the
// future engine-side wiring (`POST /operator/events`, positions
// model, disclosure store) is a single clearly-labelled line so
// a future contributor knows exactly where to solder in the real
// backend without changing the contract. We do NOT pretend to
// have done something when we have not — a silent success here
// would be the worst kind of failure for the operator.

/// `/state-override <label>` — operator self-declared state.
/// Risk-increasing; the friction ladder has already gated this
/// by the time the handler runs (see `dispatch::run`'s
/// decision branch). Emits a single `Command` line in the
/// conversation pane naming the declared label so the override
/// is visible in the audit trail. The POST to
/// `/operator/events` will land once the engine endpoint
/// ships (see ADR-016 + ADDENDUM_A §2.1 table row 1) — a
/// placeholder comment is kept below rather than a silent
/// todo because the operator must not infer the claim already
/// reached the engine.
fn state_override_cmd(label: Option<StateOverrideLabel>) -> DispatchOutput {
    let Some(label) = label else {
        // Missing / unrecognized label — honest usage hint.
        // Listing the full set inline avoids operators having
        // to bounce through `/help`.
        return single_warn(
            "/state-override <label> — one of FRESH | STEADY | ELEVATED | TILT | FATIGUED | RECOVERY",
        );
    };
    DispatchOutput {
        lines: vec![OutputLine::command(format!(
            "/state-override — label declared: {name}  (engine POST /operator/events pending)",
            name = label.as_str(),
        ))],
        ..Default::default()
    }
}

/// `/continue` — acknowledge the most recent coaching notice
/// and resume. Today's coaching buffer is not wired (no engine
/// coaching stream has landed) so the handler surfaces a
/// pending-infrastructure line rather than pretending to
/// dismiss something that was not there. Risk-neutral.
fn continue_cmd() -> DispatchOutput {
    DispatchOutput {
        lines: vec![OutputLine::system(
            "/continue — acknowledged  (coaching buffer pending; no notices queued right now)",
        )],
        ..Default::default()
    }
}

/// `/close [coin]` — per-coin position close. Risk-reducing;
/// the friction-asymmetry invariant keeps this friction-exempt
/// at every label. Until the positions-model + engine POST
/// path land the handler reports the would-be effect and
/// clearly tags it as pending, following the same pattern as
/// `/execute` and `/kill`. A bare `/close` without a coin
/// surfaces a usage hint so the operator is never left
/// wondering whether a "close everything" (which is
/// `/flatten-all`) just happened.
fn close_cmd(coin: Option<&str>) -> DispatchOutput {
    let Some(raw) = coin else {
        return DispatchOutput {
            lines: vec![OutputLine::warn(
                "/close <coin> — name the coin (try /pos to see open symbols; /flatten-all closes all)",
            )],
            ..Default::default()
        };
    };
    let coin = raw.trim();
    if coin.is_empty() {
        return DispatchOutput {
            lines: vec![OutputLine::warn(
                "/close <coin> — name the coin (try /pos to see open symbols; /flatten-all closes all)",
            )],
            ..Default::default()
        };
    }
    DispatchOutput {
        lines: vec![OutputLine::system(format!(
            "/close {coin} — noted  (positions model + engine POST pending; no order was placed)"
        ))],
        ..Default::default()
    }
}

/// `/wrap-off` — opt out of the daily-wrap generator for
/// *this session only*. The TUI honors the flag on session
/// exit; the dispatcher resolves an absolute `true` target so
/// downstream state assignment is trivial. A second
/// invocation is kept idempotent (still emits "already off"
/// to confirm the command landed) because silence would
/// make a follow-up `/wrap-off` look broken.
fn wrap_off_cmd() -> DispatchOutput {
    let body =
        "/wrap-off — daily wrap skipped for this session  (next session runs the wrap again)";
    DispatchOutput {
        lines: vec![OutputLine::system(body)],
        wrap_off_toggle: Some(true),
        ..Default::default()
    }
}

/// `/coaching reset` — clear the rolling coaching notice
/// buffer. Emits the `coaching_reset` signal; the TUI wires
/// it to `AppState::clear_coaching_buffer` once the buffer
/// ships. Today the buffer does not exist, so the signal is
/// a noop on the receiving side — we still emit it here so
/// the contract is stable and the future TUI change is a
/// one-line addition.
fn coaching_reset_cmd() -> DispatchOutput {
    DispatchOutput {
        lines: vec![OutputLine::system(
            "/coaching reset — buffer cleared  (coaching stream pending; nothing was queued)",
        )],
        coaching_reset: true,
        ..Default::default()
    }
}

/// `/disclosure-override --i-know-what-i-am-doing` — defeat
/// progressive disclosure. Risk-increasing; the friction
/// ladder has already gated this before the handler runs.
///
/// We additionally require the exact confirm phrase inside
/// the handler so an operator at STEADY (where the ladder
/// would Proceed) cannot bypass the guard by typing a bare
/// `/disclosure-override`. This is a hard, handler-level
/// guard separate from friction — the intent of the phrase
/// is operator acknowledgement, not rate-limiting.
fn disclosure_override_cmd(confirmed: bool) -> DispatchOutput {
    if !confirmed {
        let phrase = DISCLOSURE_OVERRIDE_CONFIRM;
        return DispatchOutput {
            lines: vec![OutputLine::alert(format!(
                "/disclosure-override — phrase required: `/disclosure-override {phrase}`",
            ))],
            ..Default::default()
        };
    }
    DispatchOutput {
        lines: vec![OutputLine::command(
            "/disclosure-override — progressive disclosure bypassed for this session  (disclosure store pending; no milestone was written)",
        )],
        ..Default::default()
    }
}

/// `/rate <trade_id> <1..=10>` — attach a conviction rating
/// to a past trade. M1_PLAN §7a line 119 + Addendum A §10.
///
/// Argument-shape guard comes first: a missing `trade_id`,
/// a missing `rating`, or a `rating` outside `1..=10`
/// surfaces a usage hint naming the full range — silently
/// accepting out-of-range values would launder a typo into a
/// recorded conviction. The parser already filters non-u8
/// tokens, so by the time we get here `rating == Some(n)`
/// means `n ∈ 1..=10`; the bound re-check is a defence-in-
/// depth against future parser refactors (ADR-017 honesty
/// bar: handler must not assume upstream invariants).
///
/// On the happy path we emit a single `Command` line
/// acknowledging the local record and clearly tagging the
/// engine POST as pending. The classifier-side wiring —
/// feeding an `EventKind::Conviction` into the local
/// operator-state event stream so replay reconstructs the
/// rating deterministically — attaches to the same sink the
/// engine writes land on when ADR-016's `POST /operator/events`
/// ships. Today that sink is not exposed through
/// `DispatchContext` (no write-side trait alongside
/// `StateSource`), so the rating lives in the conversation log
/// alone. Adding the sink is a one-trait addition on the same
/// ADR; the honest thing to do now is show the operator exactly
/// where the rating ended up.
async fn rate_cmd(
    ctx: &DispatchContext,
    trade_id: Option<&str>,
    rating: Option<u8>,
) -> DispatchOutput {
    use chrono::Utc;
    use zero_operator_state::{Event, EventKind};

    let trade_id = trade_id.map(str::trim).filter(|s| !s.is_empty());
    let Some(trade_id) = trade_id else {
        return DispatchOutput {
            lines: vec![OutputLine::warn(
                "/rate <trade_id> <1..=10> — name the trade and a conviction rating (1 low, 10 high)",
            )],
            ..Default::default()
        };
    };
    let Some(rating) = rating else {
        return DispatchOutput {
            lines: vec![OutputLine::warn(format!(
                "/rate {trade_id} <1..=10> — rating must be an integer in 1..=10 (1 low, 10 high)"
            ))],
            ..Default::default()
        };
    };
    // Defensive re-check: parser already filters to 1..=10,
    // but a future refactor that loosens the parser must not
    // silently let a 0 or 11 through.
    if !(1..=10).contains(&rating) {
        return DispatchOutput {
            lines: vec![OutputLine::warn(format!(
                "/rate {trade_id} {rating} — rating must be an integer in 1..=10 (1 low, 10 high)"
            ))],
            ..Default::default()
        };
    }

    // Engine wire format: `zero_operator_state::EventKind::Conviction`.
    // The classifier treats this as append-only and idempotent at the
    // event-log layer, so a retry on transport failure cannot
    // double-count. `ts` is wall-clock now — the engine's classifier
    // sorts events by `ts` during replay, so skewed clocks produce
    // an out-of-order event, not a corrupt snapshot.
    let event = Event::new(
        Utc::now(),
        EventKind::Conviction {
            trade_id: trade_id.to_string(),
            rating,
        },
    );

    let tail = post_operator_event_tail(ctx, &event).await;
    DispatchOutput {
        lines: vec![OutputLine::command(format!(
            "/rate {trade_id} {rating} — recorded{tail}"
        ))],
        ..Default::default()
    }
}

/// Post an operator-state event and render a one-phrase tail the
/// caller appends to its acknowledgement line. Factored out so
/// every rewirable stub (`/rate`, `/break`, future `/break-end`)
/// emits the exact same vocabulary for the three outcomes:
///
/// * `, posted` — engine accepted the event (2xx).
/// * `, engine unreachable (kept locally)` — transport failed; the
///   command's effect still stands in the conversation log, the
///   operator knows the engine did not see it.
/// * ` (engine client unavailable)` — no `HttpClient` in the
///   context at all (e.g. `--no-engine` mode, tests). Signalled
///   with an em-dash-neutral wording so the operator never infers
///   a partial-success.
///
/// Keeping this vocabulary tight (one phrase per state) is
/// deliberate: a future audit that greps for `posted` or
/// `unreachable` in conversation logs should find a single shape,
/// not five near-synonyms.
async fn post_operator_event_tail(
    ctx: &DispatchContext,
    event: &zero_operator_state::Event,
) -> String {
    let Some(http) = &ctx.http else {
        return "  (engine client unavailable; not posted)".to_string();
    };
    match http.post_operator_event(event).await {
        Ok(_) => ", posted to engine".to_string(),
        Err(e) => {
            // Log the underlying reason so `/doctor` or the logfile
            // retains the real failure, but keep the operator-
            // facing wording stable — they do not need the reqwest
            // error taxonomy at the conversation pane.
            tracing::debug!(error = %e, "operator-event POST failed");
            ", engine unreachable (kept locally)".to_string()
        }
    }
}

/// Tiny helper — a [`DispatchOutput`] that is only one Warn
/// line. Parallels [`single_alert`] for consistency.
fn single_warn(msg: impl Into<String>) -> DispatchOutput {
    DispatchOutput {
        lines: vec![OutputLine::warn(msg.into())],
        ..Default::default()
    }
}

fn render_share_json(
    summary: &crate::session::SessionSummary,
    events: &[crate::session::ReplayEvent],
) -> String {
    // A hand-rolled serialization would avoid `serde_json`, but
    // `zero-commands` already pulls it in transitively and the
    // structured value buys us forward compatibility: a future
    // `/share --to file.json` can reuse the same shape. Keys are
    // explicit (not derived via Serialize on `SessionSummary`)
    // so the `/share` contract is visible in one place and not
    // coupled to field renames in the session-source types.
    use serde_json::{Value, json};
    let events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "kind": replay_kind_str(e.kind),
                "at_ms": e.at_ms,
                "text": e.text,
            })
        })
        .collect();
    let body = json!({
        "ulid": summary.ulid,
        "started_at_ms": summary.started_at_ms,
        "ended_at_ms": summary.ended_at_ms,
        "engine_base_url": summary.engine_base_url,
        "cli_version": summary.cli_version,
        "parent_ulid": summary.parent_ulid,
        "n_events": summary.n_events,
        "events": events,
    });
    // Pretty-print: an operator staring at 3 KB of packed JSON
    // in a narrow terminal is not getting a share; they're
    // getting a wall. Indentation costs bytes — but share is an
    // explicit, operator-driven action, not ambient output.
    serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".into())
}

const fn replay_kind_str(k: ReplayKind) -> &'static str {
    match k {
        ReplayKind::Prompt => "prompt",
        ReplayKind::System => "system",
        ReplayKind::Command => "command",
        ReplayKind::Warn => "warn",
        ReplayKind::Alert => "alert",
    }
}

/// Tiny helper — a [`DispatchOutput`] that is only one alert line.
fn single_alert(msg: impl Into<String>) -> DispatchOutput {
    DispatchOutput {
        lines: vec![OutputLine::alert(msg.into())],
        ..Default::default()
    }
}

/// Tiny helper — a [`DispatchOutput`] that is only one System
/// line. Parallels [`single_alert`] / [`single_warn`] for the
/// usage-hint arms where the line is informational, not
/// alarming.
fn single_system(msg: impl Into<String>) -> DispatchOutput {
    DispatchOutput {
        lines: vec![OutputLine::system(msg.into())],
        ..Default::default()
    }
}

/// Epoch-ms → `YYYY-MM-DD HH:MM UTC`. Chosen for parity with the
/// `summarize` helper in `zero-tui::app::session` so the
/// interactive resume banner and the `/sessions` list use the
/// same clock wording. A non-parseable value falls back to the
/// epoch string rather than crashing the render.
fn format_ms_short(ms: i64) -> String {
    use chrono::{DateTime, TimeZone, Utc};
    let secs = ms.div_euclid(1000);
    let nanos = u32::try_from(ms.rem_euclid(1000) * 1_000_000).unwrap_or(0);
    let dt: DateTime<Utc> = Utc
        .timestamp_opt(secs, nanos)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default());
    dt.format("%Y-%m-%d %H:%M UTC").to_string()
}

async fn break_stub(ctx: &DispatchContext, minutes: Option<u32>) -> DispatchOutput {
    use chrono::Utc;
    use zero_operator_state::{Event, EventKind};

    // Engine wire format: `zero_operator_state::EventKind::BreakStarted`.
    // `planned_ms` is carried in milliseconds because that is the
    // classifier's native unit — the `/break` CLI parser accepts
    // minutes for operator ergonomics, and the conversion lives
    // here so the wire contract is narrow. `u64::from` on a `u32`
    // minute count cannot overflow the 64 bit product with 60_000
    // so no saturating-mul is needed.
    let planned_ms = minutes.map(|m| u64::from(m) * 60_000);
    let event = Event::new(Utc::now(), EventKind::BreakStarted { planned_ms });

    let tail = post_operator_event_tail(ctx, &event).await;
    let note = minutes.map_or_else(
        || format!("/break — noted{tail}"),
        |m| format!("/break {m}m — noted{tail}"),
    );
    DispatchOutput {
        lines: vec![OutputLine::system(note)],
        ..Default::default()
    }
}

/// Placeholder error type — the dispatcher's public signature
/// reserves a Result<…> slot for future commands that need to
/// refuse execution rather than emit a warn line.
#[derive(Debug, thiserror::Error)]
pub enum Never {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{DispatchContext, StaticLabel, dispatch};
    use crate::command::Command;
    use crate::friction::FrictionDecision;
    use crate::risk::RiskDirection;
    use zero_engine_client::EngineState;
    use zero_operator_state::friction::FrictionLevel;
    use zero_operator_state::label::Label;

    fn ctx_with_label(l: Label) -> DispatchContext {
        DispatchContext::new(None, EngineState::shared()).with_state(Arc::new(StaticLabel(l)))
    }

    #[tokio::test]
    async fn empty_input_returns_none() {
        let ctx = DispatchContext::new(None, EngineState::shared());
        let out = dispatch(&ctx, "").await.unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn help_renders_many_lines() {
        let ctx = DispatchContext::new(None, EngineState::shared());
        let out = dispatch(&ctx, "/help").await.unwrap().unwrap();
        assert!(out.lines.len() >= 6);
        assert!(!out.quit);
        assert!(out.mode_change.is_none());
    }

    // ---------- doctor-row wrap helper ----------
    //
    // These tests pin the fix for the 2026-04-22 paper cut
    // where the ERROR row `engine token unset — pass --token,
    // set ZERO_API_TOKEN, or run \`zero init --force\`` (84
    // cols) got clipped at the 80-col terminal edge, eating
    // the remediation hint. The operator saw `...or run \``
    // and had nothing to act on.

    #[test]
    fn short_doctor_row_emits_single_line_unchanged() {
        use super::{OutputLine, wrap_doctor_row};
        use crate::config::DoctorSeverity;

        let out = wrap_doctor_row("  ok    ", "keychain reachable", DoctorSeverity::Ok);
        assert_eq!(out.len(), 1);
        match &out[0] {
            OutputLine::System(s) => assert_eq!(s, "  ok    keychain reachable"),
            other => panic!("expected System, got {other:?}"),
        }
    }

    #[test]
    fn long_doctor_row_wraps_preserving_all_text() {
        use super::{DOCTOR_ROW_PREFIX_COLS, OutputLine, wrap_doctor_row};
        use crate::config::DoctorSeverity;

        // Literal string from main.rs::resolved_token_source,
        // the row that failed in the screenshot.
        let msg =
            "engine token unset — pass --token, set ZERO_API_TOKEN, or run `zero init --force`";
        let out = wrap_doctor_row("  ERROR ", msg, DoctorSeverity::Error);

        // Must emit ≥2 lines — the whole point of the fix.
        assert!(
            out.len() >= 2,
            "expected wrap to produce ≥2 lines, got {} ({out:?})",
            out.len(),
        );

        // Every emitted line must be Alert — the wrap preserves
        // severity so a wrapped ERROR does not visually
        // downgrade into a mixed-color paragraph.
        for line in &out {
            assert!(
                matches!(line, OutputLine::Alert(_)),
                "expected Alert for every line of a wrapped ERROR, got {line:?}",
            );
        }

        // No character of the original message may be lost. We
        // strip the prefix / indent from each emitted line and
        // concatenate the whitespace-separated remainder; it
        // must equal the original message when collapsed.
        let joined: String = out
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let OutputLine::Alert(s) = line else {
                    unreachable!()
                };
                let body = if i == 0 {
                    s.strip_prefix("  ERROR ").expect("first line keeps prefix")
                } else {
                    s.strip_prefix(&" ".repeat(DOCTOR_ROW_PREFIX_COLS))
                        .expect("continuation uses indent")
                };
                body.to_owned()
            })
            .collect::<Vec<_>>()
            .join(" ");
        let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(
            normalize(&joined),
            normalize(msg),
            "wrapped rows must preserve every word of the original message",
        );
    }

    #[test]
    fn doctor_row_continuation_aligns_under_message_column() {
        use super::{DOCTOR_ROW_PREFIX_COLS, OutputLine, wrap_doctor_row};
        use crate::config::DoctorSeverity;

        let msg = "config file missing at /Users/forge/Library/Application Support/zero/config.toml — run `zero init`";
        let out = wrap_doctor_row("  warn  ", msg, DoctorSeverity::Warn);
        assert!(out.len() >= 2);

        for (i, line) in out.iter().enumerate() {
            let OutputLine::Warn(s) = line else {
                panic!("expected Warn, got {line:?}");
            };
            if i == 0 {
                assert!(
                    s.starts_with("  warn  "),
                    "first line must start with severity prefix, got {s:?}",
                );
            } else {
                let expected_indent = " ".repeat(DOCTOR_ROW_PREFIX_COLS);
                assert!(
                    s.starts_with(&expected_indent),
                    "continuation line {i} must align under message column (10 spaces), got {s:?}",
                );
                // The char at column 10 must be non-space (body
                // starts immediately) — otherwise we've stacked
                // indents or split a whitespace run.
                let at_col_10: Option<char> = s.chars().nth(DOCTOR_ROW_PREFIX_COLS);
                assert!(
                    at_col_10.is_some_and(|c| !c.is_whitespace()),
                    "continuation line {i} body must start at col 10, got {s:?}",
                );
            }
        }
    }

    #[test]
    fn doctor_row_single_long_token_is_never_broken() {
        use super::{OutputLine, wrap_doctor_row};
        use crate::config::DoctorSeverity;

        // A URL-like token that alone exceeds the body budget.
        // Breaking mid-character would destroy paste-ability —
        // the whole point of the URL is the operator copies it
        // into a browser. Emit it on its own line, un-split.
        let url = "https://docs.getzero.dev/runbook/reconnecting-forever-after-rotating-your-token-thoroughly";
        let out = wrap_doctor_row("  ERROR ", url, DoctorSeverity::Error);

        // Concatenate every body and confirm the URL is intact.
        let joined: String = out
            .iter()
            .map(|line| {
                let OutputLine::Alert(s) = line else {
                    unreachable!()
                };
                s.trim_start().to_owned()
            })
            .collect::<String>();
        // First-line prefix "ERROR " may or may not be present
        // depending on where wrap fell; strip it robustly.
        let joined = joined.trim_start_matches("ERROR ").to_owned();
        assert!(
            joined.contains(url),
            "URL token must survive un-broken across wrap boundaries; joined={joined:?}",
        );
    }

    #[tokio::test]
    async fn quit_sets_quit_flag() {
        let ctx = DispatchContext::new(None, EngineState::shared());
        let out = dispatch(&ctx, "/quit").await.unwrap().unwrap();
        assert!(out.quit);
    }

    #[tokio::test]
    async fn state_sets_overlay_signal() {
        use crate::command::OverlayTarget;
        let ctx = DispatchContext::new(None, EngineState::shared());
        let out = dispatch(&ctx, "/state").await.unwrap().unwrap();
        assert_eq!(out.show_overlay, Some(OverlayTarget::State));
        assert!(!out.quit);
        assert!(out.lines.is_empty(), "overlay command emits no lines");
        assert_eq!(out.risk, Some(RiskDirection::Neutral));
    }

    #[tokio::test]
    async fn state_under_tilt_still_opens_overlay() {
        // /state is Neutral — must never be gated.
        use crate::command::OverlayTarget;
        let ctx = ctx_with_label(Label::Tilt);
        let out = dispatch(&ctx, "/state").await.unwrap().unwrap();
        assert_eq!(out.show_overlay, Some(OverlayTarget::State));
        assert_eq!(out.friction, Some(FrictionDecision::Proceed));
    }

    #[tokio::test]
    async fn clear_sets_clear_flag() {
        let ctx = DispatchContext::new(None, EngineState::shared());
        let out = dispatch(&ctx, "/clear").await.unwrap().unwrap();
        assert!(out.clear_log);
    }

    #[tokio::test]
    async fn unknown_emits_warn() {
        let ctx = DispatchContext::new(None, EngineState::shared());
        let out = dispatch(&ctx, "/nope").await.unwrap().unwrap();
        assert_eq!(out.lines.len(), 1);
        matches!(out.lines[0], super::OutputLine::Warn(_));
    }

    #[tokio::test]
    async fn status_without_http_emits_alert() {
        let ctx = DispatchContext::new(None, EngineState::shared());
        let out = dispatch(&ctx, "/status").await.unwrap().unwrap();
        assert!(
            matches!(&out.lines[0], super::OutputLine::Alert(s) if s.contains("engine client"))
        );
    }

    // -------------------------------------------------------------
    // Friction ladder — dispatch-level enforcement
    // -------------------------------------------------------------

    #[tokio::test]
    async fn execute_under_steady_proceeds() {
        let ctx = ctx_with_label(Label::Steady);
        let out = dispatch(&ctx, "/execute").await.unwrap().unwrap();
        assert_eq!(out.risk, Some(RiskDirection::Increases));
        assert_eq!(out.friction, Some(FrictionDecision::Proceed));
        // /execute stub emits a confirmation line (Command line)
        assert!(matches!(
            out.lines.first(),
            Some(super::OutputLine::Command(_))
        ));
    }

    #[tokio::test]
    async fn execute_under_elevated_pauses_without_running() {
        let ctx = ctx_with_label(Label::Elevated);
        let out = dispatch(&ctx, "/execute").await.unwrap().unwrap();
        assert_eq!(out.risk, Some(RiskDirection::Increases));
        assert!(matches!(
            out.friction,
            Some(FrictionDecision::Pause {
                level: FrictionLevel::L1,
                ..
            })
        ));
        // advisory line must indicate friction + NOT the "accepted" stub
        let joined = join_lines(&out);
        assert!(joined.contains("friction"), "{joined:?}");
        assert!(!joined.contains("accepted"), "{joined:?}");
        // pending_command carries the resolved Command so the TUI
        // can re-dispatch via run_bypass_friction after the pause.
        assert_eq!(out.pending_command, Some(Command::Execute));
    }

    #[tokio::test]
    async fn execute_under_tilt_requires_typed_confirm() {
        let ctx = ctx_with_label(Label::Tilt);
        let out = dispatch(&ctx, "/execute").await.unwrap().unwrap();
        assert!(matches!(
            out.friction,
            Some(FrictionDecision::TypedConfirm {
                level: FrictionLevel::L2,
                ..
            })
        ));
        assert_eq!(
            out.friction
                .as_ref()
                .and_then(FrictionDecision::confirm_word)
                .as_deref(),
            Some("execute")
        );
        let joined = join_lines(&out);
        assert!(joined.contains("type 'execute'"), "{joined:?}");
        assert!(!joined.contains("accepted"), "{joined:?}");
        assert_eq!(out.pending_command, Some(Command::Execute));
    }

    #[tokio::test]
    async fn proceed_path_leaves_pending_command_empty() {
        // When a command actually ran, there is no post-friction
        // work to do. `pending_command` must stay `None` so the
        // TUI does not open an overlay for commands that never
        // needed gating.
        let ctx = ctx_with_label(Label::Steady);
        let out = dispatch(&ctx, "/execute").await.unwrap().unwrap();
        assert_eq!(out.friction, Some(FrictionDecision::Proceed));
        assert!(
            out.pending_command.is_none(),
            "Proceed path must not carry pending_command"
        );
    }

    #[tokio::test]
    async fn bypass_friction_runs_command_ignoring_label() {
        // Post-friction entrypoint: the TUI has honored the
        // pause + any typed confirmation; the command runs
        // straight through `run`. Even under TILT this must
        // proceed — the caller earned it by waiting.
        let ctx = ctx_with_label(Label::Tilt);
        let out = super::run_bypass_friction(&ctx, Command::Execute).await;
        assert_eq!(out.friction, Some(FrictionDecision::Proceed));
        assert_eq!(out.risk, Some(RiskDirection::Increases));
        let joined = join_lines(&out);
        assert!(
            joined.contains("accepted"),
            "expected execute stub: {joined}"
        );
    }

    #[tokio::test]
    async fn bypass_friction_on_neutral_command_is_harmless() {
        // /help is Neutral — bypassing friction on it is a no-op
        // as far as gating goes. Asserts the invariant that the
        // bypass path does not lower the risk-asymmetry guarantee:
        // a Neutral command runs the same in both paths.
        let ctx = DispatchContext::new(None, EngineState::shared());
        let out = super::run_bypass_friction(&ctx, Command::Help).await;
        assert_eq!(out.risk, Some(RiskDirection::Neutral));
        assert_eq!(out.friction, Some(FrictionDecision::Proceed));
    }

    /// The architectural tripwire: a Reduces command at TILT still
    /// runs, with a Proceed decision. If this test ever flips,
    /// someone has broken the risk-asymmetry invariant that makes
    /// the whole thing worth building.
    #[tokio::test]
    async fn kill_under_tilt_still_proceeds() {
        let ctx = ctx_with_label(Label::Tilt);
        let out = dispatch(&ctx, "/kill").await.unwrap().unwrap();
        assert_eq!(out.risk, Some(RiskDirection::Reduces));
        assert_eq!(
            out.friction,
            Some(FrictionDecision::Proceed),
            "Reduces commands MUST never be gated"
        );
    }

    #[tokio::test]
    async fn flatten_under_tilt_still_proceeds() {
        let ctx = ctx_with_label(Label::Tilt);
        let out = dispatch(&ctx, "/flatten-all").await.unwrap().unwrap();
        assert_eq!(out.friction, Some(FrictionDecision::Proceed));
    }

    #[tokio::test]
    async fn status_under_tilt_still_proceeds() {
        let ctx = ctx_with_label(Label::Tilt);
        let out = dispatch(&ctx, "/status").await.unwrap().unwrap();
        assert_eq!(out.risk, Some(RiskDirection::Neutral));
        assert_eq!(out.friction, Some(FrictionDecision::Proceed));
    }

    fn join_lines(out: &super::DispatchOutput) -> String {
        out.lines
            .iter()
            .map(|l| match l {
                super::OutputLine::System(s)
                | super::OutputLine::Command(s)
                | super::OutputLine::Warn(s)
                | super::OutputLine::Alert(s) => s.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
