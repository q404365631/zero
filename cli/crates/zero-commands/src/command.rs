//! The `Command` enum — exhaustive list of what the TUI, command
//! palette, and non-interactive entrypoint can dispatch.
//!
//! Each variant carries a const [`RiskDirection`] and resolves to
//! a single handler inside `dispatch`. Adding a command is three
//! steps: add a variant, add a `CatalogEntry` in [`CATALOG`], add
//! a match arm in `dispatch::run`.

use crate::parse::ParsedLine;
use crate::risk::RiskDirection;

/// A mode switch the dispatcher can emit. Mirrors `zero-tui::Mode`
/// but lives here so `zero-commands` does not have to depend on
/// the TUI crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeTarget {
    Conversation,
    Positions,
    Decisions,
    Heat,
}

impl ModeTarget {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Positions => "positions",
            Self::Decisions => "decisions",
            Self::Heat => "heat",
        }
    }
}

/// A modal overlay the TUI should paint on top of the current
/// mode. Same decoupling rationale as [`ModeTarget`]: dispatch
/// side-effects stay addressable without the command crate
/// depending on any widget types.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayTarget {
    /// Full-screen-ish state overview, sourced from the engine's
    /// operator-state mirror (ADR-016). See Addendum A §2.3 for
    /// the semantic shape.
    State,
    /// Gate-level verdict for a single coin, fetched from
    /// `GET /evaluate/{coin}`. The payload is the engine's
    /// [`zero_engine_client::Evaluation`] so the overlay renders
    /// exactly what the engine said, no local interpretation.
    Verdict(Box<zero_engine_client::Evaluation>),
}

impl OverlayTarget {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Verdict(_) => "verdict",
        }
    }
}

// `Evaluation` contains `f64` fields so it is `PartialEq` but not
// `Eq`. We assert `Eq` manually at the enum level because every
// variant is equality-decidable under `PartialEq` in practice and
// `DispatchOutput` (which embeds `Option<OverlayTarget>`) derives
// `Eq`. Accepting `f64::NaN` into a `Verdict` would produce a
// reflexive-inequality — the engine does not emit NaN confidence,
// and any regression will surface in dispatch tests.
impl Eq for OverlayTarget {}

/// An operator command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Quit,
    Clear,
    SwitchMode(ModeTarget),
    Status,
    Brief,
    Risk,
    /// `/hl-status [coin]` — read-only Hyperliquid public info status.
    /// This cannot sign payloads or place orders, so it is Neutral.
    HyperliquidStatus {
        symbol: Option<String>,
    },
    Regime {
        coin: Option<String>,
    },
    /// `/evaluate <coin>` — fetch the engine's gate-level verdict
    /// for `coin` and surface it as a verdict overlay. `coin` is
    /// required; a missing argument is resolved to the command
    /// and the dispatcher emits a usage hint so the picker and
    /// `/help` paths stay consistent.
    ///
    /// `extras` preserves any trailing tokens the operator typed
    /// after the coin (e.g. `/evaluate sol short`). The engine
    /// endpoint does not take a direction — `direction` is a
    /// property of the verdict, not an input — so extras are
    /// only surfaced as a warning during dispatch. We keep them
    /// here (rather than discarding at parse time) so the
    /// warning can echo the exact tokens the operator typed,
    /// which is more informative than a generic "extra args".
    Evaluate {
        coin: Option<String>,
        extras: Vec<String>,
    },
    Positions,
    /// `/pulse [limit]` — stream the engine's recent-events tail
    /// (signals, rejections, state transitions). `limit` is clamped
    /// client-side to `1..=100` by [`HttpClient::pulse`]. A missing
    /// limit falls back to [`Self::default_pulse_limit`].
    Pulse {
        limit: Option<u32>,
    },
    /// `/approaching` — coins within distance-to-gate of an entry
    /// or exit trigger. No args; results come sorted by ascending
    /// distance.
    Approaching,
    /// `/rejections [coin] [limit]` — recent gate rejections. Either
    /// argument can be omitted; numeric tokens resolve to `limit`,
    /// non-numeric tokens to `coin`. `limit` is clamped to `1..=500`
    /// by the HTTP layer.
    Rejections {
        coin: Option<String>,
        limit: Option<u32>,
    },
    Kill,
    FlattenAll,
    PauseEntries,
    Break {
        minutes: Option<u32>,
    },
    /// Stub risk-*increasing* command. Lets the friction machinery
    /// demonstrate end-to-end even before composition changes have
    /// a real POST endpoint. Carries `RiskDirection::Increases` so
    /// the gate evaluates. Real execution wiring (`/execute` v2)
    /// lands with the live-trade pack.
    Execute,
    /// Open the full-screen operator-state overview overlay
    /// (Addendum A §2.3). Read-only — opens a modal sourced from
    /// the engine mirror; the dispatcher itself emits no lines.
    State,
    /// `/sessions [limit]` — list recent sessions. Default limit is
    /// [`Self::default_sessions_limit`]; the impl clamps higher
    /// values to that ceiling so a stray `/sessions 100000` does
    /// not blow the conversation pane.
    Sessions {
        limit: Option<u32>,
    },
    /// `/resume <ulid|label>` — append the prior session's events
    /// into the current log, silently (without re-persisting).
    /// The argument can be a ulid (full or 6+ char prefix) or a
    /// human label set via [`Self::Save`]. Missing argument is
    /// resolved to the command and the dispatcher emits a usage
    /// hint, keeping the picker path consistent with `/evaluate`.
    Resume {
        needle: Option<String>,
    },
    /// `/fork` — start a new session whose `parent_ulid` is the
    /// current one. Takes no arguments; the ulid is generated by
    /// the session store. Rendered as a single "forked → <ulid>"
    /// confirmation line.
    Fork,
    /// `/heat` — composite heat readout combining the
    /// risk-summary percentages (drawdown / daily-loss /
    /// exposure) with kill-switch + circuit-breaker state into a
    /// single actionable "how hot am I?" line. Distinct from
    /// `/risk` (which is a terse risk-only readout) because heat
    /// folds in guardrail-proximity and circuit state so an
    /// operator scanning for "am I close to a limit?" can get
    /// that answer in one token. No args — heat is always the
    /// current state.
    Heat,
    /// `/save <label>` — attach a human-friendly alias to the
    /// current session. Labels are resolved later by `/resume
    /// <label>` so operators can name a session "pre-cpi" or
    /// "scratch" without memorizing ulids. Overwriting a label
    /// is allowed and intentional.
    Save {
        label: Option<String>,
    },
    /// `/replay <ulid|label>` — paint a prior session into the
    /// conversation log **without** switching to it. Identical to
    /// `/resume` in every respect except that the `SessionSource`
    /// adapter is not asked to rotate the active write target.
    /// This is the path operators use when they want to study a
    /// past session while continuing to record into the current
    /// one; `/resume` implies "I am picking this up again",
    /// `/replay` implies "I am looking at it".
    Replay {
        needle: Option<String>,
    },
    /// `/share [ulid|label]` — render a shareable text snapshot of
    /// a session (metadata + events) as a single command block in
    /// the conversation pane. Argument is optional; when omitted
    /// the current session is shared. The dispatcher emits JSON
    /// wrapped in a fenced block so the operator can select-and-
    /// copy without format drift. File / clipboard export is
    /// deferred — a snapshot in the log is the minimal viable
    /// share primitive and avoids host-I/O policy decisions.
    Share {
        needle: Option<String>,
    },
    /// `/config <action>` — read-only introspection of the
    /// operator's on-disk config + secret-resolution state.
    /// Intentionally read-only at this layer: write paths
    /// (`zero init`, `zero pair`) already have dedicated
    /// entrypoints and we do not want the TUI to silently
    /// rewrite `config.toml` from a slash command. Missing /
    /// unknown action resolves to a usage hint rather than a
    /// silent no-op.
    Config {
        action: ConfigAction,
    },
    /// `/verbose [on|off|toggle]` — toggle the TUI's verbose
    /// rendering mode. Today that means "include date +
    /// seconds in log timestamps" instead of the default HH:MM:SS.
    /// Future verbose-gated surfaces (full event payload dumps,
    /// richer friction reasoning) will key off the same flag.
    ///
    /// Argument grammar:
    /// - bare `/verbose` → [`VerboseAction::Toggle`]
    /// - `/verbose on`  → [`VerboseAction::On`]
    /// - `/verbose off` → [`VerboseAction::Off`]
    /// - anything else  → [`VerboseAction::Unknown`] so the
    ///   dispatcher can surface a usage hint. Silent acceptance
    ///   of an unknown argument would make the command seem
    ///   inert.
    Verbose {
        action: VerboseAction,
    },
    /// `/state-override <label>` — operator-declared override of
    /// the engine-computed behavioural label. Risk-*increasing*
    /// (see [`RiskDirection::Increases`]) because a healthier-
    /// than-observed claim unlocks lower friction; the ladder
    /// *must* still gate it so an operator declaring STEADY
    /// while the engine sees TILT pays the full L2 typed-confirm
    /// cost. Passing `None` is resolved to the command so the
    /// dispatcher can emit a usage hint with the valid labels.
    StateOverride {
        label: Option<StateOverrideLabel>,
    },
    /// `/continue` — acknowledge the most-recent coaching
    /// notice and resume. No-op when no coaching is queued.
    /// Neutral risk (pure acknowledgement).
    Continue,
    /// `/close [coin]` — close a single position. Per-coin
    /// sibling to `/flatten-all`; risk-*reducing*, friction-
    /// exempt at every state. `coin` is optional — a bare
    /// `/close` resolves to the most recently actioned symbol
    /// (when the positions model ships). For now the handler
    /// surfaces a "pending positions model" line rather than
    /// pretending to close anything — silence here would be
    /// the worst possible failure mode for a risk-reducer.
    Close {
        coin: Option<String>,
    },
    /// `/wrap-off` — skip the daily wrap for *this session only*.
    /// The next session runs the wrap again (per ADDENDUM_A §9.1,
    /// the opt-out cannot be sticky). Neutral risk.
    WrapOff,
    /// `/coaching reset` — clear the rolling coaching notice
    /// buffer. Neutral risk. Kept distinct from `/clear` (which
    /// empties the whole conversation log) because operators
    /// sometimes want to quiet coaching without losing the
    /// decision trail.
    CoachingReset,
    /// `/disclosure-override --i-know-what-i-am-doing` — jump
    /// ahead in progressive disclosure. Risk-*increasing*: the
    /// operator is defeating a guardrail designed to throttle
    /// feature exposure to earned competence. The `confirmed`
    /// flag carries whether the literal phrase was typed. A
    /// bare `/disclosure-override` or a typo in the phrase
    /// resolves to `confirmed = false` so the dispatcher can
    /// emit a usage hint naming the exact words required —
    /// silent rejection would make the command seem broken.
    DisclosureOverride {
        confirmed: bool,
    },
    /// `/rate <trade_id> <1..=10>` — attach a conviction rating
    /// to a past trade, feeding the operator-state classifier
    /// and the eventual calibration overlay (Addendum A §10,
    /// M1_PLAN §7a line 119). Neutral risk: a rating is a
    /// self-report about a closed trade, not a position-change.
    ///
    /// Parse semantics: the rating is an integer in `1..=10`
    /// (spec wording; the classifier's event field is `u8`).
    /// Values outside the range, non-numeric tokens, or a
    /// missing argument resolve to `rating = None` so the
    /// dispatcher can emit a usage hint citing the full range
    /// — silently clamping to 1 or 10 would launder a typo
    /// into a recorded conviction. `trade_id` is passed
    /// through verbatim (it's the engine's opaque identifier);
    /// an empty one resolves to `None` and the same usage path.
    ///
    /// The handler is an **honest stub** for the engine POST
    /// half: the rating is journaled locally via the operator-
    /// state sink (so the classifier observes it deterministically
    /// on replay), and the pane line says "recorded locally;
    /// engine POST pending" so the operator never infers a
    /// silent server-side success. The engine-side POST lands
    /// with the rest of the ADR-016 operator-state writes.
    Rate {
        trade_id: Option<String>,
        rating: Option<u8>,
    },
    /// Operator typed a shell-style invocation like `zero doctor`
    /// inside the TUI prompt. Carried separately from
    /// [`Command::Unknown`] so the dispatcher can emit a targeted
    /// "you're already inside zero — did you mean /<rest>?" hint
    /// instead of the generic "unknown command" warning.
    ///
    /// `rest` is the whitespace-joined args exactly as typed, so
    /// the hint can reproduce the operator's intent verbatim
    /// (`zero --version` → hint mentions `/version`,
    /// `zero` alone → hint mentions no-command). The hint is
    /// produced at dispatch time, not at parse time, so the
    /// exact wording stays next to the other user-facing copy.
    ZeroPrefix {
        rest: String,
    },
    /// `/auto on | off | status` — toggle Auto mode, which
    /// instructs the engine to take Plan-mode verdicts **without**
    /// operator confirmation. Risk-*increasing*: flipping the
    /// engine from a gated Plan posture to an auto-accept posture
    /// unlocks engine-initiated position changes, the same kind
    /// of exposure surface `/execute` opens. The friction ladder
    /// gates `/auto on` exactly like `/execute` — a TILT operator
    /// unlocking auto-acceptance at 2 AM is the canonical tired-
    /// operator footgun.
    ///
    /// `/auto off` and `/auto status` are **Neutral** — turning
    /// the accelerator off is a risk-*reducer*-shaped action and
    /// status is read-only. Risk direction therefore depends on
    /// the action; [`Self::risk`] resolves it by inspecting the
    /// [`AutoAction`] carried here. A bare `/auto` resolves to
    /// [`AutoAction::Missing`] so the dispatcher can emit a usage
    /// hint rather than a silent toggle — guessing the operator's
    /// intent at this much exposure would be an honesty failure.
    Auto {
        action: AutoAction,
    },
    /// `/headless start | stop | status` — spawn / stop / query
    /// the operator-local supervisor daemon (ADR-006). The
    /// command itself is **Neutral**: starting the daemon does
    /// not take new positions (the daemon is a watchdog + kill-
    /// switch surface), stopping it removes the supervisor but
    /// does not touch live exposure, and status is read-only.
    ///
    /// The CLI does not implement the supervisor; dispatch
    /// routes each action through the [`crate::SupervisorSource`]
    /// trait on [`DispatchContext`]. When no adapter is
    /// attached (tests + `--no-persist` paths), the dispatcher
    /// emits a single "headless supervisor unavailable" alert
    /// rather than pretending. Unknown / missing actions route
    /// to a usage hint — same honesty contract as `/config`.
    Headless {
        action: HeadlessAction,
    },
    Unknown(String),
}

/// Labels the operator may self-declare via `/state-override`.
///
/// Mirrors `zero_operator_state::Label` shape-wise but lives
/// here so the command crate does not take a dep on operator-
/// state types just to parse an argument. The adapter in the
/// TUI / engine routes each variant to the real classifier
/// label without an additional mapping layer — the names are
/// one-to-one by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateOverrideLabel {
    Fresh,
    Steady,
    Elevated,
    Tilt,
    Fatigued,
    Recovery,
}

impl StateOverrideLabel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "FRESH",
            Self::Steady => "STEADY",
            Self::Elevated => "ELEVATED",
            Self::Tilt => "TILT",
            Self::Fatigued => "FATIGUED",
            Self::Recovery => "RECOVERY",
        }
    }

    /// Parse a caller-supplied token. Case-insensitive. Returns
    /// `None` on an unrecognized label so the parser can route
    /// to the usage-hint arm.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "FRESH" => Some(Self::Fresh),
            "STEADY" => Some(Self::Steady),
            "ELEVATED" => Some(Self::Elevated),
            "TILT" => Some(Self::Tilt),
            "FATIGUED" => Some(Self::Fatigued),
            "RECOVERY" => Some(Self::Recovery),
            _ => None,
        }
    }
}

/// The exact phrase `/disclosure-override` requires. Declared
/// here so tests, the parser, and the help text all reference
/// the same string — drift between what the help says and what
/// the parser accepts would be an honesty bug.
pub const DISCLOSURE_OVERRIDE_CONFIRM: &str = "--i-know-what-i-am-doing";

/// The `/verbose` argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerboseAction {
    On,
    Off,
    Toggle,
    Unknown(String),
}

/// The `/config` subcommand.
///
/// [`ConfigAction::Missing`] models the no-arg invocation (so
/// the dispatcher can emit a usage hint without string-parsing
/// the variant back). [`ConfigAction::Unknown`] preserves the
/// typed token so the usage line can say exactly what was
/// rejected — silent acceptance of an unknown action would
/// leave operators wondering whether the command ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigAction {
    Show,
    Doctor,
    Missing,
    Unknown(String),
}

/// The `/auto` subcommand.
///
/// Only [`AutoAction::On`] is risk-*increasing*; the others are
/// Neutral. [`AutoAction::Missing`] models a bare `/auto` so the
/// dispatcher can emit a usage hint instead of silently toggling.
/// [`AutoAction::Unknown`] preserves the typed token so the hint
/// can quote it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoAction {
    On,
    Off,
    Status,
    Missing,
    Unknown(String),
}

impl AutoAction {
    /// `true` when the action flips auto-acceptance from off to on.
    /// The dispatcher's `risk()` function keys off this so the
    /// friction ladder gates `/auto on` but not `/auto off|status`.
    #[must_use]
    pub const fn is_risk_increasing(&self) -> bool {
        matches!(self, Self::On)
    }
}

/// The `/headless` subcommand.
///
/// All variants are Neutral (see [`Command::Headless`] docs). The
/// `Missing` / `Unknown` variants let the dispatcher emit usage
/// hints with the exact token the operator typed — identical
/// honesty contract to [`ConfigAction`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadlessAction {
    Start,
    Stop,
    Status,
    Missing,
    Unknown(String),
}

impl Command {
    /// Compile-time-style risk classification. `const` so callers
    /// can `match` in const contexts (e.g. CI lints).
    #[must_use]
    pub const fn risk(&self) -> RiskDirection {
        match self {
            // Navigation / info — no exposure change.
            Self::Help
            | Self::Clear
            | Self::SwitchMode(_)
            | Self::Status
            | Self::Brief
            | Self::Risk
            | Self::HyperliquidStatus { .. }
            | Self::Regime { .. }
            | Self::Evaluate { .. }
            | Self::Positions
            | Self::Pulse { .. }
            | Self::Approaching
            | Self::Rejections { .. }
            | Self::State
            | Self::Heat
            | Self::Sessions { .. }
            | Self::Resume { .. }
            | Self::Fork
            | Self::Save { .. }
            | Self::Replay { .. }
            | Self::Share { .. }
            | Self::Config { .. }
            | Self::Verbose { .. }
            | Self::Continue
            | Self::WrapOff
            | Self::CoachingReset
            | Self::Rate { .. }
            | Self::ZeroPrefix { .. }
            | Self::Headless { .. }
            | Self::Unknown(_) => RiskDirection::Neutral,

            // Risk-reducing. Instant, friction-exempt, always honored.
            // `/quit` is a risk-reducer because the operator is
            // stepping away from the terminal. `/close` is per-coin
            // position close — sibling to `/flatten-all` and sharing
            // its Reduces classification so the friction-asymmetry
            // invariant ( `Reduces` never gated ) covers both paths.
            Self::Quit
            | Self::Kill
            | Self::FlattenAll
            | Self::PauseEntries
            | Self::Break { .. }
            | Self::Close { .. } => RiskDirection::Reduces,

            // Risk-increasing. Subject to the friction ladder.
            // `/state-override` and `/disclosure-override` are
            // both operator self-declarations that defeat a
            // guardrail; the ladder must gate them for the same
            // reason it gates `/execute`.
            Self::Execute | Self::StateOverride { .. } | Self::DisclosureOverride { .. } => {
                RiskDirection::Increases
            }

            // `/auto`'s risk direction depends on the action. `on`
            // unlocks engine-initiated position changes and joins
            // the friction ladder as Increases; `off` and `status`
            // are Neutral (turning the accelerator off / reading
            // state). `Missing` / `Unknown` are resolved to the
            // command so the dispatcher can emit a usage hint —
            // treating them as Neutral keeps the unresolved form
            // un-gated (typing `/auto` alone should not trip L2
            // friction at TILT just because the operator wanted
            // to see the usage line).
            Self::Auto { action } => {
                if action.is_risk_increasing() {
                    RiskDirection::Increases
                } else {
                    RiskDirection::Neutral
                }
            }
        }
    }

    /// Display name for `/help` and the picker.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Help => "/help",
            Self::Quit => "/quit",
            Self::Clear => "/clear",
            Self::SwitchMode(ModeTarget::Conversation) => "/conv",
            Self::SwitchMode(ModeTarget::Positions) => "/positions (mode)",
            Self::SwitchMode(ModeTarget::Decisions) => "/decisions",
            Self::SwitchMode(ModeTarget::Heat) => "/heat-mode",
            Self::Heat => "/heat",
            Self::Status => "/status",
            Self::Brief => "/brief",
            Self::Risk => "/risk",
            Self::HyperliquidStatus { .. } => "/hl-status",
            Self::Regime { .. } => "/regime",
            Self::Evaluate { .. } => "/evaluate",
            Self::Positions => "/pos",
            Self::Pulse { .. } => "/pulse",
            Self::Approaching => "/approaching",
            Self::Rejections { .. } => "/rejections",
            Self::Kill => "/kill",
            Self::FlattenAll => "/flatten-all",
            Self::PauseEntries => "/pause-entries",
            Self::Break { .. } => "/break",
            Self::Execute => "/execute",
            Self::State => "/state",
            Self::Sessions { .. } => "/sessions",
            Self::Resume { .. } => "/resume",
            Self::Fork => "/fork",
            Self::Save { .. } => "/save",
            Self::Replay { .. } => "/replay",
            Self::Share { .. } => "/share",
            Self::Config { .. } => "/config",
            Self::Verbose { .. } => "/verbose",
            Self::StateOverride { .. } => "/state-override",
            Self::Continue => "/continue",
            Self::Close { .. } => "/close",
            Self::WrapOff => "/wrap-off",
            Self::CoachingReset => "/coaching reset",
            Self::DisclosureOverride { .. } => "/disclosure-override",
            Self::Rate { .. } => "/rate",
            Self::ZeroPrefix { .. } => "(zero-prefix)",
            Self::Auto { .. } => "/auto",
            Self::Headless { .. } => "/headless",
            Self::Unknown(_) => "(unknown)",
        }
    }

    /// Default limit for `/pulse` when the operator omits it.
    /// Chosen to fit comfortably in a scroll-less conversation
    /// pane but still be meaningful; the HTTP layer clamps to
    /// `1..=100` so raising this later is safe.
    #[must_use]
    pub const fn default_pulse_limit() -> u32 {
        20
    }

    /// Default limit for `/rejections` when the operator omits it.
    /// Matches `/pulse` for visual parity; the HTTP layer clamps
    /// to `1..=500`.
    #[must_use]
    pub const fn default_rejections_limit() -> u32 {
        20
    }

    /// Default limit for `/sessions` when the operator omits it.
    /// Twenty rows is "everything I did this week" for a typical
    /// session cadence; higher values make the pane scroll to read
    /// the newest entries, which defeats the purpose of a listing.
    /// Callers in `dispatch` clamp above this so `/sessions 1000`
    /// still shows a tight, navigable list.
    #[must_use]
    pub const fn default_sessions_limit() -> u32 {
        20
    }

    /// Hard ceiling on `/sessions` so a stray high value cannot
    /// spawn a multi-page readout that hides the prompt.
    #[must_use]
    pub const fn max_sessions_limit() -> u32 {
        50
    }
}

/// Static catalog of user-visible slash commands, exposed for
/// command pickers / help pages / documentation generators.
///
/// Kept in *listing order* (not alphabetical): diagnostics first,
/// then live read-outs, then risk-reducing levers, then the gated
/// risk-increasing action. Mode-switchers are omitted because
/// operators reach them via `Ctrl+1..4`; leaving them out of the
/// picker prevents stray mode changes from a mis-typed `/`.
pub const COMMAND_CATALOG: &[CommandInfo] = &[
    CommandInfo {
        name: "/help",
        summary: "list commands",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/status",
        summary: "operator + engine snapshot",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/brief",
        summary: "one-line situation readout",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/risk",
        summary: "risk posture",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/hl-status",
        summary: "read-only Hyperliquid info status",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/heat",
        summary: "composite heat (risk + circuit)",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/regime",
        summary: "market regime (optional coin)",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/evaluate",
        summary: "gate verdict for a coin (overlay)",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/pos",
        summary: "open positions",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/pulse",
        summary: "recent engine events",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/approaching",
        summary: "coins near a gate",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/rejections",
        summary: "recent gate rejections",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/state",
        summary: "operator-state overlay",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/sessions",
        summary: "list recent sessions",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/resume",
        summary: "replay a past session into the log",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/fork",
        summary: "start a new session, linked to this one",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/save",
        summary: "label the current session",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/replay",
        summary: "show a past session without switching",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/share",
        summary: "dump a session as copyable JSON",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/config show",
        summary: "show resolved config values",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/config doctor",
        summary: "self-diagnose config + secrets",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/verbose",
        summary: "toggle rich log timestamps",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/continue",
        summary: "acknowledge coaching notice",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/rate",
        summary: "attach conviction rating to a past trade",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/coaching reset",
        summary: "clear coaching notice buffer",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/wrap-off",
        summary: "skip the daily wrap (this session only)",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/clear",
        summary: "clear conversation log",
        risk: RiskDirection::Neutral,
    },
    CommandInfo {
        name: "/pause-entries",
        summary: "block new positions",
        risk: RiskDirection::Reduces,
    },
    CommandInfo {
        name: "/break",
        summary: "operator-initiated pause",
        risk: RiskDirection::Reduces,
    },
    CommandInfo {
        name: "/close",
        summary: "close one position (per-coin)",
        risk: RiskDirection::Reduces,
    },
    CommandInfo {
        name: "/flatten-all",
        summary: "close all positions",
        risk: RiskDirection::Reduces,
    },
    CommandInfo {
        name: "/kill",
        summary: "hard stop — close + halt",
        risk: RiskDirection::Reduces,
    },
    CommandInfo {
        name: "/quit",
        summary: "exit the CLI",
        risk: RiskDirection::Reduces,
    },
    CommandInfo {
        name: "/state-override",
        summary: "declare operator-state label (gated)",
        risk: RiskDirection::Increases,
    },
    CommandInfo {
        name: "/disclosure-override",
        summary: "bypass progressive disclosure (gated)",
        risk: RiskDirection::Increases,
    },
    CommandInfo {
        name: "/execute",
        summary: "place a new order (gated)",
        risk: RiskDirection::Increases,
    },
    // `/auto on` joins the friction ladder. The catalog row is
    // labeled Increases — that is the most dangerous action on
    // the command and the one pickers should colour-code for.
    // `/auto off|status` land on the same head and degrade to
    // Neutral at dispatch time; a single row keeps the picker
    // clean.
    CommandInfo {
        name: "/auto",
        summary: "toggle auto-accept (on: gated, off/status: neutral)",
        risk: RiskDirection::Increases,
    },
    CommandInfo {
        name: "/headless",
        summary: "start/stop/status the supervisor daemon",
        risk: RiskDirection::Neutral,
    },
];

/// Picker-facing row. Keep the struct tiny: `name` is the literal
/// the picker inserts on Tab; `summary` is the human-readable
/// description rendered to the right.
#[derive(Debug, Clone, Copy)]
pub struct CommandInfo {
    pub name: &'static str,
    pub summary: &'static str,
    pub risk: RiskDirection,
}

/// Resolve a parsed line to a [`Command`]. Unrecognized heads
/// resolve to [`Command::Unknown`]; empty input returns `None` so
/// the caller can skip dispatch silently.
//
// The function is long by design — it is a single flat dispatch
// table from canonical head → Command variant, plus a handful of
// multi-token subcommands (`/config`, `/coaching`, `/disclosure-
// override`). Splitting the arms into helper fns would scatter a
// grep-able, single-source registry across the file and trade
// legibility for a line count. The `match` itself is where
// reviewers look to verify "is `/foo` a thing, and what does it
// parse to?" — we keep it in one place.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn resolve(line: &ParsedLine) -> Option<Command> {
    if line.is_empty() {
        return None;
    }
    let head = line.canonical_head();
    let cmd = match head.as_str() {
        "help" | "?" => Command::Help,
        "quit" | "exit" | "q" => Command::Quit,
        "clear" | "cls" => Command::Clear,
        "conv" | "conversation" => Command::SwitchMode(ModeTarget::Conversation),
        "pos-mode" | "positions-mode" => Command::SwitchMode(ModeTarget::Positions),
        "decisions" => Command::SwitchMode(ModeTarget::Decisions),
        // `heat` is the inline readout; the mode-switch variant is
        // still reachable for operators who explicitly want the full
        // pane (via `Ctrl+4` or `/heat-mode`). Keeping the short word
        // on the inline command matches operator expectation: typing
        // `/heat` should answer "how hot am I?" in the same pane.
        "heat" => Command::Heat,
        "heat-mode" | "heatmode" => Command::SwitchMode(ModeTarget::Heat),
        "status" => Command::Status,
        "brief" => Command::Brief,
        "risk" => Command::Risk,
        "hl-status" | "hl" | "hyperliquid" => Command::HyperliquidStatus {
            symbol: line.args.first().cloned(),
        },
        "regime" => Command::Regime {
            coin: line.args.first().cloned(),
        },
        "evaluate" | "eval" => Command::Evaluate {
            coin: line.args.first().cloned(),
            extras: line.args.iter().skip(1).cloned().collect(),
        },
        "positions" | "pos" => Command::Positions,
        "pulse" => Command::Pulse {
            limit: line.args.first().and_then(|s| s.parse::<u32>().ok()),
        },
        "approaching" | "near" => Command::Approaching,
        "rejections" | "rej" => {
            // Accept the two args in any order: the first numeric
            // token resolves to `limit`, the first non-numeric to
            // `coin`. Keeps `/rejections BTC` and `/rejections 50`
            // and `/rejections BTC 50` all doing the obvious thing.
            let mut coin: Option<String> = None;
            let mut limit: Option<u32> = None;
            for a in &line.args {
                if let Ok(n) = a.parse::<u32>() {
                    if limit.is_none() {
                        limit = Some(n);
                    }
                } else if coin.is_none() {
                    coin = Some(a.clone());
                }
            }
            Command::Rejections { coin, limit }
        }
        "kill" => Command::Kill,
        "flatten-all" | "flatten" => Command::FlattenAll,
        "pause-entries" | "pause" => Command::PauseEntries,
        "break" => Command::Break {
            minutes: line.args.first().and_then(|s| s.parse::<u32>().ok()),
        },
        "execute" | "exec" | "e" => Command::Execute,
        "state" => Command::State,
        "sessions" | "ls-sessions" => Command::Sessions {
            limit: line.args.first().and_then(|s| s.parse::<u32>().ok()),
        },
        "resume" => Command::Resume {
            needle: line.args.first().cloned(),
        },
        "fork" => Command::Fork,
        "save" => Command::Save {
            label: line.args.first().cloned(),
        },
        "replay" => Command::Replay {
            needle: line.args.first().cloned(),
        },
        "share" | "export" => Command::Share {
            needle: line.args.first().cloned(),
        },
        "state-override" | "stateoverride" => {
            let label = line.args.first().and_then(|s| StateOverrideLabel::parse(s));
            Command::StateOverride { label }
        }
        "continue" | "cont" => Command::Continue,
        "rate" => {
            // `/rate <trade_id> <rating>`. Either argument may
            // be missing; the dispatcher surfaces a usage hint
            // on any shape other than "id + numeric in 1..=10".
            // We route the *numeric* argument to `rating`
            // regardless of position so operators can type
            // `/rate 8 t-001` without getting a silent miss —
            // picking the first parsable u8 lets the id-first
            // and rating-first orders resolve identically.
            let mut trade_id: Option<String> = None;
            let mut rating: Option<u8> = None;
            for a in &line.args {
                if rating.is_none()
                    && let Ok(n) = a.parse::<u8>()
                    && (1..=10).contains(&n)
                {
                    rating = Some(n);
                    continue;
                }
                if trade_id.is_none() {
                    trade_id = Some(a.clone());
                }
            }
            Command::Rate { trade_id, rating }
        }
        "close" => Command::Close {
            coin: line.args.first().cloned(),
        },
        "wrap-off" | "wrapoff" => Command::WrapOff,
        // `/coaching reset` is a two-token form. A bare
        // `/coaching` without `reset` resolves to Unknown so
        // the usage hint fires — there is no other coaching
        // subcommand today and silent acceptance would be a
        // lie about what the command does. A single-token
        // `/coaching-reset` alias is accepted for operators who
        // prefer the dash form (consistent with /wrap-off).
        "coaching" => match line.args.first().map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("reset") => Command::CoachingReset,
            Some(other) => Command::Unknown(format!("coaching {other}")),
            None => Command::Unknown("coaching".to_owned()),
        },
        "coaching-reset" => Command::CoachingReset,
        "disclosure-override" | "disclosureoverride" => {
            // Require the exact confirm flag. Accept it as
            // either a raw arg ("--i-know-what-i-am-doing")
            // or as a bareword with the leading dashes
            // stripped so operators who hand-type the phrase
            // land in the same place.
            let confirmed = line.args.iter().any(|a| {
                a == DISCLOSURE_OVERRIDE_CONFIRM
                    || a.trim_start_matches('-')
                        == DISCLOSURE_OVERRIDE_CONFIRM.trim_start_matches('-')
            });
            Command::DisclosureOverride { confirmed }
        }
        "verbose" => {
            let action = match line.args.first().map(|s| s.to_ascii_lowercase()).as_deref() {
                None | Some("toggle") => VerboseAction::Toggle,
                Some("on" | "1" | "true") => VerboseAction::On,
                Some("off" | "0" | "false") => VerboseAction::Off,
                Some(other) => VerboseAction::Unknown(other.to_owned()),
            };
            Command::Verbose { action }
        }
        "config" => {
            let action = match line.args.first().map(String::as_str) {
                None => ConfigAction::Missing,
                Some("show" | "view" | "list" | "ls") => ConfigAction::Show,
                Some("doctor" | "diag" | "diagnose" | "check") => ConfigAction::Doctor,
                Some(other) => ConfigAction::Unknown(other.to_owned()),
            };
            Command::Config { action }
        }
        // `/auto` subcommand. Case-insensitive first arg.
        // Unknown / missing arg resolves to a usage hint at
        // dispatch time — silent acceptance of `/auto foo`
        // would be a honesty bug on a risk-increasing surface.
        // `true` / `1` / `false` / `0` are accepted as friendly
        // aliases for `on` / `off` because scripts and muscle
        // memory will both reach for them.
        "auto" => {
            let action = match line.args.first().map(|s| s.to_ascii_lowercase()).as_deref() {
                None => AutoAction::Missing,
                Some("on" | "1" | "true") => AutoAction::On,
                Some("off" | "0" | "false") => AutoAction::Off,
                Some("status" | "stat" | "show") => AutoAction::Status,
                Some(other) => AutoAction::Unknown(other.to_owned()),
            };
            Command::Auto { action }
        }
        // `/headless` subcommand. Mirrors `/auto` for the
        // parser — case-insensitive, missing / unknown arg
        // resolves to a usage hint. No `true` / `false`
        // aliases: `start` / `stop` are the spawn-ish verbs
        // operators reach for and conflating them with booleans
        // would obscure the daemon lifecycle.
        "headless" => {
            let action = match line.args.first().map(|s| s.to_ascii_lowercase()).as_deref() {
                None => HeadlessAction::Missing,
                Some("start" | "up") => HeadlessAction::Start,
                Some("stop" | "down") => HeadlessAction::Stop,
                Some("status" | "stat" | "show") => HeadlessAction::Status,
                Some(other) => HeadlessAction::Unknown(other.to_owned()),
            };
            Command::Headless { action }
        }
        // `/doctor` is a top-level alias for `/config doctor`.
        // An operator hitting a broken-auth / broken-engine
        // state is going to type the single most obvious word
        // ("doctor") before they think about namespacing. The
        // original nested-only form is preserved so operators
        // who think in `/config` still find it there, and both
        // routes hit the same dispatch arm. Aliases like `diag`
        // / `diagnose` / `check` also resolve here so the
        // top-level surface matches the nested one 1:1.
        "doctor" | "diag" | "diagnose" | "check" => Command::Config {
            action: ConfigAction::Doctor,
        },
        // An operator in a broken session naturally re-types what
        // the README + `zero --help` told them to run: `zero
        // doctor`, `zero init`, `zero --version`. Inside the TUI
        // that parses as head=`zero`, so falling through to
        // `Command::Unknown` would give the generic "unknown
        // command: /zero" — technically true, operationally
        // cruel. Intercept here and carry the tail verbatim so
        // the dispatcher can emit a teaching hint that names the
        // correct in-TUI form (`/doctor`, `/init` if/when it
        // ships, `/version` if/when it ships). Works only for
        // the literal bareword `zero` at column 0 — a slash
        // `/zero` or a suffix (`zero-foo`) falls through
        // normally. Args are re-joined with single spaces; this
        // loses original whitespace but the hint only needs to
        // reproduce intent, not be round-trippable.
        "zero" => Command::ZeroPrefix {
            rest: line.args.join(" "),
        },
        _ => Command::Unknown(head),
    };
    Some(cmd)
}

#[cfg(test)]
mod tests {
    use super::{
        Command, ConfigAction, DISCLOSURE_OVERRIDE_CONFIRM, ModeTarget, StateOverrideLabel,
        VerboseAction, resolve,
    };
    use crate::parse::parse_line;
    use crate::risk::RiskDirection;

    fn r(line: &str) -> Option<Command> {
        resolve(&parse_line(line))
    }

    #[test]
    fn empty_input_returns_none() {
        assert_eq!(r(""), None);
        assert_eq!(r("   "), None);
    }

    #[test]
    fn common_commands_resolve() {
        assert_eq!(r("/help"), Some(Command::Help));
        assert_eq!(r("?"), Some(Command::Help));
        assert_eq!(r("/quit"), Some(Command::Quit));
        assert_eq!(r("q"), Some(Command::Quit));
        assert_eq!(r("/status"), Some(Command::Status));
        assert_eq!(r("/brief"), Some(Command::Brief));
        assert_eq!(r("/risk"), Some(Command::Risk));
    }

    #[test]
    fn regime_takes_optional_coin() {
        assert_eq!(r("/regime"), Some(Command::Regime { coin: None }));
        assert_eq!(
            r("/regime BTC"),
            Some(Command::Regime {
                coin: Some("BTC".into())
            })
        );
    }

    #[test]
    fn break_parses_minutes() {
        assert_eq!(r("/break"), Some(Command::Break { minutes: None }));
        assert_eq!(r("/break 15"), Some(Command::Break { minutes: Some(15) }));
    }

    #[test]
    fn mode_switches() {
        assert_eq!(
            r("/conv"),
            Some(Command::SwitchMode(ModeTarget::Conversation))
        );
        assert_eq!(
            r("/decisions"),
            Some(Command::SwitchMode(ModeTarget::Decisions))
        );
        // `/heat` is the inline heat readout; the mode variant is
        // reachable via the explicit `/heat-mode` synonym (and Ctrl+4).
        assert_eq!(r("/heat-mode"), Some(Command::SwitchMode(ModeTarget::Heat)));
    }

    #[test]
    fn heat_resolves_to_inline_readout() {
        assert_eq!(r("/heat"), Some(Command::Heat));
        // Trailing junk ignored — heat takes no args.
        assert_eq!(r("/heat something"), Some(Command::Heat));
    }

    #[test]
    fn heat_is_neutral_risk() {
        assert_eq!(Command::Heat.risk(), RiskDirection::Neutral);
    }

    #[test]
    fn evaluate_takes_optional_coin() {
        assert_eq!(
            r("/evaluate"),
            Some(Command::Evaluate {
                coin: None,
                extras: vec![]
            })
        );
        assert_eq!(
            r("/evaluate BTC"),
            Some(Command::Evaluate {
                coin: Some("BTC".into()),
                extras: vec![]
            })
        );
        assert_eq!(
            r("/eval eth"),
            Some(Command::Evaluate {
                coin: Some("eth".into()),
                extras: vec![]
            })
        );
    }

    #[test]
    fn evaluate_preserves_extra_args_for_warning() {
        // `/evaluate sol short` — the trailing `short` must be
        // kept on the command so the dispatcher can warn about
        // it explicitly; silently dropping it would let an
        // operator believe the bias was accepted.
        assert_eq!(
            r("/evaluate sol short"),
            Some(Command::Evaluate {
                coin: Some("sol".into()),
                extras: vec!["short".into()],
            })
        );
        assert_eq!(
            r("/evaluate BTC long now please"),
            Some(Command::Evaluate {
                coin: Some("BTC".into()),
                extras: vec!["long".into(), "now".into(), "please".into()],
            })
        );
    }

    #[test]
    fn evaluate_is_neutral_risk() {
        assert_eq!(
            Command::Evaluate {
                coin: Some("BTC".into()),
                extras: vec![],
            }
            .risk(),
            RiskDirection::Neutral
        );
    }

    #[test]
    fn pulse_parses_optional_limit() {
        assert_eq!(r("/pulse"), Some(Command::Pulse { limit: None }));
        assert_eq!(r("/pulse 50"), Some(Command::Pulse { limit: Some(50) }));
        // Non-numeric argument is silently discarded — /pulse never
        // took a coin, and flagging would surprise operators.
        assert_eq!(r("/pulse BTC"), Some(Command::Pulse { limit: None }));
    }

    #[test]
    fn approaching_takes_no_args() {
        assert_eq!(r("/approaching"), Some(Command::Approaching));
        assert_eq!(r("/near"), Some(Command::Approaching));
        assert_eq!(r("/approaching ignored"), Some(Command::Approaching));
    }

    #[test]
    fn rejections_parses_coin_and_limit_in_any_order() {
        assert_eq!(
            r("/rejections"),
            Some(Command::Rejections {
                coin: None,
                limit: None
            })
        );
        assert_eq!(
            r("/rejections BTC"),
            Some(Command::Rejections {
                coin: Some("BTC".into()),
                limit: None
            })
        );
        assert_eq!(
            r("/rejections 50"),
            Some(Command::Rejections {
                coin: None,
                limit: Some(50)
            })
        );
        assert_eq!(
            r("/rejections BTC 50"),
            Some(Command::Rejections {
                coin: Some("BTC".into()),
                limit: Some(50)
            })
        );
        assert_eq!(
            r("/rejections 50 BTC"),
            Some(Command::Rejections {
                coin: Some("BTC".into()),
                limit: Some(50)
            })
        );
        assert_eq!(
            r("/rej"),
            Some(Command::Rejections {
                coin: None,
                limit: None
            })
        );
    }

    #[test]
    fn new_read_commands_are_neutral() {
        assert_eq!(
            Command::HyperliquidStatus { symbol: None }.risk(),
            RiskDirection::Neutral
        );
        assert_eq!(
            Command::Pulse { limit: None }.risk(),
            RiskDirection::Neutral
        );
        assert_eq!(Command::Approaching.risk(), RiskDirection::Neutral);
        assert_eq!(
            Command::Rejections {
                coin: None,
                limit: None
            }
            .risk(),
            RiskDirection::Neutral
        );
    }

    #[test]
    fn hyperliquid_status_takes_optional_symbol() {
        assert_eq!(
            r("/hl-status"),
            Some(Command::HyperliquidStatus { symbol: None })
        );
        assert_eq!(
            r("/hl BTC"),
            Some(Command::HyperliquidStatus {
                symbol: Some("BTC".into())
            })
        );
        assert_eq!(
            r("/hyperliquid ETH"),
            Some(Command::HyperliquidStatus {
                symbol: Some("ETH".into())
            })
        );
    }

    #[test]
    fn sessions_parses_optional_limit() {
        assert_eq!(r("/sessions"), Some(Command::Sessions { limit: None }));
        assert_eq!(r("/sessions 5"), Some(Command::Sessions { limit: Some(5) }));
        // Alias + non-numeric gracefully drops the limit.
        assert_eq!(r("/ls-sessions"), Some(Command::Sessions { limit: None }));
        assert_eq!(r("/sessions BTC"), Some(Command::Sessions { limit: None }));
    }

    #[test]
    fn resume_takes_optional_needle() {
        assert_eq!(r("/resume"), Some(Command::Resume { needle: None }));
        assert_eq!(
            r("/resume 01H"),
            Some(Command::Resume {
                needle: Some("01H".into())
            })
        );
        // Labels are free-form strings (no quoting needed for
        // single tokens); `/resume scratch` should carry the word
        // intact to the dispatcher.
        assert_eq!(
            r("/resume scratch"),
            Some(Command::Resume {
                needle: Some("scratch".into())
            })
        );
    }

    #[test]
    fn fork_takes_no_args_and_ignores_extras() {
        assert_eq!(r("/fork"), Some(Command::Fork));
        // Trailing junk is ignored for consistency with /approaching;
        // operators hit Enter without clearing the prompt sometimes.
        assert_eq!(r("/fork ignored"), Some(Command::Fork));
    }

    #[test]
    fn save_parses_label() {
        assert_eq!(r("/save"), Some(Command::Save { label: None }));
        assert_eq!(
            r("/save pre-cpi"),
            Some(Command::Save {
                label: Some("pre-cpi".into())
            })
        );
    }

    #[test]
    fn session_cohort_is_neutral_risk() {
        assert_eq!(
            Command::Sessions { limit: None }.risk(),
            RiskDirection::Neutral
        );
        assert_eq!(
            Command::Resume { needle: None }.risk(),
            RiskDirection::Neutral
        );
        assert_eq!(Command::Fork.risk(), RiskDirection::Neutral);
        assert_eq!(Command::Save { label: None }.risk(), RiskDirection::Neutral);
        assert_eq!(
            Command::Replay { needle: None }.risk(),
            RiskDirection::Neutral
        );
        assert_eq!(
            Command::Share { needle: None }.risk(),
            RiskDirection::Neutral
        );
    }

    #[test]
    fn replay_takes_optional_needle() {
        assert_eq!(r("/replay"), Some(Command::Replay { needle: None }));
        assert_eq!(
            r("/replay 01HOLD"),
            Some(Command::Replay {
                needle: Some("01HOLD".into())
            })
        );
        assert_eq!(
            r("/replay pre-cpi"),
            Some(Command::Replay {
                needle: Some("pre-cpi".into())
            })
        );
    }

    #[test]
    fn share_takes_optional_needle_and_export_alias() {
        assert_eq!(r("/share"), Some(Command::Share { needle: None }));
        assert_eq!(
            r("/share 01HOLD"),
            Some(Command::Share {
                needle: Some("01HOLD".into())
            })
        );
        // `export` is a learnability alias; operators coming from
        // other CLIs reach for it first. Both paths resolve to the
        // same variant so `/help` only lists one.
        assert_eq!(
            r("/export 01HOLD"),
            Some(Command::Share {
                needle: Some("01HOLD".into())
            })
        );
    }

    #[test]
    fn config_subcommand_parses_known_actions_and_aliases() {
        assert_eq!(
            r("/config show"),
            Some(Command::Config {
                action: ConfigAction::Show
            })
        );
        // `view`, `ls`, `list` all fold into Show — operators
        // coming from different CLIs land on the same handler.
        assert_eq!(
            r("/config view"),
            Some(Command::Config {
                action: ConfigAction::Show
            })
        );
        assert_eq!(
            r("/config ls"),
            Some(Command::Config {
                action: ConfigAction::Show
            })
        );
        assert_eq!(
            r("/config doctor"),
            Some(Command::Config {
                action: ConfigAction::Doctor
            })
        );
        assert_eq!(
            r("/config check"),
            Some(Command::Config {
                action: ConfigAction::Doctor
            })
        );
    }

    #[test]
    fn zero_prefix_is_intercepted_with_typed_tail() {
        // `zero doctor` and friends must NOT fall through to
        // `Command::Unknown` — that would give the generic
        // "unknown command: /zero" and bury the teaching
        // opportunity. Instead they resolve to `ZeroPrefix`
        // carrying the typed tail verbatim so the dispatcher
        // can echo intent back.
        match r("zero doctor") {
            Some(Command::ZeroPrefix { rest }) => assert_eq!(rest, "doctor"),
            other => panic!("expected ZeroPrefix, got {other:?}"),
        }
        match r("zero --version") {
            Some(Command::ZeroPrefix { rest }) => assert_eq!(rest, "--version"),
            other => panic!("expected ZeroPrefix, got {other:?}"),
        }
        match r("zero init --force") {
            Some(Command::ZeroPrefix { rest }) => assert_eq!(rest, "init --force"),
            other => panic!("expected ZeroPrefix, got {other:?}"),
        }
        match r("zero") {
            Some(Command::ZeroPrefix { rest }) => assert_eq!(rest, ""),
            other => panic!("expected ZeroPrefix, got {other:?}"),
        }
    }

    #[test]
    fn slash_zero_also_triggers_prefix_hint() {
        // Canonicalization strips the leading `/` before the
        // registry match (see `ParsedLine::canonical_head`), so
        // `/zero doctor` and `zero doctor` both hit the same
        // arm and both get the teaching hint. This is
        // deliberate: an operator who typed `/zero doctor`
        // because they half-remembered the slash prefix has
        // made exactly the same mistake as one who typed
        // `zero doctor`, and deserves the same help. Pinning
        // both shapes here means a future parser refactor
        // (e.g. distinguishing slash-prefixed commands from
        // bare commands) cannot silently regress this.
        match r("/zero doctor") {
            Some(Command::ZeroPrefix { rest }) => assert_eq!(rest, "doctor"),
            other => panic!("expected ZeroPrefix, got {other:?}"),
        }
    }

    #[test]
    fn zero_prefix_is_neutral_risk() {
        // ZeroPrefix never mutates anything; it's a hint-emit.
        // The `risk()` mapping adds it to the Neutral bucket
        // next to Unknown, and this pins that. If a future
        // refactor accidentally promotes it to Increases /
        // Reduces the friction gate would fire on a bare
        // `zero doctor` typo, which is absurd.
        let cmd = Command::ZeroPrefix {
            rest: String::new(),
        };
        assert_eq!(cmd.risk(), RiskDirection::Neutral);
    }

    #[test]
    fn doctor_top_level_alias_resolves_to_config_doctor() {
        // An operator hitting broken-auth state types the single
        // most obvious word. This test pins that `/doctor`,
        // `/diag`, `/diagnose`, `/check`, and the slash-less
        // `doctor` form all land on the same Config/Doctor
        // dispatch as the nested `/config doctor`. Keeping the
        // variants in the registry in one place means a future
        // reviewer checks one test, not four.
        for input in ["/doctor", "doctor", "/diag", "/diagnose", "/check"] {
            assert_eq!(
                r(input),
                Some(Command::Config {
                    action: ConfigAction::Doctor
                }),
                "input {input:?} did not alias to /config doctor",
            );
        }
    }

    #[test]
    fn config_bare_invocation_is_missing_action() {
        // Must resolve to the command (so the dispatcher can
        // emit a usage hint) rather than falling through to
        // Unknown — the latter would make `/config` look like
        // a typo even though it is a valid command stem.
        assert_eq!(
            r("/config"),
            Some(Command::Config {
                action: ConfigAction::Missing
            })
        );
    }

    #[test]
    fn config_unknown_action_preserved_for_hint() {
        // Keep the original token so the dispatcher can say
        // exactly what was rejected. Silent acceptance would
        // leave operators wondering whether `/config secrets`
        // did something.
        assert_eq!(
            r("/config secrets"),
            Some(Command::Config {
                action: ConfigAction::Unknown("secrets".into())
            })
        );
    }

    #[test]
    fn config_is_neutral_risk() {
        assert_eq!(
            Command::Config {
                action: ConfigAction::Show
            }
            .risk(),
            RiskDirection::Neutral
        );
        assert_eq!(
            Command::Config {
                action: ConfigAction::Doctor
            }
            .risk(),
            RiskDirection::Neutral
        );
    }

    #[test]
    fn verbose_parses_on_off_toggle() {
        assert_eq!(
            r("/verbose"),
            Some(Command::Verbose {
                action: VerboseAction::Toggle
            })
        );
        assert_eq!(
            r("/verbose toggle"),
            Some(Command::Verbose {
                action: VerboseAction::Toggle
            })
        );
        assert_eq!(
            r("/verbose on"),
            Some(Command::Verbose {
                action: VerboseAction::On
            })
        );
        assert_eq!(
            r("/verbose ON"),
            Some(Command::Verbose {
                action: VerboseAction::On
            })
        );
        assert_eq!(
            r("/verbose off"),
            Some(Command::Verbose {
                action: VerboseAction::Off
            })
        );
        // Booleans accepted too — operators script these.
        assert_eq!(
            r("/verbose true"),
            Some(Command::Verbose {
                action: VerboseAction::On
            })
        );
        assert_eq!(
            r("/verbose 0"),
            Some(Command::Verbose {
                action: VerboseAction::Off
            })
        );
    }

    #[test]
    fn verbose_preserves_unknown_token_for_usage_hint() {
        assert_eq!(
            r("/verbose maybe"),
            Some(Command::Verbose {
                action: VerboseAction::Unknown("maybe".into())
            })
        );
    }

    #[test]
    fn verbose_is_neutral_risk() {
        assert_eq!(
            Command::Verbose {
                action: VerboseAction::Toggle
            }
            .risk(),
            RiskDirection::Neutral
        );
    }

    #[test]
    fn state_override_parses_canonical_labels() {
        // Case-insensitive; the valid set matches the engine-
        // side classifier. An unknown token resolves to the
        // command with `None` so the dispatcher can surface a
        // usage hint naming the valid labels — silent drop
        // would leave operators wondering whether the typo
        // was the reason nothing changed.
        assert_eq!(
            r("/state-override STEADY"),
            Some(Command::StateOverride {
                label: Some(StateOverrideLabel::Steady),
            })
        );
        assert_eq!(
            r("/state-override steady"),
            Some(Command::StateOverride {
                label: Some(StateOverrideLabel::Steady),
            })
        );
        assert_eq!(
            r("/state-override Tilt"),
            Some(Command::StateOverride {
                label: Some(StateOverrideLabel::Tilt),
            })
        );
        assert_eq!(
            r("/state-override blue"),
            Some(Command::StateOverride { label: None })
        );
        assert_eq!(
            r("/state-override"),
            Some(Command::StateOverride { label: None })
        );
    }

    #[test]
    fn state_override_is_increases_risk() {
        // Friction asymmetry: a self-declared label must be
        // gated exactly like `/execute` because it can unlock
        // lower-friction risky moves.
        assert_eq!(
            Command::StateOverride {
                label: Some(StateOverrideLabel::Steady)
            }
            .risk(),
            RiskDirection::Increases
        );
    }

    #[test]
    fn continue_parses_with_alias() {
        assert_eq!(r("/continue"), Some(Command::Continue));
        assert_eq!(r("/cont"), Some(Command::Continue));
        assert_eq!(Command::Continue.risk(), RiskDirection::Neutral);
    }

    #[test]
    fn rate_parses_id_and_rating_in_either_order() {
        // Canonical shape — trade id first, then rating.
        assert_eq!(
            r("/rate t-001 8"),
            Some(Command::Rate {
                trade_id: Some("t-001".into()),
                rating: Some(8),
            })
        );
        // Rating first, id second: operators under pressure
        // should not get a silent miss for a transposed order.
        // The numeric token binds to `rating` irrespective of
        // position; the first non-numeric binds to `trade_id`.
        assert_eq!(
            r("/rate 8 t-001"),
            Some(Command::Rate {
                trade_id: Some("t-001".into()),
                rating: Some(8),
            })
        );
        // Boundary values inside the 1..=10 window parse.
        assert_eq!(
            r("/rate t 1"),
            Some(Command::Rate {
                trade_id: Some("t".into()),
                rating: Some(1),
            })
        );
        assert_eq!(
            r("/rate t 10"),
            Some(Command::Rate {
                trade_id: Some("t".into()),
                rating: Some(10),
            })
        );
    }

    #[test]
    fn rate_rejects_out_of_range_and_missing_arguments() {
        // Bare invocation: both slots None so the dispatcher
        // emits a usage hint naming the full 1..=10 range.
        assert_eq!(
            r("/rate"),
            Some(Command::Rate {
                trade_id: None,
                rating: None,
            })
        );
        // Out-of-range numerics are *not* bound as `rating` —
        // silently clamping to 1 or 10 would launder a typo,
        // so the parser instead routes `0` / `11` to the
        // `trade_id` slot (first non-numeric-or-in-range
        // token). The usage hint fires because `rating` is
        // still None.
        assert_eq!(
            r("/rate 0"),
            Some(Command::Rate {
                trade_id: Some("0".into()),
                rating: None,
            })
        );
        assert_eq!(
            r("/rate 11"),
            Some(Command::Rate {
                trade_id: Some("11".into()),
                rating: None,
            })
        );
        // A well-formed id with no rating: the id binds, the
        // rating stays None so the handler's shape-check fires.
        assert_eq!(
            r("/rate t-001"),
            Some(Command::Rate {
                trade_id: Some("t-001".into()),
                rating: None,
            })
        );
    }

    #[test]
    fn rate_is_neutral_risk() {
        assert_eq!(
            Command::Rate {
                trade_id: Some("t".into()),
                rating: Some(5),
            }
            .risk(),
            RiskDirection::Neutral,
            "/rate is a self-report about a past trade, not a position change",
        );
    }

    #[test]
    fn close_takes_optional_coin() {
        assert_eq!(r("/close"), Some(Command::Close { coin: None }));
        assert_eq!(
            r("/close BTC"),
            Some(Command::Close {
                coin: Some("BTC".into())
            })
        );
        // The asymmetry invariant: /close must be a Reduces so
        // the friction ladder never gates a per-coin close.
        assert_eq!(Command::Close { coin: None }.risk(), RiskDirection::Reduces);
    }

    #[test]
    fn wrap_off_parses_with_alias_and_is_neutral() {
        assert_eq!(r("/wrap-off"), Some(Command::WrapOff));
        assert_eq!(r("/wrapoff"), Some(Command::WrapOff));
        assert_eq!(Command::WrapOff.risk(), RiskDirection::Neutral);
    }

    #[test]
    fn coaching_reset_parses_two_token_and_dash_forms() {
        assert_eq!(r("/coaching reset"), Some(Command::CoachingReset));
        assert_eq!(r("/coaching RESET"), Some(Command::CoachingReset));
        assert_eq!(r("/coaching-reset"), Some(Command::CoachingReset));
        // Bare `/coaching` without `reset` is honest-fail —
        // there is no other coaching subcommand today.
        assert!(matches!(r("/coaching"), Some(Command::Unknown(_))));
        assert!(matches!(r("/coaching wut"), Some(Command::Unknown(_))));
        assert_eq!(Command::CoachingReset.risk(), RiskDirection::Neutral);
    }

    #[test]
    fn disclosure_override_requires_exact_phrase() {
        // No phrase → confirmed=false; dispatcher emits a
        // usage alert naming the exact words.
        assert_eq!(
            r("/disclosure-override"),
            Some(Command::DisclosureOverride { confirmed: false })
        );
        // Exact flag word with dashes:
        let exact = format!("/disclosure-override {DISCLOSURE_OVERRIDE_CONFIRM}");
        assert_eq!(
            r(&exact),
            Some(Command::DisclosureOverride { confirmed: true })
        );
        // Dashless variant (operator hand-types the phrase)
        // resolves the same — we do not gatekeep on punctuation.
        assert_eq!(
            r("/disclosure-override i-know-what-i-am-doing"),
            Some(Command::DisclosureOverride { confirmed: true })
        );
        // A wrong phrase leaves confirmed=false, not Unknown —
        // so the handler can tell the operator exactly what they
        // were missing.
        assert_eq!(
            r("/disclosure-override yolo"),
            Some(Command::DisclosureOverride { confirmed: false })
        );
    }

    #[test]
    fn disclosure_override_is_increases_risk_regardless_of_confirm() {
        // Risk classification does not depend on argument
        // correctness — both confirmed/unconfirmed carry the
        // same Increases tag so the friction gate evaluates
        // consistently.
        assert_eq!(
            Command::DisclosureOverride { confirmed: true }.risk(),
            RiskDirection::Increases
        );
        assert_eq!(
            Command::DisclosureOverride { confirmed: false }.risk(),
            RiskDirection::Increases
        );
    }

    #[test]
    fn risk_classification_holds() {
        assert_eq!(Command::Help.risk(), RiskDirection::Neutral);
        assert_eq!(Command::Status.risk(), RiskDirection::Neutral);
        assert_eq!(Command::Quit.risk(), RiskDirection::Reduces);
        assert_eq!(Command::Kill.risk(), RiskDirection::Reduces);
        assert_eq!(Command::FlattenAll.risk(), RiskDirection::Reduces);
        assert_eq!(Command::PauseEntries.risk(), RiskDirection::Reduces);
        assert_eq!(
            Command::Break { minutes: None }.risk(),
            RiskDirection::Reduces
        );
    }
}
